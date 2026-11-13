// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The patch content rule and the attribute rule (WP-06 S2 T-02):
//! `docs/engineering/protocol/manifest-patch.md` section 4, item 1, and
//! section 4.2. Each test names the item of the normative text it checks.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_proto::types::BlockFormat;
use rivqen_template::limits::{MAX_LEGACY_NAME_BYTES, TemplateLimits};
use rivqen_template::rqp::content::{
    ContentError, ContentViolation, FOREIGN_ALLOWED, PATCH_FORBIDDEN, check_attribute,
    check_patch_content,
};
use rivqen_template::rqp::{MarkupCode, RqpBlock, RqpDocument, split_rqp};

const BLOCK_START: &str = "<div data-rq-block=\"a\">";
const BLOCK_END: &str = "</div>";

/// A valid analysis with one `html` block and one `json` block, for the
/// descriptors and the template revision. The content rule does not read
/// the hashes, so the tests build analyses with these descriptors and their
/// own ranges.
fn base() -> RqpDocument {
    split_rqp(
        b"<div data-rq-block=\"a\">x</div><script type=\"application/json\" data-rq-block=\"j\">{}</script>",
        &TemplateLimits::default(),
    )
    .unwrap()
}

/// An analysis with the given ranges and formats.
fn analysis(blocks: &[(usize, usize, BlockFormat)]) -> RqpDocument {
    let base = base();
    let html = base.blocks[0].descriptor.clone();
    let json = base.blocks[1].descriptor.clone();
    assert_eq!(html.format, BlockFormat::Html);
    assert_eq!(json.format, BlockFormat::Json);
    RqpDocument {
        template_revision: base.template_revision,
        blocks: blocks
            .iter()
            .map(|&(start, end, format)| RqpBlock {
                descriptor: if format == BlockFormat::Html {
                    html.clone()
                } else {
                    json.clone()
                },
                start,
                end,
            })
            .collect(),
    }
}

/// `prefix`, then each content in a `div` block with a `between` text after
/// it, then `suffix`; the analysis has one `html` block per content.
fn document(prefix: &str, contents: &[&str], suffix: &str) -> (Vec<u8>, RqpDocument) {
    let mut text = String::from(prefix);
    let mut ranges = Vec::new();
    for content in contents {
        text.push_str(BLOCK_START);
        let start = text.len();
        text.push_str(content);
        ranges.push((start, text.len(), BlockFormat::Html));
        text.push_str(BLOCK_END);
        text.push('\n');
    }
    text.push_str(suffix);
    (text.into_bytes(), analysis(&ranges))
}

/// The rule on one patched `html` block with this content.
fn check(content: &str) -> Result<(), ContentViolation> {
    check_bytes(content.as_bytes())
}

fn check_bytes(content: &[u8]) -> Result<(), ContentViolation> {
    let mut text = BLOCK_START.as_bytes().to_vec();
    let start = text.len();
    text.extend_from_slice(content);
    let end = text.len();
    text.extend_from_slice(BLOCK_END.as_bytes());
    let analysis = analysis(&[(start, end, BlockFormat::Html)]);
    check_patch_content(&text, &analysis, &[0], &TemplateLimits::default()).map_err(|error| {
        assert_eq!(error.block, 0);
        error.violation
    })
}

fn limits(document: usize, open: usize) -> TemplateLimits {
    let default = TemplateLimits::default();
    TemplateLimits::new(
        document,
        default.max_blocks(),
        default.max_block_bytes(),
        MAX_LEGACY_NAME_BYTES,
        open,
    )
}

/// `check_attribute` for an attribute of an `a` element in HTML context.
fn href(value: &str) -> Result<(), ContentViolation> {
    check_attribute(b"a", false, b"href", value.as_bytes())
}

fn img(name: &str, value: &str) -> Result<(), ContentViolation> {
    check_attribute(b"img", false, name.as_bytes(), value.as_bytes())
}

use ContentViolation::{CharacterReference, EventHandler, ForbiddenElement, ForeignElement};
use ContentViolation::{Scheme, Srcset};

// ---------------------------------------------------------------------------
// Scope (section 2.4 step 9; what the rule reads).

#[test]
fn content_scope_accepts_plain_content() {
    assert_eq!(check(""), Ok(()));
    assert_eq!(
        check("<p class=\"x\">Price: <b>42</b></p><img src=\"/i.png\" alt=\"\">"),
        Ok(())
    );
}

#[test]
fn content_scope_json_block_is_not_read() {
    // The bytes of the range would reject as html; as `json` they are not read.
    let text = b"<p><script onclick=x>javascript:x</script></p>";
    let analysis = analysis(&[(3, text.len() - 4, BlockFormat::Json)]);
    assert_eq!(
        check_patch_content(text, &analysis, &[0], &TemplateLimits::default()),
        Ok(())
    );
}

#[test]
fn content_scope_json_only_patch_does_not_scan() {
    // A scan of this document stops at once (M-01), but a patch of `json`
    // blocks only does not read the document.
    let text = b"<script type=\"application/json\" data-rq-block=\"j\">{}</script>";
    let analysis = analysis(&[(50, 52, BlockFormat::Json)]);
    assert_eq!(
        check_patch_content(text, &analysis, &[0], &limits(10, 256)),
        Ok(())
    );
}

#[test]
fn content_scope_block_without_operation_is_not_read() {
    let (text, analysis) = document("", &["<script>x</script>", "<b>ok</b>"], "");
    assert_eq!(
        check_patch_content(&text, &analysis, &[1], &TemplateLimits::default()),
        Ok(())
    );
    assert_eq!(
        check_patch_content(&text, &analysis, &[], &TemplateLimits::default()),
        Ok(())
    );
    assert_eq!(
        check_patch_content(&text, &analysis, &[0], &TemplateLimits::default()),
        Err(ContentError {
            block: 0,
            violation: ForbiddenElement
        })
    );
}

#[test]
fn content_scope_tags_outside_the_content_are_not_checked() {
    // The block start tag itself, a tag before and a tag after the block.
    let text =
        b"<p onclick=x></p><div data-rq-block=\"a\" onclick=\"x\">ok</div><script>y</script>";
    let start = text.windows(4).position(|w| w == b">ok<").unwrap() + 1;
    let analysis = analysis(&[(start, start + 2, BlockFormat::Html)]);
    assert_eq!(
        check_patch_content(text, &analysis, &[0], &TemplateLimits::default()),
        Ok(())
    );
}

