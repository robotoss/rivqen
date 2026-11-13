// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! RQP markup analysis (`split_rqp`; WP-06 S1 contract).
//!
//! Normative text: docs/engineering/protocol/markup.md (M-01 to M-27) and
//! docs/engineering/protocol/manifest-patch.md section 1.3 (template and
//! revisions). A port of the candidate `tools/rqp-markup/rust` (DL-161).
//!
//! The names, the types and the signatures marked "contract" are the S1
//! contract: a lane replaces the function bodies and adds private modules
//! and the module [`scan`], and it does not change a contract item.
//!
//! # Layout (port of the candidate, DL-161)
//!
//! | Module | Candidate file | Content |
//! |---|---|---|
//! | `tokenizer` (private) | `src/tokenizer.rs` | WHATWG tokenizer subset with byte spans (M-03, M-04) |
//! | `names` (private) | `src/names.rs` | tag name interning and the name sets of section 2.1 |
//! | `rules` (private) | `src/rules.rs` | one pass: token stack M-05, rules M-06 to M-26 |
//! | `json` (private) | `src/json.rs` | JSON text check of M-25 (depth at most 64) |
//! | `analysis` (private) | `src/lib.rs` | M-01, M-02, result M-27, template revision |
//! | [`scan`] (public) | `src/scan.rs` | tokens, gaps and raw attributes for WP-06 S2 |
//!
//! WP-06 S2 contract (H-142), new code from the normative text:
//!
//! | Module | Content |
//! |---|---|
//! | [`locate`] | manifest locator and head walk (manifest-patch section 1.7) |
//! | [`full`] | client checks of a full document (section 1.8) |
//! | [`patch`] | build and check of the patched document (section 2.4, steps 8 and 9) |
//! | [`content`] | patch content rule and attribute rule (section 4, section 4.2) |
//!
//! Limits (DL-163): M-01 is the smaller of `max_document_bytes` and
//! [`MAX_MARKUP_INPUT_BYTES`](crate::limits::MAX_MARKUP_INPUT_BYTES); the
//! M-26 limits are `max_blocks`, `max_block_bytes` and `max_open_elements`
//! of [`TemplateLimits`], each capped at its `rqp/1` constant. A lower value
//! gives `RQP_MARKUP_LIMIT` earlier; a higher value has no effect.
//! [`scan::scan`] takes the same limits: the M-01 input limit and a token
//! stack of at most `max_open_elements() + 2` elements.

use core::fmt;

use rivqen_integrity::{PageRevision, TemplateRevision};
use rivqen_proto::compute::page_revision;
use rivqen_proto::types::{BlockDescriptor, Limits, Policy};

use crate::limits::TemplateLimits;

mod analysis;
pub mod content;
pub mod full;
mod json;
pub mod locate;
mod names;
pub mod patch;
mod rules;
pub mod scan;
mod tokenizer;

/// Error code of the markup analysis (markup section 2.10). Contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MarkupCode {
    /// `RQP_MARKUP_ENCODING`
    Encoding,
    /// `RQP_MARKUP_LIMIT`
    Limit,
    /// `RQP_MARKUP_INVALID_ID`
    InvalidId,
    /// `RQP_MARKUP_FORBIDDEN_ELEMENT`
    ForbiddenElement,
    /// `RQP_MARKUP_DUPLICATE`
    Duplicate,
    /// `RQP_MARKUP_NESTED`
    Nested,
    /// `RQP_MARKUP_STRUCTURE`
    Structure,
    /// `RQP_MARKUP_BAD_JSON`
    BadJson,
    /// `RQP_MARKUP_RESERVED`
    Reserved,
}

impl MarkupCode {
    /// The wire name of the code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Encoding => "RQP_MARKUP_ENCODING",
            Self::Limit => "RQP_MARKUP_LIMIT",
            Self::InvalidId => "RQP_MARKUP_INVALID_ID",
            Self::ForbiddenElement => "RQP_MARKUP_FORBIDDEN_ELEMENT",
            Self::Duplicate => "RQP_MARKUP_DUPLICATE",
            Self::Nested => "RQP_MARKUP_NESTED",
            Self::Structure => "RQP_MARKUP_STRUCTURE",
            Self::BadJson => "RQP_MARKUP_BAD_JSON",
            Self::Reserved => "RQP_MARKUP_RESERVED",
        }
    }
}

/// The document is not valid RQP markup. The error holds the code and the
/// rule id only, never document bytes. The caller maps it to its catalog
/// code (`MANIFEST_INVALID` for a full document, `PATCH_CONTENT_REJECTED`
/// after a patch; H-142, WP-06 S2). Contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkupError {
    code: MarkupCode,
    rule: &'static str,
}

impl MarkupError {
    /// An error with a code and the rule id (for example `"M-23"`).
    #[must_use]
    pub const fn new(code: MarkupCode, rule: &'static str) -> Self {
        Self { code, rule }
    }

    /// The code.
    #[must_use]
    pub const fn code(self) -> MarkupCode {
        self.code
    }

    /// The rule id.
    #[must_use]
    pub const fn rule(self) -> &'static str {
        self.rule
    }
}

impl fmt::Display for MarkupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.code.as_str(), self.rule)
    }
}

impl core::error::Error for MarkupError {}

/// One block of a valid document (markup M-27). Contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RqpBlock {
    /// Id, format and the SHA-256 of the exact content bytes.
    pub descriptor: BlockDescriptor,
    /// Byte offset of the first content byte: the byte after the `>` of the
    /// start tag.
    pub start: usize,
    /// Byte offset of the `<` of the end tag. The content is
    /// `input[start..end]`.
    pub end: usize,
}

/// The result of a valid analysis. Contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RqpDocument {
    /// `t1.` of the template (manifest and patch section 1.3).
    pub template_revision: TemplateRevision,
    /// The blocks in document order.
    pub blocks: Vec<RqpBlock>,
}

impl RqpDocument {
    /// The page revision with the policy line of the manifest members
    /// `policy` and `limits` (section 1.3, H-55); `None` for both gives the
    /// revision of the markup fixtures.
    #[must_use]
    pub fn page_revision(&self, policy: Option<&Policy>, limits: Option<&Limits>) -> PageRevision {
        let blocks: Vec<BlockDescriptor> = self
            .blocks
            .iter()
            .map(|block| block.descriptor.clone())
            .collect();
        page_revision(&self.template_revision, &blocks, policy, limits)
    }
}

/// Analyze a document with the RQP markup rules. The input limit is the
/// smaller of `limits.max_document_bytes` and the M-01 limit. Contract.
///
/// # Errors
///
/// [`MarkupError`] when the document is not valid RQP markup.
pub fn split_rqp(input: &[u8], limits: &TemplateLimits) -> Result<RqpDocument, MarkupError> {
    analysis::analyze(input, limits)
}
