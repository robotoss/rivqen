// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Tests of the manifest locator and the head walk (WP-06 S2 T-01).
//!
//! Normative text: docs/engineering/protocol/manifest-patch.md section 1.7
//! (client items 1, 3, 4, 5; "Document head" and its table of insertion
//! points). The expected values come from that text, not from another
//! implementation (DL-173).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::ops::Range;

use proptest::prelude::*;
use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_template::limits::TemplateLimits;
use rivqen_template::rqp::locate::{
    LocateError, ManifestLocation, insertion_point, locate_manifest,
};
use rivqen_template::rqp::scan::{Tag, Token, scan};
use rivqen_template::rqp::{MarkupCode, split_rqp};

/// The start tag that the server writes (section 1.7, server item 1).
const START: &str = r#"<script type="application/rivqen-manifest+json">"#;
const END: &str = "</script>";
/// A manifest text; the locator does not read it.
const TEXT: &str = r#"{"protocol":"rqp/1"}"#;

fn element() -> String {
    format!("{START}{TEXT}{END}")
}

fn point(document: &str) -> Option<usize> {
    insertion_point(document.as_bytes(), &TemplateLimits::default()).unwrap()
}

/// The offset just after the first occurrence of `marker`.
fn after(document: &str, marker: &str) -> usize {
    document.find(marker).unwrap() + marker.len()
}

/// The offset of the first occurrence of `marker`.
fn at(document: &str, marker: &str) -> usize {
    document.find(marker).unwrap()
}

fn locate(full: &str) -> Result<ManifestLocation, LocateError> {
    locate_manifest(full.as_bytes(), &TemplateLimits::default())
}

/// F: `original` with the manifest element inserted at `offset`.
fn insert(original: &str, offset: usize) -> String {
    format!(
        "{}{}{}",
        &original[..offset],
        element(),
        &original[offset..]
    )
}

// ---------------------------------------------------------------------------
// insertion_point: one test per row of the table of section 1.7.
// ---------------------------------------------------------------------------

#[test]
fn locate_point_normal_page() {
    let o = "<!DOCTYPE html>\n<html>\n<head>\n<title>x</title>\n</head>\n<body></body></html>";
    assert_eq!(point(o), Some(after(o, "<head>")));
}

#[test]
fn locate_point_comment_doctype_html_lang_before_head() {
    let o = "<!-- c -->\n<!DOCTYPE html>\n<html lang=\"en\">\n<head><title>x</title>";
    assert_eq!(point(o), Some(after(o, "<head>")));
}

#[test]
fn locate_point_stray_end_tag_before_head() {
    let o = "<!DOCTYPE html><html></p><head><title>x</title>";
    assert_eq!(point(o), Some(after(o, "<head>")));
}

#[test]
fn locate_point_head_without_end_tag_body_closes_it() {
    let o = "<html><head><title>x</title><body><p>y</p></body></html>";
    assert_eq!(point(o), Some(after(o, "<head>")));
}

#[test]
fn locate_point_head_with_noscript_img() {
    let o = "<html><head><noscript><img src=\"a.png\"></noscript></head><body></body>";
    assert_eq!(point(o), Some(after(o, "<head>")));
}

#[test]
fn locate_point_head_then_template_with_head_in_body() {
    let o = "<html><head></head><body><template><head></head></template></body>";
    assert_eq!(point(o), Some(after(o, "<head>")));
}

#[test]
fn locate_point_no_head_start_tag_title_is_implied_head() {
    let o = "<!DOCTYPE html>\n<html>\n  <title data-rq-block=\"t\">x</title>\n</head>\n<body>";
    assert_eq!(point(o), Some(at(o, "<title")));
}

#[test]
fn locate_point_meta_before_head_is_implied_head() {
    let o = "<!DOCTYPE html><html><meta charset=\"utf-8\"><head><title>x</title>";
    assert_eq!(point(o), Some(at(o, "<meta")));
}

#[test]
fn locate_point_text_before_html_or_head_is_none() {
    assert_eq!(point("x<html><head>"), None);
    assert_eq!(point("<html>x<head>"), None);
    assert_eq!(point("<html> &nbsp; <head>"), None);
}

#[test]
fn locate_point_noscript_or_template_before_head_is_none() {
    assert_eq!(point("<html><noscript></noscript><head>"), None);
    assert_eq!(point("<html><template></template><head>"), None);
}

