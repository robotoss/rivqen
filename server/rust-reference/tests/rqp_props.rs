// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Property tests of `rqp::respond`: no panic on any page bytes and request
//! fields; F is the same for every request of one page and configuration
//! (H-51); no outcome keeps an `Rq-*` field of A; a delta is smaller than F and
//! within the limit.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use proptest::prelude::*;
use rivqen_reference::model::{AppResponse, Body, Field, Request, Response};
use rivqen_reference::rqp::config::RqpConfig;
use rivqen_reference::rqp::select::{Invalid, Outcome, select};
use serde_json::json;

const URL: &str = "https://shop.example.com/catalog";

/// Markup pieces that reach valid pages with blocks and every step.
fn piece() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("<!DOCTYPE html>".to_owned()),
        Just("<html>".to_owned()),
        Just("<head>".to_owned()),
        Just("</head>".to_owned()),
        Just("<body>".to_owned()),
        Just("<title data-rq-block=\"t\">".to_owned()),
        Just("</title>".to_owned()),
        Just("<p data-rq-block=\"p\">".to_owned()),
        Just("<div data-rq-block=\"d\">".to_owned()),
        Just("</p>".to_owned()),
        Just("</div>".to_owned()),
        Just("<script type=\"application/json\" data-rq-block=\"j\">{}</script>".to_owned()),
        Just("<a onclick=\"x\">y</a>".to_owned()),
        Just("<img src=\"data:image/png,x\">".to_owned()),
        Just("<svg><path d=\"M0\"/></svg>".to_owned()),
        Just("<!-- c -->".to_owned()),
        Just("<meta charset=\"utf-8\">".to_owned()),
        Just("\"".to_owned()),
        Just("\n".to_owned()),
        "[a-z0-9 <>/=\"&;#]{0,8}",
    ]
}

fn page() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        prop::collection::vec(piece(), 0..24).prop_map(|p| p.concat().into_bytes()),
        prop::collection::vec(any::<u8>(), 0..256),
    ]
}

fn field_line() -> impl Strategy<Value = Field> {
    let name = prop_oneof![
        Just("Rq-Version"),
        Just("Rq-Capabilities"),
        Just("Rq-Have"),
        Just("Rq-Nonce"),
        Just("If-None-Match"),
        Just("Accept"),
        Just("Range"),
        Just("If-Match"),
        Just("If-Modified-Since"),
    ];
    let value = prop_oneof![
        Just("1".to_owned()),
        Just("(block-patch)".to_owned()),
        Just("text/html, application/vnd.rivqen.delta+json".to_owned()),
        Just(":AAECAwQFBgcICQoLDA0ODw==:".to_owned()),
        Just(format!("\"r1.{}\"", "A".repeat(43))),
        Just("*".to_owned()),
        "t=t1\\.[A-Za-z0-9_-]{43}(, b=:[A-Za-z0-9+/]{0,24}=?=?:)?",
        "[ -~]{0,40}",
    ];
    (name, value).prop_map(|(n, v)| Field::new(n, &v))
}

fn app_fields() -> impl Strategy<Value = Vec<Field>> {
    let extra = prop_oneof![
        Just(Field::new("Rq-Version", "1")),
        Just(Field::new("rq-x", "1")),
        Just(Field::new("Vary", "Accept-Encoding, Rq-Nonce")),
        Just(Field::new("ETag", "\"app\"")),
        Just(Field::new("Content-Type", "text/html; charset=iso-8859-1")),
        Just(Field::new("Content-Encoding", "gzip")),
        Just(Field::new("Content-Length", "7")),
    ];
    prop::collection::vec(extra, 0..4).prop_map(|mut extra| {
        let mut fields = vec![Field::new("Content-Type", "text/html; charset=utf-8")];
        fields.append(&mut extra);
        fields
    })
}

fn config() -> impl Strategy<Value = RqpConfig> {
    prop_oneof![
        Just(RqpConfig::default()),
        Just(RqpConfig::from_json(&json!({"policy": {"ttl_seconds": 60}})).unwrap()),
        Just(
            RqpConfig::from_json(
                &json!({"maxPatchBytes": 64, "limits": {"max_patch_bytes": 2048}})
            )
            .unwrap()
        ),
    ]
}

fn method() -> impl Strategy<Value = String> {
    prop_oneof![Just("GET"), Just("HEAD"), Just("POST")].prop_map(str::to_owned)
}

