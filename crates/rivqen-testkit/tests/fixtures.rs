// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! Fixture loader tests.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::fs;
use std::path::PathBuf;

use rivqen_testkit::fixtures::{
    ExpectedBody, FixtureError, load_legacy, load_legacy_at, load_malformed, load_rqp_protocol,
    load_rqp_protocol_at,
};

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    fs::remove_dir_all(&dir).ok();
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn testkit_loads_all_rqp_protocol() {
    let all = load_rqp_protocol().unwrap();
    assert_eq!(all.server.len(), 141);
    assert_eq!(all.client.len(), 153);
    for f in all.server.iter().chain(&all.client) {
        assert!(f.case.is_object() && f.expected.is_object(), "{}", f.id);
    }
    for f in &all.server {
        assert!(f.page.is_some(), "{} has no page bytes", f.id);
    }
    // A client fixture has page bytes exactly when its context names a snapshot page.
    let with_page = all.client.iter().filter(|f| f.page.is_some()).count();
    let named = all
        .client
        .iter()
        .filter(|f| {
            f.case
                .get("context")
                .and_then(|c| c.get("snapshot"))
                .is_some_and(|s| !s.is_null())
        })
        .count();
    assert_eq!(with_page, named);
    assert_eq!(with_page, 95);
    let mut with_body = 0;
    for f in &all.client {
        let resp = f.client_response().unwrap();
        match (&f.response_body, resp.body_bytes) {
            (Some(b), Some(n)) => {
                assert_eq!(b.len() as u64, n, "{}", f.id);
                with_body += 1;
            }
            (None, None | Some(0)) => {}
            _ => panic!("{}: body file and body_bytes disagree", f.id),
        }
        f.client_result().unwrap();
        f.client_context().unwrap();
        f.expected_request().unwrap();
    }
    assert_eq!(with_body, 145);
}

#[test]
fn testkit_reads_server_delta_binding() {
    let all = load_rqp_protocol().unwrap();
    let delta = all
        .server
        .iter()
        .find(|f| f.id == "FX-RQ-P09-DELTA")
        .unwrap();
    let ExpectedBody::Delta(d) = delta.server_body().unwrap() else {
        panic!("not a delta");
    };
    assert!(d.binding.starts_with("d1."), "{}", d.binding);
    assert_eq!(d.inputs.nonce, "AAECAwQFBgcICQoLDA0ODw");
    assert_eq!(d.blocks.len(), 1);
    let kinds: Vec<_> = all
        .server
        .iter()
        .map(|f| f.server_body().unwrap())
        .collect();
    assert!(kinds.iter().any(|k| matches!(k, ExpectedBody::Full { .. })));
    assert!(kinds.iter().any(|k| matches!(k, ExpectedBody::App)));
}

#[test]
fn testkit_reads_client_result() {
    let all = load_rqp_protocol().unwrap();
    let c = all
        .client
        .iter()
        .find(|f| f.id == "FX-RQ-C01-OTHER-BASE")
        .unwrap();
    let r = c.client_result().unwrap();
    assert_eq!(r.outcome, "rejected");
    assert_eq!(r.error.as_deref(), Some("INTEGRITY_ERROR"));
    assert_eq!(r.next.as_deref(), Some("fallback-load"));
    assert!(r.snapshot.is_none());
    let req = c.expected_request().unwrap();
    assert_eq!(req.method, "GET");
    assert!(req.headers.iter().any(|h| h.is("rq-nonce")));
    assert_eq!(c.client_response().unwrap().status, 200);
}

#[test]
fn testkit_loads_all_legacy() {
    let all = load_legacy().unwrap();
    assert_eq!(all.len(), 335);
    for f in &all {
        assert!(f.case.is_object() && f.expected.is_object(), "{}", f.id);
        if f.case.get("page").is_some() {
            assert!(f.page.is_some(), "{}", f.id);
        }
    }
    assert_eq!(all.iter().filter(|f| f.page.is_some()).count(), 59);
}

#[test]
fn testkit_loads_all_malformed() {
    let m = load_malformed().unwrap();
    assert_eq!(m.cases.len(), 22);
    assert!(m.cases.iter().all(|f| f.page.is_some()));
    assert_eq!(m.seed_pages.len(), 33);
    assert_eq!(m.seed_fields.len(), 21);
    // Seeds are raw bytes, not text.
    let utf16 = m
        .seed_pages
        .iter()
        .find(|s| s.name == "utf16le-bom.html")
        .unwrap();
    assert!(utf16.bytes.starts_with(&[0xFF, 0xFE]));
    let truncated = m
        .seed_pages
        .iter()
        .find(|s| s.name == "truncated-utf8.html")
        .unwrap();
    let on_disk =
        fs::read(rivqen_testkit::fixtures_root().join("malformed/seeds/pages/truncated-utf8.html"))
            .unwrap();
    assert_eq!(truncated.bytes, on_disk);
}

#[test]
fn testkit_reports_bad_fixture() {
    let root = scratch("bad-fixture");
    let dir = root.join("FX-RQ-C99-BAD");
    fs::create_dir_all(&dir).unwrap();
    fs::create_dir_all(root.join("pages")).unwrap();
    fs::write(dir.join("case.json"), "{}").unwrap();
    // expected.json is missing.
    let err = load_rqp_protocol_at(&root).unwrap_err();
    assert!(matches!(err, FixtureError::Io { .. }));
    assert!(err.to_string().contains("FX-RQ-C99-BAD"), "{err}");
    assert!(err.to_string().contains("expected.json"), "{err}");

    fs::write(dir.join("expected.json"), "{ not json").unwrap();
    let err = load_rqp_protocol_at(&root).unwrap_err();
    assert!(matches!(err, FixtureError::Json { .. }));
    assert!(err.to_string().contains("expected.json"), "{err}");

    // A named page that does not exist.
    let leg = scratch("bad-page");
    let d = leg.join("FX-LEG-X");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("case.json"), r#"{"page":"nope"}"#).unwrap();
    fs::write(d.join("expected.json"), "{}").unwrap();
    let err = load_legacy_at(&leg).unwrap_err();
    assert!(err.to_string().contains("nope.html"), "{err}");
}

#[test]
fn testkit_reports_shape_error_with_path() {
    let root = scratch("bad-shape");
    let dir = root.join("FX-RQ-C99-SHAPE");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("case.json"), "{}").unwrap();
    fs::write(dir.join("expected.json"), "{}").unwrap();
    let all = load_rqp_protocol_at(&root).unwrap();
    let err = all.client[0].client_result().unwrap_err();
    assert!(matches!(err, FixtureError::Shape { .. }));
    assert!(err.to_string().contains("FX-RQ-C99-SHAPE"), "{err}");
}
