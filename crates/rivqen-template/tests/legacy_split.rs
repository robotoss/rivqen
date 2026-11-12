// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The canonical legacy split: title rule C-06, block rule C-07, data order
//! C-08 (legacy wire section 10; legacy markers section 4; CL-41).

#![cfg(feature = "legacy")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use rivqen_integrity::legacy::sha1_hex;
use rivqen_template::legacy::{
    DataBlock, Diagnostic, NotApplied, Split, SplitOutcome, etag_v1, split_v1, template_tag_v1,
};
use rivqen_template::limits::TemplateLimits;

fn applied(html: &str) -> Split {
    match split_v1(html.as_bytes(), &TemplateLimits::default()) {
        Ok(SplitOutcome::Applied(split)) => split,
        other => panic!("not applied: {other:?}"),
    }
}

fn not_applied(html: &[u8]) -> NotApplied {
    match split_v1(html, &TemplateLimits::default()) {
        Ok(SplitOutcome::NotApplied(reason)) => reason,
        other => panic!("applied: {other:?}"),
    }
}

fn entries(split: &Split) -> Vec<(&str, &str)> {
    split
        .data
        .iter()
        .map(|block| (block.key.as_str(), block.value.as_str()))
        .collect()
}

// --- Title rule C-06 ------------------------------------------------------

#[test]
fn legacy_split_title_first_match_is_replaced_once() {
    let split = applied("<title>A</title><svg><title>B</title></svg>");
    assert_eq!(split.template, "{title}<svg><title>B</title></svg>");
    assert_eq!(entries(&split), [("{title}", "<title>A</title>")]);
}

#[test]
fn legacy_split_title_is_ascii_case_insensitive_with_attributes() {
    let split = applied("<head><TiTlE lang=\"en\">Shop</tItLe></head>");
    assert_eq!(split.template, "<head>{title}</head>");
    assert_eq!(split.data[0].value, "<TiTlE lang=\"en\">Shop</tItLe>");
}

#[test]
fn legacy_split_title_does_not_fold_non_ascii_letters() {
    // U+0130 (capital I with dot) is not an ASCII `i`.
    let html = "<T\u{130}TLE>x</title>";
    let split = applied(html);
    assert_eq!(split.template, html);
    assert_eq!(entries(&split), [("{title}", "")]);
}

#[test]
fn legacy_split_title_lf_stops_a_match_and_cr_does_not() {
    let split = applied("<title>a\nb</title>");
    assert_eq!(split.template, "<title>a\nb</title>");
    assert_eq!(split.data[0].value, "");

    let split = applied("<title>a\r\rb</title>");
    assert_eq!(split.template, "{title}");
    assert_eq!(split.data[0].value, "<title>a\r\rb</title>");
}

#[test]
fn legacy_split_title_after_an_lf_the_next_opener_matches() {
    let split = applied("<title>a\n</title><title>b</title>");
    assert_eq!(split.template, "<title>a\n</title>{title}");
    assert_eq!(split.data[0].value, "<title>b</title>");
}

#[test]
fn legacy_split_title_lazy_content_ends_at_the_first_close() {
    // The first opener takes the nested opener as content.
    let split = applied("<title><title>x</title></title>");
    assert_eq!(split.template, "{title}</title>");
    assert_eq!(split.data[0].value, "<title><title>x</title>");
    // The content can be empty and the opener needs no `>`.
    let split = applied("<title</title>");
    assert_eq!(split.data[0].value, "<title</title>");
}

#[test]
fn legacy_split_title_value_is_empty_when_nothing_matches() {
    for html in [
        "",
        "<title>Open",
        "</title><title>",
        "<titl></title>",
        "<title>x</title >",
    ] {
        let split = applied(html);
        assert_eq!(split.template, html, "{html:?}");
        assert_eq!(entries(&split), [("{title}", "")], "{html:?}");
    }
}

#[test]
fn legacy_split_title_inside_a_block_stays_in_the_block_value() {
    // C-07 runs on the result of C-06 (function fixture FN-SPLIT-TITLE-06).
    let split = applied("<body><!--sonicdiff-a--><title>In</title><!--sonicdiff-a-end--></body>");
    assert_eq!(split.template, "<body>{a}</body>");
    assert_eq!(
        entries(&split),
        [
            ("{title}", "<title>In</title>"),
            ("{a}", "<!--sonicdiff-a-->{title}<!--sonicdiff-a-end-->"),
        ]
    );
}