#[test]
fn content_scope_tag_at_the_first_content_byte_is_checked() {
    assert_eq!(check("<script>x</script>"), Err(ForbiddenElement));
}

#[test]
fn content_scope_tag_at_the_end_offset_is_not_content() {
    // `end` is the `<` of the block end tag: a start tag that begins there
    // is not content (only a wrong analysis can put a start tag there).
    let text = b"<div data-rq-block=\"a\">x<script></div>";
    let analysis = analysis(&[(23, 24, BlockFormat::Html)]);
    assert_eq!(
        check_patch_content(text, &analysis, &[0], &TemplateLimits::default()),
        Ok(())
    );
    let analysis = self::analysis(&[(23, 25, BlockFormat::Html)]);
    assert!(check_patch_content(text, &analysis, &[0], &TemplateLimits::default()).is_err());
}

#[test]
fn content_scope_title_block_is_rcdata_and_has_no_start_tags() {
    // The rule scans the whole document: the content of a `title` block is
    // RCDATA, so `<script>` in it is text. A scan of the content alone would
    // read a start tag.
    let text = b"<title data-rq-block=\"t\"><script>x</title>";
    assert_eq!(&text[25..34], b"<script>x");
    let analysis = analysis(&[(25, 34, BlockFormat::Html)]);
    assert_eq!(
        check_patch_content(text, &analysis, &[0], &TemplateLimits::default()),
        Ok(())
    );
}

#[test]
fn content_scope_index_without_a_block_rejects() {
    let (text, analysis) = document("", &["ok"], "");
    let error =
        check_patch_content(&text, &analysis, &[0, 7], &TemplateLimits::default()).unwrap_err();
    assert_eq!(error.block, 7);
    assert!(matches!(error.violation, ContentViolation::Markup(_)));
}

#[test]
fn content_scope_range_outside_the_document_rejects() {
    let text = b"<div data-rq-block=\"a\">ok</div>";
    for (start, end) in [(23, 32), (23, usize::MAX), (26, 25), (40, 41)] {
        let analysis = analysis(&[(start, end, BlockFormat::Html)]);
        let error =
            check_patch_content(text, &analysis, &[0], &TemplateLimits::default()).unwrap_err();
        assert_eq!(error.block, 0);
        assert!(
            matches!(error.violation, ContentViolation::Markup(_)),
            "{start}..{end}"
        );
    }
    // Also for a `json` block, and an empty content is in range.
    let analysis = analysis(&[(23, 99, BlockFormat::Json)]);
    assert!(check_patch_content(text, &analysis, &[0], &TemplateLimits::default()).is_err());
    let empty = b"<div data-rq-block=\"a\"></div>";
    let analysis = self::analysis(&[(23, 23, BlockFormat::Html)]);
    assert_eq!(
        check_patch_content(empty, &analysis, &[0], &TemplateLimits::default()),
        Ok(())
    );
}

#[test]
fn content_scope_range_not_at_the_tags_of_a_block_rejects() {
    // S2-2: the ranges must be the ranges of this document. A content range
    // starts after the `>` of the start tag and ends at the `<` of the end
    // tag; the ranges of another document reject, also for a `json` block.
    let text = b"<div data-rq-block=\"a\">ok</div>";
    assert_eq!(&text[23..25], b"ok");
    for format in [BlockFormat::Html, BlockFormat::Json] {
        for (start, end) in [(24, 25), (23, 24), (0, 25), (23, 31), (31, 31)] {
            let analysis = analysis(&[(start, end, format)]);
            let error =
                check_patch_content(text, &analysis, &[0], &TemplateLimits::default()).unwrap_err();
            assert!(
                matches!(error.violation, ContentViolation::Markup(_)),
                "{start}..{end}"
            );
        }
        let analysis = analysis(&[(23, 25, format)]);
        assert_eq!(
            check_patch_content(text, &analysis, &[0], &TemplateLimits::default()),
            Ok(())
        );
    }
    // The analysis of the stored original with the bytes of the patched
    // document: the event handler is in the new content, outside the old
    // range, and the call rejects.
    let (original, stored) = document("", &["ok"], "");
    let patched = String::from_utf8(original.clone())
        .unwrap()
        .replace("ok", "<img src=x onerror=x()>ok");
    assert_ne!(patched.as_bytes(), original.as_slice());
    assert!(
        check_patch_content(
            patched.as_bytes(),
            &stored,
            &[0],
            &TemplateLimits::default()
        )
        .is_err()
    );
}

#[test]
fn content_scope_overlapping_ranges_reject() {
    // Each range starts after a `>` and ends at a `<`, so only the overlap
    // rejects.
    let text = b"x>ab>cd<ef<";
    let analysis = analysis(&[(2, 7, BlockFormat::Html), (5, 10, BlockFormat::Html)]);
    let error =
        check_patch_content(text, &analysis, &[0, 1], &TemplateLimits::default()).unwrap_err();
    assert_eq!(error.block, 1);
    assert!(matches!(error.violation, ContentViolation::Markup(_)));
    // Adjacent ranges do not overlap.
    let text = b"x>ab><cd<";
    let analysis = self::analysis(&[(2, 5, BlockFormat::Html), (5, 8, BlockFormat::Html)]);
    assert_eq!(
        check_patch_content(text, &analysis, &[1, 0], &TemplateLimits::default()),
        Ok(())
    );
}

#[test]
fn content_scope_duplicate_index_is_one_block() {
    let (text, analysis) = document("", &["ok", "<b>x</b>"], "");
    assert_eq!(
        check_patch_content(&text, &analysis, &[1, 0, 1, 0], &TemplateLimits::default()),
        Ok(())
    );
}

