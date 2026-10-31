// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! One test or more per step of negotiation §4, the charset rule and the
//! inputs that the steps leave open (DL-089), through `select::select` and the
//! step that it reports.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::fs;
use std::path::Path;

use rivqen_reference::model::{AppResponse, Body, Field, Request, Response};
use rivqen_reference::rqp::config::RqpConfig;
use rivqen_reference::rqp::select::{Full, Invalid, Outcome, select};
use serde_json::{Value, json};

const URL: &str = "https://shop.example.com/catalog";
const R_A: &str = "r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8";
const T: &str = "t1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM";
const B_A: &str = ":0Aqua3+/d6ReDehqKiXfLM8hC9ypGZS5:";
const NONCE: &str = ":AAECAwQFBgcICQoLDA0ODw==:";

fn page_a() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/rqp/examples/kat/page.html");
    String::from_utf8(fs::read(path).unwrap()).unwrap()
}

fn page_b() -> String {
    page_a().replace("120.00", "130.00")
}

fn app(body: &str) -> AppResponse {
    AppResponse {
        status: 200,
        fields: vec![
            Field::new("Content-Type", "text/html; charset=utf-8"),
            Field::new("Cache-Control", "no-cache"),
        ],
        body: body.as_bytes().to_vec(),
    }
}

fn with(mut a: AppResponse, name: &str, value: &str) -> AppResponse {
    a.fields.push(Field::new(name, value));
    a
}

fn first_visit() -> Request {
    Request {
        method: "GET".to_owned(),
        url: URL.to_owned(),
        fields: vec![
            Field::new("Rq-Version", "1"),
            Field::new("Rq-Capabilities", "(block-patch)"),
            Field::new("Accept", "text/html"),
        ],
    }
}

/// R(page-a): the full delta request of a client with a snapshot of page A.
fn delta_request() -> Request {
    Request {
        method: "GET".to_owned(),
        url: URL.to_owned(),
        fields: vec![
            Field::new("Rq-Version", "1"),
            Field::new("Rq-Capabilities", "(block-patch)"),
            Field::new("Accept", "text/html, application/vnd.rivqen.delta+json"),
            Field::new("If-None-Match", &format!("\"{R_A}\"")),
            Field::new("Rq-Have", &format!("t={T}, b={B_A}")),
            Field::new("Rq-Nonce", NONCE),
        ],
    }
}

fn set(mut r: Request, name: &str, value: &str) -> Request {
    r.fields.retain(|f| !f.is(name));
    r.fields.push(Field::new(name, value));
    r
}

fn without(mut r: Request, name: &str) -> Request {
    r.fields.retain(|f| !f.is(name));
    r
}

fn run(request: &Request, a: &AppResponse) -> (Response, Outcome) {
    select(&RqpConfig::default(), request, a)
}

fn outcome(request: &Request, a: &AppResponse) -> Outcome {
    run(request, a).1
}

fn values<'a>(r: &'a Response, name: &str) -> Vec<&'a str> {
    r.fields
        .iter()
        .filter(|f| f.is(name))
        .map(|f| f.value.as_str())
        .collect()
}

fn config(v: &Value) -> RqpConfig {
    RqpConfig::from_json(v).unwrap()
}

