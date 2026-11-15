// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! `rqp::patch`: `render_rqp` and `check_patch` (manifest-patch section 2.4,
//! steps 8 and 9, and section 4.1), and `compute::page_revision_of`
//! (DL-175).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::format_push_string,
    clippy::single_match_else
)]

use proptest::prelude::*;
use rivqen_integrity::{PageRevision, TemplateRevision, sha256};
use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_proto::compute::{
    binding, next_page_revision, page_revision, page_revision_of, snapshot_page_revision,
};
use rivqen_proto::delta::{VerifiedDelta, verify_delta};
use rivqen_proto::types::{
    BlockDescriptor, BlockId, Limits, MAX_BLOCK_BYTES, MAX_PATCH_BYTES, Mode, Nonce, PatchOp,
    Policy, SnapshotMeta,
};
use rivqen_template::limits::TemplateLimits;
use rivqen_template::rqp::patch::{
    PatchError, PatchedDocument, RenderRqpError, Replacement, check_patch, render_rqp,
};
use rivqen_template::rqp::{RqpDocument, split_rqp};

const URL: &str = "https://shop.example.com/catalog";
const NONCE: Nonce = Nonce([7; 16]);

/// title (html), price (html), cart (json).
const PAGE: &str = "<!DOCTYPE html><html><head>\
<title data-rq-block=\"title\">Shop</title></head><body>\
<p data-rq-block=\"price\">120.00</p>\
<script type=\"application/json\" data-rq-block=\"cart\">{\"items\":3}</script>\
</body></html>";

fn limits() -> TemplateLimits {
    TemplateLimits::default()
}

fn analyze(document: &str) -> RqpDocument {
    split_rqp(document.as_bytes(), &limits()).unwrap()
}

fn meta_of(analysis: &RqpDocument, policy: Option<Policy>, limits: Option<Limits>) -> SnapshotMeta {
    let mut meta = SnapshotMeta {
        template_revision: analysis.template_revision,
        page_revision: PageRevision::parse("r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8")
            .unwrap(),
        blocks: analysis
            .blocks
            .iter()
            .map(|b| b.descriptor.clone())
            .collect(),
        policy,
        limits,
    };
    meta.page_revision = snapshot_page_revision(&meta);
    meta
}

fn op(id: &str, content: &str) -> PatchOp {
    PatchOp {
        block_id: BlockId::parse(id).unwrap(),
        content: content.to_owned(),
    }
}

/// A delta that `verify_delta` accepts for `meta`.
fn verified(meta: &SnapshotMeta, ops: &[PatchOp]) -> VerifiedDelta {
    let next = next_page_revision(meta, ops, MAX_BLOCK_BYTES).unwrap();
    let bound = binding(URL, &meta.page_revision, &NONCE, &next);
    let operations: Vec<String> = ops
        .iter()
        .map(|o| {
            format!(
                r#"{{"block_id":"{}","content":{}}}"#,
                o.block_id,
                serde_json::to_string(&o.content).unwrap()
            )
        })
        .collect();
    let body = format!(
        r#"{{"binding":"{bound}","operations":[{}]}}"#,
        operations.join(",")
    );
    verify_delta(
        body.as_bytes(),
        MAX_PATCH_BYTES,
        meta,
        Some(URL),
        Some(&NONCE),
        MAX_BLOCK_BYTES,
    )
    .unwrap()
}

fn patch(document: &str, ops: &[PatchOp]) -> Result<PatchedDocument, PatchError> {
    let analysis = analyze(document);
    let meta = meta_of(&analysis, None, None);
    let delta = verified(&meta, ops);
    check_patch(document.as_bytes(), &analysis, &meta, &delta, &limits())
}

fn code_of<T: std::fmt::Debug>(result: Result<T, PatchError>) -> (PatchError, ErrorCode) {
    let error = result.unwrap_err();
    (error, error.code())
}

fn small(max_document_bytes: usize) -> TemplateLimits {
    TemplateLimits::new(max_document_bytes, 256, 1 << 20, 128, 256)
}