#[test]
fn content_scope_first_violation_in_document_order_wins() {
    let (text, analysis) = document(
        "",
        &[
            "ok",
            "<p onclick=x><link>",
            "<embed>",
            "<a href=\"javascript:x\">",
        ],
        "",
    );
    let all = TemplateLimits::default();
    assert_eq!(
        check_patch_content(&text, &analysis, &[3, 2, 1, 0], &all),
        Err(ContentError {
            block: 1,
            violation: EventHandler
        })
    );
    assert_eq!(
        check_patch_content(&text, &analysis, &[3, 2], &all),
        Err(ContentError {
            block: 2,
            violation: ForbiddenElement
        })
    );
    assert_eq!(
        check_patch_content(&text, &analysis, &[3], &all),
        Err(ContentError {
            block: 3,
            violation: Scheme
        })
    );
    // In one tag: the name before the attributes, the attributes in order.
    assert_eq!(check("<script onclick=x>"), Err(ForbiddenElement));
    assert_eq!(check("<p href=javascript:x onclick=x>"), Err(Scheme));
    assert_eq!(check("<p onclick=x href=javascript:x>"), Err(EventHandler));
}

#[test]
fn content_scope_scan_stop_before_the_block_rejects() {
    // Token stack limit: max_open_elements 4 gives a stack of 6.
    let deep = "<b>".repeat(8);
    let (text, analysis) = document(&deep, &["ok"], "");
    let error = check_patch_content(&text, &analysis, &[0], &limits(1 << 20, 4)).unwrap_err();
    assert_eq!(error.block, 0);
    let ContentViolation::Markup(markup) = error.violation else {
        panic!("{error:?}");
    };
    assert_eq!((markup.code(), markup.rule()), (MarkupCode::Limit, "M-26"));
    // The same document with the default limits passes.
    assert_eq!(
        check_patch_content(&text, &analysis, &[0], &TemplateLimits::default()),
        Ok(())
    );
}

#[test]
fn content_scope_scan_stop_in_the_content_rejects() {
    let deep = format!("{}<script>", "<b>".repeat(8));
    let (text, analysis) = document("", &[&deep], "");
    let error = check_patch_content(&text, &analysis, &[0], &limits(1 << 20, 4)).unwrap_err();
    assert!(matches!(error.violation, ContentViolation::Markup(_)));
}

#[test]
fn content_scope_input_limit_rejects() {
    let (text, analysis) = document("", &["ok"], "");
    let error =
        check_patch_content(&text, &analysis, &[0], &limits(text.len() - 1, 256)).unwrap_err();
    let ContentViolation::Markup(markup) = error.violation else {
        panic!("{error:?}");
    };
    assert_eq!((markup.code(), markup.rule()), (MarkupCode::Limit, "M-01"));
    assert_eq!(
        check_patch_content(&text, &analysis, &[0], &limits(text.len(), 256)),
        Ok(())
    );
}

#[test]
fn content_scope_scan_stop_after_the_last_block_accepts() {
    let deep = "<b>".repeat(8);
    let (text, analysis) = document("", &["ok", "<i>x</i>"], &deep);
    assert_eq!(
        check_patch_content(&text, &analysis, &[0, 1], &limits(1 << 20, 4)),
        Ok(())
    );
}

#[test]
fn content_scope_scan_stop_names_the_first_unread_block() {
    let deep = "<b>".repeat(8);
    let (text, analysis) = document("", &["ok", &deep, "ok"], "");
    let error = check_patch_content(&text, &analysis, &[2, 0, 1], &limits(1 << 20, 4)).unwrap_err();
    assert_eq!(error.block, 1);
    // A violation before the stop wins (`link` is void: no push, no raw text).
    let (text, analysis) = document("", &["<link>", &deep, "ok"], "");
    assert_eq!(
        check_patch_content(&text, &analysis, &[2, 0, 1], &limits(1 << 20, 4)),
        Err(ContentError {
            block: 0,
            violation: ForbiddenElement
        })
    );
    // The stop is in block 1, which is not patched: block 2 is unread.
    let error = check_patch_content(&text, &analysis, &[2], &limits(1 << 20, 4)).unwrap_err();
    assert_eq!(error.block, 2);
}

#[test]
fn content_scope_with_the_analysis_of_split_rqp() {
    // The real pipeline: a valid patched document and its analysis.
    for (content, expected) in [
        ("<b>ok</b>", Ok(())),
        ("<script>x</script>", Err(ForbiddenElement)),
        ("<svg><a href=\"/x\">y</a></svg>", Err(ForeignElement)),
        ("<a onclick=\"x\">y</a>", Err(EventHandler)),
        ("<img src=\"data:image/png;base64,AAAA\" alt=\"\">", Ok(())),
        ("<img src=\"data:image/svg+xml,x\" alt=\"\">", Err(Scheme)),
    ] {
        let text = format!(
            "<!DOCTYPE html><html><head><title>t</title></head><body>\
             <p data-rq-block=\"p\">{content}</p>\
             <script type=\"application/json\" data-rq-block=\"j\">{{\"a\":\"<script>\"}}</script>\
             </body></html>"
        );
        let analysis = split_rqp(text.as_bytes(), &TemplateLimits::default()).unwrap();
        assert_eq!(analysis.blocks.len(), 2);
        let result = check_patch_content(
            text.as_bytes(),
            &analysis,
            &[1, 0],
            &TemplateLimits::default(),
        )
        .map_err(|e| {
            assert_eq!(e.block, 0);
            e.violation
        });
        assert_eq!(result, expected, "{content}");
    }
}

#[test]
fn content_scope_protocol_pages() {
    // Example inputs of fixtures/rqp/protocol/pages: the block `price` of
    // each page patched.
    let root = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/rqp/protocol/pages/"
    );
    for (page, expected) in [
        ("page-b.html", Ok(())),
        ("page-b-onclick.html", Err(EventHandler)),
        ("page-b-data.html", Err(Scheme)),
        ("page-b-data-svg-space.html", Err(Scheme)),
        ("page-b-iframe.html", Err(ForbiddenElement)),
        ("page-b-noscript.html", Err(ForbiddenElement)),
        ("page-b-script.html", Err(ForbiddenElement)),
        ("page-b-style.html", Err(ForbiddenElement)),
        ("page-b-svg.html", Err(EventHandler)),
    ] {
        let text = std::fs::read(format!("{root}{page}")).unwrap();
        let analysis = split_rqp(&text, &TemplateLimits::default()).unwrap();
        let price = analysis
            .blocks
            .iter()
            .position(|b| b.descriptor.id.as_str() == "price")
            .unwrap();
        let all: Vec<usize> = (0..analysis.blocks.len()).collect();
        let result = check_patch_content(&text, &analysis, &all, &TemplateLimits::default())
            .map_err(|e| {
                assert_eq!(e.block, price, "{page}");
                e.violation
            });
        assert_eq!(result, expected, "{page}");
    }
}