#[test]
fn locate_point_body_or_body_content_first_is_none() {
    assert_eq!(point("<html><body><head>"), None);
    assert_eq!(point("<html><div><head>"), None);
    assert_eq!(point("<html><p>x</p><head>"), None);
}

#[test]
fn locate_point_br_body_html_head_end_tags_before_head_are_none() {
    for end in [
        "</br>", "</body>", "</html>", "</head>", "</BR>", "</Head >",
    ] {
        let o = format!("<html>{end}<head>");
        assert_eq!(point(&o), None, "{end}");
    }
}

#[test]
fn locate_point_head_only_in_a_comment_is_none() {
    assert_eq!(point("<html><!-- <head> --><body>"), None);
    assert_eq!(point("<!-- <head> -->"), None);
}

// ---------------------------------------------------------------------------
// insertion_point: the details of items 1 and 2.
// ---------------------------------------------------------------------------

#[test]
fn locate_point_every_white_space_byte_and_bom() {
    let o = "\u{feff}\t\n\x0c\r <html>\t\n\x0c\r <head>";
    assert_eq!(point(o), Some(o.len()));
    // A byte order mark that is not at offset 0 is not white space.
    assert_eq!(point("<html>\u{feff}<head>"), None);
    assert_eq!(point(" \u{feff}<head>"), None);
    // Other space characters are not white space here.
    assert_eq!(point("\u{a0}<head>"), None);
    assert_eq!(point("\x0b<head>"), None);
    // Only the 3 bytes of the byte order mark at offset 0 count.
    assert_eq!(point("\u{feff}x<head>"), None);
    assert_eq!(point("\u{feff}\u{feff}<head>"), None);
}

#[test]
fn locate_point_empty_slash_tag_gap_is_none() {
    // `</>` gives no token: it is a gap with other bytes.
    assert_eq!(point("<html></><head>"), None);
}

#[test]
fn locate_point_end_of_input_is_none() {
    assert_eq!(point(""), None);
    assert_eq!(point("<!DOCTYPE html><html>  "), None);
    // A head start tag that the end of the input cuts gives no token.
    assert_eq!(point("<html><head"), None);
}

#[test]
fn locate_point_names_are_ascii_case_insensitive() {
    let o = "<HTML><HEAD>";
    assert_eq!(point(o), Some(o.len()));
    let o = "<Html><MeTa charset=utf-8>";
    assert_eq!(point(o), Some(at(o, "<MeTa")));
    // Not an ASCII case mapping: the Kelvin sign is not `k`.
    assert_eq!(point("<html><hea\u{212a}>"), None);
}

#[test]
fn locate_point_attributes_and_self_closing_flag_have_no_effect() {
    let o = "<html><head lang=\"en\" data-x='>'>";
    assert_eq!(point(o), Some(o.len()));
    let o = "<html><head/><title>x</title>";
    assert_eq!(point(o), Some(after(o, "<head/>")));
    let o = "<html><link/ rel=x>";
    assert_eq!(point(o), Some(at(o, "<link")));
    let o = "<html/><head>";
    assert_eq!(point(o), Some(o.len()));
}

#[test]
fn locate_point_every_implied_head_start_tag() {
    for name in [
        "base", "basefont", "bgsound", "link", "meta", "noframes", "script", "style", "title",
    ] {
        let o = format!("<!DOCTYPE html>\n<html>\n<{name}>");
        assert_eq!(point(&o), Some(at(&o, &format!("<{name}>"))), "{name}");
    }
    for name in [
        "noscript", "template", "body", "div", "p", "svg", "frameset", "img", "a",
    ] {
        assert_eq!(point(&format!("<html><{name}>")), None, "{name}");
    }
}

#[test]
fn locate_point_ignored_end_tags_and_comments_continue() {
    let o = "</div></HEADER><?pi?><!x><![CDATA[c]]></ p><!DOCTYPE html></span x=1><html><head>";
    assert_eq!(point(o), Some(o.len()));
}

#[test]
fn locate_point_tokens_after_the_stop_do_not_matter() {
    let o = "<head>x</body><head><svg><p>";
    assert_eq!(point(o), Some(6));
    let o = "<meta><head lang=en>";
    assert_eq!(point(o), Some(0));
}