// ---------------------------------------------------------------------------
// render_rqp

#[test]
fn patch_render_replaces_content_ranges() {
    let analysis = analyze(PAGE);
    let out = render_rqp(
        PAGE.as_bytes(),
        &analysis,
        &[
            Replacement {
                block: 0,
                content: b"A longer title",
            },
            Replacement {
                block: 2,
                content: b"[]",
            },
        ],
        &limits(),
    )
    .unwrap();
    let expected = PAGE
        .replace(">Shop<", ">A longer title<")
        .replace("{\"items\":3}", "[]");
    assert_eq!(String::from_utf8(out).unwrap(), expected);
}

#[test]
fn patch_render_without_replacements_is_the_original() {
    let analysis = analyze(PAGE);
    let out = render_rqp(PAGE.as_bytes(), &analysis, &[], &limits()).unwrap();
    assert_eq!(out, PAGE.as_bytes());
}

#[test]
fn patch_render_empty_content_removes_the_block_text() {
    let analysis = analyze(PAGE);
    let out = render_rqp(
        PAGE.as_bytes(),
        &analysis,
        &[Replacement {
            block: 1,
            content: b"",
        }],
        &limits(),
    )
    .unwrap();
    assert_eq!(
        String::from_utf8(out).unwrap(),
        PAGE.replace(">120.00<", "><")
    );
}

#[test]
fn patch_render_fills_an_empty_block() {
    // A block that is empty in the stored document (an empty cart) has a
    // content range of zero bytes; the replacement goes to that offset.
    let page = PAGE.replace(">120.00<", "><");
    let analysis = analyze(&page);
    assert_eq!(analysis.blocks[1].start, analysis.blocks[1].end);
    let out = render_rqp(
        page.as_bytes(),
        &analysis,
        &[Replacement {
            block: 1,
            content: b"99.50",
        }],
        &limits(),
    )
    .unwrap();
    assert_eq!(
        String::from_utf8(out).unwrap(),
        PAGE.replace("120.00", "99.50")
    );
}

#[test]
fn patch_render_rejects_blocks_not_in_increasing_order() {
    let analysis = analyze(PAGE);
    let r = |block| Replacement {
        block,
        content: b"x",
    };
    for list in [vec![r(1), r(0)], vec![r(1), r(1)]] {
        assert_eq!(
            render_rqp(PAGE.as_bytes(), &analysis, &list, &limits()),
            Err(RenderRqpError::BadReplacement)
        );
    }
}

#[test]
fn patch_render_rejects_a_block_without_a_block() {
    let analysis = analyze(PAGE);
    assert_eq!(
        render_rqp(
            PAGE.as_bytes(),
            &analysis,
            &[Replacement {
                block: 3,
                content: b"x"
            }],
            &limits()
        ),
        Err(RenderRqpError::BadReplacement)
    );
}

#[test]
fn patch_render_rejects_a_range_outside_the_original() {
    let analysis = analyze(PAGE);
    let short = &PAGE.as_bytes()[..PAGE.len() - 60];
    assert_eq!(
        render_rqp(
            short,
            &analysis,
            &[Replacement {
                block: 2,
                content: b"x"
            }],
            &limits()
        ),
        Err(RenderRqpError::BadReplacement)
    );
    // The range check is before the size check: a range outside the
    // original with a content over the size limit is a bad replacement
    // (the delta does not belong to this document), not a large patch.
    let large = vec![b'x'; PAGE.len() * 2];
    assert_eq!(
        render_rqp(
            short,
            &analysis,
            &[Replacement {
                block: 2,
                content: &large
            }],
            &small(PAGE.len())
        ),
        Err(RenderRqpError::BadReplacement)
    );
}