#[test]
fn rqp_step0_rq_fields_and_vary_members_of_a_go_in_every_outcome() {
    let a = |body: &str| {
        let a = with(app(body), "Rq-Version", "2");
        let a = with(a, "rq-x-app", "1");
        with(a, "Vary", "Accept-Encoding, RQ-Nonce")
    };
    let cases = [
        (
            first_visit(),
            a(&page_a()),
            Outcome::Full(Full::NotEligible),
        ),
        (delta_request(), a(&page_a()), Outcome::NotModified),
        (delta_request(), a(&page_b()), Outcome::Delta),
        (
            first_visit(),
            a("<p data-rq-block=\"x\">"),
            Outcome::Invalid(Invalid::Markup),
        ),
        (
            set(first_visit(), "If-Match", "*"),
            a(&page_a()),
            Outcome::OutOfScope,
        ),
    ];
    for (request, app, want) in cases {
        let (r, got) = run(&request, &app);
        assert_eq!(got, want);
        assert!(values(&r, "rq-x-app").is_empty(), "{want:?}");
        let rq_version = values(&r, "rq-version");
        let expect_version: &[&str] = match want {
            Outcome::Full(_) | Outcome::Delta | Outcome::NotModified => &["1"],
            _ => &[],
        };
        assert_eq!(rq_version, expect_version, "{want:?}");
        let vary = values(&r, "vary");
        if want == Outcome::Delta {
            assert_eq!(vary, ["Accept-Encoding", "Rq-Nonce"]);
        } else {
            assert_eq!(vary, ["Accept-Encoding"], "{want:?}");
        }
    }
}

#[test]
fn rqp_step1_scope() {
    let post = Request {
        method: "POST".to_owned(),
        ..first_visit()
    };
    assert_eq!(outcome(&post, &app(&page_a())), Outcome::OutOfScope);
    let lower = Request {
        method: "get".to_owned(),
        ..first_visit()
    };
    assert_eq!(outcome(&lower, &app(&page_a())), Outcome::OutOfScope);
    let head = Request {
        method: "HEAD".to_owned(),
        ..first_visit()
    };
    assert_eq!(
        outcome(&head, &app(&page_a())),
        Outcome::Full(Full::NotEligible)
    );
    let mut not_found = app(&page_a());
    not_found.status = 404;
    let (r, o) = run(&first_visit(), &not_found);
    assert_eq!((o, r.status), (Outcome::OutOfScope, 404));
    let mut no_content = app(&page_a());
    no_content.status = 204;
    assert_eq!(run(&first_visit(), &no_content).0.body, Body::None);
    let mut json = app("{}");
    json.fields[0].value = "application/json".to_owned();
    assert_eq!(outcome(&first_visit(), &json), Outcome::OutOfScope);
    let mut none = app(&page_a());
    none.fields.remove(0);
    assert_eq!(outcome(&first_visit(), &none), Outcome::OutOfScope);
}

#[test]
fn rqp_step1_other_preconditions_pass_a_through() {
    // DL-089 item 1.
    for name in ["If-Match", "If-Unmodified-Since", "If-Range"] {
        let (r, o) = run(&set(delta_request(), name, "\"x\""), &app(&page_b()));
        assert_eq!(o, Outcome::OutOfScope, "{name}");
        assert_eq!(r.body, Body::Bytes(page_b().into_bytes()));
    }
}

#[test]
fn rqp_step1_content_coding_passes_a_through() {
    // DL-089 item 5.
    let coded = with(app(&page_a()), "Content-Encoding", "gzip");
    assert_eq!(outcome(&first_visit(), &coded), Outcome::OutOfScope);
    let identity = with(app(&page_a()), "Content-Encoding", "identity");
    assert_eq!(
        outcome(&first_visit(), &identity),
        Outcome::Full(Full::NotEligible)
    );
}

#[test]
fn rqp_step2_charset_rule() {
    let mut a = app(&page_a());
    a.fields[0].value = "text/html; charset=iso-8859-1".to_owned();
    assert_eq!(
        outcome(&first_visit(), &a),
        Outcome::Invalid(Invalid::Charset)
    );
    let two = with(app(&page_a()), "content-type", "text/html; charset=utf-8");
    assert_eq!(
        outcome(&first_visit(), &two),
        Outcome::Invalid(Invalid::Charset)
    );
    a.fields[0].value = "TEXT/HTML; Charset=\"UTF-8\"".to_owned();
    let (r, o) = run(&first_visit(), &a);
    assert_eq!(o, Outcome::Full(Full::NotEligible));
    assert_eq!(values(&r, "content-type"), ["text/html; charset=utf-8"]);
}

