// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Build and check of the patched document (WP-06 S2 contract; H-142).
//!
//! Normative text: docs/engineering/protocol/manifest-patch.md section 2.4,
//! steps 8 and 9, and section 4.1.
//!
//! [`render_rqp`] builds the patched document: the stored original document
//! with the content bytes of each patched block replaced. [`check_patch`]
//! runs steps 8 and 9 after [`rivqen_proto::delta::verify_delta`] (steps 3
//! to 7) passed:
//!
//! 1. build the patched document with [`render_rqp`]; over
//!    `max_document_bytes`: `PATCH_TOO_LARGE`;
//! 2. [`split_rqp`](crate::rqp::split_rqp) of it: not valid,
//!    `PATCH_CONTENT_REJECTED`;
//! 3. template revision T and the blocks (`id`, `format`, order) of the
//!    snapshot: else `PATCH_CONTENT_REJECTED`;
//! 4. the page revision of the result with the `policy` and `limits` of the
//!    snapshot is R′ of the verified delta: else `INTEGRITY_ERROR`;
//! 5. step 9:
//!    [`check_patch_content`](crate::rqp::content::check_patch_content) on
//!    the patched `html` blocks: else `PATCH_CONTENT_REJECTED`.
//!
//! An error holds no document byte.

use core::fmt;

use rivqen_integrity::PageRevision;
use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_proto::delta::VerifiedDelta;
use rivqen_proto::types::SnapshotMeta;

use crate::limits::TemplateLimits;
use crate::rqp::content::ContentError;
use crate::rqp::{MarkupError, RqpDocument};

/// The new content of one block. Contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Replacement<'a> {
    /// The index of the block in the blocks of the analysis of the original
    /// document.
    pub block: usize,
    /// The new content bytes.
    pub content: &'a [u8],
}

/// Why [`render_rqp`] gives no document. Contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RenderRqpError {
    /// The patched document is larger than `max_document_bytes`
    /// (`PATCH_TOO_LARGE`). The size is computed before the output is
    /// allocated.
    TooLarge,
    /// A replacement names no block, the blocks are not in strictly
    /// increasing order, or a block range is not in the original document
    /// (`PATCH_INVALID`: the inputs do not belong together).
    BadReplacement,
}

impl RenderRqpError {
    /// A static reason without content.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::TooLarge => "the patched document is over maxDocumentBytes",
            Self::BadReplacement => "a replacement does not fit the blocks of the document",
        }
    }
}

impl fmt::Display for RenderRqpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.reason())
    }
}

impl core::error::Error for RenderRqpError {}

impl HasErrorCode for RenderRqpError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::TooLarge => ErrorCode::PatchTooLarge,
            Self::BadReplacement => ErrorCode::PatchInvalid,
        }
    }
}

/// The patched document: `original` with the content of each block in
/// `replacements` (strictly increasing block index) replaced. `document` is
/// the analysis of `original`. Contract.
///
/// # Errors
///
/// [`RenderRqpError`].
pub fn render_rqp(
    original: &[u8],
    document: &RqpDocument,
    replacements: &[Replacement<'_>],
    limits: &TemplateLimits,
) -> Result<Vec<u8>, RenderRqpError> {
    let _ = (original, document, replacements, limits);
    Err(RenderRqpError::BadReplacement)
}

/// A patched document that passed steps 8 and 9. Contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchedDocument {
    document: Vec<u8>,
    analysis: RqpDocument,
    page_revision: PageRevision,
}

impl PatchedDocument {
    /// The bytes of the patched document: the new snapshot.
    #[must_use]
    pub fn document(&self) -> &[u8] {
        &self.document
    }

    /// The analysis of the patched document: the block offsets of the new
    /// snapshot.
    #[must_use]
    pub const fn analysis(&self) -> &RqpDocument {
        &self.analysis
    }

    /// R′, the page revision of the new snapshot.
    #[must_use]
    pub const fn page_revision(&self) -> &PageRevision {
        &self.page_revision
    }

    /// The document, the analysis and R′, by value.
    #[must_use]
    pub fn into_parts(self) -> (Vec<u8>, RqpDocument, PageRevision) {
        (self.document, self.analysis, self.page_revision)
    }
}

/// Why a verified delta is not applied (section 2.4, steps 8 and 9).
/// Contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PatchError {
    /// `stored` is not the analysis of the snapshot of `meta` (another
    /// template revision or other block descriptors): `INTEGRITY_ERROR`
    /// (the inputs do not belong together; fail closed).
    Snapshot,
    /// Step 8: the patched document is over `max_document_bytes`:
    /// `PATCH_TOO_LARGE`.
    TooLarge,
    /// Step 8: the patched document is not valid RQP markup:
    /// `PATCH_CONTENT_REJECTED`.
    Markup(MarkupError),
    /// Step 8: another template revision: `PATCH_CONTENT_REJECTED`.
    TemplateChanged,
    /// Step 8: other blocks (`id`, `format` or order):
    /// `PATCH_CONTENT_REJECTED`.
    BlocksChanged,
    /// Step 8: the page revision of the patched document is not R′:
    /// `INTEGRITY_ERROR`.
    PageRevisionDiffers,
    /// Step 9: the patch content rule: `PATCH_CONTENT_REJECTED`.
    Content(ContentError),
}

impl PatchError {
    /// A static reason without content.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Snapshot => "the stored analysis is not the analysis of the snapshot",
            Self::TooLarge => "the patched document is over maxDocumentBytes",
            Self::Markup(_) => "the patched document is not valid RQP markup",
            Self::TemplateChanged => "the patched document has another template revision",
            Self::BlocksChanged => "the patched document has other blocks",
            Self::PageRevisionDiffers => "the page revision of the patched document differs",
            Self::Content(error) => error.violation.reason(),
        }
    }
}

impl fmt::Display for PatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.reason())
    }
}

impl core::error::Error for PatchError {}

impl HasErrorCode for PatchError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::Snapshot | Self::PageRevisionDiffers => ErrorCode::IntegrityError,
            Self::TooLarge => ErrorCode::PatchTooLarge,
            Self::Markup(_) | Self::TemplateChanged | Self::BlocksChanged | Self::Content(_) => {
                ErrorCode::PatchContentRejected
            }
        }
    }
}

/// Steps 8 and 9 of section 2.4 (see the module comment). `original` is the
/// stored original document of the snapshot, `stored` its analysis, `meta`
/// the checked snapshot values (T, R, L, `policy`, `limits`) and `delta` the
/// result of [`rivqen_proto::delta::verify_delta`] for this snapshot.
/// Contract.
///
/// # Errors
///
/// [`PatchError`] for the first failed check.
pub fn check_patch(
    original: &[u8],
    stored: &RqpDocument,
    meta: &SnapshotMeta,
    delta: &VerifiedDelta,
    limits: &TemplateLimits,
) -> Result<PatchedDocument, PatchError> {
    let _ = (original, stored, meta, delta, limits);
    Err(PatchError::Snapshot)
}
