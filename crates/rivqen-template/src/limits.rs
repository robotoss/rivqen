// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Limits of the template engine (DL-163; resource-limits.md section 1).
//!
//! The session builds one [`TemplateLimits`] from the checked engine
//! configuration with [`TemplateLimits::try_from`] (WP-08). A limit that
//! `rqp/1` fixes is capped at its protocol constant: a configured value can
//! only lower it.

use core::fmt;

use rivqen_model::limits::{Limit, ResourceLimits};
use rivqen_proto::types::{MAX_BLOCK_BYTES, MAX_BLOCKS};

/// Maximum legacy block name length in bytes (legacy wire C-07).
pub const MAX_LEGACY_NAME_BYTES: usize = 128;
/// Maximum depth counter D of the RQP markup analysis (markup M-26).
pub const MAX_OPEN_ELEMENTS: usize = 256;
/// Maximum input of the RQP markup analysis in bytes (markup M-01).
pub const MAX_MARKUP_INPUT_BYTES: usize = 5 * 1024 * 1024;

/// The limits that the parsers of this crate apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemplateLimits {
    /// `maxDocumentBytes`: the whole document (legacy split, legacy rebuild
    /// output). The RQP analysis uses the smaller of this and
    /// [`MAX_MARKUP_INPUT_BYTES`].
    pub max_document_bytes: usize,
    /// `maxBlocks`, at most [`MAX_BLOCKS`].
    pub max_blocks: usize,
    /// `maxBlockBytes`, at most [`MAX_BLOCK_BYTES`].
    pub max_block_bytes: usize,
    /// `maxBlockKeyBytes` in legacy mode: a longer name stays in the template
    /// (C-07), at most [`MAX_LEGACY_NAME_BYTES`].
    pub max_name_bytes: usize,
    /// `maxOpenElements`, at most [`MAX_OPEN_ELEMENTS`].
    pub max_open_elements: usize,
}

impl Default for TemplateLimits {
    /// The initial values of resource-limits.md section 1.
    fn default() -> Self {
        Self {
            max_document_bytes: MAX_MARKUP_INPUT_BYTES,
            max_blocks: MAX_BLOCKS,
            max_block_bytes: MAX_BLOCK_BYTES,
            max_name_bytes: MAX_LEGACY_NAME_BYTES,
            max_open_elements: MAX_OPEN_ELEMENTS,
        }
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

    fn try_from(limits: &ResourceLimits) -> Result<Self, Self::Error> {
        let document = to_usize(limits.max_document_bytes, Limit::MaxDocumentBytes)?;
        let blocks = to_usize(u64::from(limits.max_blocks), Limit::MaxBlocks)?;
        let block_bytes = to_usize(limits.max_block_bytes, Limit::MaxBlockBytes)?;
        let name = to_usize(
            u64::from(limits.max_block_key_bytes_legacy),
            Limit::MaxBlockKeyBytesLegacy,
        )?;
        let open = to_usize(u64::from(limits.max_open_elements), Limit::MaxOpenElements)?;
        Ok(Self {
            max_document_bytes: document,
            max_blocks: blocks.min(MAX_BLOCKS),
            max_block_bytes: block_bytes.min(MAX_BLOCK_BYTES),
            max_name_bytes: name.min(MAX_LEGACY_NAME_BYTES),
            max_open_elements: open.min(MAX_OPEN_ELEMENTS),
        })
    }
}