#[test]
fn rqp_step2_invalid_page_keeps_a_and_its_validators() {
    let page = page_a().replace("<header>", "<p data-rq-block=\"price\">x</p><header>");
    let a = with(
        with(app(&page), "ETag", "\"app-7\""),
        "Last-Modified",
        "Mon, 12 Oct 2026 08:00:00 GMT",
    );
    let (r, o) = run(&delta_request(), &a);
    assert_eq!(o, Outcome::Invalid(Invalid::Markup));
    assert_eq!(values(&r, "etag"), ["\"app-7\""]);
    assert_eq!(values(&r, "last-modified").len(), 1);
    assert_eq!(r.body, Body::Bytes(page.into_bytes()));
    // M-01 and M-02.
    assert_eq!(
        outcome(&first_visit(), &app(&"x".repeat(5_242_881))),
        Outcome::Invalid(Invalid::Markup)
    );
    let mut bad = app(&page_a());
    bad.body.push(0xFF);
    assert_eq!(
        outcome(&first_visit(), &bad),
        Outcome::Invalid(Invalid::Markup)
    );
}

#[test]
fn rqp_step2_no_insertion_point() {
    for page in [
        "Shop<html><head></head></html>",
        "<!DOCTYPE html><body><p data-rq-block=\"a\">x</p></body>",
        "<noscript></noscript><head></head>",
    ] {
        assert_eq!(
            outcome(&first_visit(), &app(page)),
            Outcome::Invalid(Invalid::NoInsertionPoint),
            "{page}"
        );
    }
}

#[test]
fn rqp_step2_origin_and_path_limits_and_unusable_urls() {
    // DL-089 item 6: an origin or a path over the limits of manifest and patch §1.6.
    let long_path = Request {
        url: format!("https://shop.example.com/{}", "p".repeat(4096)),
        ..first_visit()
    };
    assert_eq!(
        outcome(&long_path, &app(&page_a())),
        Outcome::Invalid(Invalid::Url)
    );
    let at_limit = Request {
        url: format!("https://shop.example.com/{}", "p".repeat(4095)),
        ..first_visit()
    };
    assert_eq!(
        outcome(&at_limit, &app(&page_a())),
        Outcome::Full(Full::NotEligible)
    );
    let long_origin = Request {
        url: format!("https://{}.example/", "a".repeat(505)),
        ..first_visit()
    };
    assert_eq!(
        outcome(&long_origin, &app(&page_a())),
        Outcome::Invalid(Invalid::Url)
    );
    for url in [
        "https://user@shop.example.com/",
        "https://[::1]/",
        "ftp://shop.example.com/",
    ] {
        let r = Request {
            url: url.to_owned(),
            ..first_visit()
        };
        assert_eq!(
            outcome(&r, &app(&page_a())),
            Outcome::Invalid(Invalid::Url),
            "{url}"
        );
    }
}

#[test]
fn rqp_step3_fields_of_f() {
    let a = app(&page_a());
    let a = with(a, "ETag", "\"app-1\"");
    let a = with(a, "etag", "\"app-2\"");
    let a = with(a, "Last-Modified", "Mon, 12 Oct 2026 08:00:00 GMT");
    let a = with(a, "Expires", "0");
    let a = with(a, "content-type", "text/html");
    let a = with(a, "Content-Length", "999");
    let mut a = a;
    a.fields
        .retain(|f| !(f.is("content-type") && f.value == "text/html"));
    let (r, o) = run(&first_visit(), &a);
    assert_eq!(o, Outcome::Full(Full::NotEligible));
    assert_eq!(values(&r, "etag"), [format!("\"{R_A}\"").as_str()]);
    assert_eq!(values(&r, "last-modified"), [] as [&str; 0]);
    assert_eq!(values(&r, "content-length"), [] as [&str; 0]);
    assert_eq!(values(&r, "content-type"), ["text/html; charset=utf-8"]);
    assert_eq!(values(&r, "cache-control"), ["no-cache"]);
    assert_eq!(values(&r, "expires"), ["0"]);
    assert_eq!(values(&r, "rq-version"), ["1"]);
}