#[test]
fn patch_render_size_limit_passes_at_the_limit_and_fails_one_over() {
    let analysis = analyze(PAGE);
    let growth = 10;
    let size = PAGE.len() + growth;
    let content = vec![b'a'; "Shop".len() + growth];
    let r = [Replacement {
        block: 0,
        content: &content,
    }];
    assert_eq!(
        render_rqp(PAGE.as_bytes(), &analysis, &r, &small(size))
            .unwrap()
            .len(),
        size
    );
    assert_eq!(
        render_rqp(PAGE.as_bytes(), &analysis, &r, &small(size - 1)),
        Err(RenderRqpError::TooLarge)
    );
}

#[test]
fn patch_render_size_limit_is_at_most_the_markup_input_limit() {
    // A configured `max_document_bytes` over 5 MiB does not raise the limit
    // of an RQP document (M-01): the size check gives TooLarge, not a
    // markup error of the analysis.
    const M01: usize = 5 * 1024 * 1024;
    let filler = "<!--".len() + "-->".len();
    let pad = "x".repeat(M01 - PAGE.len() - filler);
    let page = PAGE.replace("</body>", &format!("<!--{pad}--></body>"));
    assert_eq!(page.len(), M01);
    let big = small(8 * 1024 * 1024);
    let analysis = split_rqp(page.as_bytes(), &big).unwrap();
    let same = [Replacement {
        block: 0,
        content: b"Shop",
    }];
    assert_eq!(
        render_rqp(page.as_bytes(), &analysis, &same, &big)
            .unwrap()
            .len(),
        M01
    );
    let longer = [Replacement {
        block: 0,
        content: b"Shop!",
    }];
    assert_eq!(
        render_rqp(page.as_bytes(), &analysis, &longer, &big),
        Err(RenderRqpError::TooLarge)
    );
}

#[test]
fn patch_render_shrinking_patch_fits_a_limit_below_the_original() {
    let analysis = analyze(PAGE);
    let size = PAGE.len() - 5;
    let out = render_rqp(
        PAGE.as_bytes(),
        &analysis,
        &[Replacement {
            block: 1,
            content: b"1",
        }],
        &small(size),
    )
    .unwrap();
    assert_eq!(out.len(), PAGE.len() - 5);
}

#[test]
fn patch_render_errors_have_the_catalog_codes() {
    assert_eq!(RenderRqpError::TooLarge.code(), ErrorCode::PatchTooLarge);
    assert_eq!(
        RenderRqpError::BadReplacement.code(),
        ErrorCode::PatchInvalid
    );
    // Each reason is a text of its own: a reason goes to the log record.
    let too_large = RenderRqpError::TooLarge;
    let bad = RenderRqpError::BadReplacement;
    assert_ne!(too_large.reason(), bad.reason());
    for error in [too_large, bad] {
        assert!(error.reason().contains(' '));
        assert_eq!(error.to_string(), error.reason());
    }
}

// ---------------------------------------------------------------------------
// check_patch: accept path

#[test]
fn patch_check_accepts_json_patch() {
    let analysis = analyze(PAGE);
    let meta = meta_of(&analysis, None, None);
    let delta = verified(&meta, &[op("cart", "{\"items\":4}")]);
    let done = check_patch(PAGE.as_bytes(), &analysis, &meta, &delta, &limits()).unwrap();
    let expected = PAGE.replace("{\"items\":3}", "{\"items\":4}");
    assert_eq!(done.document(), expected.as_bytes());
    assert_eq!(*done.analysis(), analyze(&expected));
    assert_eq!(done.page_revision(), delta.next_revision());
    let (document, parts_analysis, revision) = done.clone().into_parts();
    assert_eq!(document, expected.as_bytes());
    assert_eq!(&parts_analysis, done.analysis());
    assert_eq!(&revision, delta.next_revision());
}

#[test]
fn patch_check_uses_operation_content() {
    // The bytes are the UTF-8 bytes of the operation content, not the JSON
    // text of the envelope.
    let done = patch(PAGE, &[op("cart", "{\"name\":\"\u{00e9}\\\"\"}")]).unwrap();
    let text = std::str::from_utf8(done.document()).unwrap();
    assert!(text.contains("{\"name\":\"\u{00e9}\\\"\"}</script>"));
}

