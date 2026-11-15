// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The cache key (cache identity section 1; DL-184). Owner: WP-07 S1 T-01.
//!
//! Contract (architect): the public items of this file. T-01 writes the body
//! of [`cache_key`] and the byte layout in the cache identity page.

use std::fmt;

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
    /// its query. The URL parser of WP-08 gives it.
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
#[must_use]
pub fn cache_key(parts: &KeyParts<'_>) -> CacheKey {
    // Stub (T-01).
    let _ = parts;
    CacheKey::from_bytes([0; 32])
}
