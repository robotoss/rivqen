// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The order of the RQP delta checks and the validated patch (diff engine
//! sections 3 and 4; manifest and patch section 2.4).
//!
//! Owner: WP-08 S1, T-03.
//!
//! [`validate`] runs checks 4 to 13 of diff engine section 3 on one delta
//! body against one stored snapshot, in this order, and stops at the first
//! failure:
//!
//! | Checks | Function | Codes |
//! |---|---|---|
//! | 4, 5, 6, 9, 10, 11 | [`rivqen_proto::delta::verify_delta`] | `PATCH_TOO_LARGE`, `PATCH_INVALID`, `INTEGRITY_ERROR` |
//! | 12, 13 | [`rivqen_template::rqp::patch::check_patch`] | `PATCH_TOO_LARGE`, `PATCH_CONTENT_REJECTED`, `INTEGRITY_ERROR` |
//!
//! Checks 1 to 3 (the navigation, the offer, the partition) are session
//! checks and come before [`validate`].
//!
//! # Invariants
//!
//! - R-CORE-05, INV-13: a [`ValidatedPatch`] exists only after all checks
//!   4 to 13 passed. It has no public constructor; [`validate`] is the only
//!   function that makes one.
//! - A failure changes nothing: [`validate`] reads the snapshot through
//!   shared references and builds the patched document on a copy.
//! - The patched document of a [`ValidatedPatch`] has the page revision R′
//!   that the client computed (check 11) on its own bytes (check 12), so its
//!   bytes are the bytes of the server document unless SHA-256 has a
//!   collision.
//! - No error, no [`fmt::Debug`] output and no [`fmt::Display`] output has
//!   content bytes, a block id, the URL or the nonce.
//!
//! # Resource limits
//!
//! Diff engine section 3, "Bounds". Checks 4 to 6 read at most
//! `min(maxPatchBytes, limits.max_patch_bytes)` bytes; [`PatchLimits`] caps
//! `maxPatchBytes` at the top of its safe range (5 MiB). Checks 9 to 11
//! hash each content once (at most `maxBlockBytes` each). Check 12 makes one
//! copy of at most the smaller of `maxDocumentBytes` and the M-01 limit and
//! one markup pass over it; check 13 reads the patched content once. The
//! replacements move the content strings of the envelope (no copy). No
//! input panics.

use core::fmt;

use rivqen_integrity::PageRevision;
use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_model::limits::{Limit, ResourceLimits};
use rivqen_proto::delta::{DeltaError, verify_delta};
use rivqen_proto::types::{BlockFormat, BlockId, Nonce, SnapshotMeta};
use rivqen_template::limits::{LimitsError, TemplateLimits};
use rivqen_template::rqp::RqpDocument;
use rivqen_template::rqp::patch::{PatchError, check_patch};

/// The cap of `maxPatchBytes`: the top of its safe range (5 MiB). A literal,
/// because a `const fn` cannot convert the `u64` of [`Limit::range`]; the
/// assertion keeps the two values equal at compile time.
const MAX_PATCH_BYTES_CAP: usize = 5 * 1024 * 1024;

const _: () = assert!(Limit::MaxPatchBytes.range().max() == 5 * 1024 * 1024);

/// The client limits of checks 4 to 13.
///
/// `maxBlockBytes` of check 10 is the value of the [`TemplateLimits`], so
/// checks 10 and 12 use one value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PatchLimits {
    max_patch_bytes: usize,
    template: TemplateLimits,
}

impl PatchLimits {
    /// The limits with `max_patch_bytes` capped at 5 MiB, the top of the
    /// safe range of `maxPatchBytes`. A configured value can only lower the
    /// limit.
    #[must_use]
    pub const fn new(max_patch_bytes: usize, template: TemplateLimits) -> Self {
        let max_patch_bytes = if max_patch_bytes < MAX_PATCH_BYTES_CAP {
            max_patch_bytes
        } else {
            MAX_PATCH_BYTES_CAP
        };
        Self {
            max_patch_bytes,
            template,
        }
    }

    /// `maxPatchBytes`, at most 5 MiB. Check 4 uses the smaller of this
    /// value and the `limits` member of the snapshot manifest.
    #[must_use]
    pub const fn max_patch_bytes(&self) -> usize {
        self.max_patch_bytes
    }

    /// The limits of the markup analysis of check 12; its
    /// `max_block_bytes` is `maxBlockBytes` of check 10.
    #[must_use]
    pub const fn template(&self) -> &TemplateLimits {
        &self.template
    }
}

impl TryFrom<&ResourceLimits> for PatchLimits {
    type Error = LimitsError;

    /// The limits of `limits`, each capped. A `max_patch_bytes` value that
    /// does not fit `usize` gets the cap.
    fn try_from(limits: &ResourceLimits) -> Result<Self, Self::Error> {
        let template = TemplateLimits::try_from(limits)?;
        let max_patch_bytes = usize::try_from(limits.max_patch_bytes).unwrap_or(usize::MAX);
        Ok(Self::new(max_patch_bytes, template))
    }
}