#[test]
fn rqp_step3_implied_head_and_head_request() {
    let page = page_a().replace("<head>\n", "");
    let (r, _) = run(&first_visit(), &app(&page));
    let Body::Bytes(body) = r.body else { panic!() };
    let at = page.find("<title").unwrap();
    assert_eq!(&body[..at], &page.as_bytes()[..at]);
    assert!(body[at..].starts_with(b"<script type=\"application/rivqen-manifest+json\">{"));
    let head = Request {
        method: "HEAD".to_owned(),
        ..first_visit()
    };
    let (h, _) = run(&head, &app(&page_a()));
    assert_eq!(h.body, Body::None);
    assert_eq!(h.fields, run(&first_visit(), &app(&page_a())).0.fields);
}

#[test]
fn rqp_step4_conditions() {
    let a = app(&page_a());
    for inm in [
        format!("\"{R_A}\""),
        format!("W/\"{R_A}\""),
        format!("\"r1.x\", \"{R_A}\""),
        "*".to_owned(),
    ] {
        assert_eq!(
            outcome(&set(delta_request(), "If-None-Match", &inm), &a),
            Outcome::NotModified,
            "{inm}"
        );
    }
    // DL-089 item 3: another syntax gives no 304 and no base.
    assert_eq!(
        outcome(&set(delta_request(), "If-None-Match", "r1.x"), &a),
        Outcome::Full(Full::NotEligible)
    );
    // DL-089 item 2: If-Modified-Since without If-None-Match is ignored.
    let ims = set(
        without(delta_request(), "If-None-Match"),
        "If-Modified-Since",
        "Mon, 12 Oct 2026 08:00:00 GMT",
    );
    assert_eq!(outcome(&ims, &a), Outcome::Full(Full::NotEligible));
    // The application validator does not match E (RQP-16).
    let app_etag = with(app(&page_a()), "ETag", "\"app-1\"");
    assert_eq!(
        outcome(&set(first_visit(), "If-None-Match", "\"app-1\""), &app_etag),
        Outcome::Full(Full::NotEligible)
    );
}

#[test]
fn rqp_step9_fields_of_the_304() {
    let a = app(&page_a());
    let a = with(a, "Expires", "0");
    let a = with(a, "Date", "Mon, 12 Oct 2026 08:00:00 GMT");
    let a = with(a, "Content-Location", "/catalog");
    let a = with(a, "Vary", "Accept-Encoding");
    let a = with(a, "X-App", "1");
    let a = with(a, "Last-Modified", "Mon, 12 Oct 2026 08:00:00 GMT");
    let a = with(a, "Set-Cookie", "a=1");
    let head = Request {
        method: "HEAD".to_owned(),
        ..delta_request()
    };
    for request in [delta_request(), head] {
        let (r, o) = run(&request, &a);
        assert_eq!(
            (o, r.status, &r.body),
            (Outcome::NotModified, 304, &Body::None)
        );
        let names: Vec<String> = r
            .fields
            .iter()
            .map(|f| f.name.to_ascii_lowercase())
            .collect();
        assert_eq!(
            names,
            [
                "cache-control",
                "expires",
                "date",
                "content-location",
                "vary",
                "etag",
                "rq-version"
            ]
        );
    }
}