// ---------------------------------------------------------------------------
// Item 1.1: PATCH_FORBIDDEN in HTML context.

#[test]
fn content_forbidden_every_name_rejects() {
    assert_eq!(PATCH_FORBIDDEN.len(), 11);
    for name in PATCH_FORBIDDEN {
        assert_eq!(check(&format!("<{name}>")), Err(ForbiddenElement), "{name}");
        assert_eq!(
            check(&format!("<{name}/>")),
            Err(ForbiddenElement),
            "{name}"
        );
        assert_eq!(
            check(&format!("x<{} a=b>", name.to_ascii_uppercase())),
            Err(ForbiddenElement),
            "{name}"
        );
    }
}

#[test]
fn content_forbidden_mixed_case_and_self_closing() {
    assert_eq!(check("<ScRiPt>x</script>"), Err(ForbiddenElement));
    assert_eq!(check("<script/>"), Err(ForbiddenElement));
    assert_eq!(check("<link/>"), Err(ForbiddenElement));
    assert_eq!(check("<meta charset=\"x\">"), Err(ForbiddenElement));
    assert_eq!(
        check("<NoScript><p>x</p></NoScript>"),
        Err(ForbiddenElement)
    );
}

#[test]
fn content_forbidden_in_the_contents_of_a_template() {
    assert_eq!(
        check("<template><script>x</script></template>"),
        Err(ForbiddenElement)
    );
    assert_eq!(
        check("<template><p><style></style></p></template>"),
        Err(ForbiddenElement)
    );
    assert_eq!(
        check("<template><p onclick=x></p></template>"),
        Err(EventHandler)
    );
}

#[test]
fn content_forbidden_end_tags_do_not_matter() {
    assert_eq!(
        check("</script></style></iframe></noscript></meta></base>"),
        Ok(())
    );
    assert_eq!(check("<p>x</p onclick=x href=javascript:x>"), Ok(()));
}

#[test]
fn content_forbidden_other_names_pass() {
    for name in [
        "scripts",
        "xscript",
        "links",
        "noscripts",
        "metas",
        "basex",
        "objects",
        "embedx",
        "img",
        "image",
        "a",
        "animate",
        "foreignobject",
        "template",
        "form",
        "input",
        "video",
        "source",
    ] {
        assert_eq!(check(&format!("<{name}>")), Ok(()), "{name}");
    }
    // The tokenizer reads U+0000 in a name as U+FFFD: not `script`.
    assert_eq!(check_bytes(b"<scr\0ipt>"), Ok(()));
}

#[test]
fn content_forbidden_raw_text_has_no_tags() {
    // After a forbidden start tag the rule already rejects; a `textarea` or
    // `xmp` is RCDATA or RAWTEXT (markup M-04): its bytes are text.
    assert_eq!(check("<textarea><script></textarea>"), Ok(()));
    assert_eq!(check("<xmp><p onclick=x></xmp>"), Ok(()));
    assert_eq!(check("<!-- <script> --><![CDATA[<iframe>]]>"), Ok(()));
}

// ---------------------------------------------------------------------------
// Item 1.2: FOREIGN_ALLOWED in a foreign region.

#[test]
fn content_foreign_allowed_names_pass() {
    assert_eq!(FOREIGN_ALLOWED.len(), 70);
    for name in FOREIGN_ALLOWED {
        assert_eq!(check(&format!("<svg><{name}/></svg>")), Ok(()), "{name}");
        assert_eq!(check(&format!("<math><{name}>")), Ok(()), "{name}");
    }
}

#[test]
fn content_foreign_names_outside_the_set_reject() {
    for name in [
        "a",
        "image",
        "foreignobject",
        "foreignObject",
        "feimage",
        "animate",
        "animatemotion",
        "animateTransform",
        "set",
        "discard",
        "mpath",
        "script",
        "style",
        "textpath",
        "cursor",
        "view",
        "maction",
        "annotation-xml",
        "iframe",
        "div",
        "img",
        "meshgradient",
        "fespotlight",
    ] {
        assert_eq!(
            check(&format!("<svg><g><{name}>")),
            Err(ForeignElement),
            "{name}"
        );
        assert_eq!(
            check(&format!("<math><mrow><{name}/>")),
            Err(ForeignElement),
            "{name}"
        );
    }
}

#[test]
fn content_foreign_region_root_is_html_context() {
    assert_eq!(check("<svg></svg><math></math><SVG/><Math/>"), Ok(()));
    // Mixed case names fold to the set.
    assert_eq!(
        check("<svg><linearGradient id=\"g\"><stop/></linearGradient><clipPath></clipPath></svg>"),
        Ok(())
    );
    // A nested `svg` or `math` is a foreign element of the set.
    assert_eq!(
        check("<svg><svg><math><mi>x</mi></math></svg></svg>"),
        Ok(())
    );
}

#[test]
fn content_foreign_region_ends_with_its_root() {
    assert_eq!(check("<svg><g></g></svg><a href=\"/x\">y</a>"), Ok(()));
    assert_eq!(check("<svg/><a href=\"/x\">y</a>"), Ok(()));
    assert_eq!(check("<svg><g></g><a href=\"/x\">"), Err(ForeignElement));
    // `script` and `style` in a region are foreign elements of item 1.2.
    assert_eq!(check("<svg><script>x</script></svg>"), Err(ForeignElement));
    assert_eq!(check("<math><style>x</style></math>"), Err(ForeignElement));
}

#[test]
fn content_foreign_attributes_are_checked() {
    assert_eq!(check("<svg><path onload=\"x\"/></svg>"), Err(EventHandler));
    assert_eq!(
        check("<svg><use href=\"javascript:x\"/></svg>"),
        Err(Scheme)
    );
    assert_eq!(
        check("<svg><use href=\"data:image/png,x\"/></svg>"),
        Err(Scheme)
    );
    assert_eq!(check("<svg><use href=\"#icon\"/></svg>"), Ok(()));
    assert_eq!(check("<math href=\"vbscript:x\">"), Err(Scheme));
}

// ---------------------------------------------------------------------------
// Section 4.2, item 1: event handlers.

