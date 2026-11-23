// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! Tests of the loader of the stored client exchanges (WP-08 S1, T-05;
//! DL-216; `fixtures/legacy/README.md` "Client `exchanges.json`").
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::fs;
use std::path::{Path, PathBuf};

use rivqen_testkit::exchange::{
    BodySource, ClientExchanges, FILE_NAME, MAX_EXCHANGES, Stop, load_all, load_all_at, load_dir,
};
use rivqen_testkit::fixtures::{FixtureError, load_json};
use rivqen_testkit::fixtures_root;
use serde_json::{Value, json};

fn legacy() -> PathBuf {
    fixtures_root().join("legacy")
}

fn one(id: &str) -> ClientExchanges {
    load_dir(&legacy().join(id), &legacy().join("pages")).unwrap()
}

fn shape(error: FixtureError) -> String {
    match error {
        FixtureError::Shape { what, .. } => what,
        other => panic!("not a shape error: {other}"),
    }
}

/// A scratch fixture tree in the target directory: `pages/a.html` and one
/// fixture `FX-LEG-CL-T-Q` whose `exchanges.json` is `file`.
fn scratch(name: &str, file: &Value) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    fs::remove_dir_all(&root).ok();
    fs::create_dir_all(root.join("pages")).unwrap();
    fs::copy(legacy().join("pages/a.html"), root.join("pages/a.html")).unwrap();
    let dir = root.join("FX-LEG-CL-T-Q");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("case.json"),
        r#"{"kind":"client","profile":"legacy-canonical"}"#,
    )
    .unwrap();
    fs::write(dir.join(FILE_NAME), serde_json::to_vec(file).unwrap()).unwrap();
    root
}

fn load_scratch(name: &str, file: &Value) -> Result<ClientExchanges, FixtureError> {
    let root = scratch(name, file);
    load_dir(&root.join("FX-LEG-CL-T-Q"), &root.join("pages"))
}

fn page_a() -> Value {
    let len = fs::metadata(legacy().join("pages/a.html")).unwrap().len();
    json!({ "page": "a", "len": len, "sha256": "0".repeat(64) })
}

/// A valid file: one complete exchange with page `a`, one refused one.
fn valid() -> Value {
    json!({
        "id": "FX-LEG-CL-T-Q",
        "format": 1,
        "cache_before": [],
        "exchanges": [
            {
                "exchange": null, "step": 1, "session": 1,
                "request": { "method": "GET", "target": "/catalog", "fields": [["accept-diff", "true"]] },
                "response": null,
                "stop": { "kind": "refused" }
            },
            {
                "exchange": 1, "step": 3, "session": 2,
                "request": { "method": "GET", "target": "/catalog", "fields": [["accept-diff", "true"]] },
                "response": {
                    "status": 200,
                    "fields": [["content-type", "text/html"]],
                    "body": { "base64": "Zm9vYmFy", "len": 6, "sha256": "f".repeat(64) }
                },
                "stop": { "kind": "cut", "at": 6 }
            }
        ]
    })
}

#[test]
fn exchange_loads_all() {
    let all = load_all().unwrap();
    assert_eq!(all.len(), 104);
    for fixture in &all {
        assert!(fixture.id.starts_with("FX-LEG-CL-"), "{}", fixture.id);
        assert!(!fixture.id.contains("--"), "{}", fixture.id);
        assert_eq!(
            fixture.dir.file_name().unwrap().to_str(),
            Some(fixture.id.as_str())
        );
    }
    let mut ids: Vec<&str> = all.iter().map(|f| f.id.as_str()).collect();
    ids.dedup();
    assert_eq!(ids.len(), 104);
    assert!(ids.is_sorted());
    // The catalog uses each form: records, refused connections, cuts, page and inline bodies.
    let exchanges = || all.iter().flat_map(|f| &f.exchanges);
    assert!(all.iter().any(|f| !f.cache_before.is_empty()));
    assert!(exchanges().any(|e| e.stop == Stop::Refused));
    assert!(exchanges().any(|e| matches!(e.stop, Stop::Cut { .. })));
    let bodies = || exchanges().filter_map(|e| e.response.as_ref()?.body.as_ref());
    assert!(bodies().any(|b| matches!(b.source(), BodySource::Page(_))));
    assert!(bodies().any(|b| *b.source() == BodySource::Inline));
    assert!(exchanges().any(|e| e.response.as_ref().is_some_and(|r| r.body.is_none())));
}

