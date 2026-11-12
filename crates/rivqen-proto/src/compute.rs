// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Values the client computes: the policy line, the page revision from a
//! [`crate::types::SnapshotMeta`] and the binding (manifest-patch sections 1.3
//! and 2.2). Owner: WP-05 S1 T-02.
//!
//! Normative text: docs/engineering/protocol/manifest-patch.md section 1.3
//! (steps 4 and 5, the policy line), section 2.2 (the binding) and section 2.4
//! (steps 3 to 6: the operation checks against the snapshot and the next page
//! revision R′).
//!
//! All functions are pure: no I/O, no clock, no logging. Memory is linear in
//! the size of the inputs. Work is linear too, except the operation check of
//! [`next_blocks`]: `O((b + o) log b)` for `b` snapshot blocks and `o`
//! operations. The functions never decode a revision:
//! they write the text forms of section 1.3, rule 1.
//!
//! - [`policy_line`]: the line `PL\t` S `\n`, or no bytes (H-55).
//! - [`page_revision`], [`snapshot_page_revision`]: steps 4 and 5.
//! - [`next_blocks`], [`next_page_revision`]: section 2.4, steps 3 to 6.
//! - [`binding`]: section 2.2 (H-107, H-108).
//!
//! The content size check of section 2.4, step 4 (`maxBlockBytes`) is in
//! [`next_blocks`] only; the caller gives the limit. [`crate::delta`] runs it
//! as check 10 of diff-engine section 3.

use core::fmt;
use std::collections::BTreeMap;

use rivqen_integrity::{
    Binding, PageRevision, Sha256Digest, TemplateRevision, b64u_encode, sha256, sha256_parts,
};
use rivqen_model::error::{ErrorCode, HasErrorCode};

use crate::types::{
    BlockDescriptor, BlockId, Limits, MAX_BLOCK_BYTES, MAX_BLOCK_ID_BYTES, Mode, Nonce, PatchOp,
    Policy, SnapshotMeta,
};

/// First line of the page revision preimage P (section 1.3, step 4).
const PAGE_REVISION_TAG: &[u8] = b"rqp-r1\n";
/// First line of the binding preimage (section 2.2).
const BINDING_TAG: &[u8] = b"rqp-d1\n";
/// Length of a revision text: the prefix and 43 base64url characters.
const REVISION_CHARS: usize = 46;
/// The longest block line: id, tab, `html`, tab, 43 characters, line feed.
const MAX_BLOCK_LINE_BYTES: usize = MAX_BLOCK_ID_BYTES + 1 + 4 + 1 + 43 + 1;
/// The longest policy line: every item, both `u32` values at their maximum.
/// `PL\tpolicy;mode=private;ttl_seconds=4294967295;offline=false;limits;max_patch_bytes=4294967295\n`
const MAX_POLICY_LINE_BYTES: usize = 94;

/// The policy line PL of section 1.3, step 4 (H-55): `"PL\t" ‖ S ‖ "\n"`.
///
/// `policy` and `limits` are the members of the manifest that carries the
/// page revision: `None` when the member is absent. Returns `None` (no bytes)
/// when both members are absent. S has the items of the section 1.3 table that
/// apply, in table order, separated by `;`. A member field that is `None`
/// writes no item; a default value is never written for it (rule 1: members,
/// not effective values).
///
/// The values are written as they are: the range checks of sections 1.4 and
/// 1.5 belong to the manifest reader.
#[must_use]
pub fn policy_line(policy: Option<&Policy>, limits: Option<&Limits>) -> Option<String> {
    if policy.is_none() && limits.is_none() {
        return None;
    }
    let mut line = String::with_capacity(MAX_POLICY_LINE_BYTES);
    line.push_str("PL\t");
    let mut items = Items::new(&mut line);
    if let Some(policy) = policy {
        items.push("policy", None);
        if let Some(mode) = policy.mode {
            items.push("mode", Some(mode_text(mode)));
        }
        if let Some(ttl) = policy.ttl_seconds {
            items.push("ttl_seconds", Some(&ttl.to_string()));
        }
        if let Some(offline) = policy.offline {
            items.push("offline", Some(if offline { "true" } else { "false" }));
        }
    }
    if let Some(limits) = limits {
        items.push("limits", None);
        if let Some(max) = limits.max_patch_bytes {
            items.push("max_patch_bytes", Some(&max.to_string()));
        }
    }
    line.push('\n');
    Some(line)
}