#[test]
fn content_attr_event_handler_names_reject() {
    for name in ["on", "onclick", "onload", "onx", "on-x", "onerror"] {
        assert_eq!(
            check_attribute(b"p", false, name.as_bytes(), b""),
            Err(EventHandler),
            "{name}"
        );
        assert_eq!(
            check_attribute(b"path", true, name.as_bytes(), b"x"),
            Err(EventHandler),
            "{name}"
        );
    }
    assert_eq!(check("<p ONLOAD=x>"), Err(EventHandler));
    assert_eq!(check("<p OnClick>"), Err(EventHandler));
}

#[test]
fn content_attr_event_other_names_pass() {
    for name in ["o", "no", "xon", "data-onclick", "aria-on", "one", "o-n"] {
        let expected = if name.starts_with("on") {
            Err(EventHandler)
        } else {
            Ok(())
        };
        assert_eq!(
            check_attribute(b"p", false, name.as_bytes(), b"x"),
            expected,
            "{name}"
        );
    }
    assert_eq!(check("<p data-onclick=x>"), Ok(()));
    assert_eq!(check_bytes(b"<p o\0nclick=x>"), Ok(()));
}

#[test]
fn content_attr_event_duplicate_that_the_tokenizer_drops_is_checked() {
    // M-03: the second attribute of the same name is dropped by the
    // tokenizer; the rule checks it too.
    assert_eq!(check("<p id=a ID=b onclick=x>"), Err(EventHandler));
    assert_eq!(
        check("<p title=a TITLE=b Title=c ONCLICK=d>"),
        Err(EventHandler)
    );
    assert_eq!(check("<a href=\"/x\" href=\"javascript:x\">"), Err(Scheme));
    assert_eq!(
        check("<img src=\"/a.png\" SRC=\"data:image/svg+xml,x\">"),
        Err(Scheme)
    );
}

// ---------------------------------------------------------------------------
// Section 4.2, item 2: a character reference at the decision byte.

#[test]
fn content_attr_reference_numeric_forms_reject() {
    for value in [
        "&#106;avascript:x",
        "&#x6A;avascript:x",
        "&#X6a;avascript:x",
        "&#0000106;avascript:x",
        "&#x00006a;avascript:x",
        "&#106avascript:x",
        "java&#115;cript:x",
        "javascript&#58;x",
        "javascript&#x3A;x",
        "java&#x09;script:x",
        "java&#9;script:x",
        "&#32;javascript:x",
        "  &#x20;javascript:x",
        "\u{1}&#x0A;javascript:x",
        "ja\tva&#115;cript:x",
        "j\na\rv+a-.1&#115;",
    ] {
        assert_eq!(href(value), Err(CharacterReference), "{value:?}");
    }
}

#[test]
fn content_attr_reference_values_that_continue_a_scheme_reject() {
    // Each value of item 2.1 and its neighbours.
    let reject = |v: u32| {
        v <= 0x20
            || v == 0x3A
            || v == 0x2B
            || v == 0x2D
            || v == 0x2E
            || (0x30..=0x39).contains(&v)
            || (0x41..=0x5A).contains(&v)
            || (0x61..=0x7A).contains(&v)
    };
    for value in (0..=0x100).chain([0x17F, 0xFF21, 0x10_FFFF, 0x11_0000]) {
        let expected = if reject(value) {
            Err(CharacterReference)
        } else {
            Ok(())
        };
        assert_eq!(href(&format!("x&#{value};y")), expected, "{value:#x}");
        assert_eq!(href(&format!("x&#x{value:x}")), expected, "{value:#x}");
        assert_eq!(href(&format!("&#X{value:X};")), expected, "{value:#x}");
    }
    // A number over 10FFFF stops at 110000 (no overflow, no wrap to `j`).
    assert_eq!(href("&#99999999999999999999999999;"), Ok(()));
    assert_eq!(href(&format!("&#{};", 0x1_0000_006A_u64)), Ok(()));
    assert_eq!(href("&#xFFFFFFFFFFFFFFFF6A;"), Ok(()));
}

#[test]
fn content_attr_reference_named_forms_reject() {
    for value in [
        "&Tab;javascript:x",
        "&NewLine;javascript:x",
        "java&colon;x",
        "&fjlig;avascript:x",
        "javascript&period;x",
        "x&plus;y:z",
        "ab&Tab;",
    ] {
        assert_eq!(href(value), Err(CharacterReference), "{value:?}");
    }
}

#[test]
fn content_attr_reference_other_forms_pass() {
    for value in [
        // Case-sensitive names; the `;` is required.
        "&COLON;x",
        "&tab;javascript",
        "java&Colon;x",
        "java&colonx",
        "java&Tabx",
        "&newline;x",
        "&PLUS;x",
        // Not a reference: no digit after `&#`, `&#x`.
        "&#;javascript:x",
        "&#x;javascript:x",
        "&#xg;",
        "&# 106;",
        // References to characters that end the scheme. All the hex digits
        // count: `&#x73c` is U+073C, not `s` (the browser reads it so too).
        "java&#x73cript:x",
        "data&amp;:x",
        "x&#47;y:z",
        "x&#x2f;javascript:x",
        "&#169; 2026",
        "&quot;javascript:x",
        "&amp;javascript:x",
        "&lt;",
        "&",
        "a&",
    ] {
        assert_eq!(href(value), Ok(()), "{value:?}");
    }
}

#[test]
fn content_attr_reference_decision_byte_is_after_the_scheme_bytes() {
    // The decision byte is not `&`: item 2 does not apply.
    for value in [
        "/&#106;avascript:x",
        "a/&#58;",
        "?a=1&b=2",
        "/p?a=1&amp;b=2",
        "Tom &amp; Jerry",
        "a &quot; b",
        "x:&#106;",
        "\u{7F}&#106;avascript:x",
        "java\u{B}&#115;cript:x",
        "java\u{C}&#115;cript:x",
        "",
        "   ",
        "javascript",
    ] {
        assert_eq!(href(value), Ok(()), "{value:?}");
    }
    // Leading bytes 00 to 20 are skipped, then the scheme bytes and the
    // tab and newline bytes.
    for value in [
        "\u{0}\u{1F} &#106;",
        "\u{C}&#106;",
        "a\u{9}\u{A}\u{D}&#58;",
        "a+b-c.d&#58;",
    ] {
        assert_eq!(href(value), Err(CharacterReference), "{value:?}");
    }
}

