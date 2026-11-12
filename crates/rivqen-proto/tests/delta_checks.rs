// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! `delta::verify_delta`: checks 4, 5, 6, 9, 10 and 11 of diff-engine
//! section 3 in that order (manifest-patch section 2.4, steps 3 to 7), the
//! fail-closed binding inputs, the error texts and the code set for any body.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use proptest::prelude::*;
use rivqen_integrity::{
    Binding, PageRevision, Sha256Digest, TemplateRevision, b64u_encode, sha256,
};
use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_proto::compute::{
    OperationError, PatchedBlock, binding, next_page_revision, snapshot_page_revision,
};
use rivqen_proto::delta::{DeltaError, IntegrityFailure, VerifiedDelta, verify_delta};
use rivqen_proto::envelope::EnvelopeError;
use rivqen_proto::json::{JsonRule, SchemaKeyword};
use rivqen_proto::types::{
    BlockDescriptor, BlockFormat, BlockId, Limits, MAX_BLOCK_BYTES, MAX_PATCH_BYTES, Nonce,
    PatchOp, Policy, SnapshotMeta,
};

const TEMPLATE: &str = "t1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM";
const PAGE_REVISION_A: &str = "r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8";
const URL: &str = "https://shop.example.com/catalog";

/// The nonce of `spec/rqp/examples/kat/kat.json` (bytes 00 to 0F).
fn kat_nonce() -> Nonce {
    let mut bytes = [0_u8; 16];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::try_from(i).unwrap();
    }
    Nonce(bytes)
}

fn block(id: &str, format: BlockFormat, content: &str) -> BlockDescriptor {
    BlockDescriptor {
        id: BlockId::parse(id).unwrap(),
        format,
        sha256: sha256(content.as_bytes()),
    }
}

