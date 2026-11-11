// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The response fields that the client reads (negotiation section 6.3, field
//! parts): `Rq-Version`, the media type and the charset of `Content-Type`, the
//! UTF-8 check of a full document and the `ETag` of a `304`. Expected values
//! come from the reference client (`client.mjs`, `http.mjs`) and the client
//! fixtures `FX-RQ-C04`, `C22` and `C26`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

mod common;

use proptest::prelude::*;
use rivqen_integrity::PageRevision;
use rivqen_model::http::FieldLine;
use rivqen_proto::fields::{ContentTypeCharset, DELTA_MEDIA_TYPE, HTML_MEDIA_TYPE, ResponseHead};

const R: &str = "r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8";
const OTHER_R: &str = "r1.psbsc6whgs8ooESisNWp6E3PrQfkg4D69WmXAo12pbI";

// Every read goes through the checked head (DL-153). The limits here are far
// above any test input; the limit behavior is in `fields_head_limit.rs`.
fn head(fields: &[FieldLine]) -> ResponseHead<'_> {
    ResponseHead::check(fields, usize::MAX, usize::MAX).expect("limits are not reached")
}

fn combined_value<'a>(fields: &'a [FieldLine], name: &str) -> Option<std::borrow::Cow<'a, str>> {
    head(fields).combined_value(name)
}

fn rq_version_is_1(fields: &[FieldLine]) -> bool {
    head(fields).rq_version_is_1()
}

fn media_type(fields: &[FieldLine]) -> Option<String> {
    head(fields).media_type()
}

fn content_type_charset(fields: &[FieldLine]) -> ContentTypeCharset {
    head(fields).content_type_charset()
}

fn decoded_as_utf8(fields: &[FieldLine], body: &[u8]) -> bool {
    head(fields).decoded_as_utf8(body)
}

fn etag_matches_snapshot(fields: &[FieldLine], r: &PageRevision) -> bool {
    head(fields).etag_matches_snapshot(r)
}

fn fields(lines: &[(&str, &str)]) -> Vec<FieldLine> {
    lines.iter().map(|(n, v)| FieldLine::new(*n, *v)).collect()
}

fn one(name: &str, value: &str) -> Vec<FieldLine> {
    fields(&[(name, value)])
}

// --- Rq-Version (sections 3.2, 6.3 item 3, 6.4 item 2) --------------------

#[test]
fn fields_response_rq_version_accepts_only_integer_1() {
    for value in ["1", "1;x", "1; a=2;b", "01", "  1  ", "\t1\t", "1;v=\"2\""] {
        assert!(rq_version_is_1(&one("rq-version", value)), "{value:?}");
    }
    for value in [
        "", "2", "0", "-1", "1.0", "\"1\"", "?1", "@1", "one", "1 2", "1,", "(1)", "%\"1\"",
        ":AQ==:", "1;", "1;A", "١",
    ] {
        assert!(!rq_version_is_1(&one("rq-version", value)), "{value:?}");
    }
    assert!(!rq_version_is_1(&[]), "absent");
}

#[test]
fn fields_response_rq_version_name_is_case_insensitive() {
    assert!(rq_version_is_1(&one("Rq-Version", "1")));
    assert!(rq_version_is_1(&one("RQ-VERSION", "1")));
    assert!(!rq_version_is_1(&one("Rq-Versions", "1")));
}

#[test]
fn fields_response_rq_version_uses_the_combined_value() {
    // Two lines are a List, not an Item (negotiation section 4, item 11).
    assert!(!rq_version_is_1(&fields(&[
        ("rq-version", "1"),
        ("rq-version", "1")
    ])));
    // A String split over two lines gets the join inside it (RFC 9651
    // section 4.2), so this combined value is one Item: the Integer 1.
    assert!(rq_version_is_1(&fields(&[
        ("rq-version", "1;a=\"x"),
        ("rq-version", "y\"")
    ])));
    // An empty line gives a trailing ", " (DL-029): the parse fails.
    assert!(!rq_version_is_1(&fields(&[
        ("rq-version", "1"),
        ("rq-version", "")
    ])));
    // Other fields do not take part.
    assert!(rq_version_is_1(&fields(&[
        ("x-rq-version", "2"),
        ("rq-version", "1")
    ])));
}

#[test]
fn fields_response_combined_value_joins_trimmed_lines() {
    assert_eq!(combined_value(&[], "a"), None);
    assert_eq!(
        combined_value(&one("A", " \tx \t"), "a").as_deref(),
        Some("x")
    );
    assert_eq!(
        combined_value(
            &fields(&[("a", " x"), ("b", "y"), ("A", "z "), ("a", "")]),
            "a"
        )
        .as_deref(),
        Some("x, z, ")
    );
}