/// The numbered exchanges are the requests of `expected.json`, with the same
/// status and the same listed response field lines (README rule 1, here from
/// the Rust side).
#[test]
fn exchange_numbers_and_statuses_equal_the_requests_of_expected_json() {
    const LISTED: [&str; 6] = [
        "etag",
        "template-tag",
        "template-change",
        "cache-offline",
        "sonic-etag-key",
        "set-cookie",
    ];
    for fixture in load_all().unwrap() {
        let expected = load_json(&fixture.dir.join("expected.json")).unwrap();
        let requests: Vec<&Value> = expected["client"]["phases"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|p| p["requests"].as_array().unwrap())
            .collect();
        let numbered: Vec<_> = fixture
            .exchanges
            .iter()
            .filter(|e| e.number.is_some())
            .collect();
        assert_eq!(numbered.len(), requests.len(), "{}", fixture.id);
        for request in requests {
            let n = u32::try_from(request["exchange"].as_u64().unwrap()).unwrap();
            let e = numbered.iter().find(|e| e.number == Some(n)).unwrap();
            assert_eq!(e.request.target, request["target"].as_str().unwrap());
            let Some(want) = request.get("response").filter(|r| !r.is_null()) else {
                continue;
            };
            let got = e.response.as_ref().unwrap();
            assert_eq!(
                u64::from(got.status),
                want["status"].as_u64().unwrap(),
                "{} {n}",
                fixture.id
            );
            let listed: Vec<Value> = got
                .fields
                .iter()
                .filter(|f| LISTED.iter().any(|name| f.is(name)))
                .map(|f| json!([f.name.to_ascii_lowercase(), f.value]))
                .collect();
            assert_eq!(&Value::Array(listed), &want["fields"], "{} {n}", fixture.id);
            if want["complete"] == json!(true) {
                assert_eq!(e.stop, Stop::Complete, "{} {n}", fixture.id);
            }
        }
    }
}

#[test]
fn exchange_page_body_is_the_page_file() {
    let fixture = one("FX-LEG-CL-CS04-Q");
    let body = fixture.exchanges[0]
        .response
        .as_ref()
        .unwrap()
        .body
        .as_ref()
        .unwrap();
    assert_eq!(*body.source(), BodySource::Page("a".to_owned()));
    assert_eq!(
        body.bytes(),
        fs::read(legacy().join("pages/a.html")).unwrap()
    );
    assert_eq!(body.len(), 296);
    assert_eq!(body.sha256().len(), 64);
}

#[test]
fn exchange_data_body_decodes_to_the_legacy_data_object() {
    let fixture = one("FX-LEG-CL-CS04-Q");
    let second = &fixture.exchanges[1];
    assert_eq!(second.number, Some(2));
    assert_eq!(second.session, 2);
    let names: Vec<&str> = second
        .request
        .fields
        .iter()
        .map(|f| f.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "accept-diff",
            "if-none-match",
            "template-tag",
            "accept-encoding"
        ]
    );
    let response = second.response.as_ref().unwrap();
    assert!(
        response
            .fields
            .iter()
            .any(|f| f.is("template-change") && f.value == "false")
    );
    let body = response.body.as_ref().unwrap();
    assert_eq!(*body.source(), BodySource::Inline);
    let data: Value = serde_json::from_slice(body.bytes()).unwrap();
    assert_eq!(data["diff"], json!(""));
    assert!(data["data"]["{price}"].as_str().unwrap().contains("130.00"));
}

#[test]
fn exchange_refused_connection_has_no_number_and_no_response() {
    let fixture = one("FX-LEG-CL-CS12-Q");
    let refused: Vec<_> = fixture
        .exchanges
        .iter()
        .filter(|e| e.stop == Stop::Refused)
        .collect();
    assert_ne!(refused.len(), 0);
    for e in refused {
        assert_eq!(e.number, None);
        assert!(e.response.is_none());
    }
}

#[test]
fn exchange_cut_is_inside_the_body() {
    for id in [
        "FX-LEG-CL-INTERRUPTED-STREAM-Q",
        "FX-LEG-CL-INTERRUPTED-BEFORE-S",
    ] {
        let fixture = one(id);
        let (at, len) = fixture
            .exchanges
            .iter()
            .find_map(|e| match e.stop {
                Stop::Cut { at } => Some((at, e.response.as_ref()?.body.as_ref()?.len())),
                _ => None,
            })
            .unwrap();
        assert!(at <= len, "{id}: {at} > {len}");
    }
}

