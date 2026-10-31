// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! One test (or more) per rule of legacy wire §10 (C-01…C-16, the four notes
//! of DL-088) and §10.1 (P-01…P-11), through `legacy::respond`. The inputs are
//! written here from the rule text; the expected values are computed from the
//! rule (SHA-1 of the bytes the rule names), not copied from a fixture.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::time::{Duration, Instant};

use rivqen_reference::legacy::{self, sha1_hex};
use rivqen_reference::model::{AppResponse, Body, Field, Profile, Request, Response};
use serde_json::Value;

const PAGE: &str = "<html><head><title>Shop</title></head><body>\
<!--sonicdiff-price--><p>1</p><!--sonicdiff-price-end-->\
<!--sonicdiff-stock--><p>2</p><!--sonicdiff-stock-end--></body></html>";
const TEMPLATE: &str = "<html><head>{title}</head><body>{price}{stock}</body></html>";
const HTML: &str = "text/html; charset=utf-8";
const NONE: [&str; 0] = [];

fn tag() -> String {
    sha1_hex(TEMPLATE.as_bytes())
}

fn etag() -> String {
    sha1_hex(PAGE.as_bytes())
}

fn app_with(page: &str, extra: &[(&str, &str)]) -> AppResponse {
    let mut fields = vec![Field::new("content-type", HTML)];
    fields.extend(extra.iter().map(|(n, v)| Field::new(n, v)));
    AppResponse {
        status: 200,
        fields,
        body: page.as_bytes().to_vec(),
    }
}

fn app(extra: &[(&str, &str)]) -> AppResponse {
    app_with(PAGE, extra)
}

fn get(fields: &[(&str, &str)]) -> Request {
    Request {
        method: "GET".into(),
        url: "https://shop.example.com/catalog".into(),
        fields: fields.iter().map(|(n, v)| Field::new(n, v)).collect(),
    }
}

/// A legacy data request for `PAGE` (accept-diff, gzip, the right tag).
fn data_request(extra: &[(&str, &str)]) -> Request {
    let t = tag();
    let mut f = vec![
        ("accept-encoding", "gzip"),
        ("accept-diff", "true"),
        ("template-tag", t.as_str()),
    ];
    f.extend_from_slice(extra);
    get(&f)
}

fn values<'a>(r: &'a Response, name: &str) -> Vec<&'a str> {
    r.fields
        .iter()
        .filter(|f| f.is(name))
        .map(|f| f.value.as_str())
        .collect()
}

fn data_json(r: &Response) -> Value {
    match &r.body {
        Body::Bytes(b) => serde_json::from_slice(b).expect("JSON body"),
        Body::None => panic!("no body"),
    }
}

fn is_pass_through(req: &Request, a: &AppResponse, r: &Response) -> bool {
    *r == rivqen_reference::model::pass_through(req, a)
}

fn is_data(r: &Response) -> bool {
    r.status == 200 && values(r, "template-change") == ["false"]
}

fn is_full(r: &Response, a: &AppResponse) -> bool {
    r.status == 200
        && values(r, "template-change") == ["true"]
        && r.body == Body::Bytes(a.body.clone())
}

// ---------------------------------------------------------------- C-01

#[test]
fn legacy_c01_only_get_200_html_is_processed() {
    let a = app(&[]);
    for method in ["HEAD", "POST", "get"] {
        let mut req = data_request(&[]);
        req.method = method.into();
        let r = legacy::respond(Profile::Canonical, &req, &a);
        assert!(is_pass_through(&req, &a, &r), "{method}");
    }
    for status in [201, 302, 304, 404] {
        let a = AppResponse { status, ..app(&[]) };
        let req = data_request(&[]);
        assert!(is_pass_through(
            &req,
            &a,
            &legacy::respond(Profile::Canonical, &req, &a)
        ));
    }
    let req = data_request(&[]);
    assert!(is_data(&legacy::respond(Profile::Canonical, &req, &a)));
}

#[test]
fn legacy_c01_needs_exactly_one_html_content_type() {
    let req = data_request(&[]);
    let mut none = app(&[]);
    none.fields.clear();
    let two = app(&[("Content-Type", HTML)]);
    let mut json_type = app(&[]);
    json_type.fields = vec![Field::new("content-type", "application/json")];
    for a in [none, two, json_type] {
        assert!(is_pass_through(
            &req,
            &a,
            &legacy::respond(Profile::Canonical, &req, &a)
        ));
    }
    let mut upper = app(&[]);
    upper.fields = vec![Field::new("Content-Type", "TEXT/HTML ;charset=UTF-8")];
    assert!(is_data(&legacy::respond(Profile::Canonical, &req, &upper)));
}

#[test]
fn legacy_c01_applies_to_every_profile() {
    let mut req = data_request(&[]);
    req.method = "POST".into();
    let a = app(&[]);
    for p in Profile::ALL {
        assert!(
            is_pass_through(&req, &a, &legacy::respond(p, &req, &a)),
            "{}",
            p.name()
        );
    }
}

// ---------------------------------------------------------------- C-02

