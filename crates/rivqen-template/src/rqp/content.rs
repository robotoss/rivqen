// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Patch content rule and attribute rule (WP-06 S2 contract; H-142, R-1).
//!
//! Normative text: docs/engineering/protocol/manifest-patch.md section 4,
//! item 1 (H-48, H-73, H-78, H-96), section 4.2 (H-59, H-77, H-83, H-83a,
//! DL-036, DL-048, DL-051, DL-052) and section 2.4, step 9.
//!
//! This is the XSS boundary of a patch (RSK-07): a gap lets a
//! server-controlled block run script. The rule fails closed.
//!
//! [`check_patch_content`] reads the tokens of the content of each patched
//! `html` block of the patched document with the [`scan`](crate::rqp::scan)
//! (the tokenizer of the analysis: M-04 state changes in HTML context, none
//! in a foreign region). For each start tag whose `<` is in the content
//! range of a patched `html` block it checks:
//!
//! 1. a start tag in HTML context whose name is in [`PATCH_FORBIDDEN`];
//! 2. a start tag in a foreign region whose name is not in
//!    [`FOREIGN_ALLOWED`];
//! 3. each attribute, also a duplicate that the tokenizer drops (M-03), with
//!    [`check_attribute`] on the raw value.
//!
//! The self-closing flag does not change the result; end tags do not
//! matter; names are compared in lower case, as the tokenizer gives them.
//! `json` blocks and the blocks without an operation are not read.
//!
//! Every violation is `PATCH_CONTENT_REJECTED`. A violation holds the block
//! index and the kind only, never content bytes, names or values.

use core::fmt;

use rivqen_model::error::{ErrorCode, HasErrorCode};

use crate::limits::TemplateLimits;
use rivqen_proto::types::BlockFormat;

use crate::rqp::{MarkupCode, MarkupError, RqpDocument};

/// Start tag names that a patch cannot bring in HTML context (section 4,
/// item 1.1; H-78, H-96). Contract.
pub const PATCH_FORBIDDEN: [&str; 11] = [
    "base", "basefont", "bgsound", "embed", "iframe", "link", "meta", "noscript", "object",
    "script", "style",
];

/// Start tag names that a patch can bring in a foreign region (section 4,
/// `FOREIGN_ALLOWED`; H-73). Every other name rejects. Contract.
pub const FOREIGN_ALLOWED: [&str; 70] = [
    "svg",
    "g",
    "path",
    "circle",
    "ellipse",
    "line",
    "polyline",
    "polygon",
    "rect",
    "defs",
    "symbol",
    "use",
    "title",
    "desc",
    "lineargradient",
    "radialgradient",
    "stop",
    "clippath",
    "mask",
    "pattern",
    "marker",
    "text",
    "tspan",
    "filter",
    "feblend",
    "fecolormatrix",
    "fecomponenttransfer",
    "fecomposite",
    "fedisplacementmap",
    "fedropshadow",
    "feflood",
    "fefunca",
    "fefuncb",
    "fefuncg",
    "fefuncr",
    "fegaussianblur",
    "femerge",
    "femergenode",
    "femorphology",
    "feoffset",
    "feturbulence",
    "math",
    "mrow",
    "mi",
    "mo",
    "mn",
    "ms",
    "mtext",
    "mspace",
    "msub",
    "msup",
    "msubsup",
    "mfrac",
    "msqrt",
    "mroot",
    "mstyle",
    "munder",
    "mover",
    "munderover",
    "mmultiscripts",
    "mprescripts",
    "none",
    "mtable",
    "mtr",
    "mtd",
    "mpadded",
    "mphantom",
    "merror",
    "semantics",
    "annotation",
];

