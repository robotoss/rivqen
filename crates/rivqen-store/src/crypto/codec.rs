// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The primitives of the two plain-text codecs, and their limits.
//!
//! - An integer is big-endian: `u8`, `u32` (4 bytes) or `u64` (8 bytes).
//! - A boolean is one byte: `00` or `01`.
//! - A byte string is its length as `u64` and the bytes; a text is a byte
//!   string of UTF-8.
//! - An optional value is `00`, or `01` and the value.
//! - A list is its count as `u64` and the items.
//!
//! The reader never panics. It checks each length and each count against
//! the remaining input before it copies, and each size against a limit. Any
//! other byte than the ones above is an error, so each value has exactly
//! one encoding.

use rivqen_model::limits::ResourceLimits;

/// The limits of the decoders (resource-limits section 1). A value over a
/// limit is corrupted: the encoder of this crate never wrote it with these
/// limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeLimits {
    /// The page, and the template: each at most this many bytes
    /// (`maxDocumentBytes`).
    pub max_document_bytes: u64,
    /// Data blocks of a file, and blocks of an RQP snapshot (`maxBlocks`).
    pub max_blocks: u64,
    /// The key of one data block (`maxBlockKeyBytes`, the larger of the
    /// legacy and the RQP value).
    pub max_block_key_bytes: u64,
    /// One data block (`maxBlockBytes`).
    pub max_block_bytes: u64,
    /// One page field (name and value), one validator, one `Vary` name
    /// (`maxHeaderBytes`).
    pub max_header_bytes: u64,
    /// All page fields together, and all `Vary` names together
    /// (`maxHeadersTotalBytes`).
    pub max_headers_total_bytes: u64,
}

impl DecodeLimits {
    /// The limits of a core configuration.
    #[must_use]
    pub fn from_resource_limits(limits: &ResourceLimits) -> Self {
        Self {
            max_document_bytes: limits.max_document_bytes,
            max_blocks: u64::from(limits.max_blocks),
            max_block_key_bytes: u64::from(
                limits
                    .max_block_key_bytes_legacy
                    .max(limits.max_block_key_bytes_rqp),
            ),
            max_block_bytes: limits.max_block_bytes,
            max_header_bytes: limits.max_header_bytes,
            max_headers_total_bytes: limits.max_headers_total_bytes,
        }
    }
}

impl Default for DecodeLimits {
    /// The limits of [`ResourceLimits::default`].
    fn default() -> Self {
        Self::from_resource_limits(&ResourceLimits::default())
    }
}

/// A decode failure. The public functions turn it into a `Corrupted` store
/// error with no reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Bad;

/// The length of a length or count field.
pub(super) const LEN_BYTES: usize = 8;

/// Reads the primitives from untrusted bytes.
pub(super) struct Reader<'a> {
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    pub(super) const fn new(input: &'a [u8]) -> Self {
        Self { rest: input }
    }

    /// The next `N` bytes.
    pub(super) fn array<const N: usize>(&mut self) -> Result<[u8; N], Bad> {
        let (head, rest) = self.rest.split_first_chunk::<N>().ok_or(Bad)?;
        self.rest = rest;
        Ok(*head)
    }

    pub(super) fn u8(&mut self) -> Result<u8, Bad> {
        let [byte] = self.array::<1>()?;
        Ok(byte)
    }

    pub(super) fn u32(&mut self) -> Result<u32, Bad> {
        self.array::<4>().map(u32::from_be_bytes)
    }

    pub(super) fn u64(&mut self) -> Result<u64, Bad> {
        self.array::<8>().map(u64::from_be_bytes)
    }

    /// A boolean: `00` or `01`.
    pub(super) fn bool(&mut self) -> Result<bool, Bad> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Bad),
        }
    }

    /// An optional value: `00`, or `01` and the value that `read` reads.
    pub(super) fn option<T>(
        &mut self,
        read: impl FnOnce(&mut Self) -> Result<T, Bad>,
    ) -> Result<Option<T>, Bad> {
        if self.bool()? {
            read(self).map(Some)
        } else {
            Ok(None)
        }
    }

    /// A byte string of at most `limit` bytes.
    pub(super) fn bytes(&mut self, limit: u64) -> Result<&'a [u8], Bad> {
        let len = self.u64()?;
        if len > limit {
            return Err(Bad);
        }
        let len = usize::try_from(len).map_err(|_| Bad)?;
        let (bytes, rest) = self.rest.split_at_checked(len).ok_or(Bad)?;
        self.rest = rest;
        Ok(bytes)
    }

    /// A UTF-8 text of at most `limit` bytes.
    pub(super) fn text(&mut self, limit: u64) -> Result<String, Bad> {
        let bytes = self.bytes(limit)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| Bad)
    }

    /// A count of at most `limit` items, where each item takes at least
    /// `min_item_bytes` of the remaining input. So a count never makes the
    /// caller loop or allocate more than the input allows.
    pub(super) fn count(&mut self, limit: u64, min_item_bytes: usize) -> Result<usize, Bad> {
        let count = self.u64()?;
        if count > limit {
            return Err(Bad);
        }
        let count = usize::try_from(count).map_err(|_| Bad)?;
        let need = count.checked_mul(min_item_bytes).ok_or(Bad)?;
        if need > self.rest.len() {
            return Err(Bad);
        }
        Ok(count)
    }

    /// The end of the input: no trailing byte.
    pub(super) fn finish(self) -> Result<(), Bad> {
        if self.rest.is_empty() {
            Ok(())
        } else {
            Err(Bad)
        }
    }
}

/// A budget of bytes for several values together (for example all page
/// fields).
pub(super) struct Budget(u64);

impl Budget {
    pub(super) const fn new(limit: u64) -> Self {
        Self(limit)
    }

    /// Takes `len` bytes from the budget.
    pub(super) fn take(&mut self, len: usize) -> Result<(), Bad> {
        let len = u64::try_from(len).map_err(|_| Bad)?;
        self.0 = self.0.checked_sub(len).ok_or(Bad)?;
        Ok(())
    }
}

/// Writes the primitives.
pub(super) struct Writer {
    out: Vec<u8>,
}

impl Writer {
    pub(super) fn with_capacity(capacity: usize) -> Self {
        Self {
            out: Vec::with_capacity(capacity),
        }
    }

    pub(super) fn into_bytes(self) -> Vec<u8> {
        self.out
    }

    pub(super) fn raw(&mut self, bytes: &[u8]) {
        self.out.extend_from_slice(bytes);
    }

    pub(super) fn u8(&mut self, value: u8) {
        self.out.push(value);
    }

    pub(super) fn u32(&mut self, value: u32) {
        self.raw(&value.to_be_bytes());
    }

    pub(super) fn u64(&mut self, value: u64) {
        self.raw(&value.to_be_bytes());
    }

    pub(super) fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }

    /// A length or a count. A `usize` has at most 64 bits on every Rust
    /// target, so the fallback is never used.
    pub(super) fn len(&mut self, len: usize) {
        self.u64(u64::try_from(len).unwrap_or(u64::MAX));
    }

    pub(super) fn bytes(&mut self, bytes: &[u8]) {
        self.len(bytes.len());
        self.raw(bytes);
    }

    pub(super) fn text(&mut self, text: &str) {
        self.bytes(text.as_bytes());
    }

    pub(super) fn option<T>(&mut self, value: Option<T>, write: impl FnOnce(&mut Self, T)) {
        match value {
            Some(value) => {
                self.bool(true);
                write(self, value);
            }
            None => self.bool(false),
        }
    }
}