/// The stored snapshot that a delta applies to.
///
/// `analysis` is the markup analysis of `original` and `meta` holds the
/// values that the client computed and checked for the snapshot (template
/// revision, page revision B, blocks, `policy` and `limits`). When the
/// three do not belong together, check 12 fails closed (`INTEGRITY_ERROR`
/// or `PATCH_CONTENT_REJECTED`), never with another page.
#[derive(Clone, Copy)]
pub struct StoredSnapshot<'a> {
    /// The original document O of the snapshot.
    pub original: &'a [u8],
    /// The markup analysis of `original`.
    pub analysis: &'a RqpDocument,
    /// The checked snapshot values.
    pub meta: &'a SnapshotMeta,
}

/// Lengths and counts only, never document bytes (errors-logging section 4).
impl fmt::Debug for StoredSnapshot<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StoredSnapshot")
            .field("original_len", &self.original.len())
            .field("blocks", &self.analysis.blocks.len())
            .finish_non_exhaustive()
    }
}

/// The values of the request that the delta answers (the inputs U and N of
/// the binding, check 11).
#[derive(Clone, Copy)]
pub struct DeltaRequest<'a> {
    /// U: the canonical final URL of the delta response (manifest and patch
    /// section 2.2). [`validate`] does not parse it. `None` or an empty text
    /// fails check 11.
    pub final_url: Option<&'a str>,
    /// N: the nonce that the request of this response sent. `None` fails
    /// check 11.
    pub nonce: Option<&'a Nonce>,
}

/// Presence only, never the URL or the nonce (the query and the nonce are
/// never logged; RQP-17).
impl fmt::Debug for DeltaRequest<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeltaRequest")
            .field("final_url", &self.final_url.map(|_| "<redacted>"))
            .field("nonce", &self.nonce.map(|_| "<redacted>"))
            .finish()
    }
}

/// The new content of one block of a [`ValidatedPatch`].
#[derive(Clone, PartialEq, Eq)]
pub struct BlockReplacement {
    block: usize,
    id: BlockId,
    format: BlockFormat,
    content: String,
}

impl BlockReplacement {
    /// The index of the block in the block list of the snapshot (and of the
    /// patched document).
    #[must_use]
    pub const fn block(&self) -> usize {
        self.block
    }

    /// The block id.
    #[must_use]
    pub const fn id(&self) -> &BlockId {
        &self.id
    }

    /// The format of the block in the snapshot.
    #[must_use]
    pub const fn format(&self) -> BlockFormat {
        self.format
    }

    /// The new content: the decoded `content` string of the operation. Page
    /// content: never logged.
    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }
}

/// The block index, the format and the content length; never the block id
/// or content bytes.
impl fmt::Debug for BlockReplacement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlockReplacement")
            .field("block", &self.block)
            .field("format", &self.format)
            .field("content_len", &self.content.len())
            .finish_non_exhaustive()
    }
}

/// A delta that passed checks 4 to 13 against one snapshot (diff engine
/// section 4).
///
/// Only [`validate`] makes one (R-CORE-05). It holds the replacements in
/// document order, the patched document (the new snapshot), its markup
/// analysis (the block offsets of the new snapshot), and the values of the
/// new snapshot: the template revision, R′, the block list L′, and the
/// `policy` and `limits` of the base snapshot.
#[derive(Clone, PartialEq, Eq)]
pub struct ValidatedPatch {
    replacements: Vec<BlockReplacement>,
    document: Vec<u8>,
    analysis: RqpDocument,
    meta: SnapshotMeta,
    base_revision: PageRevision,
}

/// Sizes and counts only, never document bytes, block ids or revisions
/// (errors-logging section 4).
impl fmt::Debug for ValidatedPatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ValidatedPatch")
            .field("replacements", &self.replacements.len())
            .field("document_len", &self.document.len())
            .field("blocks", &self.analysis.blocks.len())
            .finish_non_exhaustive()
    }
}

impl ValidatedPatch {
    /// The block replacements in document order (strictly increasing block
    /// index; check 9). Each content is the content of its block in
    /// [`Self::document`].
    #[must_use]
    pub fn replacements(&self) -> &[BlockReplacement] {
        &self.replacements
    }

    /// The bytes of the patched document: the original document O of the
    /// new snapshot.
    #[must_use]
    pub fn document(&self) -> &[u8] {
        &self.document
    }

    /// The markup analysis of [`Self::document`]: the block offsets of the
    /// new snapshot.
    #[must_use]
    pub const fn analysis(&self) -> &RqpDocument {
        &self.analysis
    }

    /// R′: the page revision of the new snapshot, which the client computed
    /// (check 11) and which the patched document has (check 12).
    #[must_use]
    pub const fn page_revision(&self) -> &PageRevision {
        &self.meta.page_revision
    }

    /// B: the page revision of the snapshot that the delta applies to (a
    /// precondition of the cache commit).
    #[must_use]
    pub const fn base_revision(&self) -> &PageRevision {
        &self.base_revision
    }

