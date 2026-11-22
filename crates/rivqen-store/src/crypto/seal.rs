// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The sealed form (ADR-011 section 3):
//!
//! ```text
//! "RQS1" ‖ nonce (24 bytes) ‖ ciphertext ‖ tag (16 bytes)
//! ```
//!
//! XChaCha20-Poly1305 with a random nonce from the key port for each seal.
//! The associated data binds the role, the schema version, the partition,
//! the cache key and the file id, so a sealed file or row that is moved to
//! another place does not open.

use std::fmt;

use chacha20poly1305::aead::AeadInOut;
use chacha20poly1305::{KeyInit, XChaCha20Poly1305};
use rivqen_cache::driver::StoreError;
use rivqen_cache::key::Partition;
use rivqen_model::ids::CacheKey;

use super::{DataKey, FileId, KeyProvider, corrupted, unavailable};

/// The first 4 bytes of every sealed file and row.
pub const MAGIC: [u8; 4] = *b"RQS1";
/// The length of the nonce in bytes.
pub const NONCE_BYTES: usize = 24;
/// The length of the tag in bytes.
pub const TAG_BYTES: usize = 16;
/// The bytes that a seal adds to the plain text: magic, nonce and tag.
pub const SEAL_OVERHEAD: usize = MAGIC.len() + NONCE_BYTES + TAG_BYTES;
/// The first part of the associated data, with the version of its layout.
pub const SEAL_DOMAIN: &[u8] = b"rivqen-seal-v1";

/// The length of the head: magic and nonce.
const HEAD_BYTES: usize = MAGIC.len() + NONCE_BYTES;
/// The head of a scoped partition field (as in the cache key).
const TAG_SCOPED: u8 = 0x01;
/// The whole partition field of `public` (as in the cache key).
const TAG_PUBLIC: u8 = 0x02;

/// What a sealed value is: an entry file or the `sealed` column of a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// The content file of an entry version.
    File,
    /// The `sealed` column of a metadata row.
    Row,
}

impl Role {
    /// The role part of the associated data.
    const fn label(self) -> &'static [u8] {
        match self {
            Self::File => b"file",
            Self::Row => b"row",
        }
    }
}

/// The associated data of one seal (ADR-011 section 3). Each part is
/// written after its length as 8 bytes big-endian:
///
/// 1. `rivqen-seal-v1`;
/// 2. the role: `file` or `row`;
/// 3. the schema version: 4 bytes big-endian;
/// 4. the partition field of the cache key: `02` for `public`, or `01`
///    and the 32 bytes of the id;
/// 5. the cache key: 32 bytes;
/// 6. the file id: 16 bytes.
///
/// [`fmt::Debug`] writes the length only.
#[derive(Clone, PartialEq, Eq)]
pub struct Aad(Vec<u8>);

impl Aad {
    /// The associated data of a value of `role` of the entry `key` in
    /// `partition`, whose version has the file `file_id`, in the schema
    /// `schema_version` (`rivqen_cache::txn::SCHEMA_VERSION`).
    #[must_use]
    pub fn new(
        role: Role,
        schema_version: u32,
        partition: Partition,
        key: &CacheKey,
        file_id: &FileId,
    ) -> Self {
        let mut out = Vec::with_capacity(160);
        put_part(&mut out, &[SEAL_DOMAIN]);
        put_part(&mut out, &[role.label()]);
        put_part(&mut out, &[&schema_version.to_be_bytes()]);
        match &partition {
            Partition::Scoped(id) => put_part(&mut out, &[&[TAG_SCOPED], id.as_bytes()]),
            Partition::Public => put_part(&mut out, &[&[TAG_PUBLIC]]),
        }
        put_part(&mut out, &[key.as_bytes()]);
        put_part(&mut out, &[file_id.as_bytes()]);
        Self(out)
    }

    /// The bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for Aad {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Aad(len={})", self.0.len())
    }
}

/// Appends the concatenation of `pieces` after its length as 8 bytes
/// big-endian.
fn put_part(out: &mut Vec<u8>, pieces: &[&[u8]]) {
    let len = pieces
        .iter()
        .fold(0_usize, |sum, piece| sum.saturating_add(piece.len()));
    out.extend_from_slice(&u64::try_from(len).unwrap_or(u64::MAX).to_be_bytes());
    for piece in pieces {
        out.extend_from_slice(piece);
    }
}

/// Seals `plain` with `key` and `aad`. The nonce is 24 bytes from
/// `nonces.random`.
///
/// # Errors
///
/// [`rivqen_cache::driver::StoreErrorKind::Unavailable`] when the port
/// gives no random bytes, or when the plain text is too long for the cipher
/// (more than 256 GiB). Nothing is returned then.
pub fn seal<P: KeyProvider + ?Sized>(
    key: &DataKey,
    nonces: &mut P,
    aad: &Aad,
    plain: &[u8],
) -> Result<Vec<u8>, StoreError> {
    const OP: &str = "seal";
    let mut nonce = [0_u8; NONCE_BYTES];
    nonces
        .random(&mut nonce)
        .map_err(|error| unavailable(error.op()))?;

    let capacity = plain.len().saturating_add(SEAL_OVERHEAD);
    let mut out = Vec::with_capacity(capacity);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(plain);
    let body = out.get_mut(HEAD_BYTES..).ok_or(unavailable(OP))?;
    let cipher = XChaCha20Poly1305::new(key.bytes().into());
    let tag = cipher
        .encrypt_inout_detached((&nonce).into(), aad.as_bytes(), body.into())
        .map_err(|_| unavailable(OP))?;
    out.extend_from_slice(tag.as_slice());
    Ok(out)
}

/// Opens `sealed` with `key` and `aad` and gives the plain text.
///
/// # Errors
///
/// [`rivqen_cache::driver::StoreErrorKind::Corrupted`] for every failure:
/// an input shorter than [`SEAL_OVERHEAD`], another magic, a tag that does
/// not match (a changed byte, another key, other associated data). The
/// error is the same for each reason.
pub fn open(key: &DataKey, aad: &Aad, sealed: &[u8]) -> Result<Vec<u8>, StoreError> {
    let fail = || corrupted("open");
    let (magic, rest) = sealed.split_first_chunk::<4>().ok_or_else(fail)?;
    let (nonce, rest) = rest.split_first_chunk::<NONCE_BYTES>().ok_or_else(fail)?;
    let (body, tag) = rest.split_last_chunk::<TAG_BYTES>().ok_or_else(fail)?;
    if *magic != MAGIC {
        return Err(fail());
    }
    let mut plain = body.to_vec();
    let cipher = XChaCha20Poly1305::new(key.bytes().into());
    cipher
        .decrypt_inout_detached(
            nonce.into(),
            aad.as_bytes(),
            plain.as_mut_slice().into(),
            tag.into(),
        )
        .map_err(|_| fail())?;
    Ok(plain)
}