#[test]
fn legacy_c02_accept_diff_must_be_one_line_true() {
    let a = app(&[]);
    let t = tag();
    for fields in [
        vec![("template-tag", t.as_str())],
        vec![("accept-diff", "TRUE"), ("template-tag", t.as_str())],
        vec![("accept-diff", "yes"), ("template-tag", t.as_str())],
        vec![
            ("accept-diff", "true"),
            ("accept-diff", "true"),
            ("template-tag", t.as_str()),
        ],
    ] {
        let req = get(&fields);
        assert!(is_pass_through(
            &req,
            &a,
            &legacy::respond(Profile::Canonical, &req, &a)
        ));
    }
    let req = get(&[("Accept-Diff", " \ttrue\t "), ("template-tag", t.as_str())]);
    assert!(is_data(&legacy::respond(Profile::Canonical, &req, &a)));
}

// ---------------------------------------------------------------- C-03

#[test]
fn legacy_c03_charset_other_than_utf8_is_pass_through() {
    let req = data_request(&[]);
    for ct in [
        "text/html; charset=iso-8859-1",
        "text/html; charset=utf-8x",
        "text/html; charset=\"utf\\-8\"",
    ] {
        let mut a = app(&[]);
        a.fields = vec![Field::new("content-type", ct)];
        assert!(
            is_pass_through(&req, &a, &legacy::respond(Profile::Canonical, &req, &a)),
            "{ct}"
        );
    }
    for ct in [
        "text/html",
        "text/html; charset=\"UTF-8\"",
        "text/html;level=1;charset=Utf-8",
    ] {
        let mut a = app(&[]);
        a.fields = vec![Field::new("content-type", ct)];
        assert!(
            is_data(&legacy::respond(Profile::Canonical, &req, &a)),
            "{ct}"
        );
    }
}

#[test]
fn legacy_c03_invalid_utf8_is_pass_through() {
    let req = data_request(&[]);
    for bad in [&b"\xff"[..], b"\xed\xa0\x80", b"\xc0\xaf", b"\xe2"] {
        let mut a = app(&[]);
        a.body.extend_from_slice(bad);
        for p in Profile::ALL {
            assert!(is_pass_through(&req, &a, &legacy::respond(p, &req, &a)));
        }
    }
}

#[test]
fn legacy_c03_dl088_note1_first_charset_parameter_counts() {
    let req = data_request(&[]);
    let cases = [
        ("text/html; charset; charset=utf-8", true),
        ("text/html; charset=latin1; charset=utf-8", false),
        ("text/html; charset=utf-8; charset=latin1", true),
        ("text/html; CHARSET = \"utf-8\" ", true),
        ("text/html; charset=\"utf-8", false),
    ];
    for (ct, processed) in cases {
        let mut a = app(&[]);
        a.fields = vec![Field::new("content-type", ct)];
        let r = legacy::respond(Profile::Canonical, &req, &a);
        assert_eq!(is_data(&r), processed, "{ct}");
    }
}

// ---------------------------------------------------------------- C-04