/// The text of a `mode` value.
const fn mode_text(mode: Mode) -> &'static str {
    match mode {
        Mode::Private => "private",
        Mode::Public => "public",
    }
}

/// Writes the items of S with a `;` between two items.
struct Items<'a> {
    line: &'a mut String,
    first: bool,
}

impl<'a> Items<'a> {
    fn new(line: &'a mut String) -> Self {
        Self { line, first: true }
    }

    /// Writes `name`, or `name=value`.
    fn push(&mut self, name: &str, value: Option<&str>) {
        if !self.first {
            self.line.push(';');
        }
        self.first = false;
        self.line.push_str(name);
        if let Some(value) = value {
            self.line.push('=');
            self.line.push_str(value);
        }
    }
}

/// The page revision of section 1.3, steps 4 and 5:
/// `"r1." ‖ B64U(SHA-256(P))` with
/// `P = "rqp-r1\n" ‖ template ‖ "\n" ‖ PL ‖ (id "\t" format "\t" sha256 "\n")*`.
///
/// `blocks` are in document order (markup M-27). `policy` and `limits` are the
/// members of the manifest (see [`policy_line`]).
#[must_use]
pub fn page_revision(
    template: &TemplateRevision,
    blocks: &[BlockDescriptor],
    policy: Option<&Policy>,
    limits: Option<&Limits>,
) -> PageRevision {
    let line = policy_line(policy, limits);
    let capacity = blocks
        .len()
        .saturating_mul(MAX_BLOCK_LINE_BYTES)
        .saturating_add(PAGE_REVISION_TAG.len())
        .saturating_add(REVISION_CHARS)
        .saturating_add(1)
        .saturating_add(MAX_POLICY_LINE_BYTES);
    let mut preimage: Vec<u8> = Vec::with_capacity(capacity);
    preimage.extend_from_slice(PAGE_REVISION_TAG);
    preimage.extend_from_slice(template.to_string().as_bytes());
    preimage.push(b'\n');
    if let Some(line) = &line {
        preimage.extend_from_slice(line.as_bytes());
    }
    for block in blocks {
        preimage.extend_from_slice(block.id.as_str().as_bytes());
        preimage.push(b'\t');
        preimage.extend_from_slice(block.format.as_str().as_bytes());
        preimage.push(b'\t');
        preimage.extend_from_slice(block.sha256.to_b64u().as_bytes());
        preimage.push(b'\n');
    }
    PageRevision::from_digest(sha256(&preimage))
}

/// The page revision that the values of `snapshot` give: its template
/// revision, its blocks and the policy line of its manifest members.
///
/// The field `snapshot.page_revision` is not read. A snapshot is consistent
/// when the result is equal to it (section 1.8, item 5).
#[must_use]
pub fn snapshot_page_revision(snapshot: &SnapshotMeta) -> PageRevision {
    snapshot_revision_of(snapshot, &snapshot.blocks)
}

/// Why an operation list does not apply to a snapshot (section 2.4, steps 3
/// and 4). `index` is the position of the operation in the envelope. The error
/// never holds a block id or content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum OperationError {
    /// The list has no operation (section 2.3, rule 3; the schema also
    /// rejects it). `PATCH_INVALID`.
    NoOperations,
    /// The block id names no block of the snapshot (section 2.3, rule 1).
    /// `PATCH_INVALID`.
    UnknownBlock {
        /// Position of the operation.
        index: usize,
    },
    /// The block already has an operation in this list (section 2.3, rule 2).
    /// `PATCH_INVALID`.
    DuplicateBlock {
        /// Position of the second operation.
        index: usize,
    },
    /// The operation is not in document order of its block (section 2.3,
    /// rule 3). `PATCH_INVALID`.
    NotDocumentOrder {
        /// Position of the operation.
        index: usize,
    },
    /// The UTF-8 bytes of the content are more than the client limit
    /// `maxBlockBytes` that the caller gives (section 2.3, rule 7; section
    /// 2.4, step 4). `PATCH_TOO_LARGE`.
    ContentTooLarge {
        /// Position of the operation.
        index: usize,
    },
}