#[test]
fn rqp_step5_every_condition_is_needed() {
    let b = app(&page_b());
    assert_eq!(outcome(&delta_request(), &b), Outcome::Delta);
    let head = Request {
        method: "HEAD".to_owned(),
        ..delta_request()
    };
    let not_eligible = [
        head,
        set(delta_request(), "Range", "bytes=0-99"),
        without(delta_request(), "Rq-Version"),
        set(delta_request(), "Rq-Version", "2"),
        set(delta_request(), "Rq-Capabilities", "(realtime-ws)"),
        without(delta_request(), "Rq-Capabilities"),
        set(delta_request(), "Accept", "text/html"),
        set(delta_request(), "Accept", "*/*"),
        without(delta_request(), "If-None-Match"),
        set(
            delta_request(),
            "If-None-Match",
            &format!("\"{R_A}\", \"r1.x\""),
        ),
        without(delta_request(), "Rq-Have"),
        set(delta_request(), "Rq-Have", "t=t1.other, b=:AAAA:"),
        without(delta_request(), "Rq-Nonce"),
        set(delta_request(), "Rq-Nonce", ":AAEC:"),
    ];
    for (i, request) in not_eligible.iter().enumerate() {
        let (r, o) = run(request, &b);
        assert_eq!(o, Outcome::Full(Full::NotEligible), "case {i}");
        assert_eq!(r.status, 200);
    }
}