#[test]
fn legacy_c04_etag_is_lowercase_sha1_of_the_body() {
    let r = legacy::respond(Profile::Canonical, &data_request(&[]), &app(&[]));
    let e = etag();
    assert_eq!(values(&r, "etag"), [e.as_str()]);
    assert_eq!(e.len(), 40);
    assert!(
        e.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
}

// ---------------------------------------------------------------- C-05

#[test]
fn legacy_c05_matching_if_none_match_gives_304() {
    let a = app(&[("etag", "\"app\""), ("x-app", "1")]);
    let e = etag();
    for inm in [
        e.clone(),
        e.to_uppercase(),
        format!("\"{e}\""),
        format!("W/\"{e}\""),
    ] {
        let req = data_request(&[("If-None-Match", inm.as_str())]);
        let r = legacy::respond(Profile::Canonical, &req, &a);
        assert_eq!(r.status, 304, "{inm}");
        assert_eq!(r.body, Body::None);
        assert_eq!(values(&r, "cache-offline"), ["store"]);
        assert_eq!(values(&r, "cache-control"), ["no-cache, private"]);
        assert_eq!(values(&r, "etag"), NONE);
        assert_eq!(values(&r, "template-tag"), NONE);
        assert_eq!(values(&r, "template-change"), NONE);
        assert_eq!(values(&r, "content-type"), [HTML]);
        assert_eq!(values(&r, "x-app"), ["1"]);
        assert_eq!(values(&r, "vary"), ["template-tag"]);
    }
}

#[test]
fn legacy_c05_other_if_none_match_gives_no_304() {
    let a = app(&[]);
    let e = etag();
    let two = [("If-None-Match", e.as_str()), ("if-none-match", e.as_str())];
    let list = format!("\"x\", \"{e}\"");
    let doubled = format!("W/W/\"{e}\"");
    let half = format!("\"{e}");
    for extra in [
        &two[..],
        &[("If-None-Match", list.as_str())],
        &[("If-None-Match", doubled.as_str())],
        &[("If-None-Match", half.as_str())],
    ] {
        let r = legacy::respond(Profile::Canonical, &data_request(extra), &a);
        assert!(is_data(&r), "{extra:?}");
    }
}

#[test]
fn legacy_c05_dl088_note2_weak_prefix_is_case_sensitive() {
    let e = etag();
    let inm = format!("w/\"{e}\"");
    let r = legacy::respond(
        Profile::Canonical,
        &data_request(&[("If-None-Match", inm.as_str())]),
        &app(&[]),
    );
    assert!(is_data(&r));
}

#[test]
fn legacy_c05_needs_a_legacy_request() {
    let e = etag();
    let t = tag();
    let req = get(&[("if-none-match", e.as_str()), ("template-tag", t.as_str())]);
    let a = app(&[]);
    assert!(is_pass_through(
        &req,
        &a,
        &legacy::respond(Profile::Canonical, &req, &a)
    ));
}

// ---------------------------------------------------------------- C-06

#[test]
fn legacy_c06_first_title_is_replaced_once() {
    let page = "<TITLE a=1>One</Title><title>Two</title>";
    let r = legacy::respond(Profile::Canonical, &data_request(&[]), &app_with(page, &[]));
    // Full response: the request tag is the tag of PAGE. Check the split.
    assert!(is_full(&r, &app_with(page, &[])));
    let s = legacy::split(Profile::Canonical, page).unwrap();
    assert_eq!(s.template, "{title}<title>Two</title>");
    assert_eq!(
        s.data[0],
        ("{title}".into(), "<TITLE a=1>One</Title>".into())
    );
    assert_eq!(
        values(&r, "template-tag"),
        [sha1_hex(s.template.as_bytes()).as_str()]
    );
}

#[test]
fn legacy_c06_lf_stops_a_title_and_no_title_gives_empty_value() {
    for page in ["<title>a\nb</title>", "<title>a", "</title><title>", "x"] {
        let s = legacy::split(Profile::Canonical, page).unwrap();
        assert_eq!(s.template, page);
        assert_eq!(s.data, vec![("{title}".to_owned(), String::new())]);
    }
    let s = legacy::split(Profile::Canonical, "<title>a\rb</title>").unwrap();
    assert_eq!(s.template, "{title}");
    let s = legacy::split(Profile::Canonical, "<title>\n<title>b</title>").unwrap();
    assert_eq!(s.template, "<title>\n{title}");
}

// ---------------------------------------------------------------- C-07

#[test]
fn legacy_c07_blocks_are_lazy_and_end_name_is_not_checked() {
    let page = "<!--SONICDIFF-a-->1<!--sonicdiff-zz-END--><!--sonicdiffb-->2<!--sonicdiff-end-->";
    let s = legacy::split(Profile::Canonical, page).unwrap();
    assert_eq!(s.template, "{a}{b}");
    assert_eq!(
        s.data,
        vec![
            ("{title}".to_owned(), String::new()),
            (
                "{a}".to_owned(),
                "<!--SONICDIFF-a-->1<!--sonicdiff-zz-END-->".to_owned()
            ),
            (
                "{b}".to_owned(),
                "<!--sonicdiffb-->2<!--sonicdiff-end-->".to_owned()
            ),
        ]
    );
}

#[test]
fn legacy_c07_content_needs_one_character() {
    let page = "<!--sonicdiff-a--><!--sonicdiff-a-end-->x<!--sonicdiff-b-end-->";
    let s = legacy::split(Profile::Canonical, page).unwrap();
    assert_eq!(s.template, "{a}");
    assert_eq!(s.data[1].1, page);
}

#[test]
fn legacy_c07_empty_and_long_names_stay_in_the_template() {
    let long = "p".repeat(129);
    let ok = "q".repeat(128);
    let page = format!(
        "<!--sonicdiff-->1<!--sonicdiff-end--><!--sonicdiff-{long}-->2<!--sonicdiff-{long}-end--><!--sonicdiff-{ok}-->3<!--sonicdiff-{ok}-end-->"
    );
    let s = legacy::split(Profile::Canonical, &page).unwrap();
    let kept = format!(
        "<!--sonicdiff-->1<!--sonicdiff-end--><!--sonicdiff-{long}-->2<!--sonicdiff-{long}-end-->"
    );
    assert_eq!(s.template, format!("{kept}{{{ok}}}"));
    assert_eq!(s.data.len(), 2);
    assert_eq!(s.blocks, 1);
}

#[test]
fn legacy_c07_repeated_name_or_title_block_is_pass_through() {
    let a = app_with(
        "<!--sonicdiff-x-->1<!--sonicdiff-x-end--><!--sonicdiff-x-->2<!--sonicdiff-x-end-->",
        &[],
    );
    let e = sha1_hex(&a.body);
    // Also when If-None-Match matches: the split runs before C-05.
    let req = data_request(&[("If-None-Match", e.as_str())]);
    assert!(is_pass_through(
        &req,
        &a,
        &legacy::respond(Profile::Canonical, &req, &a)
    ));
    let a = app_with("<!--sonicdiff-title-->1<!--sonicdiff-title-end-->", &[]);
    let req = data_request(&[]);
    assert!(is_pass_through(
        &req,
        &a,
        &legacy::respond(Profile::Canonical, &req, &a)
    ));
    // The names compare case-sensitively.
    let s = legacy::split(
        Profile::Canonical,
        "<!--sonicdiff-x-->1<!--sonicdiff-end--><!--sonicdiff-X-->2<!--sonicdiff-end--><!--sonicdiff-Title-->3<!--sonicdiff-end-->",
    );
    assert_eq!(s.map(|s| s.data.len()), Some(4));
}

#[test]
fn legacy_c07_no_end_marker_stops_the_scan() {
    let page = "<!--sonicdiff-a-->1<!--sonicdiff-b-->2";
    let s = legacy::split(Profile::Canonical, page).unwrap();
    assert_eq!(s.template, page);
    assert_eq!(s.blocks, 0);
}

// ---------------------------------------------------------------- C-08

#[test]
fn legacy_c08_template_tag_and_key_order() {
    let r = legacy::respond(Profile::Canonical, &data_request(&[]), &app(&[]));
    let t = tag();
    assert_eq!(values(&r, "template-tag"), [t.as_str()]);
    let s = legacy::split(Profile::Canonical, PAGE).unwrap();
    assert_eq!(s.template, TEMPLATE);
    let keys: Vec<&str> = s.data.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["{title}", "{price}", "{stock}"]);
}