#[test]
fn patch_check_applies_each_operation_to_its_own_block() {
    let page = "<!DOCTYPE html><html><head><title>t</title></head><body>\
<script type=\"application/json\" data-rq-block=\"a\">1</script>\
<script type=\"application/json\" data-rq-block=\"b\">2</script>\
</body></html>";
    let done = patch(page, &[op("b", "22")]).unwrap();
    assert_eq!(
        std::str::from_utf8(done.document()).unwrap(),
        page.replace(">2<", ">22<")
    );
    let done = patch(page, &[op("a", "11"), op("b", "22")]).unwrap();
    assert_eq!(
        std::str::from_utf8(done.document()).unwrap(),
        page.replace(">1<", ">11<").replace(">2<", ">22<")
    );
}

// ---------------------------------------------------------------------------
// check_patch: order and codes

#[test]
fn patch_check_order_snapshot_first() {
    let analysis = analyze(PAGE);
    let meta = meta_of(&analysis, None, None);
    let delta = verified(&meta, &[op("cart", "1")]);
    // Another template revision.
    let mut other = analysis.clone();
    other.template_revision =
        TemplateRevision::parse("t1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM").unwrap();
    assert_eq!(
        check_patch(PAGE.as_bytes(), &other, &meta, &delta, &limits()).unwrap_err(),
        PatchError::Snapshot
    );
    // Other descriptors (another document of the same shape).
    let moved = PAGE.replace("Shop", "Shoe");
    let stored_other = analyze(&moved);
    assert_eq!(
        check_patch(moved.as_bytes(), &stored_other, &meta, &delta, &limits()).unwrap_err(),
        PatchError::Snapshot
    );
    // Fewer blocks; and the Snapshot check runs before the size check.
    let mut fewer = analysis.clone();
    fewer.blocks.pop();
    assert_eq!(
        check_patch(PAGE.as_bytes(), &fewer, &meta, &delta, &limits()).unwrap_err(),
        PatchError::Snapshot
    );
    assert_eq!(
        check_patch(PAGE.as_bytes(), &fewer, &meta, &delta, &small(10)).unwrap_err(),
        PatchError::Snapshot
    );
}

#[test]
fn patch_check_order_size_before_markup() {
    // The content is also invalid markup, but the size check fails first.
    let analysis = analyze(PAGE);
    let meta = meta_of(&analysis, None, None);
    let delta = verified(&meta, &[op("cart", "</script><script>")]);
    assert_eq!(
        check_patch(
            PAGE.as_bytes(),
            &analysis,
            &meta,
            &delta,
            &small(PAGE.len())
        )
        .unwrap_err(),
        PatchError::TooLarge
    );
}

#[test]
fn patch_check_codes() {
    let analysis = analyze(PAGE);
    let meta = meta_of(&analysis, None, None);
    // PATCH_TOO_LARGE
    let delta = verified(&meta, &[op("cart", "[1,2,3,4,5,6,7,8,9]")]);
    let (error, code) = code_of(check_patch(
        PAGE.as_bytes(),
        &analysis,
        &meta,
        &delta,
        &small(PAGE.len()),
    ));
    assert_eq!(
        (error, code),
        (PatchError::TooLarge, ErrorCode::PatchTooLarge)
    );
    // PATCH_CONTENT_REJECTED, step 8 (markup).
    let (error, code) = code_of(patch(PAGE, &[op("cart", "{bad")]));
    assert!(matches!(error, PatchError::Markup(_)), "{error:?}");
    assert_eq!(code, ErrorCode::PatchContentRejected);
    // INTEGRITY_ERROR, snapshot.
    let mut fewer = analysis.clone();
    fewer.blocks.pop();
    let (error, code) = code_of(check_patch(
        PAGE.as_bytes(),
        &fewer,
        &meta,
        &delta,
        &limits(),
    ));
    assert_eq!(
        (error, code),
        (PatchError::Snapshot, ErrorCode::IntegrityError)
    );
}