#[test]
fn legacy_split_title_markers_inside_the_title_are_title_text() {
    let split = applied("<title><!--sonicdiff-a-->x<!--sonicdiff-a-end--></title>");
    assert_eq!(split.template, "{title}");
    assert_eq!(split.data.len(), 1);
}

// --- Block rule C-07 ------------------------------------------------------

#[test]
fn legacy_split_block_value_is_the_whole_match() {
    let html = "<p><!--sonicdiff-price--><b>120</b><!--sonicdiff-price-end--></p>";
    let split = applied(html);
    assert_eq!(split.template, "<p>{price}</p>");
    assert_eq!(
        entries(&split),
        [
            ("{title}", ""),
            (
                "{price}",
                "<!--sonicdiff-price--><b>120</b><!--sonicdiff-price-end-->"
            ),
        ]
    );
    assert_eq!(split.diagnostics, []);
}

#[test]
fn legacy_split_block_markers_are_ascii_case_insensitive() {
    let split = applied("<!--SonicDiff-a-->A<!--SONICDIFF-a-END-->");
    assert_eq!(split.template, "{a}");
    assert_eq!(
        split.data[1].value,
        "<!--SonicDiff-a-->A<!--SONICDIFF-a-END-->"
    );
    // A long `s` (U+017F) is not an ASCII `s`.
    let html = "<!--\u{17f}onicdiff-a-->A<!--sonicdiff-a-end-->";
    assert_eq!(applied(html).template, html);
}

#[test]
fn legacy_split_block_dash_before_the_name_is_optional() {
    let split = applied("<!--sonicdiffa-->A<!--sonicdiffa-end-->");
    assert_eq!(split.template, "{a}");
    assert_eq!(split.diagnostics, []);
}

#[test]
fn legacy_split_block_end_name_is_not_checked() {
    let split = applied(
        "<!--sonicdiff-a-->A<!--sonicdiff-b-end--><!--sonicdiff-c-->C<!--sonicdiff-c-end-->",
    );
    assert_eq!(split.template, "{a}{c}");
    assert_eq!(
        split.diagnostics,
        [Diagnostic::MismatchedEndName { index: 1 }]
    );
    // An end name in another case is another name.
    let split = applied(
        "<!--sonicdiff-x-->1<!--sonicdiff-y-end--><!--sonicdiff-a-->A<!--sonicdiff-A-end-->",
    );
    assert_eq!(split.template, "{x}{a}");
    assert_eq!(
        split.diagnostics,
        [
            Diagnostic::MismatchedEndName { index: 1 },
            Diagnostic::MismatchedEndName { index: 2 },
        ]
    );
    // An end name that starts with the opening name.
    let split = applied("<!--sonicdiff-a-->A<!--sonicdiff-ab-end-->");
    assert_eq!(split.template, "{a}");
    assert_eq!(
        split.diagnostics,
        [Diagnostic::MismatchedEndName { index: 1 }]
    );
    // An end marker without a name.
    let split = applied("<!--sonicdiff-a-->A<!--sonicdiff-end-->");
    assert_eq!(split.template, "{a}");
    assert_eq!(
        split.diagnostics,
        [Diagnostic::MismatchedEndName { index: 1 }]
    );
}

#[test]
fn legacy_split_block_end_marker_prefers_the_dash() {
    // `-end-end-->`: the greedy `-?` gives the end name `end`.
    let split = applied("<!--sonicdiff-a-->x<!--sonicdiff-end-end-->y");
    assert_eq!(split.template, "{a}y");
    assert_eq!(
        split.data[1].value,
        "<!--sonicdiff-a-->x<!--sonicdiff-end-end-->"
    );
}

#[test]
fn legacy_split_block_empty_name_stays_in_the_template() {
    let html = "<!--sonicdiff-->A<!--sonicdiff-end--><!--sonicdiff-b-->B<!--sonicdiff-b-end-->";
    let split = applied(html);
    assert_eq!(split.template, "<!--sonicdiff-->A<!--sonicdiff-end-->{b}");
    assert_eq!(split.data.len(), 2);
    assert_eq!(split.data[1].key, "{b}");
}