#[test]
fn content_attr_reference_trap_of_the_audit() {
    // The LF ends the reference `&#x73` (`s`); the URL parser removes it
    // later: `javascript:x`. On the copy of item 3 it would read `&#x73c`.
    assert_eq!(href("java&#x73\ncript:x"), Err(CharacterReference));
    assert_eq!(href("java&#x73\tcript:x"), Err(CharacterReference));
    assert_eq!(href("java&#115\r\ncript:x"), Err(CharacterReference));
    assert_eq!(
        check("<a href=\"java&#x73\ncript:x\">"),
        Err(CharacterReference)
    );
}

#[test]
fn content_attr_reference_applies_to_every_attribute() {
    assert_eq!(
        check_attribute(b"p", false, b"title", b"&#106;avascript:x"),
        Err(CharacterReference)
    );
    assert_eq!(
        check_attribute(b"g", true, b"fill", b"java&colon;"),
        Err(CharacterReference)
    );
}

// ---------------------------------------------------------------------------
// Section 4.2, items 3 and 4.1: javascript: and vbscript:.

#[test]
fn content_attr_scheme_javascript_and_vbscript_reject() {
    for value in [
        "javascript:x",
        "javascript:",
        "JavaScript:x",
        "JAVASCRIPT:alert(1)",
        " javascript:x",
        "\u{0}\u{1}\u{1F} \u{C}javascript:x",
        "jav\ta\nscri\rpt:x",
        "\t\n\rjavascript:x",
        "vbscript:x",
        "VbScRiPt:",
        " \u{8}vb\tscript:x",
    ] {
        assert_eq!(href(value), Err(Scheme), "{value:?}");
        assert_eq!(
            check_attribute(b"p", false, b"title", value.as_bytes()),
            Err(Scheme),
            "{value:?}"
        );
        assert_eq!(
            check_attribute(b"use", true, b"href", value.as_bytes()),
            Err(Scheme),
            "{value:?}"
        );
    }
    assert_eq!(check("<a href=\" JavaScript:x\">"), Err(Scheme));
    assert_eq!(check("<p data-x='vbscript:x'>"), Err(Scheme));
}

#[test]
fn content_attr_scheme_other_values_pass() {
    for value in [
        "javascript",
        "javascrip:x",
        "javascriptx:",
        "java script:x",
        "java\u{B}script:x",
        "java\u{C}script:x",
        "\u{7F}javascript:x",
        "/javascript:x",
        "xjavascript:x",
        "vbscript",
        "vbscrip:x",
        "https://example.test/javascript:x",
        "intent:x",
        "databas:x",
        "database:x",
    ] {
        assert_eq!(href(value), Ok(()), "{value:?}");
    }
    assert_eq!(check("<p data-x=\"javascript\">"), Ok(()));
}

// ---------------------------------------------------------------------------
// Section 4.2, item 4.2: data: (H-83, H-83a, DL-048, DL-051, DL-052).

const RASTER: [&str; 10] = [
    "image/png",
    "image/apng",
    "image/jpeg",
    "image/jpg",
    "image/gif",
    "image/webp",
    "image/avif",
    "image/bmp",
    "image/x-icon",
    "image/vnd.microsoft.icon",
];

#[test]
fn content_attr_data_raster_types_pass_in_the_allowed_places() {
    for kind in RASTER {
        for value in [
            format!("data:{kind};base64,AAAA"),
            format!("data:{kind},x"),
            format!("DATA:{},x", kind.to_ascii_uppercase()),
            format!("data:{kind}"),
            format!("data:{kind};charset=x;base64,AAAA"),
            format!(" \tdata:{kind},x"),
        ] {
            for (element, name) in [
                ("img", "src"),
                ("source", "src"),
                ("video", "poster"),
                ("img", "srcset"),
                ("source", "srcset"),
            ] {
                assert_eq!(
                    check_attribute(element.as_bytes(), false, name.as_bytes(), value.as_bytes()),
                    Ok(()),
                    "{element} {name} {value:?}"
                );
            }
        }
    }
    // Tab, LF and CR in the type are removed by item 3.
    assert_eq!(img("src", "data:image/p\tn\ng,x"), Ok(()));
    assert_eq!(img("src", "da\nta:image/png,x"), Ok(()));
}

#[test]
fn content_attr_data_rejects_in_other_places() {
    let value = b"data:image/png;base64,AAAA";
    for (element, foreign, name) in [
        ("a", false, "href"),
        ("img", false, "data-src"),
        ("img", false, "poster"),
        ("img", false, "alt"),
        ("video", false, "src"),
        ("source", false, "poster"),
        ("input", false, "src"),
        ("iframe", false, "src"),
        ("object", false, "data"),
        ("image", true, "href"),
        ("img", true, "src"),
        ("video", true, "poster"),
        ("source", true, "srcset"),
        ("p", false, "title"),
    ] {
        let expected = if name == "srcset" {
            Err(Srcset)
        } else {
            Err(Scheme)
        };
        assert_eq!(
            check_attribute(element.as_bytes(), foreign, name.as_bytes(), value),
            expected,
            "{element} {foreign} {name}"
        );
    }
}

#[test]
fn content_attr_data_type_part_must_be_exactly_a_raster_type() {
    for value in [
        "data:image/svg+xml,x",
        "data:image/svg,x",
        "data:image/svg-xml,x",
        "data:image/xml,x",
        "data:image/ svg+xml,x",
        "data:image/\u{B}svg+xml,x",
        "data:image/\u{C}svg+xml,x",
        "data:image/png ,x",
        "data:image/png\u{C},x",
        "data:image/png\u{0},x",
        "data: image/png,x",
        "data:\u{0}image/png,x",
        "data:image/pngx,x",
        "data:image/pn,x",
        "data:ximage/png,x",
        "data:image/vnd.microsoft.iconx,x",
        "data:image/vnd.microsoft.icox,x",
        "data:image/p%6Eg,x",
        "data:image/svg%2Bxml,x",
        "data:image/svg&#43;xml,x",
        "data:image/svg&plus;xml,x",
        "data:image/p\u{e9}ng,x",
        "data:text/html,x",
        "data:text/html;base64,x",
        "data:,x",
        "data:",
        "data:;base64,x",
        "data:image/heic,x",
        "data:image/tiff,x",
        "data:application/octet-stream,x",
    ] {
        assert_eq!(img("src", value), Err(Scheme), "{value:?}");
        assert_eq!(
            check_attribute(b"video", false, b"poster", value.as_bytes()),
            Err(Scheme),
            "{value:?}"
        );
    }
    // A very long type part reads only a bounded prefix.
    let long = format!("data:image/png{}", "x".repeat(1 << 16));
    assert_eq!(img("src", &long), Err(Scheme));
}