/// The kind of a violation of the patch content rule. Contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ContentViolation {
    /// Item 1.1: a start tag in HTML context with a name in
    /// [`PATCH_FORBIDDEN`].
    ForbiddenElement,
    /// Item 1.2: a start tag in a foreign region with a name that is not in
    /// [`FOREIGN_ALLOWED`].
    ForeignElement,
    /// Section 4.2, item 1: an attribute name that starts with `on`.
    EventHandler,
    /// Section 4.2, item 2: a character reference at the scheme decision
    /// byte that can continue or complete a scheme (H-77).
    CharacterReference,
    /// Section 4.2, item 4.1 or 4.2: a `javascript:`, `vbscript:` or
    /// `data:` value (H-59, H-83).
    Scheme,
    /// Section 4.2, item 4.3: a `srcset` with a character reference that
    /// decodes to white space, `,`, `(` or `)`, or with a candidate URL that
    /// items 2 to 4.2 reject (H-83).
    Srcset,
    /// The scan of the patched document stopped (M-01 or the token stack
    /// limit) before the end of a patched block: the rule cannot read the
    /// content, so it rejects (fail closed).
    Markup(MarkupError),
}

impl ContentViolation {
    /// A static reason without content.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::ForbiddenElement => "the content has an element that a patch cannot bring",
            Self::ForeignElement => "the content has a foreign element outside the allowed set",
            Self::EventHandler => "the content has an event-handler attribute",
            Self::CharacterReference => {
                "the content has an attribute with a character reference at the scheme"
            }
            Self::Scheme => "the content has an attribute with a forbidden URL scheme",
            Self::Srcset => "the content has a srcset that the rule rejects",
            Self::Markup(_) => "the scan of the patched document stopped",
        }
    }
}

impl fmt::Display for ContentViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.reason())
    }
}

impl core::error::Error for ContentViolation {}

impl HasErrorCode for ContentViolation {
    fn code(&self) -> ErrorCode {
        ErrorCode::PatchContentRejected
    }
}

/// A violation in one patched block. Contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentError {
    /// The index of the block in the blocks of the analysis.
    pub block: usize,
    /// The first violation in the block, in document order.
    pub violation: ContentViolation,
}

impl fmt::Display for ContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "block {}: {}", self.block, self.violation)
    }
}

impl core::error::Error for ContentError {}

impl HasErrorCode for ContentError {
    fn code(&self) -> ErrorCode {
        ErrorCode::PatchContentRejected
    }
}

/// The patch content rule (section 2.4, step 9) on the patched document
/// `document`. `analysis` is the markup analysis of `document` (section 2.4,
/// step 8) and `patched` the indices of the blocks with an operation, in
/// any order. An index that names no block, or a block whose content range
/// is not in `document`, rejects with [`ContentViolation::Markup`]. The
/// first violation in document order wins. Contract.
///
/// # Errors
///
/// [`ContentError`] for the first violation.
pub fn check_patch_content(
    document: &[u8],
    analysis: &RqpDocument,
    patched: &[usize],
    limits: &TemplateLimits,
) -> Result<(), ContentError> {
    // Contract stub (fail closed): exact for `json` blocks, which the rule
    // does not read; every patched `html` block rejects until T-02.
    let _ = (document, limits);
    for &block in patched {
        let Some(entry) = analysis.blocks.get(block) else {
            return Err(ContentError {
                block,
                violation: ContentViolation::Markup(MarkupError::new(
                    MarkupCode::Structure,
                    "stub",
                )),
            });
        };
        if entry.descriptor.format == BlockFormat::Html {
            return Err(ContentError {
                block,
                violation: ContentViolation::ForbiddenElement,
            });
        }
    }
    Ok(())
}

/// The attribute rule of section 4.2 for one attribute of one start tag.
/// `element` is the tag name in lower case, `foreign` is `true` for a start
/// tag in a foreign region, `name` is the attribute name in lower case and
/// `raw_value` the raw value (no character reference decoding; empty for an
/// attribute without a value). Contract.
///
/// # Errors
///
/// The [`ContentViolation`] of the first item that rejects.
pub fn check_attribute(
    element: &[u8],
    foreign: bool,
    name: &[u8],
    raw_value: &[u8],
) -> Result<(), ContentViolation> {
    let _ = (element, foreign, name, raw_value);
    Err(ContentViolation::EventHandler)
}