#[test]
fn patch_check_page_revision_differs_is_integrity_error() {
    // The delta was verified for a snapshot without a policy; the caller
    // passes the same blocks with a policy line: R' differs.
    let analysis = analyze(PAGE);
    let plain = meta_of(&analysis, None, None);
    let delta = verified(&plain, &[op("cart", "1")]);
    let policy = Policy {
        mode: Some(Mode::Public),
        ttl_seconds: None,
        offline: None,
    };
    let with_policy = meta_of(&analysis, Some(policy), None);
    let (error, code) = code_of(check_patch(
        PAGE.as_bytes(),
        &analysis,
        &with_policy,
        &delta,
        &limits(),
    ));
    assert_eq!(
        (error, code),
        (PatchError::PageRevisionDiffers, ErrorCode::IntegrityError)
    );
}

#[test]
fn patch_check_page_revision_uses_policy_and_limits_of_the_snapshot() {
    let analysis = analyze(PAGE);
    let policy = Policy {
        mode: Some(Mode::Public),
        ttl_seconds: Some(60),
        offline: Some(true),
    };
    let lim = Limits {
        max_patch_bytes: Some(2048),
    };
    let meta = meta_of(&analysis, Some(policy), Some(lim));
    let delta = verified(&meta, &[op("cart", "1")]);
    let done = check_patch(PAGE.as_bytes(), &analysis, &meta, &delta, &limits()).unwrap();
    assert_eq!(done.page_revision(), delta.next_revision());
}

#[test]
fn patch_check_html_operation_reaches_the_content_rule() {
    // The step 9 wiring: a patched html block goes to the content rule.
    // The content is valid markup (step 8 passes) with an event handler.
    let (error, code) = code_of(patch(PAGE, &[op("price", "<b onclick=\"x()\">130.00</b>")]));
    assert!(matches!(error, PatchError::Content(_)), "{error:?}");
    assert_eq!(code, ErrorCode::PatchContentRejected);
}

#[test]
fn patch_check_accepts_html_patch() {
    let done = patch(PAGE, &[op("price", "<b>130.00</b>")]).unwrap();
    let expected = PAGE.replace("120.00", "<b>130.00</b>");
    assert_eq!(done.document(), expected.as_bytes());
    assert_eq!(*done.analysis(), analyze(&expected));
}

// ---------------------------------------------------------------------------
// check_patch: section 4.1 cases rejected in step 8

fn assert_step8(id: &str, content: &str) {
    let (error, code) = code_of(patch(PAGE, &[op(id, content)]));
    assert!(
        matches!(
            error,
            PatchError::Markup(_) | PatchError::TemplateChanged | PatchError::BlocksChanged
        ),
        "{id} {content:?}: {error:?}"
    );
    assert_eq!(code, ErrorCode::PatchContentRejected);
}

#[test]
fn patch_check_markup_content_that_closes_the_block_early() {
    assert_step8("price", "1</p><p>2");
    assert_step8("title", "x</title><title>y");
}

#[test]
fn patch_check_markup_content_that_leaves_an_element_open() {
    assert_step8("price", "<b>bold");
    assert_step8("price", "<div><span>x</div>");
}

#[test]
fn patch_check_markup_textarea_and_select_in_html_content() {
    assert_step8("price", "<textarea>x</textarea>");
    assert_step8("price", "<select><option>x</option></select>");
}

#[test]
fn patch_check_markup_invalid_json_in_a_json_block() {
    assert_step8("cart", "{\"items\":");
    assert_step8("cart", "not json");
}

#[test]
fn patch_check_markup_script_end_in_a_json_block() {
    assert_step8("cart", "\"</script x\"");
}

#[test]
fn patch_check_markup_title_end_in_a_title_block() {
    assert_step8("title", "a </title b");
}

#[test]
fn patch_check_markup_manifest_like_script() {
    assert_step8(
        "price",
        "<script type=\"application/rivqen-manifest+json\">{}</script>",
    );
}