// ---------------------------------------------------------------- C-09

#[test]
fn legacy_c09_cache_control_is_replaced() {
    let a = app(&[("cache-control", "max-age=60"), ("Cache-Control", "public")]);
    let r = legacy::respond(Profile::Canonical, &data_request(&[]), &a);
    assert_eq!(values(&r, "cache-control"), ["no-cache, private"]);
}

#[test]
fn legacy_c09_no_store_keeps_the_lines_of_a() {
    let a = app(&[
        ("cache-control", "max-age=1"),
        ("cache-control", "max-age=1, NO-STORE"),
    ]);
    let r = legacy::respond(Profile::Canonical, &data_request(&[]), &a);
    assert_eq!(
        values(&r, "cache-control"),
        ["max-age=1", "max-age=1, NO-STORE"]
    );
}

#[test]
fn legacy_c09_dl088_note3_no_store_with_a_value_counts() {
    let a = app(&[("cache-control", "no-store=1")]);
    let r = legacy::respond(Profile::Canonical, &data_request(&[]), &a);
    assert_eq!(values(&r, "cache-control"), ["no-store=1"]);
    let a = app(&[("cache-control", "no-storex")]);
    let r = legacy::respond(Profile::Canonical, &data_request(&[]), &a);
    assert_eq!(values(&r, "cache-control"), ["no-cache, private"]);
}

// ---------------------------------------------------------------- C-10

#[test]
fn legacy_c10_application_value_replaces_the_default_on_200() {
    for (v, want) in [
        ("true", "true"),
        ("false", "false"),
        ("store", "store"),
        ("http", "http"),
        (" http ", "http"),
        ("TRUE", "true"),
        ("x", "true"),
    ] {
        let r = legacy::respond(
            Profile::Canonical,
            &data_request(&[]),
            &app(&[("cache-offline", v)]),
        );
        assert_eq!(values(&r, "cache-offline"), [want], "{v}");
    }
    let r = legacy::respond(Profile::Canonical, &data_request(&[]), &app(&[]));
    assert_eq!(values(&r, "cache-offline"), ["true"]);
}

#[test]
fn legacy_c10_304_is_always_store() {
    let e = etag();
    let r = legacy::respond(
        Profile::Canonical,
        &data_request(&[("if-none-match", e.as_str())]),
        &app(&[("cache-offline", "false")]),
    );
    assert_eq!(r.status, 304);
    assert_eq!(values(&r, "cache-offline"), ["store"]);
}

#[test]
fn legacy_c10_dl088_note4_two_lines_give_the_default() {
    let a = app(&[("cache-offline", "false"), ("cache-offline", "x")]);
    let r = legacy::respond(Profile::Canonical, &data_request(&[]), &a);
    assert_eq!(values(&r, "cache-offline"), ["true"]);
}

// ---------------------------------------------------------------- C-11

#[test]
fn legacy_c11_data_response_body() {
    let r = legacy::respond(Profile::Canonical, &data_request(&[]), &app(&[]));
    assert!(is_data(&r));
    let v = data_json(&r);
    let obj = v.as_object().unwrap();
    assert_eq!(obj.len(), 4);
    assert_eq!(v["template-tag"], tag().as_str());
    assert_eq!(v["html-sha1"], etag().as_str());
    assert_eq!(v["diff"], "");
    assert_eq!(v["data"]["{title}"], "<title>Shop</title>");
    assert_eq!(
        v["data"]["{price}"],
        "<!--sonicdiff-price--><p>1</p><!--sonicdiff-price-end-->"
    );
    // No HTML escaping: the text has the raw `<`.
    let Body::Bytes(b) = &r.body else { panic!() };
    assert!(String::from_utf8_lossy(b).contains("<title>Shop</title>"));
}

#[test]
fn legacy_c11_template_tag_must_be_one_exact_line() {
    let a = app(&[]);
    let t = tag();
    let upper = t.to_uppercase();
    for fields in [
        vec![("accept-diff", "true")],
        vec![("accept-diff", "true"), ("template-tag", upper.as_str())],
        vec![
            ("accept-diff", "true"),
            ("template-tag", t.as_str()),
            ("template-tag", t.as_str()),
        ],
    ] {
        let r = legacy::respond(Profile::Canonical, &get(&fields), &a);
        assert!(is_full(&r, &a), "{fields:?}");
    }
}