#[test]
fn legacy_split_block_two_dashes_is_not_an_opener() {
    let html = "<!--sonicdiff--a-->A<!--sonicdiff--a-end-->";
    let split = applied(html);
    assert_eq!(split.template, html);
    assert_eq!(split.data.len(), 1);
}

#[test]
fn legacy_split_block_needs_one_content_byte() {
    let html = "<!--sonicdiff-a--><!--sonicdiff-a-end-->";
    assert_eq!(applied(html).template, html);
    // The lazy content then reaches the next end marker.
    let split = applied("<!--sonicdiff-a--><!--sonicdiff-a-end-->x<!--sonicdiff-a-end-->");
    assert_eq!(split.template, "{a}");
}

#[test]
fn legacy_split_block_content_ends_at_the_first_end_marker() {
    let html = "<!--sonicdiff-a--><!--sonicdiff-b-->B<!--sonicdiff-b-end--><!--sonicdiff-a-end-->";
    let split = applied(html);
    assert_eq!(split.template, "{a}<!--sonicdiff-a-end-->");
    assert_eq!(
        split.data[1].value,
        "<!--sonicdiff-a--><!--sonicdiff-b-->B<!--sonicdiff-b-end-->"
    );
    assert_eq!(
        split.diagnostics,
        [Diagnostic::MismatchedEndName { index: 1 }]
    );
}

#[test]
fn legacy_split_block_markers_with_spaces_are_text() {
    let html = "<!-- sonicdiff-a -->A<!-- sonicdiff-a-end -->";
    assert_eq!(applied(html).template, html);
}

#[test]
fn legacy_split_block_without_end_marker_stays_and_ends_the_scan() {
    let html = "<!--sonicdiff-a-->A<!--sonicdiff-b-->B";
    let split = applied(html);
    assert_eq!(split.template, html);
    assert_eq!(split.data.len(), 1);
}

#[test]
fn legacy_split_block_markers_in_script_and_comments_count() {
    let split = applied(
        "<script>var s='<!--sonicdiff-a-->1<!--sonicdiff-a-end-->';</script><!-- <!--sonicdiff-b-->2<!--sonicdiff-b-end--> -->",
    );
    assert_eq!(split.template, "<script>var s='{a}';</script><!-- {b} -->");
}

#[test]
fn legacy_split_block_content_keeps_every_byte() {
    let content = "\r\n\0é\u{1F600}<!--x-->";
    let html = format!("<!--sonicdiff-a-->{content}<!--sonicdiff-a-end-->");
    let split = applied(&html);
    assert_eq!(split.data[1].value, html);
}

#[test]
fn legacy_split_duplicate_name_is_not_applied() {
    let html =
        b"<!--sonicdiff-a-->A<!--sonicdiff-a-end--><!--sonicdiff-a-->B<!--sonicdiff-a-end-->";
    assert_eq!(not_applied(html), NotApplied::DuplicateName);
    // Names compare case-sensitively.
    let split = applied(
        "<!--sonicdiff-a-->A<!--sonicdiff-a-end--><!--sonicdiff-A-->B<!--sonicdiff-A-end-->",
    );
    assert_eq!(split.template, "{a}{A}");
}

#[test]
fn legacy_split_duplicate_names_that_stay_in_the_template_are_allowed() {
    let name = "n".repeat(129);
    let block = format!("<!--sonicdiff-{name}-->x<!--sonicdiff-{name}-end-->");
    let html = format!("{block}{block}");
    let split = applied(&html);
    assert_eq!(split.template, html);
    assert_eq!(split.data.len(), 1);
}

#[test]
fn legacy_split_block_named_title_is_not_applied() {
    let html = b"<title>T</title><!--sonicdiff-title-->x<!--sonicdiff-title-end-->";
    assert_eq!(not_applied(html), NotApplied::BlockNamedTitle);
    // `Title` is another name.
    let split = applied("<!--sonicdiff-Title-->x<!--sonicdiff-Title-end-->");
    assert_eq!(split.data[1].key, "{Title}");
}