#[test]
fn content_attr_data_in_documents() {
    assert_eq!(
        check("<img src=\"data:image/png;base64,iVBORw0KGgo=\" alt=\"\">"),
        Ok(())
    );
    assert_eq!(check("<IMG SRC=\"DATA:IMAGE/PNG,x\">"), Ok(()));
    assert_eq!(
        check("<video poster=\"data:image/jpeg;base64,AAAA\"></video>"),
        Ok(())
    );
    assert_eq!(
        check("<picture><source srcset=\"data:image/webp;base64,AAAA\"></picture>"),
        Ok(())
    );
    assert_eq!(check("<a href=\"data:image/png,x\">y</a>"), Err(Scheme));
    // In a foreign region `image` is outside the set (item 1.2) before its
    // attribute is read; the attribute alone rejects too.
    assert_eq!(
        check("<svg><image href=\"data:image/png,x\"/></svg>"),
        Err(ForeignElement)
    );
    assert_eq!(
        check_attribute(b"image", true, b"href", b"data:image/png,x"),
        Err(Scheme)
    );
}

// ---------------------------------------------------------------------------
// Section 4.2, item 4.3: srcset.

#[test]
fn content_attr_srcset_separator_references_reject() {
    for reference in [
        "&#9;",
        "&#x09;",
        "&#10;",
        "&#xA",
        "&#12;",
        "&#x0c;",
        "&#13;",
        "&#32;",
        "&#x20",
        "&#44;",
        "&#x2C;",
        "&#x2c",
        "&#40;",
        "&#x28;",
        "&#41;",
        "&#x29",
        "&#0000044;",
        "&Tab;",
        "&NewLine;",
        "&comma;",
        "&lpar;",
        "&rpar;",
    ] {
        let value = format!("/a.png 1x,{reference}/b.png 2x");
        assert_eq!(img("srcset", &value), Err(Srcset), "{value:?}");
        let value = format!("/a.png{reference}");
        assert_eq!(img("srcset", &value), Err(Srcset), "{value:?}");
    }
    for value in [
        "/a.png 1x,&#33;b.png",
        "/a.png?a=1&amp;b=2 1x",
        "/a&#169;.png",
        "/a&comma.png",
        "/a&COMMA;.png",
        "/a&#;.png",
        "/a&#x2B;.png",
        "/a&#11;.png",
    ] {
        assert_eq!(img("srcset", value), Ok(()), "{value:?}");
    }
}

#[test]
fn content_attr_srcset_candidates_are_checked_as_src() {
    for value in [
        "javascript:x",
        "/a.png 1x, javascript:x 2x",
        "/a.png 1x,javascript:x 2x",
        ",,,javascript:x",
        " , javascript:x",
        "/a.png,, javascript:x",
        "/a.png 1x, &#106;avascript:x 2x",
        "/a.png 1x, java&colon;x",
        "/a.png 1x, \u{1}javascript:x",
        "/a.png 1x, VBSCRIPT:x",
        "/a.png (1x), javascript:x",
        "/a.png 1x (a), data:image/svg+xml,x 2x",
        "/a.png 1x, data:text/html,x 2x",
        "/a.png, (x, javascript:y)",
        "a b c d, javascript:x",
    ] {
        assert_eq!(img("srcset", value), Err(Srcset), "{value:?}");
    }
    for value in [
        "",
        " ,, ",
        "/a.png",
        "/a.png 1x, /b.png 2x",
        "/a.png 1x, data:image/png;base64,AAAA 2x",
        "data:image/png;base64,AAAA,BBBB 1x",
        "data:image/gif,x, /b.png 2x",
        // A comma inside the URL does not split it.
        "/a.png,javascript:x",
        // A comma inside parentheses does not end the descriptors.
        "/a.png (1x, javascript:x) 2x",
        "/a.png (1x, javascript:x",
    ] {
        assert_eq!(img("srcset", value), Ok(()), "{value:?}");
    }
}

#[test]
fn content_attr_srcset_of_every_element() {
    // The `data:` exception follows the element: as `src` of `video` or of
    // a foreign element a `data:` candidate rejects.
    let value = b"/a.png 1x, data:image/png,x 2x";
    assert_eq!(check_attribute(b"img", false, b"srcset", value), Ok(()));
    assert_eq!(check_attribute(b"source", false, b"srcset", value), Ok(()));
    assert_eq!(
        check_attribute(b"video", false, b"srcset", value),
        Err(Srcset)
    );
    assert_eq!(
        check_attribute(b"div", false, b"srcset", value),
        Err(Srcset)
    );
    assert_eq!(check_attribute(b"img", true, b"srcset", value), Err(Srcset));
    // `/a.png,` ends with a comma: the comma goes, and the next candidate
    // starts after the white space.
    assert_eq!(
        check_attribute(b"div", false, b"srcset", b"/a.png, javascript:x"),
        Err(Srcset)
    );
    assert_eq!(
        check_attribute(b"div", false, b"srcset", b"/a.png,javascript:x"),
        Ok(())
    );
    assert_eq!(
        check_attribute(b"div", false, b"srcset", b"/a.png 1x, javascript:x"),
        Err(Srcset)
    );
    assert_eq!(check("<p srcset=\"/a 1x, javascript:x 2x\">"), Err(Srcset));
    assert_eq!(
        check("<img SRCSET=\"/a 1x,&#32;data:text/html,x 2x\">"),
        Err(Srcset)
    );
}

#[test]
fn content_attr_srcset_is_linear() {
    // Many candidates and many references: one pass each.
    let many = "/a.png 1x,".repeat(100_000);
    assert_eq!(img("srcset", &many), Ok(()));
    let refs = "x&#0000000033".repeat(50_000);
    assert_eq!(img("srcset", &refs), Ok(()));
    let parens = format!("/a.png {}", "(".repeat(100_000));
    assert_eq!(img("srcset", &parens), Ok(()));
}