// ---------------------------------------------------------------- C-12

#[test]
fn legacy_c12_full_response_keeps_the_body() {
    let a = app(&[]);
    let r = legacy::respond(Profile::Canonical, &get(&[("accept-diff", "true")]), &a);
    assert!(is_full(&r, &a));
    assert_eq!(values(&r, "etag"), [etag().as_str()]);
    assert_eq!(values(&r, "template-tag"), [tag().as_str()]);
}

// ---------------------------------------------------------------- C-13

#[test]
fn legacy_c13_legacy_fields_of_a_are_replaced() {
    let a = app(&[
        ("ETag", "\"app\""),
        ("template-tag", "x"),
        ("Template-Change", "x"),
        ("sonic-etag-key", "k"),
        ("cdn-cache-control", "max-age=600"),
        ("Surrogate-Control", "max-age=600"),
        ("content-length", "5"),
        ("transfer-encoding", "chunked"),
        ("connection", "keep-alive"),
        ("set-cookie", "a=b"),
        ("x-app", "1"),
    ]);
    let r = legacy::respond(Profile::Canonical, &data_request(&[]), &a);
    assert_eq!(values(&r, "etag"), [etag().as_str()]);
    assert_eq!(values(&r, "template-tag"), [tag().as_str()]);
    assert_eq!(values(&r, "template-change"), ["false"]);
    for gone in [
        "sonic-etag-key",
        "cdn-cache-control",
        "surrogate-control",
        "content-length",
        "transfer-encoding",
        "connection",
    ] {
        assert_eq!(values(&r, gone), NONE, "{gone}");
    }
    assert_eq!(values(&r, "set-cookie"), ["a=b"]);
    assert_eq!(values(&r, "x-app"), ["1"]);
    assert_eq!(values(&r, "content-type"), [HTML]);
    // The kept lines of A keep their order, before the legacy lines.
    assert_eq!(r.fields[0], Field::new("content-type", HTML));
    assert_eq!(r.fields[1], Field::new("set-cookie", "a=b"));
}

// ---------------------------------------------------------------- C-14

#[test]
fn legacy_c14_accept_encoding_does_not_decide() {
    let a = app(&[]);
    let t = tag();
    for ae in [None, Some("identity"), Some("br")] {
        let mut f = vec![("accept-diff", "true"), ("template-tag", t.as_str())];
        if let Some(v) = ae {
            f.push(("accept-encoding", v));
        }
        let r = legacy::respond(Profile::Canonical, &get(&f), &a);
        assert!(is_data(&r), "{ae:?}");
        assert_eq!(values(&r, "content-encoding"), NONE);
    }
}

// ---------------------------------------------------------------- C-15

#[test]
fn legacy_c15_vary_template_tag_after_the_vary_of_a() {
    let r = legacy::respond(
        Profile::Canonical,
        &data_request(&[]),
        &app(&[("vary", "Cookie")]),
    );
    assert_eq!(values(&r, "vary"), ["Cookie", "template-tag"]);
    for v in ["*", "Accept, *", "accept, TEMPLATE-TAG"] {
        let r = legacy::respond(Profile::Canonical, &data_request(&[]), &app(&[("vary", v)]));
        assert_eq!(values(&r, "vary"), [v]);
    }
    let e = etag();
    let r = legacy::respond(
        Profile::Canonical,
        &data_request(&[("if-none-match", e.as_str())]),
        &app(&[]),
    );
    assert_eq!(values(&r, "vary"), ["template-tag"]);
}

// ---------------------------------------------------------------- C-16

fn openers_page(size: usize) -> String {
    let mut s = String::from(PAGE);
    while s.len() < size {
        s.push_str("<title");
        s.push_str("<!--sonicdiff-a-->x");
    }
    s
}

/// Linear time, with a bound that a quadratic scan cannot meet even in a
/// debug build (256 KiB of openers: about 1.3e10 steps if quadratic).
#[test]
fn legacy_c16_openers_are_linear() {
    let page = openers_page(256 * 1024);
    let a = app_with(&page, &[]);
    let start = Instant::now();
    let r = legacy::respond(Profile::Canonical, &data_request(&[]), &a);
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "{:?}",
        start.elapsed()
    );
    assert_eq!(r.status, 200);
    let s = legacy::split(Profile::Canonical, &page).unwrap();
    assert_eq!(s.data.len(), 3);
    for p in Profile::ALL {
        let start = Instant::now();
        assert_eq!(legacy::respond(p, &data_request(&[]), &a).status, 200);
        assert!(start.elapsed() < Duration::from_secs(5));
    }
}

