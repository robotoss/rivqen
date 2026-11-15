// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Limits of the template engine (DL-163; resource-limits.md section 1).
//!
//! The session builds one [`TemplateLimits`] from the checked engine
//! configuration with [`TemplateLimits::try_from`] (WP-08). Every way to
//! build a value caps each limit (WP-06 S1 security review, finding S1-4):
//!
//! - a limit that `rqp/1` fixes (`maxBlocks`, `maxBlockBytes`,
//!   `maxOpenElements`) at its protocol constant;
//! - the legacy name length at [`MAX_LEGACY_NAME_BYTES`] (legacy wire C-07);
//! - `maxDocumentBytes` at the top of its safe range (32 MiB,
//!   [`Limit::range`]).
//!
//! A configured value can only lower a limit. The fields are private, so no
//! code can make a value that is over a cap.

use core::fmt;

use rivqen_model::limits::{Limit, ResourceLimits};
use rivqen_proto::types::{MAX_BLOCK_BYTES, MAX_BLOCKS};

/// Maximum legacy block name length in bytes (legacy wire C-07).
pub const MAX_LEGACY_NAME_BYTES: usize = 128;
/// Maximum depth counter D of the RQP markup analysis (markup M-26).
pub const MAX_OPEN_ELEMENTS: usize = 256;
/// Maximum input of the RQP markup analysis in bytes (markup M-01).
pub const MAX_MARKUP_INPUT_BYTES: usize = 5 * 1024 * 1024;

/// The cap of `max_document_bytes`: the top of the safe range of
/// `maxDocumentBytes` (32 MiB). A literal, because a `const fn` cannot
/// convert the `u64` of [`Limit::range`] without a cast; the assertion below
/// keeps the two values equal at compile time.
const MAX_DOCUMENT_BYTES_CAP: usize = 32 * 1024 * 1024;

const _: () = assert!(Limit::MaxDocumentBytes.range().max() == 32 * 1024 * 1024);

/// The smaller of `value` and `cap` (`Ord::min` is not `const`).
const fn cap(value: usize, cap: usize) -> usize {
    if value < cap { value } else { cap }
}

/// The limits that the parsers of this crate apply.
///
/// Build it with [`TemplateLimits::default`], [`TemplateLimits::try_from`] or
/// [`TemplateLimits::new`]. Each of them caps every value (see the module
/// comment), so each getter gives at most its cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemplateLimits {
    document_bytes: usize,
    blocks: usize,
    block_bytes: usize,
    name_bytes: usize,
    open_elements: usize,
}

impl TemplateLimits {
    /// Limits with each value capped: `max_document_bytes` at 32 MiB (the
    /// top of the safe range of `maxDocumentBytes`), `max_blocks` at
    /// [`MAX_BLOCKS`], `max_block_bytes` at [`MAX_BLOCK_BYTES`],
    /// `max_name_bytes` at [`MAX_LEGACY_NAME_BYTES`] and `max_open_elements`
    /// at [`MAX_OPEN_ELEMENTS`]. A value over its cap gives the cap.
    #[must_use]
    pub const fn new(
        max_document_bytes: usize,
        max_blocks: usize,
        max_block_bytes: usize,
        max_name_bytes: usize,
        max_open_elements: usize,
    ) -> Self {
        Self {
            document_bytes: cap(max_document_bytes, MAX_DOCUMENT_BYTES_CAP),
            blocks: cap(max_blocks, MAX_BLOCKS),
            block_bytes: cap(max_block_bytes, MAX_BLOCK_BYTES),
            name_bytes: cap(max_name_bytes, MAX_LEGACY_NAME_BYTES),
            open_elements: cap(max_open_elements, MAX_OPEN_ELEMENTS),
        }
    }

    /// `maxDocumentBytes`: the whole document (legacy split, legacy rebuild
    /// output), at most 32 MiB. The RQP analysis and the scan use the
    /// smaller of this and [`MAX_MARKUP_INPUT_BYTES`].
    #[must_use]
    pub const fn max_document_bytes(&self) -> usize {
        self.document_bytes
    }

    /// `maxBlocks`, at most [`MAX_BLOCKS`].
    #[must_use]
    pub const fn max_blocks(&self) -> usize {
        self.blocks
    }

    /// `maxBlockBytes`, at most [`MAX_BLOCK_BYTES`].
    #[must_use]
    pub const fn max_block_bytes(&self) -> usize {
        self.block_bytes
    }

    /// `maxBlockKeyBytes` in legacy mode: a longer name stays in the
    /// template (C-07). At most [`MAX_LEGACY_NAME_BYTES`].
    #[must_use]
    pub const fn max_name_bytes(&self) -> usize {
        self.name_bytes
    }

    /// `maxOpenElements`, at most [`MAX_OPEN_ELEMENTS`].
    #[must_use]
    pub const fn max_open_elements(&self) -> usize {
        self.open_elements
    }
}

impl Default for TemplateLimits {
    /// The initial values of resource-limits.md section 1.
    fn default() -> Self {
        Self::new(
            MAX_MARKUP_INPUT_BYTES,
            MAX_BLOCKS,
            MAX_BLOCK_BYTES,
            MAX_LEGACY_NAME_BYTES,
            MAX_OPEN_ELEMENTS,
        )
    }
}

/// A limit value that does not fit `usize` on this target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LimitsError {
    limit: Limit,
}

impl LimitsError {
    /// The limit that does not fit.
    #[must_use]
    pub const fn limit(self) -> Limit {
        self.limit
    }
}

impl fmt::Display for LimitsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the limit {} does not fit this target",
            self.limit.name()
        )
    }
}

impl core::error::Error for LimitsError {}

fn to_usize(value: u64, limit: Limit) -> Result<usize, LimitsError> {
    usize::try_from(value).map_err(|_| LimitsError { limit })
}

impl TryFrom<&ResourceLimits> for TemplateLimits {
    type Error = LimitsError;

    /// The limits of `limits`, each capped as by [`TemplateLimits::new`]. A
    /// value that does not fit `usize` is an error, not a cap.
    fn try_from(limits: &ResourceLimits) -> Result<Self, Self::Error> {
        let document = to_usize(limits.max_document_bytes, Limit::MaxDocumentBytes)?;
        let blocks = to_usize(u64::from(limits.max_blocks), Limit::MaxBlocks)?;
        let block_bytes = to_usize(limits.max_block_bytes, Limit::MaxBlockBytes)?;
        let name = to_usize(
            u64::from(limits.max_block_key_bytes_legacy),
            Limit::MaxBlockKeyBytesLegacy,
        )?;
        let open = to_usize(u64::from(limits.max_open_elements), Limit::MaxOpenElements)?;
        Ok(Self::new(document, blocks, block_bytes, name, open))
    }
}

#[cfg(test)]
mod tests {
    use super::{Limit, LimitsError};

    // A `LimitsError` comes only from a target where `usize` is smaller than
    // `u64`, so the test makes one directly.
    #[test]
    fn limits_error_names_the_limit() {
        let error = LimitsError {
            limit: Limit::MaxDocumentBytes,
        };
        assert_eq!(error.limit(), Limit::MaxDocumentBytes);
        assert_eq!(
            error.to_string(),
            "the limit maxDocumentBytes does not fit this target"
        );
    }
}
