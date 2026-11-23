// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! `rqp::ValidatedPatch` (diff engine section 4): what a validated patch
//! gives, and that its texts have no content.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

mod rqp_common;

use rivqen_diff::rqp::{DeltaRequest, PatchLimits, validate};
use rivqen_model::limits::ResourceLimits;
use rivqen_proto::types::{BlockFormat, Limits, Mode, Nonce, Policy};
use rivqen_template::limits::TemplateLimits;
use rivqen_template::rqp::split_rqp;

use rqp_common::{NONCE, PAGE, Snap, URL, delta, request, run, server_delta};

fn next_page() -> String {
    PAGE.replace(">Shop<", ">A longer title<")
        .replace("{\"items\":3}", "[]")
}

#[test]
fn rqp_validated_gives_the_next_document() {
    let base = Snap::new(PAGE);
    let next = Snap::new(&next_page());
    let patch = run(&delta(&base, &next), &base).unwrap();
    assert_eq!(patch.document(), next_page().as_bytes());
    assert_eq!(patch.analysis(), &next.analysis);
    assert_eq!(patch.page_revision(), &next.revision());
    assert_eq!(patch.base_revision(), &base.revision());
    assert_eq!(patch.snapshot_meta(), &next.meta);
}

#[test]
fn rqp_validated_replacements_in_document_order() {
    let base = Snap::new(PAGE);
    let next = Snap::new(&next_page());
    let patch = run(&delta(&base, &next), &base).unwrap();
    let got: Vec<(usize, String, BlockFormat, &str)> = patch
        .replacements()
        .iter()
        .map(|r| (r.block(), r.id().to_string(), r.format(), r.content()))
        .collect();
    assert_eq!(
        got,
        [
            (0, "title".to_owned(), BlockFormat::Html, "A longer title"),
            (2, "cart".to_owned(), BlockFormat::Json, "[]"),
        ]
    );
    // Each content is the content of its block in the patched document.
    for replacement in patch.replacements() {
        let block = &patch.analysis().blocks[replacement.block()];
        assert_eq!(
            &patch.document()[block.start..block.end],
            replacement.content().as_bytes()
        );
        assert_eq!(&block.descriptor.id, replacement.id());
    }
}

#[test]
fn rqp_validated_decoded_content() {
    // The content is the decoded JSON string: `<` is `<`.
    let base = Snap::new(PAGE);
    let next = Snap::new(&PAGE.replace(">120.00<", "><b>1</b><"));
    let body = String::from_utf8(delta(&base, &next))
        .unwrap()
        .replace("\"<b>", "\"\\u003cb>");
    assert!(body.contains("\\u003cb>"));
    let patch = run(body.as_bytes(), &base).unwrap();
    assert_eq!(patch.replacements()[0].content(), "<b>1</b>");
    assert_eq!(patch.document(), next.original.as_slice());
}

#[test]
fn rqp_validated_every_block_form() {
    // The H-7 form: an operation for every block, also for unchanged ones.
    let base = Snap::new(PAGE);
    let next = Snap::new(&PAGE.replace(">120.00<", ">130.00<"));
    let body = server_delta(&base, &next, &[0, 1, 2], URL, &NONCE);
    let patch = run(&body, &base).unwrap();
    assert_eq!(patch.replacements().len(), 3);
    assert_eq!(patch.document(), next.original.as_slice());
    assert_eq!(patch.snapshot_meta(), &next.meta);
}

#[test]
fn rqp_validated_keeps_policy_and_limits() {
    let policy = Some(Policy {
        mode: Some(Mode::Public),
        ttl_seconds: Some(60),
        offline: Some(true),
    });
    let limits = Some(Limits {
        max_patch_bytes: Some(4096),
    });
    let base = Snap::with_members(PAGE, policy, limits);
    let next = Snap::with_members(&next_page(), policy, limits);
    // R′ has the policy line of the snapshot.
    assert_ne!(next.revision(), Snap::new(&next_page()).revision());
    let patch = run(&delta(&base, &next), &base).unwrap();
    assert_eq!(patch.snapshot_meta(), &next.meta);
    assert_eq!(patch.snapshot_meta().policy, policy);
    assert_eq!(patch.snapshot_meta().limits, limits);
}