/// C-16 acceptance: 256 KiB of `<title` and `<!--sonicdiff-a-->x` openers in
/// less than 200 ms in a release build. Run:
/// `cargo test --release legacy_linear_time -- --ignored`.
#[test]
#[ignore = "timing test: cargo test --release legacy_linear_time -- --ignored"]
fn legacy_linear_time() {
    let limit = if cfg!(debug_assertions) {
        Duration::from_secs(5)
    } else {
        Duration::from_millis(200)
    };
    for page in [
        openers_page(256 * 1024),
        "<title".repeat(256 * 1024 / 6),
        "<!--sonicdiff-a-->x".repeat(256 * 1024 / 19),
        "<!--sonicdiff-a-->".repeat(256 * 1024 / 18),
        format!("<!--sonicdiff-{}", "a".repeat(256 * 1024)),
    ] {
        let a = app_with(&page, &[]);
        for p in Profile::ALL {
            let start = Instant::now();
            let r = legacy::respond(p, &data_request(&[]), &a);
            let took = start.elapsed();
            assert!(took < limit, "{}: {took:?}", p.name());
            assert_eq!(r.status, 200);
        }
    }
}

// ---------------------------------------------------------------- P-01

#[test]
fn legacy_p01_java_first_accept_diff_line() {
    let a = app(&[]);
    let t = tag();
    let r = legacy::respond(
        Profile::Java,
        &get(&[
            ("Accept-Diff", "true"),
            ("accept-diff", "false"),
            ("template-tag", t.as_str()),
        ]),
        &a,
    );
    assert_eq!(values(&r, "cache-offline"), ["true"]);
    let r = legacy::respond(
        Profile::Java,
        &get(&[
            ("accept-diff", "false"),
            ("accept-diff", "true"),
            ("template-tag", t.as_str()),
        ]),
        &a,
    );
    assert!(
        values(&r, "cache-offline").is_empty(),
        "not a legacy request: P-02"
    );
}

#[test]
fn legacy_p01_node_any_accept_diff_and_gzip_or_deflate() {
    let a = app(&[]);
    let t = tag();
    for (ad, ae, legacy_req) in [
        ("false", "gzip", true),
        ("yes", "br, deflate;q=0", true),
        ("true", "identity", false),
        ("", "gzip", false),
        ("true", "x-gzip", false),
    ] {
        let req = get(&[
            ("accept-encoding", ae),
            ("accept-diff", ad),
            ("template-tag", t.as_str()),
        ]);
        let r = legacy::respond(Profile::Node, &req, &a);
        assert_eq!(!is_pass_through(&req, &a, &r), legacy_req, "{ad:?} {ae:?}");
    }
    // Any accept-encoding line counts; no accept-diff line is not legacy.
    let req = get(&[
        ("accept-encoding", "br"),
        ("Accept-Encoding", "gzip"),
        ("accept-diff", "1"),
    ]);
    assert!(!is_pass_through(
        &req,
        &a,
        &legacy::respond(Profile::Node, &req, &a)
    ));
    let req = get(&[("accept-encoding", "gzip")]);
    assert!(is_pass_through(
        &req,
        &a,
        &legacy::respond(Profile::Node, &req, &a)
    ));
}

#[test]
fn legacy_p01_php_accept_diff_name_is_exact() {
    let a = app(&[]);
    let t = tag();
    let req = get(&[("Accept-Diff", "true"), ("Template-Tag", t.as_str())]);
    assert!(is_pass_through(
        &req,
        &a,
        &legacy::respond(Profile::Php, &req, &a)
    ));
    // The first exact line counts; other names compare case-insensitively.
    let req = get(&[
        ("Accept-Diff", "false"),
        ("accept-diff", " true "),
        ("Template-Tag", t.as_str()),
    ]);
    assert!(is_data(&legacy::respond(Profile::Php, &req, &a)));
    let req = get(&[("accept-diff", "false"), ("accept-diff", "true")]);
    assert!(is_pass_through(
        &req,
        &a,
        &legacy::respond(Profile::Php, &req, &a)
    ));
}

// ---------------------------------------------------------------- P-02

#[test]
fn legacy_p02_java_processes_a_request_that_is_not_legacy() {
    let a = app(&[("cache-offline", "false"), ("cache-control", "max-age=5")]);
    let e = etag();
    let t = tag();
    let req = get(&[("if-none-match", e.as_str()), ("template-tag", t.as_str())]);
    let r = legacy::respond(Profile::Java, &req, &a);
    assert!(is_data(&r), "never a 304, the data rule of P-04");
    assert_eq!(values(&r, "etag"), [e.as_str()]);
    assert_eq!(values(&r, "cache-offline"), ["false"]);
    assert_eq!(values(&r, "cache-control"), ["max-age=5"]);
    assert_eq!(values(&r, "vary"), ["template-tag"]);
    let r = legacy::respond(Profile::Java, &get(&[]), &a);
    assert!(is_full(&r, &a));
}

#[test]
fn legacy_p02_node_and_php_pass_through() {
    let a = app(&[]);
    let t = tag();
    let req = get(&[("accept-encoding", "gzip"), ("template-tag", t.as_str())]);
    for p in [Profile::Node, Profile::Php, Profile::Canonical] {
        assert!(is_pass_through(&req, &a, &legacy::respond(p, &req, &a)));
    }
}

// ---------------------------------------------------------------- P-03

