// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The legacy golden suite (`fixtures/legacy`) for each profile and the
//! malformed suite (`fixtures/malformed/legacy`), run in-process through
//! `legacy::respond` and compared as the runner compares
//! (`server/conformance/src/compare.mjs`, `fixtures/legacy/README.md`):
//! status; for each listed field name, its values in order (names ASCII
//! case-insensitive, `[]` = absent); the body by kind; Content-Length.
//! Also the split of each page (`split` of expected.json).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::fs;
use std::path::{Path, PathBuf};

use rivqen_reference::legacy::{self, sha1_hex};
use rivqen_reference::model::{AppResponse, Body, Field, Profile, Request, Response};
use serde_json::{Value, json};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn legacy_dir() -> PathBuf {
    root().join("fixtures/legacy")
}

fn malformed_dir() -> PathBuf {
    root().join("fixtures/malformed/legacy")
}

fn read_json(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn suffix(profile: Profile) -> Option<&'static str> {
    match profile {
        Profile::Canonical => None,
        Profile::Java => Some("JAVA"),
        Profile::Node => Some("NODE"),
        Profile::Php => Some("PHP"),
    }
}

/// One loaded fixture.
struct Case {
    id: String,
    profile: Profile,
    request: Request,
    app: AppResponse,
    expected: Value,
}

fn fields_of(list: &Value) -> Vec<Field> {
    list.as_array()
        .expect("headers")
        .iter()
        .map(|pair| Field::new(pair[0].as_str().unwrap(), pair[1].as_str().unwrap()))
        .collect()
}

fn load(dir: &Path, id: &str, profile: Profile) -> Case {
    let expected = read_json(&dir.join(id).join("expected.json"));
    let own = read_json(&dir.join(id).join("case.json"));
    // A profile fixture takes page and app of the fixture that it replaces.
    let base = match own.get("replaces").and_then(Value::as_str) {
        Some(base) => {
            assert_eq!(Some(base), id.split("--").next(), "{id}: replaces");
            read_json(&dir.join(base).join("case.json"))
        }
        None => own,
    };
    let page = base["page"].as_str().expect("page");
    let body = fs::read(dir.join("pages").join(format!("{page}.html"))).expect("page bytes");
    let default_app =
        json!({ "status": 200, "headers": [["content-type", "text/html; charset=utf-8"]] });
    let app = base.get("app").unwrap_or(&default_app);
    let req = &expected["request"];
    Case {
        id: id.to_owned(),
        profile,
        request: Request {
            method: req["method"].as_str().unwrap().to_owned(),
            url: req["target"].as_str().unwrap().to_owned(),
            fields: fields_of(&req["headers"]),
        },
        app: AppResponse {
            status: u16::try_from(app["status"].as_u64().unwrap()).unwrap(),
            fields: fields_of(&app["headers"]),
            body,
        },
        expected,
    }
}

fn ids(dir: &Path) -> Vec<String> {
    let mut ids: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.starts_with("FX-LEG-"))
        .filter(|n| !is_client(dir, n))
        .collect();
    ids.sort();
    ids
}

/// A client fixture (`kind: "client"`, fixtures/legacy/README.md "Client fixtures")
/// checks a client session, not a server answer: the server tests skip it.
fn is_client(dir: &Path, id: &str) -> bool {
    let path = dir.join(id).join("case.json");
    path.is_file() && read_json(&path)["kind"].as_str() == Some("client")
}

/// The selection rule of `fixtures/legacy/README.md`: canonical runs every
/// canonical fixture; a profile runs its own fixtures and the canonical
/// fixtures that list it in `profiles_same` and that it does not replace.
fn selected(dir: &Path, profile: Profile) -> Vec<Case> {
    let all = ids(dir);
    let own: Vec<&String> = match suffix(profile) {
        Some(s) => all
            .iter()
            .filter(|id| id.ends_with(&format!("--{s}")))
            .collect(),
        None => Vec::new(),
    };
    let replaced: Vec<&str> = own.iter().filter_map(|id| id.split("--").next()).collect();
    let mut out = Vec::new();
    for id in all.iter().filter(|id| !id.contains("--")) {
        if profile == Profile::Canonical {
            out.push(load(dir, id, profile));
            continue;
        }
        if replaced.contains(&id.as_str()) {
            continue;
        }
        let expected = read_json(&dir.join(id).join("expected.json"));
        let same = expected["profiles_same"]
            .as_array()
            .is_some_and(|a| a.iter().any(|p| p.as_str() == Some(profile.name())));
        if same {
            out.push(load(dir, id, profile));
        }
    }
    out.extend(own.iter().map(|id| load(dir, id, profile)));
    out
}

fn values<'a>(fields: &'a [Field], name: &str) -> Vec<&'a str> {
    fields
        .iter()
        .filter(|f| f.is(name))
        .map(|f| f.value.as_str())
        .collect()
}

