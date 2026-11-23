// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The cache key (cache identity section 1; DL-184). Owner: WP-07 S1 T-01.
//!
//! Contract (architect): the public items of this file. T-01 writes the body
//! of [`cache_key`] and the byte layout in the cache identity page.

use std::fmt;

use rivqen_integrity::sha256_parts;
use rivqen_model::ids::{CacheKey, PartitionId};

/// The partition that holds an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Partition {
    /// The partition of one account, tenant and environment.
    Scoped(PartitionId),
    /// The shared `public` partition (cache identity rule 3; off by default,
    /// H-81).
    Public,
}

/// The protocol variant of an entry (cache identity section 1, `variant`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Protocol {
    /// Legacy mode.
    Legacy,
    /// RQP.
    Rqp,
}

/// The keyed digest of one request value that a response `Vary` names
/// (DL-127, CL-12). The value itself is never stored.
///
/// [`fmt::Debug`] writes no byte of it.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct VaryDigest([u8; 32]);

impl VaryDigest {
    /// A digest from its 32 bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The 32 bytes of the digest.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for VaryDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("VaryDigest(..)")
    }
}

/// One field that the response `Vary` names, with the digest of the request
/// value; `None` when the request had no such field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaryEntry {
    /// Field name in ASCII lower case.
    pub name: String,
    /// Digest of the request value.
    pub value: Option<VaryDigest>,
}

/// The inputs of the cache key.
///
/// [`fmt::Debug`] writes no part of the URL: the query can hold a secret.
#[derive(Clone, Copy)]
pub struct KeyParts<'a> {
    /// The partition of the entry.
    pub partition: Partition,
    /// The serialized origin (cache identity section 2, steps 1 and 2).
    pub origin: &'a str,
    /// The canonical URL of cache identity section 2 (steps 1 to 6), with
    /// its query. `rivqen_url::ParsedUrl::cache_url` gives it.
    pub canonical_url: &'a str,
    /// The `Vary` entries, sorted by name; no name twice.
    pub vary: &'a [VaryEntry],
    /// The protocol variant.
    pub protocol: Protocol,
    /// Language or region that the app declares as part of the variant.
    pub locale: Option<&'a str>,
}

impl fmt::Debug for KeyParts<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyParts")
            .field("partition", &self.partition)
            .field("protocol", &self.protocol)
            .field("vary", &self.vary.len())
            .finish_non_exhaustive()
    }
}

/// The cache key of `parts`: SHA-256 over the parts in the order of the
/// formula of cache identity section 1, each with a length prefix. The
/// method is always `GET`.
///
/// The exact bytes are in cache identity section 1 ("Byte layout"). The
/// function sorts the `Vary` entries itself (by name, then absent before
/// present, then by digest bytes), so the order of `parts.vary` does not
/// change the key; it does not change the case of a name.
#[must_use]
pub fn cache_key(parts: &KeyParts<'_>) -> CacheKey {
    let mut vary: Vec<&VaryEntry> = parts.vary.iter().collect();
    vary.sort_unstable_by(|a, b| {
        a.name
            .as_bytes()
            .cmp(b.name.as_bytes())
            .then_with(|| digest_bytes(a).cmp(&digest_bytes(b)))
    });
    let count = len_u64(vary.len()).to_be_bytes();

    let mut fields: Vec<Field<'_>> = vec![
        Field::plain(DOMAIN_TAG),
        match &parts.partition {
            Partition::Scoped(id) => Field::new(TAG_SCOPED, id.as_bytes()),
            Partition::Public => Field::new(TAG_PUBLIC, EMPTY),
        },
        Field::plain(parts.origin.as_bytes()),
        Field::plain(parts.canonical_url.as_bytes()),
        Field::plain(METHOD),
        Field::plain(&count),
    ];
    for entry in &vary {
        fields.push(Field::plain(entry.name.as_bytes()));
        fields.push(match &entry.value {
            Some(digest) => Field::new(PRESENT, digest.as_bytes()),
            None => Field::new(ABSENT, EMPTY),
        });
    }
    fields.push(Field::plain(match parts.protocol {
        Protocol::Legacy => PROTOCOL_LEGACY,
        Protocol::Rqp => PROTOCOL_RQP,
    }));
    fields.push(match parts.locale {
        Some(locale) => Field::new(PRESENT, locale.as_bytes()),
        None => Field::new(ABSENT, EMPTY),
    });

    let prefixes: Vec<[u8; 8]> = fields.iter().map(Field::len_prefix).collect();
    let mut slices: Vec<&[u8]> = Vec::new();
    for (prefix, field) in prefixes.iter().zip(&fields) {
        slices.push(prefix);
        slices.push(field.head);
        slices.push(field.body);
    }
    CacheKey::from_bytes(*sha256_parts(&slices).as_bytes())
}

/// The domain tag with the version of the layout. A new layout gets a new
/// tag, so a key of one layout never equals a key of another.
const DOMAIN_TAG: &[u8] = b"rivqen-cache-key-v1";
/// The method of every cached request (cache identity section 1).
const METHOD: &[u8] = b"GET";
/// Head of a `Partition::Scoped` field; the 32 bytes of the id follow.
const TAG_SCOPED: &[u8] = &[0x01];
/// The whole `Partition::Public` field: one byte, so no id can equal it.
const TAG_PUBLIC: &[u8] = &[0x02];
/// Head of an optional value that is absent; nothing follows.
const ABSENT: &[u8] = &[0x00];
/// Head of an optional value that is present; the value follows.
const PRESENT: &[u8] = &[0x01];
/// No bytes.
const EMPTY: &[u8] = &[];
/// The protocol field of [`Protocol::Legacy`].
const PROTOCOL_LEGACY: &[u8] = b"legacy";
/// The protocol field of [`Protocol::Rqp`].
const PROTOCOL_RQP: &[u8] = b"rqp";

/// One field of the hash input: `head ‖ body`, written after its length as
/// 8 bytes big-endian.
#[derive(Clone, Copy)]
struct Field<'a> {
    head: &'a [u8],
    body: &'a [u8],
}

impl<'a> Field<'a> {
    const fn new(head: &'a [u8], body: &'a [u8]) -> Self {
        Self { head, body }
    }

    const fn plain(body: &'a [u8]) -> Self {
        Self { head: EMPTY, body }
    }

    /// The length of `head ‖ body` as 8 bytes big-endian.
    fn len_prefix(&self) -> [u8; 8] {
        // A slice holds at most `isize::MAX` bytes and a head at most one,
        // so the sum always fits; `usize::MAX` is never reached.
        let len = self.head.len().saturating_add(self.body.len());
        len_u64(len).to_be_bytes()
    }
}

/// `n` as `u64`. Every Rust target has a `usize` of at most 64 bits, so the
/// fallback is never used.
fn len_u64(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

/// The digest bytes of a `Vary` entry, for the sort order.
fn digest_bytes(entry: &VaryEntry) -> Option<&[u8; 32]> {
    entry.value.as_ref().map(VaryDigest::as_bytes)
}