#[test]
fn locate_point_scan_stop_is_an_error() {
    let limits = TemplateLimits::new(16, 256, 1 << 20, 128, 256);
    let error = insertion_point(b"<html><head>.........", &limits).unwrap_err();
    assert_eq!(error.code(), MarkupCode::Limit);
    assert_eq!(error.rule(), "M-01");
    // The stack limit of the scan: `html` start tags push.
    let limits = TemplateLimits::new(1 << 20, 256, 1 << 20, 128, 2);
    let o = "<html>".repeat(5) + "<head>";
    let error = insertion_point(o.as_bytes(), &limits).unwrap_err();
    assert_eq!(error.code(), MarkupCode::Limit);
    assert_eq!(error.rule(), "M-26");
}

// ---------------------------------------------------------------------------
// locate_manifest: item 1, the type attribute.
// ---------------------------------------------------------------------------

fn located_with_type(type_attribute: &str) -> bool {
    let full = format!("<html><head><script {type_attribute}>{TEXT}</script><title>x</title>");
    match locate(&full) {
        Ok(_) => true,
        Err(LocateError::NoManifest) => false,
        Err(other) => panic!("{type_attribute}: {other:?}"),
    }
}

#[test]
fn locate_type_exact_value_matches() {
    assert!(located_with_type(
        r#"type="application/rivqen-manifest+json""#
    ));
    assert!(located_with_type("type='application/rivqen-manifest+json'"));
    assert!(located_with_type("type=application/rivqen-manifest+json"));
}

#[test]
fn locate_type_ascii_case_insensitive() {
    assert!(located_with_type(
        r#"TYPE="APPLICATION/RIVQEN-MANIFEST+JSON""#
    ));
    assert!(located_with_type(
        r#"Type="Application/Rivqen-Manifest+Json""#
    ));
    let full =
        format!("<html><head><SCRIPT type=\"application/rivqen-manifest+json\">{TEXT}</SCRIPT>");
    assert!(locate(&full).is_ok());
}

#[test]
fn locate_type_other_values_do_not_match() {
    for value in [
        r#"type=" application/rivqen-manifest+json""#,
        r#"type="application/rivqen-manifest+json ""#,
        r#"type="application/rivqen-manifest+json;charset=utf-8""#,
        r#"type="application/rivqen-manifest+jso""#,
        r#"type="application/rivqen-manifest+jsonn""#,
        r#"type="application/rivqen-manifest&#43;json""#,
        r#"type="&#97;pplication/rivqen-manifest+json""#,
        r#"type="application/json""#,
        r#"type="""#,
        "type",
        r#"data-type="application/rivqen-manifest+json""#,
        r#"types="application/rivqen-manifest+json""#,
    ] {
        assert!(!located_with_type(value), "{value}");
    }
}

#[test]
fn locate_type_non_ascii_never_matches() {
    // U+0131 (dotless i) and U+212A (Kelvin sign) fold to ASCII in Unicode.
    assert!(!located_with_type(
        "type=\"application/r\u{131}vqen-manifest+json\""
    ));
    assert!(!located_with_type(
        "type=\"application/rivqen-manifest+jsonK\""
    ));
    assert!(!located_with_type(
        "type=\"application/rivqen-manifest+json\u{212a}\""
    ));
    assert!(!located_with_type(
        "type=\"application/rivqen-manifest+jso\u{e9}\""
    ));
}

#[test]
fn locate_type_only_the_first_type_attribute_counts() {
    assert!(!located_with_type(
        r#"type="text/plain" type="application/rivqen-manifest+json""#
    ));
    assert!(located_with_type(
        r#"type="application/rivqen-manifest+json" type="text/plain""#
    ));
    assert!(located_with_type(
        r#"async type="application/rivqen-manifest+json""#
    ));
}

#[test]
fn locate_type_only_script_start_tags() {
    for name in ["style", "template", "div", "scripts"] {
        let full = format!(
            "<html><head><{name} type=\"application/rivqen-manifest+json\">{TEXT}</{name}>"
        );
        assert_eq!(
            locate(&full).unwrap_err(),
            LocateError::NoManifest,
            "{name}"
        );
    }
    // An end tag with the attribute is not a start tag.
    let full = format!("<html><head></script type=\"application/rivqen-manifest+json\">{TEXT}");
    assert_eq!(locate(&full).unwrap_err(), LocateError::NoManifest);
}

#[test]
fn locate_type_not_in_comments_or_raw_text() {
    let e = element();
    for full in [
        format!("<html><head><!-- {e} -->"),
        format!("<html><head><title>{e}</title>"),
        format!("<html><head><style>{e}</style>"),
        format!("<html><head><script>var s = '{e}';</script>"),
        format!("<html><head><noscript>{e}</noscript>"),
        format!("<html><head><textarea>{e}</textarea>"),
        format!("<html><head><p title='{e}'>"),
    ] {
        assert_eq!(
            locate(&full).unwrap_err(),
            LocateError::NoManifest,
            "{full}"
        );
    }
}