#[test]
fn rqp_step6_change_set() {
    let b = app(&page_b());
    let delta_ops = |request: &Request| -> Vec<String> {
        let (r, o) = run(request, &b);
        assert_eq!(o, Outcome::Delta);
        let Body::Bytes(body) = r.body else { panic!() };
        let d: Value = serde_json::from_slice(&body).unwrap();
        d["operations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|op| op["block_id"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(delta_ops(&delta_request()), ["price"]);
    // `b` absent, of 16 bytes (8 × (N − 1)) or of 32 bytes: all blocks (H-7).
    assert_eq!(
        delta_ops(&set(delta_request(), "Rq-Have", &format!("t={T}"))),
        ["title", "price", "cart"]
    );
    assert_eq!(
        delta_ops(&set(
            delta_request(),
            "Rq-Have",
            &format!("t={T}, b=:0Aqua3+/d6ReDehqKiXfLA==:")
        )),
        ["title", "price", "cart"]
    );
    assert_eq!(
        delta_ops(&set(
            delta_request(),
            "Rq-Have",
            &format!("t={T}, b=:0Aqua3+/d6ReDehqKiXfLM8hC9ypGZS5AAAAAAAAAAA=:")
        )),
        ["title", "price", "cart"]
    );
    // The prefixes of the current page: empty change set.
    let current = ":0Aqua3+/d6RsH1JxhxBDMs8hC9ypGZS5:";
    assert_eq!(
        outcome(
            &set(delta_request(), "Rq-Have", &format!("t={T}, b={current}")),
            &b
        ),
        Outcome::Full(Full::EmptyChangeSet)
    );
}

#[test]
fn rqp_step6_zero_blocks() {
    let page = "<!DOCTYPE html><html><head><title>x</title></head><body>z</body></html>";
    let (first, _) = run(&first_visit(), &app(page));
    let etag = values(&first, "etag")[0].to_owned();
    let t = rqp_markup::analyze(page.as_bytes())
        .unwrap()
        .template_revision;
    let request = set(
        set(
            delta_request(),
            "If-None-Match",
            &format!("\"r1.{}\"", "A".repeat(43)),
        ),
        "Rq-Have",
        &format!("t={t}, b=::"),
    );
    assert_eq!(
        outcome(&request, &app(page)),
        Outcome::Full(Full::EmptyChangeSet)
    );
    let request = set(request, "Rq-Have", &format!("t={t}"));
    assert_eq!(
        outcome(&request, &app(page)),
        Outcome::Full(Full::EmptyChangeSet)
    );
    assert_eq!(
        outcome(&set(request, "If-None-Match", &etag), &app(page)),
        Outcome::NotModified
    );
}

#[test]
fn rqp_step7_patch_content_rule() {
    for content in [
        "<a onclick=\"x\">y</a>",
        "<script>alert(1)</script>",
        "<a href=\"data:text/html,x\">y</a>",
        "<svg><title>x</title><path onload=\"x\"/></svg>",
    ] {
        let page = page_a().replace("120.00", content);
        assert_eq!(
            outcome(&delta_request(), &app(&page)),
            Outcome::Full(Full::PatchContent),
            "{content}"
        );
    }
    // Unchanged blocks are not checked: a script in the cart block (json) and an
    // onclick in an unchanged block do not stop the delta.
    let page = page_b().replace(">Shop<", "><").replace(
        "<header>",
        "<div data-rq-block=\"x\"><a onclick=\"x\">y</a></div><header>",
    );
    let t = rqp_markup::analyze(page.as_bytes()).unwrap();
    let prefixes: Vec<u8> = t
        .blocks
        .iter()
        .flat_map(|b| {
            use sha2::Digest;
            let mut digest = sha2::Sha256::digest(&page.as_bytes()[b.start..b.end])[..8].to_vec();
            if b.id == "title" {
                digest[0] ^= 1;
            }
            digest
        })
        .collect();
    let b = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, prefixes);
    let request = set(
        delta_request(),
        "Rq-Have",
        &format!("t={}, b=:{b}:", t.template_revision),
    );
    assert_eq!(outcome(&request, &app(&page)), Outcome::Delta);
}

#[test]
fn rqp_step7_json_block_content_is_not_checked() {
    // The content of a `json` block is script data: no start tags (§4 item 2).
    let request = set(delta_request(), "Rq-Have", &format!("t={T}"));
    let page = page_b().replace("{\"items\":3}", "{\"a\":\"<b onclick=x>\"}");
    assert_eq!(outcome(&request, &app(&page)), Outcome::Delta);
}

#[test]
fn rqp_step7_patch_limit_boundary() {
    // D of the KAT price change has 115 bytes.
    let b = app(&page_b());
    let at = config(&json!({"maxPatchBytes": 115}));
    assert_eq!(select(&at, &delta_request(), &b).1, Outcome::Delta);
    let Body::Bytes(d) = select(&at, &delta_request(), &b).0.body else {
        panic!()
    };
    assert_eq!(d.len(), 115);
    let below = config(&json!({"maxPatchBytes": 114}));
    assert_eq!(
        select(&below, &delta_request(), &b).1,
        Outcome::Full(Full::TooLarge)
    );
}

#[test]
fn rqp_step7_hint_bounds_the_delta() {
    // A hint smaller than maxPatchBytes is written and bounds D (§2.5 item 5).
    let big = page_b().replace("130.00", &"9".repeat(2000));
    // The configuration changes the page revision; any other base works.
    let request = set(
        delta_request(),
        "If-None-Match",
        &format!("\"{}\"", other_base()),
    );
    let hinted = config(&json!({"limits": {"max_patch_bytes": 1024}}));
    assert_eq!(
        select(&hinted, &request, &app(&big)).1,
        Outcome::Full(Full::TooLarge)
    );
    let hinted_large = config(&json!({"limits": {"max_patch_bytes": 4096}}));
    assert_eq!(
        select(&hinted_large, &request, &app(&big)).1,
        Outcome::Delta
    );
}

/// Any base other than the current page revision.
fn other_base() -> String {
    format!("r1.{}", "B".repeat(43))
}

#[test]
fn rqp_step7_delta_must_be_smaller_than_f() {
    // Each `"` costs 2 bytes in D and 1 byte in F: find the first size where D
    // is not smaller than F, and check that one quote less gives D = F − 1.
    let mut previous: Option<(usize, usize)> = None;
    for k in 0..2000 {
        let page = page_a().replace("120.00", &"\"".repeat(k));
        let a = app(&page);
        let (f, _) = run(&first_visit(), &a);
        let Body::Bytes(f) = f.body else { panic!() };
        let (d, o) = run(&delta_request(), &a);
        match o {
            Outcome::Delta => {
                let Body::Bytes(d) = d.body else { panic!() };
                assert!(d.len() < f.len());
                previous = Some((d.len(), f.len()));
            }
            Outcome::Full(Full::NotSmaller) => {
                let (d_len, f_len) = previous.expect("a delta for a small block");
                assert_eq!(d_len + 1, f_len, "k = {k}");
                return;
            }
            other => panic!("{other:?}"),
        }
    }
    panic!("D never reached the size of F");
}

#[test]
fn rqp_step7_fields_of_d() {
    let a = app(&page_b());
    let a = with(a, "ETag", "\"app\"");
    let a = with(a, "Last-Modified", "Mon, 12 Oct 2026 08:00:00 GMT");
    let a = with(a, "Expires", "0");
    let a = with(a, "X-Content-Type-Options", "other");
    let a = with(a, "Set-Cookie", "a=1");
    let (r, o) = run(&delta_request(), &a);
    assert_eq!(o, Outcome::Delta);
    for gone in ["etag", "last-modified", "expires"] {
        assert!(values(&r, gone).is_empty(), "{gone}");
    }
    assert_eq!(
        values(&r, "content-type"),
        ["application/vnd.rivqen.delta+json"]
    );
    assert_eq!(values(&r, "x-content-type-options"), ["nosniff"]);
    assert_eq!(values(&r, "cache-control"), ["no-store"]);
    assert_eq!(values(&r, "rq-version"), ["1"]);
    assert_eq!(values(&r, "vary"), ["Rq-Nonce"]);
    assert_eq!(values(&r, "set-cookie"), ["a=1"]);
}

#[test]
fn rqp_step7_binding_has_weak_base_value_and_query() {
    let b = app(&page_b());
    let weak = set(delta_request(), "If-None-Match", &format!("W/\"{R_A}\""));
    assert_eq!(run(&weak, &b).0.body, run(&delta_request(), &b).0.body);
    let query = Request {
        url: format!("{URL}?sort=price&page=2"),
        ..delta_request()
    };
    assert_ne!(run(&query, &b).0.body, run(&delta_request(), &b).0.body);
}

#[test]
fn rqp_step8_policy_change_only_gives_f() {
    // H-55: the page revision changes, the change set is empty.
    let policy = config(&json!({"policy": {"ttl_seconds": 60}}));
    let (r, o) = select(&policy, &delta_request(), &app(&page_a()));
    assert_eq!(o, Outcome::Full(Full::EmptyChangeSet));
    assert_ne!(values(&r, "etag"), [format!("\"{R_A}\"").as_str()]);
}

#[test]
fn rqp_step7_large_adversarial_blocks_are_linear() {
    // Four changed `html` blocks of about 1 MiB each that pass the patch
    // content rule: every attribute loop of §4.2 runs to the end. A quadratic
    // step would take hours here; the bound is generous for a debug build.
    let n = 1_000_000;
    let blocks = [
        format!("<img srcset=\"{}\">", ", ".repeat(n / 2)),
        format!("<b title=\"&#{}\">x</b>", "1".repeat(n)),
        format!("<img srcset=\"{}\">", "&#169".repeat(n / 5)),
        format!("<b{}>x</b>", " a".repeat(n / 2)),
    ];
    let mut page = String::from("<!DOCTYPE html><html><head></head><body>");
    for (i, b) in blocks.iter().enumerate() {
        page.push_str(&format!("<div data-rq-block=\"b{i}\">{b}</div>"));
    }
    page.push_str("</body></html>");
    let doc = rqp_markup::analyze(page.as_bytes()).unwrap();
    let request = set(
        delta_request(),
        "Rq-Have",
        &format!("t={}", doc.template_revision),
    );
    let start = std::time::Instant::now();
    let (_, o) = run(&request, &app(&page));
    assert_eq!(o, Outcome::Full(Full::TooLarge));
    assert!(start.elapsed().as_secs() < 60, "{:?}", start.elapsed());
}