#[test]
fn patch_check_early_close_changes_the_template() {
    // The block ends at the first end tag, so the rest becomes template.
    // Id and format are in the start tag, so a changed template is the
    // reachable form of "other blocks"; `BlocksChanged` is a second guard.
    for content in ["x</p><i>y", "x</p><p data-rq-block=\"z\">y", "x</p>y<p>"] {
        let (error, code) = code_of(patch(PAGE, &[op("price", content)]));
        assert_eq!(
            (error, code),
            (PatchError::TemplateChanged, ErrorCode::PatchContentRejected)
        );
    }
}

// ---------------------------------------------------------------------------
// Errors hold no content

#[test]
fn patch_errors_hold_no_content() {
    const MARK: &str = "SECRETMARK";
    let analysis = analyze(PAGE);
    let meta = meta_of(&analysis, None, None);
    let bad = verified(&meta, &[op("cart", &format!("{{bad {MARK}"))]);
    let markup = check_patch(PAGE.as_bytes(), &analysis, &meta, &bad, &limits()).unwrap_err();
    let too_large =
        check_patch(PAGE.as_bytes(), &analysis, &meta, &bad, &small(PAGE.len())).unwrap_err();
    let html = patch(
        PAGE,
        &[op("price", &format!("<b onclick=\"{MARK}\">{MARK}</b>"))],
    )
    .unwrap_err();
    assert!(matches!(html, PatchError::Content(_)), "{html:?}");
    // Each reason is a text of its own: a reason goes to the log record.
    assert_ne!(markup.reason(), too_large.reason());
    assert_ne!(markup.reason(), html.reason());
    assert_ne!(too_large.reason(), html.reason());
    for error in [markup, too_large, html] {
        assert!(error.reason().contains(' '));
        assert_eq!(error.to_string(), error.reason());
        let text = format!("{error} {error:?}");
        assert!(!text.contains(MARK), "{text}");
        assert!(!text.contains("120.00"), "{text}");
    }
    let render = render_rqp(
        PAGE.as_bytes(),
        &analysis,
        &[Replacement {
            block: 9,
            content: MARK.as_bytes(),
        }],
        &limits(),
    )
    .unwrap_err();
    assert!(!format!("{render} {render:?}").contains(MARK));
    // A replacement: Debug gives the block index and the length (S2-1).
    let replacement = Replacement {
        block: 1,
        content: MARK.as_bytes(),
    };
    let text = format!("{replacement:?} {replacement:#?}");
    assert!(!text.contains(MARK), "{text}");
    assert!(!text.contains(&format!("{:?}", MARK.as_bytes())), "{text}");
    assert!(text.contains("block: 1"), "{text}");
    assert!(
        text.contains(&format!("content_len: {}", MARK.len())),
        "{text}"
    );
    // An accepted patched document: Debug gives sizes only.
    let done = patch(PAGE, &[op("price", &format!("<b>{MARK}</b>"))]).unwrap();
    let text = format!("{done:?} {done:#?}");
    assert!(!text.contains(MARK), "{text}");
    assert!(!text.contains('<'), "{text}");
    assert!(
        text.contains(&format!("document_len: {}", done.document().len())),
        "{text}"
    );
    assert!(text.contains("blocks: 3"), "{text}");
}

// ---------------------------------------------------------------------------
// page_revision_of (DL-175)

#[test]
fn patch_page_revision_of_equals_page_revision() {
    let analysis = analyze(PAGE);
    let blocks: Vec<BlockDescriptor> = analysis
        .blocks
        .iter()
        .map(|b| b.descriptor.clone())
        .collect();
    let policy = Policy {
        mode: Some(Mode::Private),
        ttl_seconds: Some(5),
        offline: None,
    };
    let lim = Limits {
        max_patch_bytes: Some(4096),
    };
    for (p, l) in [
        (None, None),
        (Some(&policy), None),
        (Some(&policy), Some(&lim)),
    ] {
        let by_slice = page_revision(&analysis.template_revision, &blocks, p, l);
        let by_ref = page_revision_of(&analysis.template_revision, blocks.iter(), p, l);
        assert_eq!(by_slice, by_ref);
        assert_eq!(analysis.page_revision(p, l), by_slice);
    }
    let none: [&BlockDescriptor; 0] = [];
    assert_eq!(
        page_revision_of(&analysis.template_revision, none, None, None),
        page_revision(&analysis.template_revision, &[], None, None)
    );
}