#[test]
fn locate_type_first_match_wins() {
    let o = "<html><head><title>x</title></head><body></body>";
    let full = insert(o, after(o, "<head>"));
    let full = format!("{full}{}", element());
    let location = locate(&full).unwrap();
    assert_eq!(location.element().start, after(o, "<head>"));
}

#[test]
fn locate_type_no_manifest() {
    assert_eq!(locate("").unwrap_err(), LocateError::NoManifest);
    assert_eq!(
        locate("<html><head><title>x</title>").unwrap_err(),
        LocateError::NoManifest
    );
}

// ---------------------------------------------------------------------------
// locate_manifest: item 4, the manifest text M; item 5, O.
// ---------------------------------------------------------------------------

#[test]
fn locate_text_is_between_the_tags() {
    let o = "<!DOCTYPE html><html><head><title>x</title>";
    let full = insert(o, after(o, "<head>"));
    let location = locate(&full).unwrap();
    let start = after(o, "<head>");
    assert_eq!(
        location.element(),
        start..start + START.len() + TEXT.len() + END.len()
    );
    assert_eq!(
        location.text(),
        start + START.len()..start + START.len() + TEXT.len()
    );
    assert_eq!(
        location.manifest_text(full.as_bytes()).unwrap(),
        TEXT.as_bytes()
    );
    assert_eq!(location.original(full.as_bytes()).unwrap(), o.as_bytes());
}

#[test]
fn locate_text_can_be_empty() {
    let full = format!("<head>{START}{END}");
    let location = locate(&full).unwrap();
    assert!(location.text().is_empty());
    assert_eq!(location.manifest_text(full.as_bytes()).unwrap(), b"");
    assert_eq!(location.original(full.as_bytes()).unwrap(), b"<head>");
}

#[test]
fn locate_text_end_tag_forms() {
    for end in [
        "</SCRIPT>",
        "</script >",
        "</script\t>",
        "</script/>",
        "</script x=\"y\">",
    ] {
        let full = format!("<head>{START}{TEXT}{end}<title>t</title>");
        let location = locate(&full).unwrap();
        assert_eq!(
            location.manifest_text(full.as_bytes()).unwrap(),
            TEXT.as_bytes(),
            "{end}"
        );
        assert_eq!(
            location.element().end,
            6 + START.len() + TEXT.len() + end.len()
        );
    }
}

#[test]
fn locate_text_ends_at_the_end_tag_of_the_tokenizer() {
    // `</scripts>` is not an appropriate end tag; the text goes on.
    let full = format!("<head>{START}a</scripts>b{END}");
    let location = locate(&full).unwrap();
    assert_eq!(
        location.manifest_text(full.as_bytes()).unwrap(),
        b"a</scripts>b"
    );
    // In the double-escaped state the first `</script>` does not end the
    // script data.
    let full = format!("<head>{START}<!--<script>x</script>-->{END}");
    let location = locate(&full).unwrap();
    assert_eq!(
        location.manifest_text(full.as_bytes()).unwrap(),
        b"<!--<script>x</script>-->"
    );
}

#[test]
fn locate_text_without_end_tag_is_unterminated() {
    for full in [
        format!("<head>{START}{TEXT}"),
        format!("<head>{START}{TEXT}</script"),
        format!("<head>{START}{TEXT}</scrip>"),
        format!("<head>{START}<!--<script>{TEXT}</script>"),
        format!("<head>{START}"),
    ] {
        assert_eq!(
            locate(&full).unwrap_err(),
            LocateError::Unterminated,
            "{full}"
        );
    }
}

#[test]
fn locate_text_self_closing_start_tag_still_has_text() {
    // M-04: the state changes also with the self-closing flag.
    let start = r#"<script type="application/rivqen-manifest+json"/>"#;
    let full = format!("<head>{start}{TEXT}{END}");
    let location = locate(&full).unwrap();
    assert_eq!(
        location.manifest_text(full.as_bytes()).unwrap(),
        TEXT.as_bytes()
    );
}

// ---------------------------------------------------------------------------
// locate_manifest: item 3, the position on O.
// ---------------------------------------------------------------------------

#[test]
fn locate_position_after_head_start_tag() {
    let o = "<!DOCTYPE html>\n<html>\n<head lang=\"en\">\n<title>x</title>";
    assert!(locate(&insert(o, after(o, "<head lang=\"en\">"))).is_ok());
}