#[test]
fn rqp_validated_manifest_limit_lowers_patch_limit() {
    // The `limits` member of the snapshot is smaller than the body: check 4.
    let limits = Some(Limits {
        max_patch_bytes: Some(1024),
    });
    let base = Snap::with_members(PAGE, None, limits);
    let next = Snap::with_members(
        &PAGE.replace(">120.00<", &format!(">{}<", "x".repeat(1100))),
        None,
        limits,
    );
    let error = run(&delta(&base, &next), &base).unwrap_err();
    assert_eq!(error.check(), 4);
}

#[test]
fn rqp_validated_is_the_snapshot_of_the_next_patch() {
    // A ValidatedPatch gives every value that the next delta needs.
    let first = Snap::new(PAGE);
    let second = Snap::new(&next_page());
    let third = Snap::new(&next_page().replace(">120.00<", ">7.00<"));
    let patch = run(&delta(&first, &second), &first).unwrap();
    let stored = Snap::from_patch(&patch);
    assert_eq!(stored, second);
    let again = run(&delta(&stored, &third), &stored).unwrap();
    assert_eq!(again.document(), third.original.as_slice());
    let (replacements, document, analysis, meta) = again.into_parts();
    assert_eq!(replacements.len(), 1);
    assert_eq!(document, third.original);
    assert_eq!(analysis, third.analysis);
    assert_eq!(meta, third.meta);
}

#[test]
fn rqp_validated_limits() {
    let from_config = PatchLimits::try_from(&ResourceLimits::default()).unwrap();
    assert_eq!(from_config.max_patch_bytes(), 256 * 1024);
    assert_eq!(from_config.template(), &TemplateLimits::default());
    let small = TemplateLimits::new(4096, 8, 512, 16, 32);
    let low = PatchLimits::new(1024, small);
    assert_eq!(low.max_patch_bytes(), 1024);
    assert_eq!(low.template(), &small);
    let high = PatchLimits::new(5 * 1024 * 1024 + 1, TemplateLimits::default());
    assert_eq!(high.max_patch_bytes(), 5 * 1024 * 1024);
    let config = ResourceLimits {
        max_patch_bytes: u64::MAX,
        ..ResourceLimits::default()
    };
    let capped = PatchLimits::try_from(&config).unwrap();
    assert_eq!(capped.max_patch_bytes(), 5 * 1024 * 1024);
}

#[test]
fn rqp_validated_debug_has_no_content() {
    let page = PAGE
        .replace("\"price\"", "\"zqblockid\"")
        .replace(">120.00<", ">zqold<");
    let url = "https://zqhost.example/zqpath";
    let nonce = Nonce([0xA5; 16]);
    let nonce_text = rivqen_integrity::b64u_encode(&nonce.0);
    let base = Snap::new(&page);
    let next = Snap::new(&page.replace(">zqold<", ">zqnew<"));
    let body = server_delta(&base, &next, &[1], url, &nonce);
    let req = request(url, &nonce);
    let limits = PatchLimits::try_from(&ResourceLimits::default()).unwrap();
    let patch = validate(&body, &base.stored(), &req, &limits).unwrap();
    let texts = [
        format!("{patch:?}"),
        format!("{:?}", patch.replacements()),
        format!("{:?}", base.stored()),
        format!("{req:?}"),
        format!(
            "{:?}",
            DeltaRequest {
                final_url: None,
                nonce: None
            }
        ),
    ];
    for text in &texts {
        for secret in ["zq", "example", nonce_text.as_str()] {
            assert!(!text.contains(secret), "{text} has {secret}");
        }
    }
    assert!(texts[0].contains("document_len"), "{}", texts[0]);
    assert!(texts[1].contains("content_len"), "{}", texts[1]);
    assert!(texts[2].contains("original_len"), "{}", texts[2]);
    assert!(texts[3].contains("<redacted>"), "{}", texts[3]);
}

#[test]
fn rqp_validated_analysis_is_of_the_document() {
    // The analysis of the patch is the analysis of its own bytes, not of the
    // stored original: the block offsets moved.
    let base = Snap::new(PAGE);
    let next = Snap::new(&next_page());
    let patch = run(&delta(&base, &next), &base).unwrap();
    let again = split_rqp(patch.document(), &TemplateLimits::default()).unwrap();
    assert_eq!(patch.analysis(), &again);
    assert_ne!(
        patch.analysis().blocks[1].start,
        base.analysis.blocks[1].start
    );
}