#[test]
fn legacy_p03_304_test_per_profile() {
    let a = app(&[]);
    let e = etag();
    let upper = e.to_uppercase();
    let quoted = format!("\"{e}\"");
    let cases: [(&str, [bool; 3]); 3] = [
        (e.as_str(), [true, true, true]),
        (upper.as_str(), [true, false, false]),
        (quoted.as_str(), [false, false, false]),
    ];
    for (inm, want) in cases {
        for (p, w) in [Profile::Java, Profile::Node, Profile::Php]
            .into_iter()
            .zip(want)
        {
            let req = data_request(&[("If-None-Match", inm), ("if-none-match", "x")]);
            let r = legacy::respond(p, &req, &a);
            assert_eq!(r.status == 304, w, "{} {inm}", p.name());
        }
    }
    // The first line counts.
    let req = data_request(&[("if-none-match", "x"), ("If-None-Match", e.as_str())]);
    assert_eq!(legacy::respond(Profile::Node, &req, &a).status, 200);
}

// ---------------------------------------------------------------- P-04

#[test]
fn legacy_p04_first_template_tag_line() {
    let a = app(&[]);
    let t = tag();
    for p in [Profile::Java, Profile::Node, Profile::Php] {
        let r = legacy::respond(p, &data_request(&[("template-tag", "x")]), &a);
        assert!(is_data(&r), "{}", p.name());
        let req = get(&[
            ("accept-encoding", "gzip"),
            ("accept-diff", "true"),
            ("template-tag", "x"),
            ("template-tag", t.as_str()),
        ]);
        assert!(is_full(&legacy::respond(p, &req, &a), &a), "{}", p.name());
    }
}

#[test]
fn legacy_p04_node_full_without_blocks() {
    let page = "<title>T</title><p>no markers</p>";
    let a = app_with(page, &[]);
    let t = sha1_hex(b"{title}<p>no markers</p>");
    let req = get(&[
        ("accept-encoding", "gzip"),
        ("accept-diff", "true"),
        ("template-tag", t.as_str()),
    ]);
    let r = legacy::respond(Profile::Node, &req, &a);
    assert!(is_full(&r, &a));
    assert_eq!(values(&r, "template-tag"), [t.as_str()]);
    for p in [Profile::Canonical, Profile::Java, Profile::Php] {
        assert!(is_data(&legacy::respond(p, &req, &a)), "{}", p.name());
    }
    // An empty-name block counts as an extracted block in legacy-node.
    let page = "<!--sonicdiff-->1<!--sonicdiff-end-->";
    let a = app_with(page, &[]);
    let t = sha1_hex(b"{auto0}");
    let req = get(&[
        ("accept-encoding", "gzip"),
        ("accept-diff", "true"),
        ("template-tag", t.as_str()),
    ]);
    assert!(is_data(&legacy::respond(Profile::Node, &req, &a)));
}

// ---------------------------------------------------------------- P-05

#[test]
fn legacy_p05_cache_control_per_profile() {
    let e = etag();
    for (p, want) in [(Profile::Java, "no-cache"), (Profile::Php, "no-cache")] {
        let r = legacy::respond(
            p,
            &data_request(&[]),
            &app(&[("cache-control", "max-age=60")]),
        );
        assert_eq!(values(&r, "cache-control"), [want]);
        let r = legacy::respond(
            p,
            &data_request(&[("if-none-match", e.as_str())]),
            &app(&[]),
        );
        assert_eq!(r.status, 304);
        assert_eq!(values(&r, "cache-control"), [want]);
        let r = legacy::respond(
            p,
            &data_request(&[]),
            &app(&[("cache-control", "no-store")]),
        );
        assert_eq!(values(&r, "cache-control"), ["no-store"]);
    }
    let r = legacy::respond(
        Profile::Node,
        &data_request(&[]),
        &app(&[("cache-control", "max-age=60")]),
    );
    assert_eq!(values(&r, "cache-control"), ["max-age=60"]);
    let r = legacy::respond(Profile::Node, &data_request(&[]), &app(&[]));
    assert_eq!(values(&r, "cache-control"), NONE);
}

// ---------------------------------------------------------------- P-06

#[test]
fn legacy_p06_cache_offline_on_200() {
    let a = app(&[("cache-offline", " false "), ("cache-offline", "x")]);
    let r = legacy::respond(Profile::Java, &data_request(&[]), &a);
    assert_eq!(values(&r, "cache-offline"), ["true", " false ", "x"]);
    for p in [Profile::Node, Profile::Php] {
        let r = legacy::respond(p, &data_request(&[]), &a);
        assert_eq!(values(&r, "cache-offline"), ["true"], "{}", p.name());
    }
}

// ---------------------------------------------------------------- P-07

#[test]
fn legacy_p07_cache_offline_on_304() {
    let e = etag();
    let req = data_request(&[("if-none-match", e.as_str())]);
    let a = app(&[("cache-offline", "false")]);
    let r = legacy::respond(Profile::Java, &req, &a);
    assert_eq!(r.status, 304);
    assert_eq!(values(&r, "cache-offline"), ["true", "false", "store"]);
    for p in [Profile::Node, Profile::Php] {
        let r = legacy::respond(p, &req, &a);
        assert_eq!(r.status, 304);
        assert_eq!(values(&r, "cache-offline"), ["store"], "{}", p.name());
    }
}

// ---------------------------------------------------------------- P-08