// --- Media type (section 6.3, items 2 and 3) ------------------------------

#[test]
fn fields_response_media_type_is_the_lower_case_essence() {
    let cases = [
        ("text/html; charset=utf-8", Some(HTML_MEDIA_TYPE)),
        ("Application/Vnd.Rivqen.Delta+JSON", Some(DELTA_MEDIA_TYPE)),
        (
            " application/vnd.rivqen.delta+json ;x=1",
            Some(DELTA_MEDIA_TYPE),
        ),
        ("; charset=utf-8", None),
        ("", None),
        (" \t", None),
        (", text/html; charset=utf-8", Some(", text/html")),
    ];
    for (value, want) in cases {
        assert_eq!(
            media_type(&one("content-type", value)).as_deref(),
            want,
            "{value:?}"
        );
    }
    assert_eq!(media_type(&[]), None);
    assert_eq!(
        media_type(&fields(&[
            ("content-type", "text/html"),
            ("Content-Type", "text/html")
        ]))
        .as_deref(),
        Some("text/html, text/html")
    );
}

// --- Charset rule (section 4, items 1 to 3; H-62) --------------------------

/// Each value and the class that `contentTypeCharset` (http.mjs) gives.
#[test]
fn fields_response_content_type_charset_matches_the_oracle() {
    use ContentTypeCharset::{Absent, Other, Utf8};
    let cases = [
        ("text/html; charset=utf-8", Utf8),
        ("text/html;charset=\"UTF-8\"", Utf8),
        ("text/html; charset=\" utf8 \"", Utf8),
        ("text/html; charset=iso-2022-jp", Other),
        ("text/html", Absent),
        ("text/html; charset=", Absent),
        ("text/html; charset=\"\"", Other),
        ("text/html; charset=\"utf-8", Utf8),
        ("text/html; charset =\"utf-8\"", Absent),
        ("text/html; charset=utf-8, text/html", Other),
        ("text/html; foo=\"a;b\"; charset=utf-8", Utf8),
        ("text/html; charset=\\\"utf-8\\\"", Other),
        ("text/html; charset=\"utf\\-8\"", Utf8),
        ("text/html;;charset=utf-8", Utf8),
        ("text/html; charset=utf-8 \t", Utf8),
        ("text/html; charset=x; charset=utf-8", Other),
        ("text/html; charset=\u{20ac}; charset=utf-8", Utf8),
        ("text/html; charset=\"a,b\"", Other),
        ("text/html; CHARSET=UNICODE-1-1-UTF-8", Utf8),
        ("text/html; charset=\x0cutf-8\x0c", Absent),
        (", text/html; charset=utf-8", Utf8),
        ("text/html; charset", Absent),
        ("text/html; charset;charset=utf-8", Utf8),
        ("text/html; charset=\"utf-8\"x", Utf8),
    ];
    for (value, want) in cases {
        assert_eq!(
            content_type_charset(&one("content-type", value)),
            want,
            "{value:?}"
        );
    }
}

#[test]
fn fields_response_content_type_charset_labels() {
    for label in [
        "unicode-1-1-utf-8",
        "unicode11utf8",
        "unicode20utf8",
        "utf-8",
        "utf8",
        "x-unicode20utf8",
        "UTF8",
        "\" utf-8 \"",
    ] {
        let value = format!("text/html; charset={label}");
        assert_eq!(
            content_type_charset(&one("content-type", &value)),
            ContentTypeCharset::Utf8,
            "{value:?}"
        );
    }
    for label in ["utf-16", "utf_8", "utf-8x", "\"utf-8\u{ff}\""] {
        let value = format!("text/html; charset={label}");
        assert_eq!(
            content_type_charset(&one("content-type", &value)),
            ContentTypeCharset::Other,
            "{value:?}"
        );
    }
    // FF, LF and characters above U+00FF are not quoted-string text: the
    // parameter does not count.
    for label in ["\"\x0c utf-8 \n\"", "u\u{0307}tf-8"] {
        let value = format!("text/html; charset={label}");
        assert_eq!(
            content_type_charset(&one("content-type", &value)),
            ContentTypeCharset::Absent,
            "{value:?}"
        );
    }
}