#[test]
fn locate_position_white_space_after_head_rejects() {
    let o = "<html><head>\n  <title>x</title>";
    let full = insert(o, after(o, "<head>\n"));
    assert_eq!(locate(&full).unwrap_err(), LocateError::WrongPosition);
    let full = insert(o, after(o, "<head>\n  "));
    assert_eq!(locate(&full).unwrap_err(), LocateError::WrongPosition);
}

#[test]
fn locate_position_comment_after_head_rejects() {
    let o = "<html><head><!-- h --><title>x</title>";
    let full = insert(o, after(o, "<!-- h -->"));
    assert_eq!(locate(&full).unwrap_err(), LocateError::WrongPosition);
}

#[test]
fn locate_position_implied_head() {
    let o = "<!DOCTYPE html>\n<html>\n<meta charset=\"utf-8\">\n<title>x</title>";
    assert!(locate(&insert(o, at(o, "<meta"))).is_ok());
    // White space before the manifest is before the stop tag in O too.
    assert!(locate(&insert(o, at(o, "\n<meta"))).is_err());
    let o = "<html>\n<title data-rq-block=\"t\">x</title>";
    assert!(locate(&insert(o, at(o, "<title"))).is_ok());
}

#[test]
fn locate_position_white_space_after_implied_head_manifest_rejects() {
    // FX-RQ-C21-IMPLIED-WHITESPACE-AFTER: O has the line feed before
    // `<meta>`, so P is after it.
    let o = "<!DOCTYPE html>\n<html>\n<meta charset=\"utf-8\">";
    let full = insert(o, after(o, "<html>"));
    assert_eq!(locate(&full).unwrap_err(), LocateError::WrongPosition);
    // A comment after the manifest also moves P.
    let o = "<html><!-- c --><meta charset=\"utf-8\">";
    let full = insert(o, after(o, "<html>"));
    assert_eq!(locate(&full).unwrap_err(), LocateError::WrongPosition);
}

#[test]
fn locate_position_before_head_lang_rejects() {
    // A walk on F stops at the manifest `script` (an implied head); the walk
    // on O stops at `<head lang="en">`.
    let o = "<!DOCTYPE html>\n<html>\n<head lang=\"en\">\n<title>x</title>";
    let full = insert(o, at(o, "<head"));
    assert_eq!(locate(&full).unwrap_err(), LocateError::WrongPosition);
}

#[test]
fn locate_position_before_text_body_or_noscript_rejects() {
    for o in [
        "<html>text<head>",
        "<html><body><p>x</p></body>",
        "<html><div>x</div>",
        "<html><noscript><img src=x></noscript><head>",
        "<html><template></template>",
    ] {
        let full = insert(o, after(o, "<html>"));
        assert_eq!(locate(&full).unwrap_err(), LocateError::NoHead, "{o}");
    }
}

#[test]
fn locate_position_old_position_before_head_end_tag_rejects() {
    let o = "<html><head><title>x</title></head><body>";
    let full = insert(o, at(o, "</head>"));
    assert_eq!(locate(&full).unwrap_err(), LocateError::WrongPosition);
}

#[test]
fn locate_position_head_in_template_rejects() {
    let o = "<html><head><title>x</title><body><template><head></head></template>";
    let full = insert(o, after(o, "<template><head>"));
    assert_eq!(locate(&full).unwrap_err(), LocateError::WrongPosition);
}

#[test]
fn locate_position_no_head_in_o_is_no_head() {
    for o in ["", "<html>", "<html></body>", "<!-- c -->", "<html><body>"] {
        let full = insert(o, o.len());
        assert_eq!(locate(&full).unwrap_err(), LocateError::NoHead, "{o:?}");
    }
    // Text in front of the element.
    let full = insert("x<head>", 1);
    assert_eq!(locate(&full).unwrap_err(), LocateError::NoHead);
}

#[test]
fn locate_position_at_offset_zero() {
    // O starts with the implied-head start tag.
    assert!(locate(&insert("<meta charset=utf-8>", 0)).is_ok());
    assert!(locate(&insert("<title>x</title>", 0)).is_ok());
    // O starts with `<head>`: P is 6, not 0.
    assert_eq!(
        locate(&insert("<head>", 0)).unwrap_err(),
        LocateError::WrongPosition
    );
}

