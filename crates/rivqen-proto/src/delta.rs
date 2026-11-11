// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Delta checks 4 to 6 and 9 to 11 of diff-engine section 3, in that order,
//! on one envelope against one snapshot (no apply, no content rules).
//! Owner: WP-05 S2 T-01.
//!
//! Normative text: docs/engineering/core/diff-engine.md section 3 and
//! docs/engineering/protocol/manifest-patch.md section 2.4, steps 3 to 7.
//! [`verify_delta`] is the only function that runs these checks together:
//!
//! | Check | Step of manifest-patch 2.4 | Function | Code |
//! |---|---|---|---|
//! | 4 | size | [`crate::envelope::parse_envelope`] | `PATCH_TOO_LARGE` |
//! | 5 | JSON rules | [`crate::envelope::parse_envelope`] | `PATCH_INVALID` |
//! | 6 | schema | [`crate::envelope::parse_envelope`] | `PATCH_INVALID` |
//! | 9 | 3 | [`crate::compute::next_blocks`] | `PATCH_INVALID` |
//! | 10 | 4, 5 | [`crate::compute::next_blocks`] | `PATCH_TOO_LARGE` |
//! | 11 | 6, 7 | [`crate::compute::page_revision`], [`crate::compute::binding`] | `INTEGRITY_ERROR` |
//!
//! Checks 1 to 3 (transport, offer, partition) come before and checks 12 and
//! 13 (apply on a copy, content rules) come after; both belong to the session
//! (WP-08). Only a [`VerifiedDelta`] goes on to check 12.
//!
//! Bounds (diff-engine section 3, "Bounds"): checks 4 to 6 read at most
//! `min(maxPatchBytes, limits.max_patch_bytes)` bytes once; checks 9 to 11
//! hash each content once, the page revision preimage once and the binding
//! preimage once. Work is `O(n + (b + o) log b)` for `n` body bytes, `b`
//! snapshot blocks and `o` operations (at most 256). No input panics.

use core::fmt;

use rivqen_integrity::{Binding, PageRevision};
use rivqen_model::error::{ErrorCode, HasErrorCode};

use crate::compute::{
    OperationError, PatchedBlock, binding, next_blocks_with_hashes, snapshot_revision_of,
};
use crate::envelope::{EnvelopeError, parse_envelope};
use crate::types::{BlockDescriptor, Nonce, PatchEnvelope, SnapshotMeta};

/// Why the binding of check 11 does not match. Every case is
/// `INTEGRITY_ERROR` (manifest-patch section 2.4, step 7): an input that the
/// client cannot compute is a mismatch, never another error (fail closed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum IntegrityFailure {
    /// The caller has no canonical final URL U (`None` or an empty text).
    NoFinalUrl,
    /// The caller has no nonce N of the request.
    NoNonce,
    /// The binding of U, B, N and R′ is not the `binding` of the envelope.
    BindingDiffers,
}

impl IntegrityFailure {
    /// A static reason without input.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::NoFinalUrl => "no canonical final URL for the binding",
            Self::NoNonce => "no request nonce for the binding",
            Self::BindingDiffers => "binding differs from the computed binding",
        }
    }
}

/// The first failed check of [`verify_delta`].
///
/// [`fmt::Display`] and [`fmt::Debug`] write the check number, the code and a
/// static reason with at most an operation index or a JSON rule: never body
/// bytes, content, a block id, the URL or the nonce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DeltaError {
    /// Checks 4 to 6: size, JSON rules, schema.
    Envelope(EnvelopeError),
    /// Checks 9 and 10: the operations against the snapshot, the content
    /// size.
    Operations(OperationError),
    /// Check 11: the binding.
    Integrity(IntegrityFailure),
}

impl DeltaError {
    /// The number of the failed check in the table of diff-engine section 3:
    /// 4, 5, 6, 9, 10 or 11.
    #[must_use]
    pub const fn check(&self) -> u8 {
        match self {
            Self::Envelope(EnvelopeError::TooLarge) => 4,
            Self::Envelope(EnvelopeError::Json(_)) => 5,
            Self::Envelope(EnvelopeError::Schema(_)) => 6,
            Self::Operations(OperationError::ContentTooLarge { .. }) => 10,
            Self::Operations(_) => 9,
            Self::Integrity(_) => 11,
        }
    }
}

impl fmt::Display for DeltaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "delta check {}: ", self.check())?;
        match self {
            // The envelope error writes its own code.
            Self::Envelope(e) => write!(f, "{e}"),
            Self::Operations(e) => write!(f, "{}: {e}", e.code()),
            Self::Integrity(e) => write!(f, "{}: {}", self.code(), e.reason()),
        }
    }
}

impl core::error::Error for DeltaError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Envelope(e) => Some(e),
            Self::Operations(e) => Some(e),
            Self::Integrity(_) => None,
        }
    }
}