#[test]
fn fields_response_content_type_charset_quoted_values() {
    use ContentTypeCharset::{Other, Utf8};
    for (value, want) in [
        // A backslash at the end of a quoted string stays.
        ("text/html; charset=\"utf-8\\", Other),
        // A quoted value of another parameter is skipped as a whole.
        ("text/html; a=\"x;charset=latin1\"; charset=utf-8", Utf8),
        // A character above U+00FF is not quoted-string text: the parameter
        // is skipped and the next one counts.
        ("text/html; charset=\"\u{100}\"; charset=utf-8", Utf8),
        // U+00FF is quoted-string text, so the first parameter counts.
        ("text/html; charset=\"\u{ff}\"; charset=utf-8", Other),
        ("text/html; charset=\"\t\"", Other),
    ] {
        assert_eq!(
            content_type_charset(&one("content-type", value)),
            want,
            "{value:?}"
        );
    }
}

#[test]
fn fields_response_content_type_charset_needs_one_line_and_one_type() {
    assert_eq!(content_type_charset(&[]), ContentTypeCharset::Absent);
    assert_eq!(
        content_type_charset(&fields(&[
            ("content-type", "text/html; charset=utf-8"),
            ("Content-Type", "text/html; charset=utf-8"),
        ])),
        ContentTypeCharset::Other
    );
    assert_eq!(
        content_type_charset(&one("content-type", ",, text/html; charset=utf-8 ,")),
        ContentTypeCharset::Utf8
    );
    assert_eq!(
        content_type_charset(&one("content-type", "")),
        ContentTypeCharset::Other
    );
    assert_eq!(
        content_type_charset(&one("content-type", "text/html; x=\"a,b\", text/plain")),
        ContentTypeCharset::Other
    );
    assert_eq!(
        content_type_charset(&one(
            "content-type",
            "text/html; x=\"a\\\",b\"; charset=utf-8"
        )),
        ContentTypeCharset::Utf8
    );
}

#[test]
fn fields_response_decoded_as_utf8_bom_or_charset() {
    let iso = one("content-type", "text/html; charset=iso-2022-jp");
    let utf8 = one("content-type", "text/html; charset=utf-8");
    assert!(decoded_as_utf8(&iso, b"\xef\xbb\xbf<!doctype html>"));
    assert!(!decoded_as_utf8(&iso, b"<!doctype html>"));
    assert!(decoded_as_utf8(&utf8, b"<!doctype html>"));
    assert!(!decoded_as_utf8(&[], b"\xef\xbb<"));
    assert!(!decoded_as_utf8(&[], b""));
}

// --- ETag of a 304 (section 6.3, item 1.3) --------------------------------

#[test]
fn fields_response_etag_weak_comparison_with_the_snapshot() {
    let r = PageRevision::parse(R).unwrap();
    assert!(etag_matches_snapshot(&[], &r), "no ETag");
    for value in [
        format!("\"{R}\""),
        format!("W/\"{R}\""),
        format!(" \tW/\"{R}\" "),
    ] {
        assert!(etag_matches_snapshot(&one("ETag", &value), &r), "{value}");
    }
    for value in [
        format!("\"{OTHER_R}\""),
        format!("w/\"{R}\""),
        format!("W/ \"{R}\""),
        format!("W/W/\"{R}\""),
        R.to_owned(),
        format!("\"{R}\"x"),
        format!("\"{R}"),
        String::new(),
        "*".to_owned(),
    ] {
        assert!(!etag_matches_snapshot(&one("etag", &value), &r), "{value}");
    }
    // Two lines: the combined value is a list, never one tag.
    let tag = format!("\"{R}\"");
    assert!(!etag_matches_snapshot(
        &fields(&[("etag", &tag), ("etag", &tag)]),
        &r
    ));
}

// --- Client fixtures -------------------------------------------------------

/// The response field lines, the body and the outcome of a client fixture,
/// read through `rivqen-testkit`.
fn client(id: &str) -> (Vec<FieldLine>, Vec<u8>, String) {
    let fixture = common::client_fixture(id);
    let response = fixture.client_response().unwrap();
    let body = fixture.response_body.clone().unwrap_or_default();
    let outcome = fixture.client_result().unwrap().outcome;
    (response.headers, body, outcome)
}

#[test]
fn fields_response_utf8_check_matches_c26_fixtures() {
    for (id, utf8) in [
        ("FX-RQ-C26-BOM-CHARSET-ISO-2022-JP", true),
        ("FX-RQ-C26-BOM-NO-CHARSET", true),
        ("FX-RQ-C26-CHARSET-ISO-2022-JP", false),
        ("FX-RQ-C26-NO-CHARSET", false),
        ("FX-RQ-C26-TWO-CONTENT-TYPE-LINES", false),
        ("FX-RQ-C05-TYPE-UPPERCASE", true),
    ] {
        let (f, body, outcome) = client(id);
        assert!(rq_version_is_1(&f), "{id}");
        assert_eq!(media_type(&f).as_deref(), Some(HTML_MEDIA_TYPE), "{id}");
        assert_eq!(decoded_as_utf8(&f, &body), utf8, "{id}");
        // The fixture outcome: a document that fails the check is rejected.
        assert_eq!(outcome != "rejected", utf8, "{id}");
    }
}