#[test]
fn locate_position_short_token_before_the_element() {
    // A 3-byte comment before the element is a token, not a gap.
    assert!(locate(&insert("<!><meta>", 3)).is_ok());
    assert!(locate(&insert("<?><meta>", 3)).is_ok());
    assert_eq!(
        locate(&insert("<a><meta>", 3)).unwrap_err(),
        LocateError::NoHead
    );
}

#[test]
fn locate_position_byte_order_mark() {
    let o = "\u{feff}<html><head><title>x</title>";
    assert!(locate(&insert(o, after(o, "<head>"))).is_ok());
    let o = "\u{feff}<meta charset=utf-8>";
    assert!(locate(&insert(o, 3)).is_ok());
    // The manifest before the byte order mark: O starts with it, P is 3.
    let o = "\u{feff}<meta charset=utf-8>";
    assert_eq!(
        locate(&insert(o, 0)).unwrap_err(),
        LocateError::WrongPosition
    );
}

#[test]
fn locate_position_byte_order_mark_split_by_the_element() {
    let bom = [0xEF_u8, 0xBB, 0xBF];
    let element = element().into_bytes();
    for cut in 1..3 {
        // O is the byte order mark and `<meta>`; F has the element inside
        // the byte order mark. P of O is 3, so the position is wrong.
        let mut full = bom[..cut].to_vec();
        full.extend_from_slice(&element);
        full.extend_from_slice(&bom[cut..]);
        full.extend_from_slice(b"<meta>");
        let result = locate_manifest(&full, &TemplateLimits::default());
        assert_eq!(result.unwrap_err(), LocateError::WrongPosition, "{cut}");
        // Without the rest of the byte order mark, O starts with other bytes.
        let mut full = bom[..cut].to_vec();
        full.extend_from_slice(&element);
        full.extend_from_slice(b"<meta>");
        let result = locate_manifest(&full, &TemplateLimits::default());
        assert_eq!(result.unwrap_err(), LocateError::NoHead, "{cut}");
    }
}

#[test]
fn locate_position_bytes_before_element_and_after_merge_in_o() {
    // `<` before the element and `!-- c -->` after it: O has a comment there,
    // but the `<` is a text byte of F, so the walk stops with no head.
    let full = format!("<html><{}!-- c --><meta>", element());
    assert_eq!(locate(&full).unwrap_err(), LocateError::NoHead);
}

#[test]
fn locate_position_scan_stop_before_match_is_markup() {
    let limits = TemplateLimits::new(32, 256, 1 << 20, 128, 256);
    let full = insert("<head>", 6);
    let error = locate_manifest(full.as_bytes(), &limits).unwrap_err();
    assert_eq!(
        error,
        LocateError::Markup(rivqen_template::rqp::MarkupError::new(
            MarkupCode::Limit,
            "M-01"
        ))
    );
    // Stack limit before the match.
    let limits = TemplateLimits::new(1 << 20, 256, 1 << 20, 128, 4);
    let o = "<div>".repeat(10);
    let full = insert(&o, o.len());
    let error = locate_manifest(full.as_bytes(), &limits).unwrap_err();
    assert!(
        matches!(error, LocateError::Markup(e) if e.code() == MarkupCode::Limit && e.rule() == "M-26")
    );
}

#[test]
fn locate_position_full_stack_before_the_element_is_markup_and_o_is_invalid() {
    // The one allowed difference from the walk on a copy of O (module
    // comment of `locate`): a full token stack of `html` elements before
    // the manifest stops the scan of F. The markup analysis of O rejects
    // such a document anyway (M-26).
    let limits = TemplateLimits::default();
    let o = "<html>".repeat(limits.max_open_elements() + 2)
        + "<meta charset=utf-8><title data-rq-block=\"t\">x</title>";
    let full = insert(&o, at(&o, "<meta"));
    let error = locate_manifest(full.as_bytes(), &limits).unwrap_err();
    assert!(matches!(error, LocateError::Markup(e) if e.code() == MarkupCode::Limit));
    assert_eq!(
        insertion_point(o.as_bytes(), &limits).unwrap(),
        Some(at(&o, "<meta"))
    );
    assert_eq!(
        split_rqp(o.as_bytes(), &limits).unwrap_err().code(),
        MarkupCode::Limit
    );
    // One element less: the scan of F does not stop.
    let o = "<html>".repeat(limits.max_open_elements() + 1) + "<meta charset=utf-8>";
    let full = insert(&o, at(&o, "<meta"));
    assert!(locate_manifest(full.as_bytes(), &limits).is_ok());
    assert_eq!(
        split_rqp(o.as_bytes(), &limits).unwrap_err().code(),
        MarkupCode::Limit
    );
}

