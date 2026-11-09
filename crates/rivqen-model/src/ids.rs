// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Identifiers and revisions (data-model section 1).

use core::error::Error;
use core::fmt;

/// A constructor refused its input. It names the type and a static reason, and
/// never the rejected value (ERR-06).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValueError {
    /// The type that was built, for example `"Revision"`.
    pub kind: &'static str,
    /// Why the value was refused, for example `"too long"`.
    pub reason: &'static str,
}

impl ValueError {
    pub(crate) const fn new(kind: &'static str, reason: &'static str) -> Self {
        Self { kind, reason }
    }
}

impl fmt::Display for ValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid {}: {}", self.kind, self.reason)
    }
}

impl Error for ValueError {}

/// Writes the first `n` bytes of `bytes` as lowercase hex.
fn write_hex_prefix(f: &mut fmt::Formatter<'_>, bytes: &[u8], n: usize) -> fmt::Result {
    for byte in bytes.iter().take(n) {
        write!(f, "{byte:02x}")?;
    }
    Ok(())
}

/// One navigation session: 128 bits, opaque, never derived from the URL.
///
/// The core has no random source. The adapter gives 16 random bytes.
/// [`fmt::Debug`] writes the first 8 hex characters (4 bytes) only: enough to
/// follow one session in a log, not the whole id.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionId([u8; 16]);

impl SessionId {
    /// A session id from 16 bytes that the caller drew from a CSPRNG.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// The 16 bytes of the id.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl fmt::Debug for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionId(")?;
        write_hex_prefix(f, &self.0, 4)?;
        f.write_str(")")
    }
}

/// Version of the session state. Events with an old generation are ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Generation(u64);

impl Generation {
    /// The first generation of a session.
    pub const FIRST: Self = Self(0);

    /// A generation from its number.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The number of the generation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// The next generation, or `None` when the counter is exhausted.
    #[must_use]
    pub const fn next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(n) => Some(Self(n)),
            None => None,
        }
    }
}

/// One HTTP request or channel message, monotonic per engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RequestId(u64);

impl RequestId {
    /// The first request id of an engine.
    pub const FIRST: Self = Self(0);

    /// A request id from its number.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The number of the request id.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// The next request id, or `None` when the counter is exhausted.
    #[must_use]
    pub const fn next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(n) => Some(Self(n)),
            None => None,
        }
    }
}

/// Account, tenant or environment scope: a 256-bit keyed hash that the adapter
/// computes. Never a token.
///
/// [`fmt::Debug`] writes the first 8 hex characters (4 bytes) only.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PartitionId([u8; 32]);

impl PartitionId {
    /// A partition id from the 32 bytes of the keyed hash.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The 32 bytes of the id.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for PartitionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PartitionId(")?;
        write_hex_prefix(f, &self.0, 4)?;
        f.write_str(")")
    }
}

/// Version of a page or template, compared byte for byte.
///
/// DESIGN: the text is 1 to [`Revision::MAX_LEN`] bytes of visible ASCII
/// (0x21 to 0x7E: no space, no control byte). That holds the RQP forms
/// (`t1.…`, `r1.…`) and the legacy forms (lowercase hex of `etag` and
/// `template-tag`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Revision(String);

impl Revision {
    /// The longest revision text in bytes.
    pub const MAX_LEN: usize = 128;

    /// A revision from text.
    ///
    /// # Errors
    ///
    /// [`ValueError`] when the text is empty, longer than [`Self::MAX_LEN`]
    /// bytes, or holds a byte outside 0x21 to 0x7E.
    pub fn new(text: &str) -> Result<Self, ValueError> {
        if text.is_empty() {
            return Err(ValueError::new("Revision", "empty"));
        }
        if text.len() > Self::MAX_LEN {
            return Err(ValueError::new("Revision", "too long"));
        }
        if !text.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
            return Err(ValueError::new("Revision", "byte outside visible ASCII"));
        }
        Ok(Self(text.to_owned()))
    }

    /// The revision text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Revision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