#[test]
fn fields_response_rq_version_matches_c04_and_c15_fixtures() {
    for id in ["FX-RQ-C04-NO-RQ-VERSION", "FX-RQ-C15-NO-RQP-SERVER"] {
        let (f, _, outcome) = client(id);
        assert!(!rq_version_is_1(&f), "{id}");
        assert_eq!(outcome, "normal", "{id}");
    }
}

#[test]
fn fields_response_etag_matches_c22_fixtures() {
    for (id, matches) in [
        ("FX-RQ-C22-304-NO-ETAG", true),
        ("FX-RQ-C22-304-WEAK-ETAG", true),
        ("FX-RQ-C22-304-OTHER-ETAG", false),
    ] {
        let (f, _, outcome) = client(id);
        let ctx = common::client_fixture(id).client_context_data().unwrap();
        let r = PageRevision::parse(&ctx.snapshot.unwrap().page_revision).unwrap();
        assert_eq!(etag_matches_snapshot(&f, &r), matches, "{id}");
        assert_eq!(outcome == "not-modified", matches, "{id}");
    }
}

// --- No panic and linear reads ---------------------------------------------

fn hostile_value() -> impl Strategy<Value = String> {
    const ALPHABET: &[u8] = b" \t\r\n\x0c,;=\"\\:/W*-.01aA";
    let unit = prop_oneof![
        3 => prop::sample::select(ALPHABET).prop_map(|c| char::from(c).to_string()),
        1 => (prop::sample::select(ALPHABET), 1..256usize)
            .prop_map(|(c, n)| char::from(c).to_string().repeat(n)),
        1 => Just("charset=utf-8".to_owned()),
        1 => Just("\u{ff}\u{100}".to_owned()),
    ];
    prop::collection::vec(unit, 0..48).prop_map(|v| v.concat())
}

proptest! {
    #[test]
    fn fields_response_reads_never_panic(
        lines in prop::collection::vec(
            (prop::sample::select(&["content-type", "etag", "rq-version", "x"][..]), hostile_value()),
            0..4,
        ),
        body in prop::collection::vec(any::<u8>(), 0..4),
    ) {
        let f: Vec<FieldLine> = lines.iter().map(|(n, v)| FieldLine::new(*n, v.clone())).collect();
        let r = PageRevision::parse(R).unwrap();
        let _a = rq_version_is_1(&f);
        let _b = media_type(&f);
        let _c = content_type_charset(&f);
        let _d = decoded_as_utf8(&f, &body);
        let _e = etag_matches_snapshot(&f, &r);
    }
}

#[test]
fn fields_response_reads_are_linear() {
    use std::time::{Duration, Instant};
    const LIMIT: Duration = Duration::from_secs(2);
    let n = 64 * 1024;
    for value in [
        format!("text/html{}", ";".repeat(n)),
        format!("text/html; charset=\"{}", "\\\"".repeat(n / 2)),
        format!("text/html; charset=utf-8{}x", " ".repeat(n)),
        format!("text/html{}", " ; a".repeat(n / 4)),
        format!("text/html; {}", "charset=;".repeat(n / 9)),
        format!("{}text/html", ",".repeat(n)),
        "\"\\".repeat(n / 2),
        format!("W/{}", " ".repeat(n)),
    ] {
        let f = vec![
            FieldLine::new("content-type", value.clone()),
            FieldLine::new("etag", value.clone()),
            FieldLine::new("rq-version", value),
        ];
        let r = PageRevision::parse(R).unwrap();
        let start = Instant::now();
        let _a = rq_version_is_1(&f);
        let _b = media_type(&f);
        let _c = content_type_charset(&f);
        let _e = etag_matches_snapshot(&f, &r);
        let took = start.elapsed();
        assert!(took < LIMIT, "{took:?}");
    }
}

#[test]
fn fields_response_charset_needs_a_semicolon_before_the_parameter() {
    for v in ["xcharset=utf-8", "text/htmlcharset=utf-8"] {
        assert_eq!(
            content_type_charset(&one("Content-Type", v)),
            ContentTypeCharset::Absent,
            "{v}"
        );
        assert!(!decoded_as_utf8(&one("Content-Type", v), b"x"), "{v}");
    }
}

#[test]
fn fields_response_charset_skips_the_rest_after_a_quoted_value() {
    let f = one(
        "Content-Type",
        "text/html;a=\"b\"xcharset=bad;charset=utf-8",
    );
    assert_eq!(content_type_charset(&f), ContentTypeCharset::Utf8);
}