#[test]
fn legacy_p08_content_type_on_304() {
    let e = etag();
    let req = data_request(&[("if-none-match", e.as_str())]);
    let a = app(&[]);
    for (p, kept) in [
        (Profile::Java, false),
        (Profile::Node, false),
        (Profile::Php, true),
        (Profile::Canonical, true),
    ] {
        let r = legacy::respond(p, &req, &a);
        assert_eq!(r.status, 304);
        assert_eq!(!values(&r, "content-type").is_empty(), kept, "{}", p.name());
        // On 200 every profile keeps it.
        let r = legacy::respond(p, &data_request(&[]), &a);
        assert_eq!(values(&r, "content-type"), [HTML], "{}", p.name());
    }
}

// ---------------------------------------------------------------- P-09

#[test]
fn legacy_p09_block_names_per_profile() {
    let long = "n".repeat(200);
    let page = format!(
        "<!--sonicdiff-->1<!--sonicdiff-end--><!--sonicdiff-{long}-->2<!--sonicdiff-end--><!--sonicdiff-->3<!--sonicdiff-end-->"
    );
    let keys = |p| -> Vec<String> {
        legacy::split(p, &page)
            .unwrap()
            .data
            .into_iter()
            .map(|(k, _)| k)
            .collect()
    };
    let long_key = format!("{{{long}}}");
    assert_eq!(keys(Profile::Java), ["{title}", "{}", long_key.as_str()]);
    assert_eq!(keys(Profile::Php), ["{title}", "{}", long_key.as_str()]);
    assert_eq!(
        keys(Profile::Node),
        ["{title}", "{auto0}", long_key.as_str(), "{auto1}"]
    );
    assert_eq!(keys(Profile::Canonical), ["{title}"]);
    let s = legacy::split(Profile::Node, &page).unwrap();
    assert_eq!(s.template, format!("{{auto0}}{long_key}{{auto1}}"));
}

// ---------------------------------------------------------------- P-10

#[test]
fn legacy_p10_repeated_keys_keep_the_later_value_at_the_first_position() {
    let page = "<title>T</title><!--sonicdiff-x-->1<!--sonicdiff-x-end--><!--sonicdiff-y-->2<!--sonicdiff-y-end--><!--sonicdiff-x-->3<!--sonicdiff-x-end-->";
    for p in [Profile::Java, Profile::Node, Profile::Php] {
        let s = legacy::split(p, page).unwrap();
        assert_eq!(s.template, "{title}{x}{y}{x}");
        assert_eq!(
            s.data,
            vec![
                ("{title}".to_owned(), "<title>T</title>".to_owned()),
                (
                    "{x}".to_owned(),
                    "<!--sonicdiff-x-->3<!--sonicdiff-x-end-->".to_owned()
                ),
                (
                    "{y}".to_owned(),
                    "<!--sonicdiff-y-->2<!--sonicdiff-y-end-->".to_owned()
                ),
            ]
        );
        assert_eq!(s.blocks, 3);
    }
    assert_eq!(legacy::split(Profile::Canonical, page), None);
}

#[test]
fn legacy_p10_block_named_title_repeats_the_title_key() {
    let page = "<title>T</title><!--sonicdiff-title-->B<!--sonicdiff-title-end-->";
    for p in [Profile::Java, Profile::Node, Profile::Php] {
        let s = legacy::split(p, page).unwrap();
        assert_eq!(s.template, "{title}{title}");
        assert_eq!(
            s.data,
            vec![(
                "{title}".to_owned(),
                "<!--sonicdiff-title-->B<!--sonicdiff-title-end-->".to_owned()
            )]
        );
    }
}

// ---------------------------------------------------------------- P-11

#[test]
fn legacy_p11_data_body_is_the_same_json_value_in_every_profile() {
    let page =
        "<title>\"é\"\t&amp;</title><!--sonicdiff-u-->\u{1}/\\<\u{2028}><!--sonicdiff-u-end-->";
    let a = app_with(page, &[]);
    let t = sha1_hex(b"{title}{u}");
    let req = get(&[
        ("accept-encoding", "gzip"),
        ("accept-diff", "true"),
        ("template-tag", t.as_str()),
    ]);
    let mut bodies = Vec::new();
    for p in Profile::ALL {
        let r = legacy::respond(p, &req, &a);
        assert!(is_data(&r), "{}", p.name());
        bodies.push(data_json(&r));
    }
    assert!(bodies.windows(2).all(|w| w[0] == w[1]));
    assert_eq!(bodies[0]["data"]["{title}"], "<title>\"é\"\t&amp;</title>");
    assert_eq!(
        bodies[0]["data"]["{u}"],
        "<!--sonicdiff-u-->\u{1}/\\<\u{2028}><!--sonicdiff-u-end-->"
    );
}

// ---------------------------------------------------------------- U-01

#[test]
fn legacy_state_is_per_call() {
    let a = app(&[]);
    let first = legacy::respond(Profile::Java, &data_request(&[]), &a);
    let other = app_with("<!--sonicdiff-z-->9<!--sonicdiff-z-end-->", &[]);
    assert_eq!(
        legacy::respond(Profile::Java, &data_request(&[]), &other).status,
        200
    );
    assert_eq!(
        legacy::respond(Profile::Java, &data_request(&[]), &a),
        first
    );
}