/// The problems of `got` against the expected response (empty = pass).
fn compare(case: &Case, got: &Response) -> Vec<String> {
    let e = &case.expected;
    let mut problems = Vec::new();
    if u64::from(got.status) != e["status"].as_u64().unwrap() {
        problems.push(format!("status {}, expected {}", got.status, e["status"]));
    }
    for (name, want) in e["fields"].as_object().unwrap() {
        let want: Vec<&str> = want
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let have = values(&got.fields, name);
        if have != want {
            problems.push(format!("field {name}: expected {want:?}, got {have:?}"));
        }
    }
    // The writer owns the framing (model.rs): it adds Content-Length from the body.
    for framing in ["content-length", "transfer-encoding", "connection"] {
        if !values(&got.fields, framing).is_empty() {
            problems.push(format!("field {framing} must not be set by the engine"));
        }
    }
    if !values(&got.fields, "content-encoding").is_empty() {
        problems.push("content-encoding: the engine sends identity (C-14)".into());
    }
    // The writer never sends a body for HEAD.
    let body: Option<&[u8]> = match &got.body {
        Body::None => None,
        Body::Bytes(_) if case.request.method == "HEAD" => None,
        Body::Bytes(b) => Some(b),
    };
    match e["body"]["kind"].as_str().unwrap() {
        "none" => {
            if body.is_some_and(|b| !b.is_empty()) {
                problems.push("body: expected none".into());
            }
        }
        "page" => {
            if body != Some(case.app.body.as_slice()) {
                problems.push("body: not the application body".into());
            }
        }
        "data" => match body.map(serde_json::from_slice::<Value>) {
            Some(Ok(v)) => {
                let want = json!({
                    "data": e["body"]["data"],
                    "template-tag": e["body"]["template_tag"],
                    "html-sha1": e["body"]["html_sha1"],
                    "diff": e["body"]["diff"],
                });
                if v != want {
                    problems.push(format!("body: data {v} expected {want}"));
                }
            }
            Some(Err(err)) => problems.push(format!("body: not JSON ({err})")),
            None => problems.push("body: none, expected data".into()),
        },
        other => problems.push(format!("unknown body kind {other}")),
    }
    problems
}

fn run(cases: &[Case]) {
    let mut failures = Vec::new();
    for case in cases {
        let got = legacy::respond(case.profile, &case.request, &case.app);
        let problems = compare(case, &got);
        if !problems.is_empty() {
            failures.push(format!(
                "{} ({}): {}",
                case.id,
                case.profile.name(),
                problems.join("; ")
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn legacy_fixtures_canonical() {
    let cases = selected(&legacy_dir(), Profile::Canonical);
    assert_eq!(cases.len(), 59);
    run(&cases);
}

#[test]
fn legacy_fixtures_java() {
    let cases = selected(&legacy_dir(), Profile::Java);
    assert_eq!(cases.len(), 39);
    run(&cases);
}

#[test]
fn legacy_fixtures_node() {
    let cases = selected(&legacy_dir(), Profile::Node);
    assert_eq!(cases.len(), 39);
    run(&cases);
}

#[test]
fn legacy_fixtures_php() {
    let cases = selected(&legacy_dir(), Profile::Php);
    assert_eq!(cases.len(), 39);
    run(&cases);
}

#[test]
fn legacy_malformed() {
    let cases = selected(&malformed_dir(), Profile::Canonical);
    assert_eq!(cases.len(), 22);
    run(&cases);
}

/// `split` of expected.json: `template_sha1` and the keys in order when the
/// split runs; no split (`applied: false`) otherwise. Every fixture of every
/// profile, and the malformed cases.
#[test]
fn legacy_split_matches_expected() {
    let mut cases = Vec::new();
    for profile in Profile::ALL {
        cases.extend(selected(&legacy_dir(), profile));
    }
    cases.extend(selected(&malformed_dir(), Profile::Canonical));
    let mut failures = Vec::new();
    let mut applied = 0;
    for case in &cases {
        let want = &case.expected["split"];
        let got = legacy::split_of(case.profile, &case.request, &case.app);
        let ok = match (want["applied"].as_bool().unwrap(), &got) {
            (false, None) => true,
            (true, Some(s)) => {
                applied += 1;
                let keys: Vec<&str> = s.data.iter().map(|(k, _)| k.as_str()).collect();
                let want_keys: Vec<&str> = want["keys"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|k| k.as_str().unwrap())
                    .collect();
                sha1_hex(s.template.as_bytes()) == want["template_sha1"].as_str().unwrap()
                    && keys == want_keys
            }
            _ => false,
        };
        if !ok {
            failures.push(format!("{} ({})", case.id, case.profile.name()));
        }
    }
    assert!(failures.is_empty(), "split differs: {failures:?}");
    assert!(applied > 100, "only {applied} applied splits");
}
