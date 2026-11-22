// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Owner: WP-07 S2 T-03: the key port, the entry file format and the AEAD seal (ADR-011).
//!
//! The driver seals every entry (ADR-011 section 1, H-151): the content file
//! and the `sealed` column of its metadata row. This module gives:
//!
//! - the key port [`KeyProvider`] and the key type [`DataKey`];
//! - [`HostKeys`], a key provider for tests and the host;
//! - [`FileId`], the 16 random bytes that name one entry version;
//! - [`seal`] and [`open`] with the associated data [`Aad`];
//! - the two plain-text codecs: [`encode_file`] / [`decode_file`] and
//!   [`encode_row`] / [`decode_row`].
//!
//! Invariants:
//!
//! - No key byte, nonce or plain text is in a `Debug` text, a `Display`
//!   text or an error. Every error is a [`StoreError`] with a static
//!   operation name only.
//! - A [`DataKey`] is set to zero when it is dropped, and has no `Clone`.
//! - Every failure of [`open`] and of the two decoders is
//!   [`StoreErrorKind::Corrupted`] with no reason. Every failure of the key
//!   port is [`StoreErrorKind::Unavailable`].
//! - The decoders read untrusted bytes. They never panic, check each
//!   length against the remaining input before they copy, check each size
//!   against [`DecodeLimits`], and reject trailing bytes. A decoded value
//!   encodes to the same bytes.

mod codec;
mod file;
mod host;
mod row;
mod seal;

use std::fmt;

use rivqen_cache::driver::{StoreError, StoreErrorKind};
use rivqen_cache::key::Partition;
use zeroize::{Zeroize, ZeroizeOnDrop};

pub use codec::DecodeLimits;
pub use file::{FILE_FORMAT, FileContent, decode_file, encode_file};
pub use host::HostKeys;
pub use row::{ROW_FORMAT, RowContent, decode_row, encode_row};
pub use seal::{Aad, MAGIC, NONCE_BYTES, Role, SEAL_DOMAIN, SEAL_OVERHEAD, TAG_BYTES, open, seal};

/// The length of a data key in bytes.
pub const KEY_BYTES: usize = 32;

/// The length of a file id in bytes.
pub const FILE_ID_BYTES: usize = 16;

/// The key port (ADR-011 section 2). The platform adapter implements it with
/// the key store of the device (WP-10, WP-11); [`HostKeys`] implements it
/// for tests and the host.
///
/// Every error of an implementation is [`StoreErrorKind::Unavailable`]
/// (`STORAGE_UNAVAILABLE`): the cache is not usable and the page loads
/// normally.
pub trait KeyProvider {
    /// The data key of `partition`. The port creates the key at the first
    /// call and gives the same key until [`KeyProvider::delete_key`].
    ///
    /// # Errors
    ///
    /// [`StoreErrorKind::Unavailable`] when the key store is not available.
    fn data_key(&mut self, partition: Partition) -> Result<DataKey, StoreError>;

    /// Deletes the data key of `partition`. A later
    /// [`KeyProvider::data_key`] gives a new key. Deleting a key that does
    /// not exist is not an error.
    ///
    /// # Errors
    ///
    /// [`StoreErrorKind::Unavailable`] when the key store is not available.
    fn delete_key(&mut self, partition: Partition) -> Result<(), StoreError>;

    /// Fills `buf` with random bytes from the system source.
    ///
    /// # Errors
    ///
    /// [`StoreErrorKind::Unavailable`] when the source gives no bytes.
    fn random(&mut self, buf: &mut [u8]) -> Result<(), StoreError>;
}

/// A data key: 32 bytes (ADR-011 section 2).
///
/// The bytes are on the heap, so a move of the value copies no key byte.
/// They are set to zero on drop. There is no `Clone`, no comparison and no
/// way to read the bytes out of the crate. [`fmt::Debug`] writes
/// `DataKey(<redacted>)`.
pub struct DataKey(Box<[u8; KEY_BYTES]>);