#[test]
fn exchange_cache_before_record_is_the_expanded_first_load() {
    let fixture = one("FX-LEG-CL-DIFF-AGE-RELOAD-Q");
    let [record] = fixture.cache_before.as_slice() else {
        panic!("one record");
    };
    assert_eq!(record.partition.as_deref(), Some("rq-test-account"));
    assert_eq!(record.url, "https://shop.example.com/catalog");
    assert_eq!(record.vary, []);
    assert_eq!(
        record.page.bytes(),
        fs::read(legacy().join("pages/a.html")).unwrap()
    );
    let keys: Vec<&str> = record.data.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["{title}", "{price}", "{stock}"]);
    // The template with each placeholder replaced once by its value is the page (CL-41).
    let mut rebuilt = record.template.clone();
    for (key, value) in &record.data {
        rebuilt = rebuilt.replacen(key.as_str(), value, 1);
    }
    assert_eq!(rebuilt.as_bytes(), record.page.bytes());
    assert_eq!(record.etag.len(), 40);
    assert_eq!(record.template_tag.len(), 40);
    // The first request revalidates the seeded record.
    let request = &fixture.exchanges[0].request;
    assert!(
        request
            .fields
            .iter()
            .any(|f| f.is("if-none-match") && f.value == record.etag)
    );
}

#[test]
fn exchange_debug_has_no_body_bytes() {
    let fixture = one("FX-LEG-CL-DIFF-AGE-RELOAD-Q");
    let text = format!("{fixture:?}");
    assert!(
        !text.contains("Static header"),
        "a page or template byte in Debug"
    );
    assert!(!text.contains("sonicdiff"), "a data value in Debug");
    assert!(!text.contains("In stock"), "a data value in Debug");
    let data = one("FX-LEG-CL-CS04-Q");
    let text = format!("{data:?}");
    assert!(!text.contains("eyJk"), "base64 text in Debug");
    assert!(!text.contains("130.00"), "a data value in Debug");
    assert!(!text.contains("3e3184cb"), "a field value in Debug");
}

#[test]
fn exchange_scratch_file_loads_with_its_typed_values() {
    let e = load_scratch("exchange-valid", &valid()).unwrap();
    assert_eq!(e.exchanges.len(), 2);
    assert_eq!(e.exchanges[0].number, None);
    assert_eq!(e.exchanges[0].step, 1);
    assert_eq!(e.exchanges[1].stop, Stop::Cut { at: 6 });
    let body = e.exchanges[1]
        .response
        .as_ref()
        .unwrap()
        .body
        .as_ref()
        .unwrap();
    assert_eq!(body.bytes(), b"foobar");
    assert!(!body.is_empty());
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("exchange-valid");
    assert_eq!(load_all_at(&root).unwrap(), vec![e]);
}