#[test]
fn locate_position_order_unterminated_before_position() {
    // No end tag: item 4 fails before item 3 can run on O.
    let full = format!("<html><body>{START}{TEXT}");
    assert_eq!(locate(&full).unwrap_err(), LocateError::Unterminated);
}

// ---------------------------------------------------------------------------
// Fixture bodies (fixtures/rqp/protocol) as example inputs.
// ---------------------------------------------------------------------------

fn fixture_body(id: &str) -> Vec<u8> {
    let path = format!(
        "{}/../../fixtures/rqp/protocol/{id}/response.body",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
fn locate_position_fixture_bodies() {
    use LocateError::{NoManifest, WrongPosition};
    let rows: &[(&str, Result<(), LocateError>)] = &[
        ("FX-RQ-C21-COMMENT-DOCTYPE-BEFORE-HEAD", Ok(())),
        ("FX-RQ-C21-HEAD-NOSCRIPT-IMG", Ok(())),
        ("FX-RQ-C21-HEAD-STRAY-BODY", Ok(())),
        ("FX-RQ-C21-HEAD-STRAY-VOID-END", Ok(())),
        ("FX-RQ-C21-IMPLIED-HEAD-META", Ok(())),
        ("FX-RQ-C21-IMPLIED-HEAD-TITLE", Ok(())),
        ("FX-RQ-C21-BEFORE-HEAD-LANG", Err(WrongPosition)),
        ("FX-RQ-C21-HEAD-COMMENT-FIRST", Err(WrongPosition)),
        ("FX-RQ-C21-HEAD-IN-TEMPLATE", Err(WrongPosition)),
        ("FX-RQ-C21-HEAD-WHITESPACE", Err(WrongPosition)),
        ("FX-RQ-C21-IMPLIED-WHITESPACE-AFTER", Err(WrongPosition)),
        ("FX-RQ-C21-NO-HEAD-START-TAG", Err(WrongPosition)),
        ("FX-RQ-C21-OLD-POSITION", Err(WrongPosition)),
        ("FX-RQ-C05-TYPE-UPPERCASE", Ok(())),
        ("FX-RQ-C05-TYPE-DOTLESS-I", Err(NoManifest)),
    ];
    for (id, expected) in rows {
        let body = fixture_body(id);
        let result = locate_manifest(&body, &TemplateLimits::default()).map(|_| ());
        assert_eq!(&result, expected, "{id}");
    }
    // The position of these three depends on the walk on O, not on F.
    for id in [
        "FX-RQ-C21-MANIFEST-BEFORE-BODY",
        "FX-RQ-C21-MANIFEST-BEFORE-NOSCRIPT",
        "FX-RQ-C21-MANIFEST-BEFORE-TEXT",
    ] {
        let body = fixture_body(id);
        let result = locate_manifest(&body, &TemplateLimits::default());
        assert!(
            matches!(
                result,
                Err(LocateError::NoHead | LocateError::WrongPosition)
            ),
            "{id}: {result:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Errors and Debug.
// ---------------------------------------------------------------------------

#[test]
fn locate_errors_are_manifest_invalid_and_hold_no_bytes() {
    let errors = [
        LocateError::NoManifest,
        LocateError::Unterminated,
        LocateError::NoHead,
        LocateError::WrongPosition,
        LocateError::Markup(rivqen_template::rqp::MarkupError::new(
            MarkupCode::Limit,
            "M-01",
        )),
    ];
    for error in errors {
        assert_eq!(error.code(), ErrorCode::ManifestInvalid);
        assert_eq!(error.to_string(), error.reason());
    }
    let secret = "SECRETqz";
    let full = format!(
        "<html data-s=\"{secret}\"><head>{START}{{\"{secret}\":1}}{END}<title>{secret}</title>"
    );
    let location = locate(&full).unwrap();
    let debug = format!("{location:?}");
    assert!(!debug.contains(secret), "{debug}");
    for full in [
        format!("<html>{secret}{}", element()),
        format!("<head>{START}{secret}"),
        format!("<p>{secret}</p>"),
    ] {
        let error = locate(&full).unwrap_err();
        assert!(!format!("{error:?} {error}").contains(secret));
    }
}

#[test]
fn locate_location_of_another_document_is_none() {
    let full = insert("<head>", 6);
    let location = locate(&full).unwrap();
    assert_eq!(location.manifest_text(b"<head>"), None);
    assert_eq!(location.original(b"<head>"), None);
}

// ---------------------------------------------------------------------------
// Property: locate_manifest is the walk on a copy of O.
// ---------------------------------------------------------------------------

/// What section 1.7 says, written as directly as possible: the first
/// manifest start tag of the scan of F, the next `script` end tag, O as a
/// copy, and P on that copy. `None` when a scan stops (limits).
fn literal(full: &[u8]) -> Option<Result<Range<usize>, LocateError>> {
    let limits = TemplateLimits::default();
    let is_manifest = |tag: &Tag<'_>| -> bool {
        tag.name().eq_ignore_ascii_case(b"script")
            && tag
                .attributes()
                .map(Result::unwrap)
                .find(|a| a.name().eq_ignore_ascii_case(b"type"))
                .is_some_and(|a| {
                    a.value()
                        .eq_ignore_ascii_case(b"application/rivqen-manifest+json")
                })
    };
    let mut tokens = scan(full, &limits);
    let start = loop {
        match tokens.next() {
            None => return Some(Err(LocateError::NoManifest)),
            Some(Err(_)) => return None,
            Some(Ok(Token::StartTag(tag))) if is_manifest(&tag) => break tag.span(),
            Some(Ok(_)) => {}
        }
    };
    let end = loop {
        match tokens.next() {
            None => return Some(Err(LocateError::Unterminated)),
            Some(Err(_)) => return None,
            Some(Ok(Token::EndTag(tag))) if tag.name().eq_ignore_ascii_case(b"script") => {
                break tag.span();
            }
            Some(Ok(_)) => {}
        }
    };
    let mut o = full[..start.start].to_vec();
    o.extend_from_slice(&full[end.end..]);
    match insertion_point(&o, &limits).ok()? {
        None => Some(Err(LocateError::NoHead)),
        Some(p) if p == start.start => Some(Ok(start.start..end.end)),
        Some(_) => Some(Err(LocateError::WrongPosition)),
    }
}

const PIECES: &[&[u8]] = &[
    br#"<script type="application/rivqen-manifest+json">"#,
    b"<SCRIPT TYPE=APPLICATION/RIVQEN-MANIFEST+JSON>",
    br#"<script type="text/plain" type="application/rivqen-manifest+json">"#,
    b"{\"protocol\":\"rqp/1\"}",
    b"</script>",
    b"</SCRIPT >",
    b"<script>",
    b"<!DOCTYPE html>",
    b"<html>",
    b"<html lang=\"en\">",
    b"<head>",
    b"<HEAD lang=x>",
    b"<head/>",
    b"</head>",
    b"<meta charset=\"utf-8\">",
    b"<link/>",
    b"<title>",
    b"</title>",
    b"<style>",
    b"</style>",
    b"<body>",
    b"<div>",
    b"<noscript>",
    b"</noscript>",
    b"<template>",
    b"</template>",
    b"<svg>",
    b"</svg>",
    b"</p>",
    b"</br>",
    b"</>",
    b"<!-- c -->",
    b"<!--",
    b"-->",
    b"<![CDATA[x]]>",
    b"<?pi?>",
    b" ",
    b"\n",
    b"\t",
    b"\x0c",
    b"\r",
    b"text",
    b"<",
    b">",
    b"\xEF\xBB\xBF",
    b"\xEF",
    b"\xBB\xBF",
    b"\0",
];

fn soup() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(prop::sample::select(PIECES), 0..24).prop_map(|pieces| pieces.concat())
}

fn check_against_literal(full: &[u8]) -> Result<(), TestCaseError> {
    let result = locate_manifest(full, &TemplateLimits::default());
    if let Ok(location) = &result {
        let element = location.element();
        let text = location.text();
        prop_assert!(element.start < text.start && text.end < element.end);
        prop_assert!(element.end <= full.len());
        let o = location.original(full).unwrap();
        prop_assert_eq!(o.len() + element.len(), full.len());
        prop_assert_eq!(
            insertion_point(&o, &TemplateLimits::default()).unwrap(),
            Some(element.start)
        );
    }
    if let Some(expected) = literal(full) {
        prop_assert_eq!(result.map(|l| l.element()), expected);
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4096))]

    #[test]
    fn locate_prop_any_bytes(full in prop::collection::vec(any::<u8>(), 0..256)) {
        check_against_literal(&full)?;
    }

    #[test]
    fn locate_prop_soup(full in soup()) {
        check_against_literal(&full)?;
    }
}