impl DataKey {
    /// A key from `bytes`. The function copies the bytes and then sets
    /// `bytes` to zero, so the caller keeps no copy.
    #[must_use]
    pub fn from_bytes(bytes: &mut [u8; KEY_BYTES]) -> Self {
        let key = Self(Box::new(*bytes));
        bytes.zeroize();
        key
    }

    /// A key of zero bytes, to be filled in place.
    fn zeroed() -> Self {
        Self(Box::new([0; KEY_BYTES]))
    }

    /// A second key with the same bytes (for a provider that keeps its keys).
    fn duplicate(&self) -> Self {
        Self(Box::new(*self.0))
    }

    /// The key bytes, for the cipher of this module only.
    fn bytes(&self) -> &[u8; KEY_BYTES] {
        &self.0
    }
}

impl Drop for DataKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl ZeroizeOnDrop for DataKey {}

impl fmt::Debug for DataKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DataKey(<redacted>)")
    }
}

/// The id of one entry version: 16 random bytes from the key port (ADR-004
/// section 1). The file name is its hex text; the file id is also part of
/// the associated data of the file and of the row.
///
/// [`fmt::Debug`] writes the first 8 hex characters only.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId([u8; FILE_ID_BYTES]);

impl FileId {
    /// A new file id from the random bytes of `provider`.
    ///
    /// # Errors
    ///
    /// [`StoreErrorKind::Unavailable`] when the provider gives no bytes.
    pub fn new<P: KeyProvider + ?Sized>(provider: &mut P) -> Result<Self, StoreError> {
        let mut bytes = [0; FILE_ID_BYTES];
        provider
            .random(&mut bytes)
            .map_err(|error| unavailable(error.op()))?;
        Ok(Self(bytes))
    }

    /// A file id from its 16 bytes (the `file_id` column).
    #[must_use]
    pub const fn from_bytes(bytes: [u8; FILE_ID_BYTES]) -> Self {
        Self(bytes)
    }

    /// The 16 bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; FILE_ID_BYTES] {
        &self.0
    }

    /// The file name stem: 32 lower-case hexadecimal characters.
    #[must_use]
    pub fn to_hex(&self) -> String {
        let mut text = String::with_capacity(FILE_ID_BYTES.saturating_mul(2));
        for byte in self.0 {
            push_hex(&mut text, byte);
        }
        text
    }

    /// The file id of a file name stem: exactly 32 lower-case hexadecimal
    /// characters. `None` for any other text (upper case too), so each id
    /// has one name.
    #[must_use]
    pub fn from_hex(text: &str) -> Option<Self> {
        let digits = text.as_bytes();
        if digits.len() != FILE_ID_BYTES.saturating_mul(2) {
            return None;
        }
        let mut bytes = [0; FILE_ID_BYTES];
        let (pairs, _) = digits.as_chunks::<2>();
        for (byte, [high, low]) in bytes.iter_mut().zip(pairs) {
            *byte = (hex_value(*high)? << 4) | hex_value(*low)?;
        }
        Some(Self(bytes))
    }
}

impl fmt::Debug for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut prefix = String::with_capacity(8);
        for byte in self.0.iter().take(4) {
            push_hex(&mut prefix, *byte);
        }
        write!(f, "FileId({prefix}...)")
    }
}

/// Appends the two lower-case hex digits of `byte`.
fn push_hex(text: &mut String, byte: u8) {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    for nibble in [byte >> 4, byte & 0x0f] {
        if let Some(digit) = DIGITS.get(usize::from(nibble)) {
            text.push(char::from(*digit));
        }
    }
}

/// The value of one lower-case hex digit.
fn hex_value(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => digit.checked_sub(b'0'),
        b'a'..=b'f' => digit.checked_sub(b'a' - 10),
        _ => None,
    }
}

/// An error of the key port in the operation `op`.
const fn unavailable(op: &'static str) -> StoreError {
    StoreError::new(StoreErrorKind::Unavailable, op)
}

/// A corrupted entry, found in the operation `op`. It names no reason.
const fn corrupted(op: &'static str) -> StoreError {
    StoreError::new(StoreErrorKind::Corrupted, op)
}