/// The snapshot of the KAT page (manifest-patch section 1.1).
fn kat_snapshot() -> SnapshotMeta {
    let snapshot = SnapshotMeta {
        template_revision: TemplateRevision::parse(TEMPLATE).unwrap(),
        page_revision: PageRevision::parse(PAGE_REVISION_A).unwrap(),
        blocks: vec![
            block("title", BlockFormat::Html, "Shop"),
            block("price", BlockFormat::Html, "120.00"),
            block("cart", BlockFormat::Json, r#"{"items":3}"#),
        ],
        policy: None,
        limits: None,
    };
    assert_eq!(snapshot_page_revision(&snapshot), snapshot.page_revision);
    snapshot
}

fn op(id: &str, content: &str) -> PatchOp {
    PatchOp {
        block_id: BlockId::parse(id).unwrap(),
        content: content.to_owned(),
    }
}

/// The binding that the server writes for these operations on `snapshot`.
fn bound(snapshot: &SnapshotMeta, url: &str, nonce: &Nonce, operations: &[PatchOp]) -> Binding {
    let next = next_page_revision(snapshot, operations, MAX_BLOCK_BYTES).unwrap();
    binding(url, &snapshot.page_revision, nonce, &next)
}

fn operations_json(operations: &[PatchOp]) -> String {
    operations
        .iter()
        .map(|o| {
            format!(
                r#"{{"block_id":"{}","content":{}}}"#,
                o.block_id,
                serde_json::to_string(&o.content).unwrap()
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// An envelope text with this binding and these operations.
fn envelope(binding: &Binding, operations: &[PatchOp]) -> Vec<u8> {
    format!(
        r#"{{"binding":"{binding}","operations":[{}]}}"#,
        operations_json(operations)
    )
    .into_bytes()
}

/// A delta body that passes every check for `snapshot`, [`URL`] and the KAT
/// nonce.
fn good_body(snapshot: &SnapshotMeta, operations: &[PatchOp]) -> Vec<u8> {
    envelope(&bound(snapshot, URL, &kat_nonce(), operations), operations)
}

fn verify(body: &[u8], snapshot: &SnapshotMeta) -> Result<VerifiedDelta, DeltaError> {
    verify_delta(
        body,
        MAX_PATCH_BYTES,
        snapshot,
        Some(URL),
        Some(&kat_nonce()),
        MAX_BLOCK_BYTES,
    )
}

fn kat_patch(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/rqp/examples/valid/patch")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

// ---------------------------------------------------------------------------
// Accepted deltas

#[test]
fn delta_kat_shop_price_passes() {
    // spec/rqp/examples/kat/kat.json, bindings[0].
    let snapshot = kat_snapshot();
    let verified = verify(&kat_patch("shop-price.json"), &snapshot).unwrap();
    assert_eq!(
        verified.next_revision().to_string(),
        "r1.psbsc6whgs8ooESisNWp6E3PrQfkg4D69WmXAo12pbI"
    );
    assert_eq!(
        verified.envelope().binding.to_string(),
        "d1.orrx-_-Iflnnjs7DvvuOOdxW3FzbZpU_PTlR0jdWw54"
    );
    assert_eq!(
        verified.patched(),
        [PatchedBlock {
            block: 1,
            sha256: sha256(b"130.00")
        }]
    );
    assert_eq!(
        verified.next_blocks(),
        [
            snapshot.blocks[0].clone(),
            block("price", BlockFormat::Html, "130.00"),
            snapshot.blocks[2].clone(),
        ]
    );
}

#[test]
fn delta_kat_shop_all_passes() {
    // spec/rqp/examples/kat/kat.json, bindings[1].
    let snapshot = kat_snapshot();
    let verified = verify(&kat_patch("shop-all.json"), &snapshot).unwrap();
    assert_eq!(
        verified.next_revision().to_string(),
        "r1.ZvwDE73CDqBdDTTbZio2YJhfiHVi38DPliYIJGwg8-A"
    );
    let (envelope, next_revision, next_blocks, patched) = verified.clone().into_parts();
    assert_eq!(&envelope, verified.envelope());
    assert_eq!(&next_revision, verified.next_revision());
    assert_eq!(next_blocks, verified.next_blocks());
    assert_eq!(patched, verified.patched());
    let indices: Vec<usize> = patched.iter().map(|p| p.block).collect();
    assert_eq!(indices, [0, 1, 2]);
    for ((p, operation), block) in patched.iter().zip(&envelope.operations).zip(&next_blocks) {
        assert_eq!(p.sha256, sha256(operation.content.as_bytes()));
        assert_eq!(block.sha256, p.sha256);
    }
}

#[test]
fn delta_result_matches_the_compute_functions() {
    let snapshot = kat_snapshot();
    let ops = [op("title", "New"), op("cart", "[]")];
    let verified = verify(&good_body(&snapshot, &ops), &snapshot).unwrap();
    assert_eq!(
        *verified.next_revision(),
        next_page_revision(&snapshot, &ops, MAX_BLOCK_BYTES).unwrap()
    );
    assert_eq!(verified.envelope().operations, ops);
    assert_eq!(
        verified.patched(),
        [
            PatchedBlock {
                block: 0,
                sha256: sha256(b"New")
            },
            PatchedBlock {
                block: 2,
                sha256: sha256(b"[]")
            },
        ]
    );
    // The format of every block stays the format of the snapshot.
    let formats: Vec<BlockFormat> = verified.next_blocks().iter().map(|b| b.format).collect();
    assert_eq!(
        formats,
        [BlockFormat::Html, BlockFormat::Html, BlockFormat::Json]
    );
}

#[test]
fn delta_uses_the_snapshot_policy_line_and_limits() {
    // R′ has the policy line of the snapshot manifest (H-55), and check 4 uses
    // the limits member of the snapshot.
    let mut snapshot = kat_snapshot();
    snapshot.policy = Some(Policy {
        mode: None,
        ttl_seconds: Some(60),
        offline: Some(true),
    });
    snapshot.limits = Some(Limits {
        max_patch_bytes: Some(1024),
    });
    snapshot.page_revision = snapshot_page_revision(&snapshot);
    let ops = [op("price", "130.00")];
    let mut body = good_body(&snapshot, &ops);
    let verified = verify(&body, &snapshot).unwrap();
    assert_ne!(
        *verified.next_revision(),
        next_page_revision(&kat_snapshot(), &ops, MAX_BLOCK_BYTES).unwrap()
    );
    // JSON white space up to the hint passes; one more byte is check 4.
    body.resize(1024, b' ');
    assert!(verify(&body, &snapshot).is_ok());
    body.push(b' ');
    assert_eq!(
        verify(&body, &snapshot).unwrap_err(),
        DeltaError::Envelope(EnvelopeError::TooLarge)
    );
    // The client limit is the other bound of E.
    body.pop();
    let err = verify_delta(
        &body,
        1023,
        &snapshot,
        Some(URL),
        Some(&kat_nonce()),
        MAX_BLOCK_BYTES,
    )
    .unwrap_err();
    assert_eq!(err.check(), 4);
}

#[test]
fn delta_content_limit_is_the_parameter() {
    let snapshot = kat_snapshot();
    let ops = [op("title", "ab"), op("price", "é")];
    let body = good_body(&snapshot, &ops);
    let with = |max_block_bytes: usize| {
        verify_delta(
            &body,
            MAX_PATCH_BYTES,
            &snapshot,
            Some(URL),
            Some(&kat_nonce()),
            max_block_bytes,
        )
    };
    // "é" is 2 UTF-8 bytes: the limit counts bytes, not characters.
    assert!(with(2).is_ok());
    assert_eq!(
        with(1).unwrap_err(),
        DeltaError::Operations(OperationError::ContentTooLarge { index: 0 })
    );
}

// ---------------------------------------------------------------------------
// Check order: one input with two defects gives the earlier check

/// Defects that an input can have; each one fails exactly one check. One
/// flag per check: the property below tries every combination.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, Default)]
struct Defects {
    /// Check 4: the body is one byte over the limit.
    size: bool,
    /// Check 5: a second `binding` member (JSON rule 2), before any other
    /// defect in text order.
    duplicate: bool,
    /// Check 6: an unknown member at the end of the envelope.
    unknown_member: bool,
    /// Check 9: an operation for a block that the snapshot does not have.
    unknown_block: bool,
    /// Check 10: a content over the block limit.
    content_size: bool,
    /// Check 11: another nonce than the one of the binding.
    other_nonce: bool,
}

impl Defects {
    fn first_check(self) -> Option<u8> {
        [
            (self.size, 4),
            (self.duplicate, 5),
            (self.unknown_member, 6),
            (self.unknown_block, 9),
            (self.content_size, 10),
            (self.other_nonce, 11),
        ]
        .into_iter()
        .find(|(on, _)| *on)
        .map(|(_, check)| check)
    }

    /// Run `verify_delta` on an input with these defects.
    fn run(self) -> Result<VerifiedDelta, DeltaError> {
        let snapshot = kat_snapshot();
        let mut ops = vec![op("price", "130.00")];
        let b = bound(&snapshot, URL, &kat_nonce(), &ops);
        if self.unknown_block {
            ops.push(op("stock", "1"));
        }
        let binding_member = format!(r#""binding":"{b}""#);
        let duplicate = if self.duplicate {
            format!(",{binding_member}")
        } else {
            String::new()
        };
        let unknown = if self.unknown_member {
            r#","next_revision":"x""#
        } else {
            ""
        };
        let body = format!(
            r#"{{{binding_member}{duplicate},"operations":[{}]{unknown}}}"#,
            operations_json(&ops)
        )
        .into_bytes();
        let max_patch_bytes = if self.size {
            body.len() - 1
        } else {
            body.len()
        };
        // "130.00" is 6 bytes.
        let max_block_bytes = if self.content_size { 5 } else { 6 };
        let nonce = if self.other_nonce {
            Nonce([0xFF; 16])
        } else {
            kat_nonce()
        };
        verify_delta(
            &body,
            max_patch_bytes,
            &snapshot,
            Some(URL),
            Some(&nonce),
            max_block_bytes,
        )
    }
}

fn first_check(defects: Defects) -> u8 {
    defects.run().unwrap_err().check()
}

#[test]
fn delta_check_order_no_defect_passes() {
    assert!(Defects::default().run().is_ok());
}

#[test]
fn delta_check_order_4_before_5() {
    let d = Defects {
        size: true,
        duplicate: true,
        ..Defects::default()
    };
    assert_eq!(
        d.run().unwrap_err(),
        DeltaError::Envelope(EnvelopeError::TooLarge)
    );
    // Not JSON at all, and over the limit: still check 4.
    let snapshot = kat_snapshot();
    let err = verify_delta(
        &[0xFF; 9],
        8,
        &snapshot,
        Some(URL),
        Some(&kat_nonce()),
        MAX_BLOCK_BYTES,
    )
    .unwrap_err();
    assert_eq!(err.check(), 4);
    assert_eq!(err.code(), ErrorCode::PatchTooLarge);
}

#[test]
fn delta_check_order_5_before_6() {
    let d = Defects {
        duplicate: true,
        unknown_member: true,
        ..Defects::default()
    };
    match d.run().unwrap_err() {
        DeltaError::Envelope(EnvelopeError::Json(e)) => assert_eq!(e.rule(), JsonRule::Duplicate),
        other => panic!("check 5 expected, got {other:?}"),
    }
}

#[test]
fn delta_check_order_5_and_6_share_one_pass_in_text_order() {
    // Checks 5 and 6 read the body once and stop at the first failure in
    // text order (envelope module). A schema failure before a JSON failure
    // gives check 6; both are PATCH_INVALID, so the code is the code of the
    // table order.
    let snapshot = kat_snapshot();
    let b = bound(&snapshot, URL, &kat_nonce(), &[op("price", "130.00")]);
    let body = format!(
        r#"{{"x":1,"binding":"{b}","binding":"{b}","operations":[{}]}}"#,
        operations_json(&[op("price", "130.00")])
    );
    let err = verify(body.as_bytes(), &snapshot).unwrap_err();
    assert_eq!(err.check(), 6);
    assert_eq!(err.code(), ErrorCode::PatchInvalid);
}

#[test]
fn delta_check_order_6_before_9() {
    let d = Defects {
        unknown_member: true,
        unknown_block: true,
        ..Defects::default()
    };
    match d.run().unwrap_err() {
        DeltaError::Envelope(EnvelopeError::Schema(v)) => {
            assert_eq!(v.keyword(), SchemaKeyword::AdditionalProperties);
        }
        other => panic!("check 6 expected, got {other:?}"),
    }
}

#[test]
fn delta_check_order_9_before_10() {
    // The unknown block is the second operation, the large content the
    // first: check 9 covers the whole list before check 10 (step 3 before
    // step 4).
    let d = Defects {
        unknown_block: true,
        content_size: true,
        ..Defects::default()
    };
    assert_eq!(
        d.run().unwrap_err(),
        DeltaError::Operations(OperationError::UnknownBlock { index: 1 })
    );
}

#[test]
fn delta_check_order_10_before_11() {
    let d = Defects {
        content_size: true,
        other_nonce: true,
        ..Defects::default()
    };
    assert_eq!(
        d.run().unwrap_err(),
        DeltaError::Operations(OperationError::ContentTooLarge { index: 0 })
    );
}

#[test]
fn delta_check_order_11_alone() {
    let d = Defects {
        other_nonce: true,
        ..Defects::default()
    };
    assert_eq!(
        d.run().unwrap_err(),
        DeltaError::Integrity(IntegrityFailure::BindingDiffers)
    );
}

#[test]
fn delta_check_order_missing_inputs_come_last() {
    // A missing URL or nonce is check 11: every earlier check runs first.
    let snapshot = kat_snapshot();
    let b = bound(&snapshot, URL, &kat_nonce(), &[op("price", "130.00")]);
    let body = envelope(&b, &[op("price", "130.00"), op("stock", "1")]);
    let err = verify_delta(
        &body,
        MAX_PATCH_BYTES,
        &snapshot,
        None,
        None,
        MAX_BLOCK_BYTES,
    )
    .unwrap_err();
    assert_eq!(
        err,
        DeltaError::Operations(OperationError::UnknownBlock { index: 1 })
    );
    let err = verify_delta(
        b"{",
        MAX_PATCH_BYTES,
        &snapshot,
        None,
        None,
        MAX_BLOCK_BYTES,
    )
    .unwrap_err();
    assert_eq!(err.check(), 5);
}

proptest! {
    /// Any set of defects gives the check of the first defect in table order.
    #[test]
    fn delta_check_order_prop_first_defect_wins(
        size in any::<bool>(),
        duplicate in any::<bool>(),
        unknown_member in any::<bool>(),
        unknown_block in any::<bool>(),
        content_size in any::<bool>(),
        other_nonce in any::<bool>(),
    ) {
        let d = Defects { size, duplicate, unknown_member, unknown_block, content_size, other_nonce };
        match d.first_check() {
            None => prop_assert!(d.run().is_ok()),
            Some(check) => prop_assert_eq!(first_check(d), check),
        }
    }
}

// ---------------------------------------------------------------------------
// Check 11: inputs of the binding

#[test]
fn delta_missing_input_is_integrity_error() {
    let snapshot = kat_snapshot();
    let body = good_body(&snapshot, &[op("price", "130.00")]);
    let nonce = kat_nonce();
    let run = |url: Option<&str>, nonce: Option<&Nonce>| {
        verify_delta(
            &body,
            MAX_PATCH_BYTES,
            &snapshot,
            url,
            nonce,
            MAX_BLOCK_BYTES,
        )
    };
    assert!(run(Some(URL), Some(&nonce)).is_ok());
    for (url, n, failure) in [
        (None, Some(&nonce), IntegrityFailure::NoFinalUrl),
        (Some(""), Some(&nonce), IntegrityFailure::NoFinalUrl),
        (Some(URL), None, IntegrityFailure::NoNonce),
        (None, None, IntegrityFailure::NoFinalUrl),
        (Some(""), None, IntegrityFailure::NoFinalUrl),
    ] {
        let err = run(url, n).unwrap_err();
        assert_eq!(err, DeltaError::Integrity(failure), "{url:?}");
        assert_eq!(err.code(), ErrorCode::IntegrityError);
        assert_eq!(err.check(), 11);
    }
    // A URL with a line feed is no canonical URL: no binding matches
    // (DL-150 (3)), the same code.
    let err = run(Some("https://shop.example.com/\ncatalog"), Some(&nonce)).unwrap_err();
    assert_eq!(err, DeltaError::Integrity(IntegrityFailure::BindingDiffers));
}

#[test]
fn delta_binding_binds_url_base_nonce_and_next_revision() {
    // A delta for another URL, base, request, template or policy line, or
    // with other content, fails check 11 (manifest-patch section 2.4, step 7).
    let snapshot = kat_snapshot();
    let ops = [op("price", "130.00")];
    let body = good_body(&snapshot, &ops);
    let differs = DeltaError::Integrity(IntegrityFailure::BindingDiffers);
    let nonce = kat_nonce();
    let with_url = |url: &str| {
        verify_delta(
            &body,
            MAX_PATCH_BYTES,
            &snapshot,
            Some(url),
            Some(&nonce),
            MAX_BLOCK_BYTES,
        )
    };
    assert_eq!(with_url("https://shop.example.com/catalog/"), Err(differs));
    assert_eq!(with_url("https://SHOP.example.com/catalog"), Err(differs));
    let mut other_nonce = nonce;
    other_nonce.0[15] ^= 1;
    assert_eq!(
        verify_delta(
            &body,
            MAX_PATCH_BYTES,
            &snapshot,
            Some(URL),
            Some(&other_nonce),
            MAX_BLOCK_BYTES
        ),
        Err(differs)
    );
    let mut other_base = snapshot.clone();
    other_base.page_revision =
        PageRevision::parse("r1.psbsc6whgs8ooESisNWp6E3PrQfkg4D69WmXAo12pbI").unwrap();
    assert_eq!(verify(&body, &other_base), Err(differs));
    let mut other_template = snapshot.clone();
    other_template.template_revision = TemplateRevision::from_digest(sha256(b"other"));
    assert_eq!(verify(&body, &other_template), Err(differs));
    let mut other_policy = snapshot.clone();
    other_policy.policy = Some(Policy::default());
    assert_eq!(verify(&body, &other_policy), Err(differs));
    let mut other_block = snapshot.clone();
    other_block.blocks[0].sha256 = sha256(b"another title");
    assert_eq!(verify(&body, &other_block), Err(differs));
    // The hash of the replaced block does not take part (H-44): its new hash
    // replaces it.
    let mut replaced = snapshot.clone();
    replaced.blocks[1].sha256 = sha256(b"another price");
    assert!(verify(&body, &replaced).is_ok());
    // Content that the server did not hash.
    let b = bound(&snapshot, URL, &nonce, &ops);
    let other_content = envelope(&b, &[op("price", "131.00")]);
    assert_eq!(verify(&other_content, &snapshot), Err(differs));
}

// ---------------------------------------------------------------------------
// Error texts, codes and sources

const MARKER: &str = "zq-secret-7";

#[test]
fn delta_error_texts_hold_no_input() {
    let snapshot = kat_snapshot();
    let nonce = Nonce([0xA5; 16]);
    let url = format!("https://{MARKER}.example/{MARKER}?{MARKER}");
    let marked = [op("price", MARKER)];
    let good = envelope(&bound(&snapshot, &url, &nonce, &marked), &marked);
    let wrong = envelope(
        &bound(&snapshot, &url, &kat_nonce(), &marked),
        &[op("price", MARKER)],
    );
    let inputs: Vec<(Vec<u8>, usize, usize, Option<&str>)> = vec![
        // 4
        (good.clone(), 8, MAX_BLOCK_BYTES, Some(&url)),
        // 5
        (
            format!(r#"{{"binding":"{MARKER}\ud800"}}"#).into_bytes(),
            MAX_PATCH_BYTES,
            MAX_BLOCK_BYTES,
            Some(&url),
        ),
        // 6
        (
            format!(r#"{{"{MARKER}":"{MARKER}"}}"#).into_bytes(),
            MAX_PATCH_BYTES,
            MAX_BLOCK_BYTES,
            Some(&url),
        ),
        // 9
        (
            envelope(
                &bound(&snapshot, &url, &nonce, &marked),
                &[op(MARKER, MARKER)],
            ),
            MAX_PATCH_BYTES,
            MAX_BLOCK_BYTES,
            Some(&url),
        ),
        // 10
        (good.clone(), MAX_PATCH_BYTES, 3, Some(&url)),
        // 11: no URL, another binding
        (good, MAX_PATCH_BYTES, MAX_BLOCK_BYTES, None),
        (wrong, MAX_PATCH_BYTES, MAX_BLOCK_BYTES, Some(&url)),
    ];
    let nonce_texts = [
        b64u_encode(&nonce.0),
        "paWlpaWlpaWlpaWlpaWlpQ".to_owned(),
        "165, 165".to_owned(),
    ];
    let mut checks = Vec::new();
    for (body, max_patch, max_block, u) in &inputs {
        let err =
            verify_delta(body, *max_patch, &snapshot, *u, Some(&nonce), *max_block).unwrap_err();
        checks.push(err.check());
        let shown = format!("{err} | {err:?}");
        assert!(!shown.contains(MARKER), "{shown}");
        assert!(!shown.contains("price"), "{shown}");
        assert!(!shown.contains("example"), "{shown}");
        for n in &nonce_texts {
            assert!(!shown.contains(n.as_str()), "{shown}");
        }
        if let Some(source) = core::error::Error::source(&err) {
            assert!(!source.to_string().contains(MARKER));
        }
    }
    assert_eq!(checks, [4, 5, 6, 9, 10, 11, 11]);
    // The verified value writes no content and no URL either.
    let verified = verify_delta(
        &envelope(&bound(&snapshot, &url, &nonce, &marked), &marked),
        MAX_PATCH_BYTES,
        &snapshot,
        Some(&url),
        Some(&nonce),
        MAX_BLOCK_BYTES,
    )
    .unwrap();
    let shown = format!("{verified:?}");
    assert!(!shown.contains(MARKER), "{shown}");
    assert!(!shown.contains("example"), "{shown}");
}

#[test]
fn delta_error_texts_name_the_check_and_code() {
    assert_eq!(
        DeltaError::Envelope(EnvelopeError::TooLarge).to_string(),
        "delta check 4: PATCH_TOO_LARGE: envelope larger than min(maxPatchBytes, max_patch_bytes)"
    );
    assert_eq!(
        DeltaError::Operations(OperationError::NotDocumentOrder { index: 2 }).to_string(),
        "delta check 9: PATCH_INVALID: operation 2 is not in document order"
    );
    assert_eq!(
        DeltaError::Operations(OperationError::ContentTooLarge { index: 1 }).to_string(),
        "delta check 10: PATCH_TOO_LARGE: operation 1 has content over maxBlockBytes"
    );
    for (failure, reason) in [
        (
            IntegrityFailure::NoFinalUrl,
            "no canonical final URL for the binding",
        ),
        (
            IntegrityFailure::NoNonce,
            "no request nonce for the binding",
        ),
        (
            IntegrityFailure::BindingDiffers,
            "binding differs from the computed binding",
        ),
    ] {
        assert_eq!(failure.reason(), reason);
        assert_eq!(
            DeltaError::Integrity(failure).to_string(),
            format!("delta check 11: INTEGRITY_ERROR: {reason}")
        );
    }
}

#[test]
fn delta_error_codes_checks_and_sources() {
    let snapshot = kat_snapshot();
    let json = verify(b"{", &snapshot).unwrap_err();
    let schema = verify(b"[]", &snapshot).unwrap_err();
    let cases = [
        (
            DeltaError::Envelope(EnvelopeError::TooLarge),
            ErrorCode::PatchTooLarge,
            4,
            true,
        ),
        (json, ErrorCode::PatchInvalid, 5, true),
        (schema, ErrorCode::PatchInvalid, 6, true),
        (
            DeltaError::Operations(OperationError::NoOperations),
            ErrorCode::PatchInvalid,
            9,
            true,
        ),
        (
            DeltaError::Operations(OperationError::UnknownBlock { index: 0 }),
            ErrorCode::PatchInvalid,
            9,
            true,
        ),
        (
            DeltaError::Operations(OperationError::DuplicateBlock { index: 1 }),
            ErrorCode::PatchInvalid,
            9,
            true,
        ),
        (
            DeltaError::Operations(OperationError::ContentTooLarge { index: 0 }),
            ErrorCode::PatchTooLarge,
            10,
            true,
        ),
        (
            DeltaError::Integrity(IntegrityFailure::BindingDiffers),
            ErrorCode::IntegrityError,
            11,
            false,
        ),
    ];
    for (err, code, check, has_source) in cases {
        assert_eq!(err.code(), code, "{err}");
        assert_eq!(err.check(), check, "{err}");
        let source = core::error::Error::source(&err);
        assert_eq!(source.is_some(), has_source, "{err}");
        if let Some(source) = source {
            // The source is the inner error, whose text is in the outer text.
            assert!(err.to_string().ends_with(&source.to_string()), "{err}");
        }
    }
    // From conversions keep the inner error.
    assert_eq!(
        DeltaError::from(EnvelopeError::TooLarge),
        DeltaError::Envelope(EnvelopeError::TooLarge)
    );
    assert_eq!(
        DeltaError::from(OperationError::NoOperations),
        DeltaError::Operations(OperationError::NoOperations)
    );
}

// ---------------------------------------------------------------------------
// Any body

/// The codes that `verify_delta` can give (diff-engine section 3, checks 4
/// to 11).
const CODES: [ErrorCode; 3] = [
    ErrorCode::PatchTooLarge,
    ErrorCode::PatchInvalid,
    ErrorCode::IntegrityError,
];

/// A body that is often a valid envelope: random operations on known and
/// unknown blocks, a random or the right binding, JSON noise.
fn near_envelope() -> impl Strategy<Value = Vec<u8>> {
    let ids = prop_oneof![
        Just("title"),
        Just("price"),
        Just("cart"),
        Just("stock"),
        Just("a_b-1"),
    ];
    let ops = prop::collection::vec((ids, "[ -~é😀]{0,12}"), 0..5);
    (
        ops,
        any::<bool>(),
        prop::array::uniform32(any::<u8>()),
        0_u8..4,
    )
        .prop_map(|(ops, right, digest, noise)| {
            let ops: Vec<PatchOp> = ops.into_iter().map(|(id, c)| op(id, &c)).collect();
            let snapshot = kat_snapshot();
            let b = match next_page_revision(&snapshot, &ops, MAX_BLOCK_BYTES) {
                Ok(next) if right => binding(URL, &snapshot.page_revision, &kat_nonce(), &next),
                _ => Binding::from_digest(Sha256Digest::from_b64u(&b64u_encode(&digest)).unwrap()),
            };
            let mut body = envelope(&b, &ops);
            match noise {
                1 => body.insert(0, b' '),
                2 => {
                    body.pop();
                }
                3 => body.extend_from_slice(b",\"x\":1}"),
                _ => {}
            }
            body
        })
}

proptest! {
    /// Any bytes as the body never panic and give Ok or one of the three
    /// codes; check 4 holds for every limit.
    #[test]
    fn delta_prop_any_body(
        body in prop::collection::vec(any::<u8>(), 0..512),
        max_patch_bytes in 0_usize..600,
        max_block_bytes in 0_usize..64,
        url in prop::option::of("[ -~]{0,40}"),
        nonce in prop::option::of(prop::array::uniform16(any::<u8>())),
    ) {
        let snapshot = kat_snapshot();
        let nonce = nonce.map(Nonce);
        let result = verify_delta(
            &body, max_patch_bytes, &snapshot, url.as_deref(), nonce.as_ref(), max_block_bytes,
        );
        match result {
            Ok(_) => prop_assert!(body.len() <= max_patch_bytes),
            Err(e) => {
                prop_assert!(CODES.contains(&e.code()), "{e}");
                prop_assert_eq!(e.check() == 4, body.len() > max_patch_bytes);
            }
        }
    }

    /// Bodies near a valid envelope reach checks 9 to 11; a body passes
    /// exactly when its operations apply and its binding is the right one.
    #[test]
    fn delta_prop_near_envelope(body in near_envelope()) {
        let snapshot = kat_snapshot();
        match verify(&body, &snapshot) {
            Ok(verified) => {
                let ops = &verified.envelope().operations;
                let next = next_page_revision(&snapshot, ops, MAX_BLOCK_BYTES).unwrap();
                prop_assert_eq!(verified.next_revision(), &next);
                prop_assert_eq!(
                    verified.envelope().binding,
                    binding(URL, &snapshot.page_revision, &kat_nonce(), &next)
                );
                prop_assert_eq!(verified.patched().len(), ops.len());
            }
            Err(e) => prop_assert!(CODES.contains(&e.code()), "{e}"),
        }
    }

    /// Operations in document order with the server's binding always pass,
    /// and R′ is the one of `next_page_revision`.
    #[test]
    fn delta_prop_bound_envelope_passes(
        picks in prop::array::uniform3(prop::option::of("[ -~é]{0,16}")),
    ) {
        let snapshot = kat_snapshot();
        let ops: Vec<PatchOp> = snapshot
            .blocks
            .iter()
            .zip(picks)
            .filter_map(|(b, pick)| pick.map(|c| PatchOp { block_id: b.id.clone(), content: c }))
            .collect();
        prop_assume!(!ops.is_empty());
        let verified = verify(&good_body(&snapshot, &ops), &snapshot).unwrap();
        prop_assert_eq!(
            verified.next_revision(),
            &next_page_revision(&snapshot, &ops, MAX_BLOCK_BYTES).unwrap()
        );
    }
}

#[test]
fn delta_block_limit_of_caller_cannot_raise_the_rqp1_limit() {
    // maxBlockBytes is fixed by rqp/1 (DL-155): a caller that passes a larger
    // value (here usize::MAX) still gets PATCH_TOO_LARGE at check 10. The
    // content is two-byte characters, so it passes the schema maxLength in
    // code points (check 6) and reaches the byte check.
    let snapshot = kat_snapshot();
    let big = op("price", &"\u{e9}".repeat(MAX_BLOCK_BYTES / 2 + 1));
    let any_binding = bound(&snapshot, URL, &kat_nonce(), &[op("price", "1")]);
    let body = envelope(&any_binding, std::slice::from_ref(&big));
    let err = verify_delta(
        &body,
        2 * MAX_BLOCK_BYTES,
        &snapshot,
        Some(URL),
        Some(&kat_nonce()),
        usize::MAX,
    )
    .unwrap_err();
    assert_eq!(
        err,
        DeltaError::Operations(OperationError::ContentTooLarge { index: 0 })
    );
    assert_eq!(err.code(), ErrorCode::PatchTooLarge);
    // At the limit the content passes check 10 and fails only the binding.
    let at_limit = op("price", &"\u{e9}".repeat(MAX_BLOCK_BYTES / 2));
    let body = envelope(&any_binding, std::slice::from_ref(&at_limit));
    let err = verify_delta(
        &body,
        2 * MAX_BLOCK_BYTES,
        &snapshot,
        Some(URL),
        Some(&kat_nonce()),
        usize::MAX,
    )
    .unwrap_err();
    assert_eq!(err.code(), ErrorCode::IntegrityError);
}