#[test]
fn exchange_load_all_at_skips_profile_and_server_fixtures() {
    let root = scratch("exchange-select", &valid());
    for (name, case) in [
        (
            "FX-LEG-CL-T-Q--ANDROID",
            r#"{"kind":"client","profile":"legacy-android"}"#,
        ),
        ("FX-LEG-DATA", r#"{"id":"FX-LEG-DATA"}"#),
    ] {
        fs::create_dir_all(root.join(name)).unwrap();
        fs::write(root.join(name).join("case.json"), case).unwrap();
    }
    fs::create_dir_all(root.join("functions")).unwrap();
    let all = load_all_at(&root).unwrap();
    assert_eq!(
        all.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        ["FX-LEG-CL-T-Q"]
    );
}

#[test]
fn exchange_missing_file_is_an_io_error_with_its_path() {
    let root = scratch("exchange-missing", &valid());
    fs::remove_file(root.join("FX-LEG-CL-T-Q").join(FILE_NAME)).unwrap();
    match load_all_at(&root) {
        Err(FixtureError::Io { path, .. }) => {
            assert!(path.ends_with(Path::new("FX-LEG-CL-T-Q").join(FILE_NAME)));
        }
        other => panic!("{other:?}"),
    }
}

/// The valid file with one change must fail with a shape error that
/// contains `want`.
fn rejects(name: &str, change: impl Fn(&mut Value), want: &str) {
    let mut file = valid();
    change(&mut file);
    let what = shape(load_scratch(&format!("exchange-bad-{name}"), &file).unwrap_err());
    assert!(what.contains(want), "{name}: {what}");
}

fn page_body(page: &str, len: u64) -> Value {
    json!({ "page": page, "len": len, "sha256": "0".repeat(64) })
}

fn record_with(partition: &Value, vary: &Value) -> Value {
    json!([{
        "partition": partition, "url": "u", "vary": vary, "etag": "e", "template_tag": "t",
        "page": page_a(), "template": "x", "data": [], "fields": []
    }])
}

#[test]
fn exchange_rejects_a_file_that_breaks_a_top_level_rule() {
    rejects("format", |f| f["format"] = json!(2), "`format` is not 1");
    rejects(
        "id",
        |f| f["id"] = json!("FX-LEG-CL-OTHER-Q"),
        "not the directory name",
    );
    rejects(
        "member",
        |f| f["extra"] = json!(1),
        "unknown member `extra`",
    );
    rejects(
        "missing",
        |f| {
            f.as_object_mut().unwrap().remove("exchanges");
        },
        "member `exchanges` is missing",
    );
    rejects(
        "too-many",
        |f| {
            let first = f["exchanges"][0].clone();
            f["exchanges"] = Value::Array(vec![first; MAX_EXCHANGES + 1]);
        },
        "more than 256 entries",
    );
    rejects(
        "record-partition",
        |f| f["cache_before"] = record_with(&json!(1), &json!([])),
        "partition is not a string or null",
    );
    rejects(
        "record-vary",
        |f| f["cache_before"] = record_with(&Value::Null, &json!([["cookie", 1]])),
        "an entry is not [name, value or null]",
    );
}

#[test]
fn exchange_rejects_an_exchange_that_breaks_a_rule() {
    rejects(
        "number",
        |f| f["exchanges"][1]["exchange"] = json!(2),
        "exchange 2, expected 1",
    );
    rejects(
        "zero",
        |f| f["exchanges"][1]["exchange"] = json!(0),
        "exchange is not an integer of at least 1",
    );
    rejects(
        "refused-response",
        |f| f["exchanges"][1]["stop"] = json!({ "kind": "refused" }),
        "go together",
    );
    rejects(
        "null-number",
        |f| f["exchanges"][1]["exchange"] = Value::Null,
        "go together",
    );
    rejects(
        "cut",
        |f| f["exchanges"][1]["stop"]["at"] = json!(7),
        "the cut at 7 is after the body (6 bytes)",
    );
    rejects(
        "stop",
        |f| f["exchanges"][1]["stop"] = json!({ "kind": "reset" }),
        "is not `cut` or `refused`",
    );
    rejects(
        "status",
        |f| f["exchanges"][1]["response"]["status"] = json!(99),
        "status is not a status code",
    );
    rejects(
        "field",
        |f| f["exchanges"][1]["request"]["fields"] = json!([["a", 1]]),
        "not a pair of strings",
    );
}

#[test]
fn exchange_rejects_a_body_that_breaks_a_rule() {
    rejects(
        "len",
        |f| f["exchanges"][1]["response"]["body"]["len"] = json!(5),
        "6 bytes, not len 5",
    );
    rejects(
        "base64",
        |f| f["exchanges"][1]["response"]["body"]["base64"] = json!("Zm9vYmF"),
        "base64: the length is not a multiple of 4",
    );
    rejects(
        "sha",
        |f| f["exchanges"][1]["response"]["body"]["sha256"] = json!("F".repeat(64)),
        "64 lowercase hex digits",
    );
    rejects(
        "both",
        |f| f["exchanges"][1]["response"]["body"]["page"] = json!("a"),
        "exactly one of `page` and `base64`",
    );
    rejects(
        "page-id",
        |f| f["exchanges"][1]["response"]["body"] = page_body("../a", 1),
        "is not a page id",
    );
    rejects(
        "page-len",
        |f| f["exchanges"][1]["response"]["body"] = page_body("a", 1),
        "pages/a.html has 296 bytes, not 1",
    );
}

#[test]
fn exchange_record_with_a_null_partition_and_vary_loads() {
    let mut file = valid();
    file["cache_before"] = json!([{
        "partition": null, "url": "https://shop.example.com/catalog", "vary": [["cookie", null], ["x", "1"]],
        "etag": "e", "template_tag": "t", "page": page_a(), "template": "x",
        "data": [["{b}", "2"], ["{a}", "1"]], "fields": [["content-type", "text/html"]]
    }]);
    let e = load_scratch("exchange-record", &file).unwrap();
    let r = &e.cache_before[0];
    assert_eq!(r.partition, None);
    assert_eq!(
        r.vary,
        [
            ("cookie".to_owned(), None),
            ("x".to_owned(), Some("1".to_owned()))
        ]
    );
    // The data order of the file is kept (not the sorted order of a JSON object).
    assert_eq!(r.data[0].0, "{b}");
    assert_eq!(*r.page.source(), BodySource::Page("a".to_owned()));
}