fn no_rq_of_a(r: &Response) -> bool {
    r.fields.iter().all(|f| {
        let rq = f.name.to_ascii_lowercase().starts_with("rq-");
        (!rq || (f.name == "Rq-Version" && f.value == "1"))
            && !f.is("content-length")
            && (!f.is("vary")
                || f.value == "Rq-Nonce"
                || !f.value.to_ascii_lowercase().contains("rq-"))
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn respond_never_panics_and_keeps_no_rq_field_of_a(
        config in config(),
        method in method(),
        status in prop_oneof![Just(200_u16), Just(404), Just(304)],
        body in page(),
        fields in prop::collection::vec(field_line(), 0..8),
        app_fields in app_fields(),
        url in prop_oneof![Just(URL.to_owned()), "https://shop\\.example\\.com/[ -~]{0,30}", "[ -~]{0,30}"],
    ) {
        let request = Request { method, url, fields };
        let app = AppResponse { status, fields: app_fields, body };
        let (r, outcome) = select(&config, &request, &app);
        prop_assert!(no_rq_of_a(&r));
        let versions = r.fields.iter().filter(|f| f.is("rq-version")).count();
        match outcome {
            Outcome::Full(_) | Outcome::Delta | Outcome::NotModified => prop_assert_eq!(versions, 1),
            _ => {
                prop_assert_eq!(versions, 0);
                prop_assert_eq!(r.status, app.status);
            }
        }
        if outcome == Outcome::Delta {
            let Body::Bytes(d) = &r.body else { panic!("D without a body") };
            prop_assert!(d.len() <= config.patch_limit());
            let first = Request { method: "GET".to_owned(), url: request.url.clone(), fields: vec![] };
            let (f, _) = select(&config, &first, &app);
            let Body::Bytes(f) = f.body else { panic!("F without a body") };
            prop_assert!(d.len() < f.len());
        }
    }

    #[test]
    fn f_is_the_same_for_every_request_of_one_page(
        config in config(),
        body in page(),
        fields in prop::collection::vec(field_line(), 0..8),
    ) {
        let app = AppResponse {
            status: 200,
            fields: vec![Field::new("Content-Type", "text/html; charset=utf-8"), Field::new("Cache-Control", "no-cache")],
            body,
        };
        let browser = Request { method: "GET".to_owned(), url: URL.to_owned(), fields: vec![] };
        let (reference, outcome) = select(&config, &browser, &app);
        let rqp = Request { method: "GET".to_owned(), url: URL.to_owned(), fields };
        let (r, o) = select(&config, &rqp, &app);
        if matches!(outcome, Outcome::Full(_)) && matches!(o, Outcome::Full(_)) {
            prop_assert_eq!(r, reference);
        }
    }
}

/// A delta request for `body` with the template of its analysis and the given
/// `b` bytes (`None`: no `b`), or `None` when the page is invalid.
fn delta_request(body: &[u8], b: Option<&[u8]>) -> Option<Request> {
    use base64::Engine as _;
    let doc = rqp_markup::analyze(body).ok()?;
    let have = match b {
        Some(b) => format!(
            "t={}, b=:{}:",
            doc.template_revision,
            base64::engine::general_purpose::STANDARD.encode(b)
        ),
        None => format!("t={}", doc.template_revision),
    };
    Some(Request {
        method: "GET".to_owned(),
        url: URL.to_owned(),
        fields: vec![
            Field::new("Rq-Version", "1"),
            Field::new("Rq-Capabilities", "(block-patch)"),
            Field::new("Accept", "application/vnd.rivqen.delta+json"),
            Field::new("If-None-Match", &format!("\"r1.{}\"", "A".repeat(43))),
            Field::new("Rq-Have", &have),
            Field::new("Rq-Nonce", ":AAECAwQFBgcICQoLDA0ODw==:"),
        ],
    })
}

fn html_app(body: Vec<u8>) -> AppResponse {
    AppResponse {
        status: 200,
        fields: vec![Field::new("Content-Type", "text/html; charset=utf-8")],
        body,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Steps 6 and 7 on valid pages: D has the changed blocks in document
    /// order, each with the current content of its block.
    #[test]
    fn delta_operations_are_the_changed_blocks_in_order(
        body in page(),
        b in prop::option::of(prop::collection::vec(any::<u8>(), 0..40)),
    ) {
        let Some(request) = delta_request(&body, b.as_deref()) else { return Ok(()) };
        let app = html_app(body.clone());
        let (r, outcome) = select(&RqpConfig::default(), &request, &app);
        prop_assert!(
            matches!(outcome, Outcome::Delta | Outcome::Full(_) | Outcome::Invalid(Invalid::NoInsertionPoint)),
            "{:?}",
            outcome
        );
        if outcome == Outcome::Delta {
            let doc = rqp_markup::analyze(&body).unwrap();
            let Body::Bytes(d) = r.body else { panic!("D without a body") };
            let d: serde_json::Value = serde_json::from_slice(&d).unwrap();
            let mut last = None;
            for op in d["operations"].as_array().unwrap() {
                let i = doc.blocks.iter().position(|blk| blk.id == op["block_id"]).unwrap();
                prop_assert!(last.is_none_or(|l| l < i));
                last = Some(i);
                let blk = &doc.blocks[i];
                prop_assert_eq!(op["content"].as_str().unwrap().as_bytes(), &body[blk.start..blk.end]);
            }
            prop_assert!(last.is_some());
        }
    }
}

/// A guard: with a fixed seed, the page generator reaches D and F (with and
/// without `b`), so the properties above run the steps that matter.
#[test]
fn page_generator_reaches_the_delta_and_the_full_document() {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::{Config, TestRunner};
    let mut runner = TestRunner::new_with_rng(
        Config::default(),
        proptest::test_runner::TestRng::deterministic_rng(
            proptest::test_runner::RngAlgorithm::ChaCha,
        ),
    );
    let (mut delta, mut full) = (0, 0);
    for _ in 0..2000 {
        let body = page().new_tree(&mut runner).unwrap().current();
        let Some(request) = delta_request(&body, None) else {
            continue;
        };
        match select(&RqpConfig::default(), &request, &html_app(body)).1 {
            Outcome::Delta => delta += 1,
            Outcome::Full(_) => full += 1,
            _ => {}
        }
    }
    assert!(delta >= 20 && full >= 20, "delta {delta}, full {full}");
}