    /// The values of the new snapshot: the template revision of the base,
    /// R′, the blocks of [`Self::analysis`] (equal to L′), and the `policy`
    /// and `limits` of the base (manifest and patch section 2.4, "Apply").
    #[must_use]
    pub const fn snapshot_meta(&self) -> &SnapshotMeta {
        &self.meta
    }

    /// The replacements, the document, its analysis and the values of the
    /// new snapshot, by value.
    #[must_use]
    pub fn into_parts(self) -> (Vec<BlockReplacement>, Vec<u8>, RqpDocument, SnapshotMeta) {
        (self.replacements, self.document, self.analysis, self.meta)
    }
}

/// The first failed check of [`validate`].
///
/// [`fmt::Display`] and [`fmt::Debug`] write the check number, the code and
/// a static reason with at most an operation index, a block index, a JSON
/// rule or a markup rule: never content, a block id, the URL or the nonce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ValidateError {
    /// Checks 4, 5, 6, 9, 10 and 11.
    Delta(DeltaError),
    /// Checks 12 and 13.
    Patch(PatchError),
}

impl ValidateError {
    /// The number of the failed check in the table of diff engine section 3:
    /// 4, 5, 6, 9, 10, 11, 12 or 13.
    ///
    /// [`PatchError::Snapshot`] (the stored analysis is not the analysis of
    /// the snapshot values) is check 12: it is found when the patched
    /// document is built.
    #[must_use]
    pub const fn check(&self) -> u8 {
        match self {
            Self::Delta(error) => error.check(),
            Self::Patch(PatchError::Content(_)) => 13,
            Self::Patch(_) => 12,
        }
    }
}

impl fmt::Display for ValidateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // The delta error writes its check number and its code.
            Self::Delta(error) => write!(f, "{error}"),
            Self::Patch(error) => write!(
                f,
                "patch check {}: {}: {error}",
                self.check(),
                error.code().as_str()
            ),
        }
    }
}

impl core::error::Error for ValidateError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Delta(error) => Some(error),
            Self::Patch(error) => Some(error),
        }
    }
}

impl HasErrorCode for ValidateError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::Delta(error) => error.code(),
            Self::Patch(error) => error.code(),
        }
    }
}

impl From<DeltaError> for ValidateError {
    fn from(error: DeltaError) -> Self {
        Self::Delta(error)
    }
}

impl From<PatchError> for ValidateError {
    fn from(error: PatchError) -> Self {
        Self::Patch(error)
    }
}

/// Checks 4 to 13 of diff engine section 3, in this order, on one delta body
/// against one stored snapshot. The first failure stops the checks.
///
/// - `body`: the envelope bytes after content decoding.
/// - `snapshot`: the stored snapshot (original document, its analysis, the
///   checked values).
/// - `request`: U and N of the request that this response answers.
/// - `limits`: the client limits.
///
/// The caller runs checks 1 to 3 first. On an error the caller changes
/// neither the page nor the snapshot and does the fallback load
/// (negotiation section 6.3, item 5).
///
/// # Errors
///
/// [`ValidateError`] of the first failed check, with the code
/// `PATCH_TOO_LARGE`, `PATCH_INVALID`, `INTEGRITY_ERROR` or
/// `PATCH_CONTENT_REJECTED`, never another code.
pub fn validate(
    body: &[u8],
    snapshot: &StoredSnapshot<'_>,
    request: &DeltaRequest<'_>,
    limits: &PatchLimits,
) -> Result<ValidatedPatch, ValidateError> {
    // Checks 4, 5, 6, 9, 10 and 11.
    let delta = verify_delta(
        body,
        limits.max_patch_bytes,
        snapshot.meta,
        request.final_url,
        request.nonce,
        limits.template.max_block_bytes(),
    )?;
    // Checks 12 and 13, on a copy of the original document.
    let patched = check_patch(
        snapshot.original,
        snapshot.analysis,
        snapshot.meta,
        &delta,
        &limits.template,
    )?;
    let (document, analysis, page_revision) = patched.into_parts();
    let (envelope, _, _, entries) = delta.into_parts();
    // Check 9 gives one entry per operation, with strictly increasing block
    // indices, so envelope order is document order.
    let mut replacements = Vec::with_capacity(entries.len());
    for (operation, entry) in envelope.operations.into_iter().zip(entries) {
        // `check_patch` checked that the analysis has the blocks of the
        // snapshot, so the index names a block; the error keeps the function
        // total (fail closed).
        let block = analysis
            .blocks
            .get(entry.block)
            .ok_or(ValidateError::Patch(PatchError::Snapshot))?;
        replacements.push(BlockReplacement {
            block: entry.block,
            id: operation.block_id,
            format: block.descriptor.format,
            content: operation.content,
        });
    }
    let meta = SnapshotMeta {
        template_revision: analysis.template_revision,
        page_revision,
        blocks: analysis
            .blocks
            .iter()
            .map(|block| block.descriptor.clone())
            .collect(),
        policy: snapshot.meta.policy,
        limits: snapshot.meta.limits,
    };
    Ok(ValidatedPatch {
        replacements,
        document,
        analysis,
        meta,
        base_revision: snapshot.meta.page_revision,
    })
}
