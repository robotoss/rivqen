// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The operation checks against a snapshot and the next page revision R′
//! (manifest-patch section 2.3, rules 1 to 3 and 7; section 2.4, steps 3 to 6).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use proptest::prelude::*;
use rivqen_integrity::{PageRevision, Sha256Digest, TemplateRevision, b64u_encode, sha256};
use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_proto::compute::{
    OperationError, next_blocks, next_page_revision, page_revision, snapshot_page_revision,
};
use rivqen_proto::types::{
    BlockDescriptor, BlockFormat, BlockId, Limits, MAX_BLOCK_BYTES, Mode, PatchOp, Policy,
    SnapshotMeta,
};

const TEMPLATE: &str = "t1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM";
const PAGE_REVISION_A: &str = "r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8";
const NEXT_REVISION_PRICE: &str = "r1.psbsc6whgs8ooESisNWp6E3PrQfkg4D69WmXAo12pbI";

fn block(id: &str, format: BlockFormat, content: &str) -> BlockDescriptor {
    BlockDescriptor {
        id: BlockId::parse(id).unwrap(),
        format,
        sha256: sha256(content.as_bytes()),
    }
}

/// The snapshot of the KAT page (manifest-patch section 1.1).
fn kat_snapshot() -> SnapshotMeta {
    SnapshotMeta {
        template_revision: TemplateRevision::parse(TEMPLATE).unwrap(),
        page_revision: PageRevision::parse(PAGE_REVISION_A).unwrap(),
        blocks: vec![
            block("title", BlockFormat::Html, "Shop"),
            block("price", BlockFormat::Html, "120.00"),
            block("cart", BlockFormat::Json, r#"{"items":3}"#),
        ],
        policy: None,
        limits: None,
    }
}

fn op(id: &str, content: &str) -> PatchOp {
    PatchOp {
        block_id: BlockId::parse(id).unwrap(),
        content: content.to_owned(),
    }
}

#[test]
fn compute_next_revision_kat_price() {
    let next = next_page_revision(&kat_snapshot(), &[op("price", "130.00")]).unwrap();
    assert_eq!(next.to_string(), NEXT_REVISION_PRICE);
}

#[test]
fn compute_next_revision_keeps_unchanged_hashes_and_formats() {
    let snapshot = kat_snapshot();
    let next = next_blocks(&snapshot, &[op("title", ""), op("cart", "[]")]).unwrap();
    assert_eq!(
        next,
        vec![
            block("title", BlockFormat::Html, ""),
            snapshot.blocks[1].clone(),
            block("cart", BlockFormat::Json, "[]"),
        ]
    );
    // The snapshot itself does not change.
    assert_eq!(snapshot, kat_snapshot());
}

#[test]
fn compute_next_revision_hashes_the_utf8_bytes_of_the_content() {
    let next = next_blocks(&kat_snapshot(), &[op("price", "<b>é</b>")]).unwrap();
    assert_eq!(next[1].sha256, sha256("<b>é</b>".as_bytes()));
    assert_eq!(next[1].sha256, sha256(b"<b>\xC3\xA9</b>"));
}

#[test]
fn compute_next_revision_uses_the_snapshot_policy_line() {
    let mut snapshot = kat_snapshot();
    snapshot.policy = Some(Policy {
        mode: None,
        ttl_seconds: Some(60),
        offline: Some(true),
    });
    snapshot.limits = Some(Limits {
        max_patch_bytes: Some(65_536),
    });
    let next = next_page_revision(&snapshot, &[op("price", "130.00")]).unwrap();
    // spec/rqp/examples/kat/kat.json, with_policy_limits.page_revision_b.
    assert_eq!(
        next.to_string(),
        "r1.kX-KQFiXmNtLNzjvXP7GR3K-_79Fc1l4WEtReCgqwQQ"
    );
    snapshot.policy = Some(Policy {
        mode: Some(Mode::Private),
        ..Policy::default()
    });
    assert_ne!(
        next_page_revision(&snapshot, &[op("price", "130.00")]).unwrap(),
        next
    );
}

#[test]
fn compute_next_revision_does_not_read_the_snapshot_page_revision() {
    let mut snapshot = kat_snapshot();
    let expected = next_page_revision(&snapshot, &[op("price", "130.00")]).unwrap();
    snapshot.page_revision = PageRevision::parse(NEXT_REVISION_PRICE).unwrap();
    assert_eq!(
        next_page_revision(&snapshot, &[op("price", "130.00")]).unwrap(),
        expected
    );
    assert_eq!(
        snapshot_page_revision(&snapshot).to_string(),
        PAGE_REVISION_A
    );
}

#[test]
fn compute_next_revision_rejects_an_unknown_block() {
    let snapshot = kat_snapshot();
    assert_eq!(
        next_page_revision(&snapshot, &[op("stock", "1")]),
        Err(OperationError::UnknownBlock { index: 0 })
    );
    assert_eq!(
        next_page_revision(&snapshot, &[op("title", "x"), op("stock", "1")]),
        Err(OperationError::UnknownBlock { index: 1 })
    );
    let empty = SnapshotMeta {
        blocks: Vec::new(),
        ..kat_snapshot()
    };
    assert_eq!(
        next_page_revision(&empty, &[op("title", "x")]),
        Err(OperationError::UnknownBlock { index: 0 })
    );
}

#[test]
fn compute_next_revision_rejects_a_duplicate_block() {
    let snapshot = kat_snapshot();
    assert_eq!(
        next_page_revision(&snapshot, &[op("price", "1"), op("price", "1")]),
        Err(OperationError::DuplicateBlock { index: 1 })
    );
    // A later operation of an earlier block also breaks document order; the
    // code is the same (PATCH_INVALID).
    assert_eq!(
        next_page_revision(
            &snapshot,
            &[op("title", "1"), op("price", "1"), op("title", "1")]
        ),
        Err(OperationError::NotDocumentOrder { index: 2 })
    );
}

#[test]
fn compute_duplicate_snapshot_ids_follow_the_oracle() {
    // The reference client (client.mjs operationIndices) maps each operation
    // to the FIRST block of its id and needs strictly growing indices. With
    // two blocks `a`, the second operation on `a` has the same index as the
    // first: DuplicateBlock (PATCH_INVALID).
    let snapshot = SnapshotMeta {
        blocks: vec![
            block("a", BlockFormat::Html, "1"),
            block("a", BlockFormat::Html, "2"),
        ],
        ..kat_snapshot()
    };
    let result = next_page_revision(&snapshot, &[op("a", "x"), op("a", "y")]);
    assert_eq!(result, Err(OperationError::DuplicateBlock { index: 1 }));
    assert_eq!(result.unwrap_err().code(), ErrorCode::PatchInvalid);
    // One operation changes the first block only.
    let next = next_blocks(&snapshot, &[op("a", "x")]).unwrap();
    assert_eq!(
        next,
        vec![
            block("a", BlockFormat::Html, "x"),
            snapshot.blocks[1].clone()
        ]
    );
    // [a, b, a]: an operation on b, then on a, goes back to index 0.
    let snapshot = SnapshotMeta {
        blocks: vec![
            block("a", BlockFormat::Html, "1"),
            block("b", BlockFormat::Html, "2"),
            block("a", BlockFormat::Html, "3"),
        ],
        ..kat_snapshot()
    };
    assert_eq!(
        next_blocks(&snapshot, &[op("b", "x"), op("a", "y")]),
        Err(OperationError::NotDocumentOrder { index: 1 })
    );
}

#[test]
fn compute_next_revision_rejects_another_order() {
    assert_eq!(
        next_page_revision(&kat_snapshot(), &[op("price", "1"), op("title", "1")]),
        Err(OperationError::NotDocumentOrder { index: 1 })
    );
    assert_eq!(
        next_page_revision(
            &kat_snapshot(),
            &[op("title", "1"), op("cart", "1"), op("price", "1")]
        ),
        Err(OperationError::NotDocumentOrder { index: 2 })
    );
}

#[test]
fn compute_next_revision_rejects_no_operations() {
    assert_eq!(
        next_page_revision(&kat_snapshot(), &[]),
        Err(OperationError::NoOperations)
    );
}

#[test]
fn compute_next_revision_content_limit_is_in_utf8_bytes() {
    let snapshot = kat_snapshot();
    let at_limit = "x".repeat(MAX_BLOCK_BYTES);
    assert!(next_page_revision(&snapshot, &[op("price", &at_limit)]).is_ok());
    let over = "x".repeat(MAX_BLOCK_BYTES + 1);
    assert_eq!(
        next_page_revision(&snapshot, &[op("title", ""), op("price", &over)]),
        Err(OperationError::ContentTooLarge { index: 1 })
    );
    // MAX_BLOCK_BYTES / 2 two-byte characters are at the limit; one more is over.
    let wide = "é".repeat(MAX_BLOCK_BYTES / 2);
    assert!(next_page_revision(&snapshot, &[op("price", &wide)]).is_ok());
    let wide_over = format!("{wide}é");
    assert_eq!(
        next_page_revision(&snapshot, &[op("price", &wide_over)]),
        Err(OperationError::ContentTooLarge { index: 0 })
    );
}

#[test]
fn compute_next_revision_checks_the_list_before_the_sizes() {
    // Step 3 for the whole list comes before step 4 (the reference client
    // gives PATCH_INVALID here, not PATCH_TOO_LARGE).
    let over = "x".repeat(MAX_BLOCK_BYTES + 1);
    assert_eq!(
        next_page_revision(&kat_snapshot(), &[op("price", &over), op("stock", "1")]),
        Err(OperationError::UnknownBlock { index: 1 })
    );
}

#[test]
fn compute_operation_error_codes() {
    for error in [
        OperationError::NoOperations,
        OperationError::UnknownBlock { index: 0 },
        OperationError::DuplicateBlock { index: 1 },
        OperationError::NotDocumentOrder { index: 2 },
    ] {
        assert_eq!(error.code(), ErrorCode::PatchInvalid, "{error:?}");
    }
    assert_eq!(
        OperationError::ContentTooLarge { index: 0 }.code(),
        ErrorCode::PatchTooLarge
    );
}

#[test]
fn compute_operation_error_has_no_id_or_content() {
    const MARKER: &str = "zz-marker-91";
    let snapshot = kat_snapshot();
    let over = format!("{MARKER}{}", "x".repeat(MAX_BLOCK_BYTES));
    let errors = [
        next_page_revision(&snapshot, &[]).unwrap_err(),
        next_page_revision(&snapshot, &[op(MARKER, MARKER)]).unwrap_err(),
        next_page_revision(&snapshot, &[op("price", MARKER), op("price", MARKER)]).unwrap_err(),
        next_page_revision(&snapshot, &[op("price", MARKER), op("title", MARKER)]).unwrap_err(),
        next_page_revision(&snapshot, &[op("price", &over)]).unwrap_err(),
    ];
    let mut texts = std::collections::BTreeSet::new();
    for error in errors {
        let shown = format!("{error} {error:?}");
        assert!(!shown.contains(MARKER), "{shown}");
        assert!(!shown.contains("price"), "{shown}");
        texts.insert(error.to_string());
    }
    // Each rule has its own text.
    assert_eq!(texts.len(), 5);
    assert_eq!(
        OperationError::UnknownBlock { index: 7 }.to_string(),
        "operation 7 names no block of the snapshot"
    );
}

// ---------------------------------------------------------------------------
// Properties

/// A snapshot with `n` blocks `b0 … b(n-1)` and the given formats and hashes.
fn snapshot_of(blocks: &[(bool, [u8; 32])]) -> SnapshotMeta {
    SnapshotMeta {
        template_revision: TemplateRevision::parse(TEMPLATE).unwrap(),
        page_revision: PageRevision::parse(PAGE_REVISION_A).unwrap(),
        blocks: blocks
            .iter()
            .enumerate()
            .map(|(i, (json, hash))| BlockDescriptor {
                id: BlockId::parse(&format!("b{i}")).unwrap(),
                format: if *json {
                    BlockFormat::Json
                } else {
                    BlockFormat::Html
                },
                sha256: Sha256Digest::from_b64u(&b64u_encode(hash)).unwrap(),
            })
            .collect(),
        policy: None,
        limits: None,
    }
}

fn blocks_strategy() -> impl Strategy<Value = Vec<(bool, [u8; 32])>> {
    prop::collection::vec((any::<bool>(), prop::array::uniform32(any::<u8>())), 1..24)
}

/// Section 2.4, steps 3 to 6 written out: the error that the first bad
/// operation gives, else L′ and R′.
fn model(snapshot: &SnapshotMeta, operations: &[PatchOp]) -> Result<PageRevision, OperationError> {
    if operations.is_empty() {
        return Err(OperationError::NoOperations);
    }
    let position = |id: &BlockId| snapshot.blocks.iter().position(|b| &b.id == id);
    let mut previous: Option<usize> = None;
    for (index, operation) in operations.iter().enumerate() {
        match (position(&operation.block_id), previous) {
            (None, _) => return Err(OperationError::UnknownBlock { index }),
            (Some(p), Some(q)) if p == q => {
                return Err(OperationError::DuplicateBlock { index });
            }
            (Some(p), Some(q)) if p < q => {
                return Err(OperationError::NotDocumentOrder { index });
            }
            (Some(p), _) => previous = Some(p),
        }
    }
    let mut next = snapshot.blocks.clone();
    for operation in operations {
        let p = position(&operation.block_id).unwrap();
        next[p].sha256 = sha256(operation.content.as_bytes());
    }
    Ok(page_revision(
        &snapshot.template_revision,
        &next,
        snapshot.policy.as_ref(),
        snapshot.limits.as_ref(),
    ))
}

proptest! {
    /// R′ is computed in document order: operations in document order give
    /// the page revision of the snapshot list with the new hashes (L′), and
    /// the result does not depend on the hashes of the replaced blocks.
    #[test]
    fn compute_next_revision_is_computed_in_document_order(
        blocks in blocks_strategy(),
        picks in prop::collection::vec((any::<bool>(), "[a-z<>/ ]{0,12}"), 24),
    ) {
        let snapshot = snapshot_of(&blocks);
        let operations: Vec<PatchOp> = snapshot
            .blocks
            .iter()
            .zip(&picks)
            .filter(|(_, (pick, _))| *pick)
            .map(|(b, (_, content))| PatchOp { block_id: b.id.clone(), content: content.clone() })
            .collect();
        prop_assume!(!operations.is_empty());
        let next = next_page_revision(&snapshot, &operations);
        prop_assert_eq!(next, model(&snapshot, &operations));
        let list = next_blocks(&snapshot, &operations).unwrap();
        prop_assert_eq!(list.len(), snapshot.blocks.len());
        for (before, after) in snapshot.blocks.iter().zip(&list) {
            prop_assert_eq!(&before.id, &after.id);
            prop_assert_eq!(before.format, after.format);
        }
        // Another old hash of a replaced block gives the same R′.
        let mut other = snapshot.clone();
        let replaced = other
            .blocks
            .iter_mut()
            .find(|b| b.id == operations[0].block_id)
            .unwrap();
        replaced.sha256 = sha256(b"another old content");
        prop_assert_eq!(next_page_revision(&other, &operations), next);
    }

    /// Any order other than document order is rejected at the first
    /// operation that goes back.
    #[test]
    fn compute_next_revision_rejects_every_other_order(
        blocks in blocks_strategy(),
        order in prop::collection::vec(0_usize..24, 2..8),
    ) {
        let snapshot = snapshot_of(&blocks);
        let n = snapshot.blocks.len();
        let positions: Vec<usize> = order.iter().map(|p| p % n).collect();
        let operations: Vec<PatchOp> = positions
            .iter()
            .map(|&p| PatchOp { block_id: snapshot.blocks[p].id.clone(), content: String::new() })
            .collect();
        let result = next_page_revision(&snapshot, &operations);
        prop_assert_eq!(result, model(&snapshot, &operations));
        let increasing = positions.windows(2).all(|w| w[0] < w[1]);
        prop_assert_eq!(result.is_ok(), increasing);
    }

    /// Any list of ids, known or not, gives the result of the model and never
    /// panics.
    #[test]
    fn compute_next_revision_matches_the_model(
        blocks in blocks_strategy(),
        ids in prop::collection::vec(0_usize..30, 0..10),
    ) {
        let snapshot = snapshot_of(&blocks);
        let operations: Vec<PatchOp> = ids
            .iter()
            .map(|i| PatchOp { block_id: BlockId::parse(&format!("b{i}")).unwrap(), content: format!("c{i}") })
            .collect();
        prop_assert_eq!(next_page_revision(&snapshot, &operations), model(&snapshot, &operations));
    }

    /// A snapshot with repeated block ids gives the result of the model (the
    /// first block of an id, as the reference client).
    #[test]
    fn compute_next_revision_with_repeated_ids_matches_the_model(
        blocks in blocks_strategy(),
        names in prop::collection::vec(0_usize..4, 24),
        ids in prop::collection::vec(0_usize..5, 0..8),
    ) {
        let mut snapshot = snapshot_of(&blocks);
        for (block, name) in snapshot.blocks.iter_mut().zip(&names) {
            block.id = BlockId::parse(&format!("b{name}")).unwrap();
        }
        let operations: Vec<PatchOp> = ids
            .iter()
            .map(|i| PatchOp { block_id: BlockId::parse(&format!("b{i}")).unwrap(), content: format!("c{i}") })
            .collect();
        prop_assert_eq!(next_page_revision(&snapshot, &operations), model(&snapshot, &operations));
    }
}