impl fmt::Display for OperationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoOperations => f.write_str("patch has no operation"),
            Self::UnknownBlock { index } => {
                write!(f, "operation {index} names no block of the snapshot")
            }
            Self::DuplicateBlock { index } => {
                write!(f, "operation {index} names a block that has an operation")
            }
            Self::NotDocumentOrder { index } => {
                write!(f, "operation {index} is not in document order")
            }
            Self::ContentTooLarge { index } => {
                write!(f, "operation {index} has content over maxBlockBytes")
            }
        }
    }
}

impl core::error::Error for OperationError {}

impl HasErrorCode for OperationError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::NoOperations
            | Self::UnknownBlock { .. }
            | Self::DuplicateBlock { .. }
            | Self::NotDocumentOrder { .. } => ErrorCode::PatchInvalid,
            Self::ContentTooLarge { .. } => ErrorCode::PatchTooLarge,
        }
    }
}

/// Section 2.4, step 3: the rules 1 to 3 of section 2.3 for the whole list.
///
/// Returns the snapshot block index of each operation, in operation order.
/// The index of an operation is the FIRST block of the snapshot with its
/// block id, and the indices must grow strictly (the reference client,
/// `tools/rqp-protocol/src/client.mjs` `operationIndices`). This rule also
/// holds for a snapshot with two blocks of the same id: only the first one
/// can take an operation. For each operation in order, the first failed rule
/// gives the error: no block with the id ([`OperationError::UnknownBlock`]),
/// the same index as the previous operation
/// ([`OperationError::DuplicateBlock`]), a smaller index
/// ([`OperationError::NotDocumentOrder`]).
///
/// Work: `O((b + o) log b)` for `b` blocks and `o` operations (one ordered
/// map of the first index of each id); memory: the map and `o` indices.
fn check_operations(
    blocks: &[BlockDescriptor],
    operations: &[PatchOp],
) -> Result<Vec<usize>, OperationError> {
    if operations.is_empty() {
        return Err(OperationError::NoOperations);
    }
    let mut first_index: BTreeMap<&BlockId, usize> = BTreeMap::new();
    for (position, block) in blocks.iter().enumerate() {
        first_index.entry(&block.id).or_insert(position);
    }
    let mut indices = Vec::with_capacity(operations.len());
    let mut previous: Option<usize> = None;
    for (index, operation) in operations.iter().enumerate() {
        let Some(&position) = first_index.get(&operation.block_id) else {
            return Err(OperationError::UnknownBlock { index });
        };
        match previous {
            Some(last) if position == last => {
                return Err(OperationError::DuplicateBlock { index });
            }
            Some(last) if position < last => {
                return Err(OperationError::NotDocumentOrder { index });
            }
            _ => {}
        }
        previous = Some(position);
        indices.push(position);
    }
    Ok(indices)
}

/// The new hash of one block with an operation (section 2.4, step 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PatchedBlock {
    /// The index of the block in the snapshot block list: the first block
    /// with the `block_id` of the operation.
    pub block: usize,
    /// `SHA-256(UTF-8(content))` of the operation.
    pub sha256: Sha256Digest,
}

/// The list L′ and the new hash of each operation, in operation order.
pub(crate) struct NextBlocks {
    /// L′: the snapshot blocks with the new hashes.
    pub(crate) blocks: Vec<BlockDescriptor>,
    /// One entry for each operation, in envelope order.
    pub(crate) patched: Vec<PatchedBlock>,
}