impl HasErrorCode for DeltaError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::Envelope(e) => e.code(),
            Self::Operations(e) => e.code(),
            Self::Integrity(_) => ErrorCode::IntegrityError,
        }
    }
}

impl From<EnvelopeError> for DeltaError {
    fn from(e: EnvelopeError) -> Self {
        Self::Envelope(e)
    }
}

impl From<OperationError> for DeltaError {
    fn from(e: OperationError) -> Self {
        Self::Operations(e)
    }
}

/// A delta that passed checks 4 to 6 and 9 to 11 against one snapshot.
///
/// Only [`verify_delta`] makes one, so a value of this type is proof that
/// the checks ran. It holds what checks 12 and 13 (WP-08) need without a
/// second hash: the envelope, R′, the list L′ and the new hash of each
/// operation. [`fmt::Debug`] writes no content (see
/// [`crate::types::PatchOp`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedDelta {
    envelope: PatchEnvelope,
    next_revision: PageRevision,
    next_blocks: Vec<BlockDescriptor>,
    patched: Vec<PatchedBlock>,
}

impl VerifiedDelta {
    /// The envelope: its binding and its operations in envelope order.
    #[must_use]
    pub const fn envelope(&self) -> &PatchEnvelope {
        &self.envelope
    }

    /// R′: the page revision that the client computed (manifest-patch
    /// section 2.4, step 6). The new snapshot gets this value (H-107).
    #[must_use]
    pub const fn next_revision(&self) -> &PageRevision {
        &self.next_revision
    }

    /// L′: the snapshot blocks in document order with the new hash of each
    /// block with an operation (section 2.4, step 5).
    #[must_use]
    pub fn next_blocks(&self) -> &[BlockDescriptor] {
        &self.next_blocks
    }

    /// For each operation, in envelope order: the snapshot block index and
    /// the hash of the new content (section 2.4, step 4).
    #[must_use]
    pub fn patched(&self) -> &[PatchedBlock] {
        &self.patched
    }

    /// The envelope, R′, L′ and the patched blocks, by value.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        PatchEnvelope,
        PageRevision,
        Vec<BlockDescriptor>,
        Vec<PatchedBlock>,
    ) {
        (
            self.envelope,
            self.next_revision,
            self.next_blocks,
            self.patched,
        )
    }
}

/// Checks 4, 5, 6, 9, 10 and 11 of diff-engine section 3, in this order, on
/// one delta body against one snapshot. The first failure stops the checks.
///
/// - `body`: the envelope bytes after content decoding.
/// - `max_patch_bytes`: the client limit `maxPatchBytes`; check 4 uses
///   `min(max_patch_bytes, snapshot.limits.max_patch_bytes)`
///   ([`crate::envelope::envelope_limit`]).
/// - `snapshot`: the values that the client computed and checked for its
///   snapshot (template revision, page revision B, blocks with full hashes,
///   `policy` and `limits` members).
/// - `final_url`: U, the canonical final URL of the delta response. The
///   function does not parse or canonicalize it (WP-08). `None` or an empty
///   text is a mismatch.
/// - `nonce`: N, the nonce that the request of this response sent. `None`
///   is a mismatch.
/// - `max_block_bytes`: the client limit `maxBlockBytes` of check 10.
///
/// # Errors
///
/// [`DeltaError`] of the first failed check: `PATCH_TOO_LARGE`,
/// `PATCH_INVALID` or `INTEGRITY_ERROR`, never another code.
pub fn verify_delta(
    body: &[u8],
    max_patch_bytes: usize,
    snapshot: &SnapshotMeta,
    final_url: Option<&str>,
    nonce: Option<&Nonce>,
    max_block_bytes: usize,
) -> Result<VerifiedDelta, DeltaError> {
    // Checks 4 to 6.
    let envelope = parse_envelope(body, max_patch_bytes, snapshot.limits)?;
    // Checks 9 and 10 (steps 3 to 5): the whole list, then size and hash of
    // each content.
    let next = next_blocks_with_hashes(snapshot, &envelope.operations, max_block_bytes)?;
    // Check 11 (steps 6 and 7).
    let url = final_url
        .filter(|url| !url.is_empty())
        .ok_or(DeltaError::Integrity(IntegrityFailure::NoFinalUrl))?;
    let nonce = nonce.ok_or(DeltaError::Integrity(IntegrityFailure::NoNonce))?;
    let next_revision = snapshot_revision_of(snapshot, &next.blocks);
    let expected: Binding = binding(url, &snapshot.page_revision, nonce, &next_revision);
    if expected != envelope.binding {
        return Err(DeltaError::Integrity(IntegrityFailure::BindingDiffers));
    }
    Ok(VerifiedDelta {
        envelope,
        next_revision,
        next_blocks: next.blocks,
        patched: next.patched,
    })
}