// ---------------------------------------------------------------------------
// Property

#[derive(Debug, Clone)]
struct Spec {
    /// (is json, initial seed, new seed)
    blocks: Vec<(bool, u8, Option<u8>)>,
}

fn content_of(json: bool, seed: u8) -> String {
    if json {
        format!("{{\"v\":{seed}}}")
    } else {
        format!("text {seed} <i>x</i>")
    }
}

fn page_of(spec: &Spec) -> String {
    let mut page = String::from("<!DOCTYPE html><html><head><title>t</title></head><body><p>s</p>");
    for (i, (json, seed, _)) in spec.blocks.iter().enumerate() {
        let content = content_of(*json, *seed);
        if *json {
            page.push_str(&format!(
                "<script type=\"application/json\" data-rq-block=\"b{i}\">{content}</script>"
            ));
        } else {
            page.push_str(&format!("<div data-rq-block=\"b{i}\">{content}</div>"));
        }
        page.push_str("<hr>");
    }
    page.push_str("</body></html>");
    page
}

fn spec() -> impl Strategy<Value = Spec> {
    prop::collection::vec(
        (any::<bool>(), any::<u8>(), prop::option::of(any::<u8>())),
        1..8,
    )
    .prop_map(|blocks| Spec { blocks })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn patch_prop_render_keeps_template(spec in spec()) {
        let page = page_of(&spec);
        let analysis = split_rqp(page.as_bytes(), &limits()).unwrap();
        prop_assert_eq!(analysis.blocks.len(), spec.blocks.len());
        let contents: Vec<(usize, String)> = spec
            .blocks
            .iter()
            .enumerate()
            .filter_map(|(i, (json, _, new))| {
                new.map(|n| (i, content_of(*json, n.wrapping_add(1))))
            })
            .collect();
        let replacements: Vec<Replacement<'_>> = contents
            .iter()
            .map(|(block, content)| Replacement { block: *block, content: content.as_bytes() })
            .collect();
        let out = render_rqp(page.as_bytes(), &analysis, &replacements, &limits()).unwrap();
        let after = split_rqp(&out, &limits()).unwrap();
        prop_assert_eq!(&after.template_revision, &analysis.template_revision);
        prop_assert_eq!(after.blocks.len(), analysis.blocks.len());
        for (i, (old, new)) in analysis.blocks.iter().zip(&after.blocks).enumerate() {
            prop_assert_eq!(&old.descriptor.id, &new.descriptor.id);
            prop_assert_eq!(old.descriptor.format, new.descriptor.format);
            match contents.iter().find(|(b, _)| *b == i) {
                Some((_, content)) => {
                    prop_assert_eq!(new.descriptor.sha256, sha256(content.as_bytes()));
                }
                None => prop_assert_eq!(old.descriptor.sha256, new.descriptor.sha256),
            }
        }
    }

    #[test]
    fn patch_prop_page_revision_of_equals_page_revision(
        spec in spec(),
        ttl in proptest::option::of(0_u32..100),
    ) {
        let analysis = split_rqp(page_of(&spec).as_bytes(), &limits()).unwrap();
        let blocks: Vec<BlockDescriptor> =
            analysis.blocks.iter().map(|b| b.descriptor.clone()).collect();
        let policy = ttl.map(|t| Policy { mode: None, ttl_seconds: Some(t), offline: None });
        prop_assert_eq!(
            page_revision_of(&analysis.template_revision, &blocks, policy.as_ref(), None),
            page_revision(&analysis.template_revision, &blocks, policy.as_ref(), None)
        );
    }
}
