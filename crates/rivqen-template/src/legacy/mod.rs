// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Canonical legacy split and rebuild (WP-06 S1 contract).
//!
//! Normative text: docs/engineering/protocol/legacy-wire.md section 10
//! (C-03, C-04, C-06, C-07, C-08, C-16), docs/engineering/protocol/legacy-markers.md
//! sections 4 and 5, docs/engineering/protocol/legacy-client.md CL-41.
//! Canonical profile only (DL-159); limits DL-163.
//!
//! The names, the types and the signatures in this file are the S1 contract:
//! a lane replaces the function bodies and adds private modules, and it does
//! not change a public item.

use core::fmt;

use rivqen_integrity::legacy::{Sha1Hex, sha1_hex};
use rivqen_model::error::{ErrorCode, HasErrorCode};

use crate::limits::TemplateLimits;

mod blocks;
mod render;
mod text;
mod title;

/// One data block of a split: the key in wire form (`{name}`, with braces;
/// `{title}` for the title rule) and the value (the whole match, markers
/// or tags included; C-06, C-07).
#[derive(Clone, PartialEq, Eq)]
pub struct DataBlock {
    /// The key with braces, for example `{price}`.
    pub key: String,
    /// The value text.
    pub value: String,
}

impl fmt::Debug for DataBlock {
    /// The key only: a value is page content.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DataBlock")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

/// A warning of a split that does not change the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Diagnostic {
    /// The end marker of the block at this index (in [`Split::data`]) has
    /// another name than its opening marker (legacy markers section 4).
    MismatchedEndName {
        /// Index of the block in [`Split::data`].
        index: usize,
    },
}

/// The result of an applied split (C-08): the template and the data in
/// order, `{title}` first, then the blocks in document order.
#[derive(Clone, PartialEq, Eq)]
pub struct Split {
    /// The template text: each extracted match replaced by its key.
    pub template: String,
    /// The data blocks, `{title}` first.
    pub data: Vec<DataBlock>,
    /// Warnings.
    pub diagnostics: Vec<Diagnostic>,
}

impl fmt::Debug for Split {
    /// Lengths and keys only: the template and the values are page content.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Split")
            .field("template_bytes", &self.template.len())
            .field("data", &self.data)
            .field("diagnostics", &self.diagnostics)
            .finish()
    }
}

/// Why a split is not applied: the page goes out (or is shown) unchanged,
/// with no legacy processing (C-03, C-07; fixture member `split.applied`
/// is `false`). Not an engine error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NotApplied {
    /// The body is not valid UTF-8 (C-03, DL-074).
    InvalidUtf8,
    /// A block name repeats, case-sensitive (C-07, DL-075; CL-41 token
    /// `duplicate-block-name`).
    DuplicateName,
    /// A block is named `title` (C-07, DL-076).
    BlockNamedTitle,
}

/// The outcome of [`split_v1`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitOutcome {
    /// The split applies.
    Applied(Split),
    /// The split does not apply.
    NotApplied(NotApplied),
}

/// A split or a rebuild over a limit (DL-163). Code `DOCUMENT_TOO_LARGE`:
/// the page loads normally, with no legacy processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LimitError {
    /// The input or the rebuilt page is over `maxDocumentBytes`.
    DocumentTooLarge,
    /// More than `maxBlocks` blocks would be extracted (the `{title}` entry
    /// does not count).
    TooManyBlocks,
    /// One block value is over `maxBlockBytes`.
    BlockTooLarge,
}

impl LimitError {
    /// A static reason, safe to log.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::DocumentTooLarge => "the legacy document is over maxDocumentBytes",
            Self::TooManyBlocks => "the legacy document has more than maxBlocks blocks",
            Self::BlockTooLarge => "a legacy block is over maxBlockBytes",
        }
    }
}

impl fmt::Display for LimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.reason())
    }
}

impl core::error::Error for LimitError {}

impl HasErrorCode for LimitError {
    fn code(&self) -> ErrorCode {
        ErrorCode::DocumentTooLarge
    }
}

/// Why a rebuild fails (legacy markers section 5). The caller discards the
/// result and loads the page normally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RenderError {
    /// Over a limit.
    Limit(LimitError),
    /// The data does not fit the template (a placeholder without a value,
    /// or a value without a placeholder; exact rule: T-01).
    Mismatch,
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Limit(limit) => fmt::Display::fmt(limit, f),
            Self::Mismatch => f.write_str("the legacy data does not fit the template"),
        }
    }
}

impl core::error::Error for RenderError {}

/// Split a legacy page with the canonical rules C-06 to C-08 in linear time
/// (C-16).
///
/// # Errors
///
/// [`LimitError`] when the page is over a limit of `limits`.
pub fn split_v1(html: &[u8], limits: &TemplateLimits) -> Result<SplitOutcome, LimitError> {
    if html.len() > limits.max_document_bytes() {
        return Err(LimitError::DocumentTooLarge);
    }
    let Ok(text) = core::str::from_utf8(html) else {
        return Ok(SplitOutcome::NotApplied(NotApplied::InvalidUtf8));
    };
    let title = title::find_title(html);
    let title_value = match title {
        Some((start, end)) => text.get(start..end),
        None => Some(""),
    };
    // A C-06 match starts at `<` and ends at `>`: always on character
    // boundaries. The `None` arms fail closed.
    let (Some(title_value), Some(joined)) = (title_value, text::Text::new(text, title)) else {
        return Ok(SplitOutcome::NotApplied(NotApplied::InvalidUtf8));
    };
    match blocks::split(&joined, title_value, limits) {
        Ok(split) => Ok(SplitOutcome::Applied(split)),
        Err(blocks::Stop::NotApplied(reason)) => Ok(SplitOutcome::NotApplied(reason)),
        Err(blocks::Stop::Limit(limit)) => Err(limit),
    }
}

/// Rebuild a page from a template and data (legacy markers section 5,
/// canonical rule).
///
/// The result is **not verified**. The placeholder of a key is its first
/// occurrence in the template (DL-166), and a page with its title inside a
/// block cannot be rebuilt (DL-167), so an `Ok` page can differ from the
/// page of the server. Before it uses the page, the caller MUST compare the
/// SHA-1 of the page ([`etag_v1`]) with the `html-sha1` member of the
/// response and with the response etag (legacy client CL-29). On a mismatch,
/// the caller discards the page and loads the page normally.
///
/// # Errors
///
/// [`RenderError`] when the data does not fit the template or the result
/// is over a limit.
pub fn render_v1(
    template: &str,
    data: &[DataBlock],
    limits: &TemplateLimits,
) -> Result<String, RenderError> {
    render::render(template, data, limits)
}

/// The legacy `etag`: lowercase hex SHA-1 of the body bytes (C-04).
#[must_use]
pub fn etag_v1(html: &[u8]) -> Sha1Hex {
    sha1_hex(html)
}

/// The legacy `template-tag`: lowercase hex SHA-1 of the UTF-8 bytes of
/// the template (C-08).
#[must_use]
pub fn template_tag_v1(template: &str) -> Sha1Hex {
    sha1_hex(template.as_bytes())
}