#[test]
fn content_scope_large_content_is_read_in_one_pass() {
    // 1 MiB of tags with attributes, upper-case names (the name buffers)
    // and long values; the last tag rejects, so every tag is read.
    let unit =
        "<P TITLE=\"Tom &amp; Jerry\" Data-X=x><IMG SRC=\"/i.png\" srcset=\"/a 1x, /b 2x\"></P>";
    let mut content = unit.repeat((1 << 20) / unit.len());
    content.push_str("<p onclick=x>");
    assert_eq!(check(&content), Err(EventHandler));
    let long_name = format!("<{}>", "A".repeat(1 << 20));
    assert_eq!(check(&long_name), Ok(()));
    let long_value = format!("<a href=\"{}javascript:x\">", "\t".repeat(1 << 20));
    assert_eq!(check(&long_value), Err(Scheme));
}

// ---------------------------------------------------------------------------
// The result table of section 4.2.

#[test]
fn content_table_rows() {
    let rows: [(&str, Result<(), ContentViolation>); 15] = [
        ("<p onclick=\"x()\"><p ONLOAD=x>", Err(EventHandler)),
        (
            "<a href=\"javascript:x\"><a href=\" JavaScript:x\">",
            Err(Scheme),
        ),
        (
            "<a href=\"vbscript:x\"><a href=\"data:text/html,x\">",
            Err(Scheme),
        ),
        ("<img src=\" \tDa\nTa:x\">", Err(Scheme)),
        (
            "<a href=\"java&#x09;script:x\"><a href=\"&#106;avascript:x\">\
             <a href=\"java&colon;x\"><a href=\"&Tab;javascript:x\">",
            Err(CharacterReference),
        ),
        ("<a href=\"java&#x73\ncript:x\">", Err(CharacterReference)),
        (
            "<p title=\"a &quot; b\"><a href=\"?a=1&b=2\"><a href=\"/p?a=1&amp;b=2\">\
             <p title=\"Tom &amp; Jerry\"><img alt=\"&#169; 2026\">",
            Ok(()),
        ),
        ("<a href=\"data&amp;:x\"><a href=\"&COLON;x\">", Ok(())),
        (
            "<p data-x=\"javascript\"><a href=\"/javascript:x\"><a href=\"database:x\">",
            Ok(()),
        ),
        (
            "<img src=\"data:image/png;base64,iVBORw0KGgo=\">\
             <video poster=\"data:image/jpeg;base64,AAAA\"></video><img src=\"DATA:IMAGE/PNG,x\">",
            Ok(()),
        ),
        (
            "<img src=\"data:image/svg+xml,x\"><img src=\"data:text/html,x\">\
             <a href=\"data:image/png,x\">",
            Err(Scheme),
        ),
        (
            "<img src=\"data:image/ svg+xml,x\"><img src=\"data:image/svg,x\">\
             <img src=\"data:image/png ,x\">",
            Err(Scheme),
        ),
        (
            "<img srcset=\"/a.png 1x, data:image/png;base64,iVBORw0KGgo= 2x\">",
            Ok(()),
        ),
        (
            "<img srcset=\"/a.png 1x, data:image/svg+xml,x 2x\">\
             <source srcset=\"/a.png 1x, javascript:x 2x\">",
            Err(Srcset),
        ),
        (
            "<img srcset=\"/a.png 1x,&#32;data:text/html,x 2x\">",
            Err(Srcset),
        ),
    ];
    for (content, expected) in rows {
        assert_eq!(check(content), expected, "{content:?}");
        // Each tag of a row alone gives the result of the row.
        for tag in content
            .split_inclusive('>')
            .filter(|t| !t.starts_with("</"))
        {
            assert_eq!(check(tag), expected, "{tag:?}");
        }
    }
    // The row with `<svg><image href="data:image/png,…">`: the element rule
    // (item 1.2) rejects first; see content_attr_data_in_documents.
    assert!(check("<svg><image href=\"data:image/png,x\">").is_err());
}

// ---------------------------------------------------------------------------
// Errors.

#[test]
fn content_error_holds_no_content() {
    const MARKER: &str = "qzsecretmarkerqz";
    let contents = [
        format!("<script data-{MARKER}=\"{MARKER}\">"),
        format!("<svg><{MARKER}>"),
        format!("<p on{MARKER}=\"{MARKER}\">"),
        format!("<a href=\"{MARKER}&#58;\">"),
        format!("<a href=\"javascript:{MARKER}\">"),
        format!("<img srcset=\"/{MARKER} 1x, javascript:{MARKER}\">"),
    ];
    let mut seen = Vec::new();
    for content in &contents {
        let violation = check(content).unwrap_err();
        let error = ContentError {
            block: 3,
            violation,
        };
        for text in [
            format!("{error}"),
            format!("{error:?}"),
            format!("{violation}"),
            format!("{violation:?}"),
            format!("{error:#?}"),
        ] {
            assert!(!text.contains(MARKER), "{text}");
            assert!(!text.contains("javascript"), "{text}");
        }
        assert_eq!(
            format!("{error}"),
            format!("block 3: {}", violation.reason())
        );
        assert_eq!(format!("{violation}"), violation.reason());
        assert_eq!(error.code(), ErrorCode::PatchContentRejected);
        assert_eq!(violation.code(), ErrorCode::PatchContentRejected);
        seen.push(violation);
    }
    assert_eq!(
        seen,
        [
            ForbiddenElement,
            ForeignElement,
            EventHandler,
            CharacterReference,
            Scheme,
            Srcset
        ]
    );
    // A scan stop: the code and the rule id only.
    let deep = format!("{}<{MARKER}>", "<b>".repeat(8));
    let (text, analysis) = document("", &[&deep], "");
    let error = check_patch_content(&text, &analysis, &[0], &limits(1 << 20, 4)).unwrap_err();
    for text in [format!("{error}"), format!("{error:?}")] {
        assert!(!text.contains(MARKER), "{text}");
    }
    assert_eq!(
        format!("{error}"),
        "block 0: the scan of the patched document stopped"
    );
}