/// Steps 3 to 5 of section 2.4; see [`next_blocks`]. Also returns the new
/// hash of each operation, so a caller does not hash the content again.
pub(crate) fn next_blocks_with_hashes(
    snapshot: &SnapshotMeta,
    operations: &[PatchOp],
    max_block_bytes: usize,
) -> Result<NextBlocks, OperationError> {
    // `maxBlockBytes` is fixed by rqp/1 (DL-155): a caller value can only
    // lower the limit, never raise it (S2 security review S2-1).
    let max_block_bytes = max_block_bytes.min(MAX_BLOCK_BYTES);
    let indices = check_operations(&snapshot.blocks, operations)?;
    let mut blocks = snapshot.blocks.clone();
    let mut patched = Vec::with_capacity(indices.len());
    for (index, (operation, position)) in operations.iter().zip(indices).enumerate() {
        if operation.content.len() > max_block_bytes {
            return Err(OperationError::ContentTooLarge { index });
        }
        // check_operations took the position from the snapshot blocks; the
        // error keeps the function total if that ever changes.
        let block = blocks
            .get_mut(position)
            .ok_or(OperationError::UnknownBlock { index })?;
        let digest = sha256(operation.content.as_bytes());
        block.sha256 = digest;
        patched.push(PatchedBlock {
            block: position,
            sha256: digest,
        });
    }
    Ok(NextBlocks { blocks, patched })
}

/// The block list L′ of section 2.4, steps 3 to 5.
///
/// Checks the operation list against the snapshot (step 3: each block id
/// names a block of the snapshot, at most one operation per block, document
/// order; an operation applies to the first snapshot block of its id), then
/// the content size of each operation (step 4: at most `max_block_bytes`
/// UTF-8 bytes, the client limit `maxBlockBytes`), and returns a copy of the
/// snapshot blocks in which each block with an operation has the hash of its
/// new content, `SHA-256(UTF-8(content))`. Every other block keeps
/// its full hash from the snapshot, never a prefix (H-44); every block keeps
/// its format.
///
/// This is the only content size check of step 4 in the crate.
///
/// # Errors
///
/// [`OperationError`]: the first rule that fails, step 3 for the whole list
/// before step 4.
pub fn next_blocks(
    snapshot: &SnapshotMeta,
    operations: &[PatchOp],
    max_block_bytes: usize,
) -> Result<Vec<BlockDescriptor>, OperationError> {
    next_blocks_with_hashes(snapshot, operations, max_block_bytes).map(|next| next.blocks)
}

/// The next page revision R′ of section 2.4, step 6: [`page_revision`] of the
/// template revision of `snapshot`, the list L′ of [`next_blocks`] and the
/// policy line of the snapshot manifest (H-55).
///
/// # Errors
///
/// [`OperationError`] from [`next_blocks`].
pub fn next_page_revision(
    snapshot: &SnapshotMeta,
    operations: &[PatchOp],
    max_block_bytes: usize,
) -> Result<PageRevision, OperationError> {
    let blocks = next_blocks(snapshot, operations, max_block_bytes)?;
    Ok(snapshot_revision_of(snapshot, &blocks))
}

/// [`page_revision`] of the template revision and the manifest members of
/// `snapshot` with another block list.
pub(crate) fn snapshot_revision_of(
    snapshot: &SnapshotMeta,
    blocks: &[BlockDescriptor],
) -> PageRevision {
    page_revision(
        &snapshot.template_revision,
        blocks,
        snapshot.policy.as_ref(),
        snapshot.limits.as_ref(),
    )
}

/// The binding of section 2.2 (H-107, H-108):
/// `"d1." ‖ B64U(SHA-256("rqp-d1" LF U LF B LF N LF R′ LF))`.
///
/// `url` is U, the canonical final URL as text; this function does not parse
/// or canonicalize it (WP-08). `base` is B, the page revision of the snapshot.
/// N is the base64url text without padding of `nonce` (22 characters). `next`
/// is R′. The URL is hashed in place; the function allocates only the three
/// fixed-length texts.
#[must_use]
pub fn binding(url: &str, base: &PageRevision, nonce: &Nonce, next: &PageRevision) -> Binding {
    let base = base.to_string();
    let nonce = b64u_encode(&nonce.0);
    let next = next.to_string();
    Binding::from_digest(sha256_parts(&[
        BINDING_TAG,
        url.as_bytes(),
        b"\n",
        base.as_bytes(),
        b"\n",
        nonce.as_bytes(),
        b"\n",
        next.as_bytes(),
        b"\n",
    ]))
}