#[test]
fn legacy_split_first_failure_in_document_order_wins() {
    let html = b"<!--sonicdiff-title-->x<!--sonicdiff-title-end--><!--sonicdiff-a-->1<!--sonicdiff-a-end--><!--sonicdiff-a-->2<!--sonicdiff-a-end-->";
    assert_eq!(not_applied(html), NotApplied::BlockNamedTitle);
    let html = b"<!--sonicdiff-a-->1<!--sonicdiff-a-end--><!--sonicdiff-a-->2<!--sonicdiff-a-end--><!--sonicdiff-title-->x<!--sonicdiff-title-end-->";
    assert_eq!(not_applied(html), NotApplied::DuplicateName);
}

#[test]
fn legacy_split_invalid_utf8_is_not_applied() {
    for html in [
        &b"<title>\xed\xa0\x80</title>"[..],
        b"\xc0\xaf",
        b"<p>\xe2",
        b"\xff<!--sonicdiff-a-->x<!--sonicdiff-a-end-->",
    ] {
        assert_eq!(not_applied(html), NotApplied::InvalidUtf8, "{html:?}");
    }
    // A BOM and NUL bytes are valid UTF-8.
    let split = applied("\u{feff}<title>\0</title>");
    assert_eq!(split.template, "\u{feff}{title}");
}

// --- Data order C-08 -----------------------------------------------------

#[test]
fn legacy_split_order_title_first_then_blocks_in_document_order() {
    let split = applied(
        "<!--sonicdiff-z-->Z<!--sonicdiff-z-end--><title>T</title><!--sonicdiff-b-->B<!--sonicdiff-b-end--><!--sonicdiff-a-->A<!--sonicdiff-a-end-->",
    );
    let keys: Vec<&str> = split.data.iter().map(|block| block.key.as_str()).collect();
    assert_eq!(keys, ["{title}", "{z}", "{b}", "{a}"]);
    assert_eq!(split.template, "{z}{title}{b}{a}");
}

#[test]
fn legacy_split_order_keys_have_braces_and_the_exact_name() {
    let split = applied(
        "<!--sonicdiff-a_1-->x<!--sonicdiff-a_1-end--><!--sonicdiff-Z9-->y<!--sonicdiff-Z9-end-->",
    );
    let keys: Vec<&str> = split.data.iter().map(|block| block.key.as_str()).collect();
    assert_eq!(keys, ["{title}", "{a_1}", "{Z9}"]);
}

// --- Hashes and debug output ---------------------------------------------

#[test]
fn legacy_split_etag_and_template_tag_hash_the_exact_bytes() {
    assert_eq!(
        etag_v1(b"abc").as_str(),
        "a9993e364706816aba3e25717850c26c9cd0d89d"
    );
    assert_eq!(
        template_tag_v1("abc").as_str(),
        "a9993e364706816aba3e25717850c26c9cd0d89d"
    );
    // No normalization: a BOM, CRLF and a trailing LF change the value.
    for (a, b) in [
        ("\u{feff}<p>", "<p>"),
        ("<p>\r\n", "<p>\n"),
        ("<p>\n", "<p>"),
        ("<P>", "<p>"),
    ] {
        assert_ne!(etag_v1(a.as_bytes()), etag_v1(b.as_bytes()));
        assert_ne!(template_tag_v1(a), template_tag_v1(b));
        assert_eq!(etag_v1(a.as_bytes()), sha1_hex(a.as_bytes()));
        assert_eq!(template_tag_v1(a), sha1_hex(a.as_bytes()));
    }
    // Bytes that are not UTF-8 still hash (the etag is over the body).
    assert_eq!(etag_v1(b"\xff\xfe"), sha1_hex(b"\xff\xfe"));
}

#[test]
fn legacy_split_debug_holds_no_page_content() {
    let outcome = split_v1(
        b"<title>SECRET-T</title><!--sonicdiff-a-->SECRET-A<!--sonicdiff-a-end-->",
        &TemplateLimits::default(),
    );
    let text = format!("{outcome:?}");
    assert!(!text.contains("SECRET"), "{text}");
    assert!(text.contains("{a}"), "{text}");
    let block = DataBlock {
        key: "{k}".to_owned(),
        value: "SECRET".to_owned(),
    };
    assert!(!format!("{block:?}").contains("SECRET"));
}
