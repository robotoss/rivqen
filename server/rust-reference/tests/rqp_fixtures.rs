// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The RQP server cases (`fixtures/rqp/protocol/FX-RQ-P*`), run in-process
//! through `rqp::respond` and compared as the runner compares
//! (`tools/rqp-protocol/check.mjs` `compareServer` and `compareServerShould`,
//! `fixtures/rqp/protocol/README.md`, `docs/engineering/protocol/fixtures.md`
//! §4.1 "Comparison of F" and §5): status; `headers_required` (the lines of the
//! name joined with ", "); `headers_forbidden`; `headers_optional` (at most one
//! line with the value); `Vary` as a set of names (no `Rq-*` member except the
//! `should` ones, no `Accept` on F and on a `304`, every `required` member);
//! the body by kind. A case of level `should` counts all its failures as SHOULD
//! failures; `vary.should` is a SHOULD rule of a MUST case.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::fs;
use std::path::{Path, PathBuf};

use rivqen_reference::model::{AppResponse, Body, Field, Request, Response};
use rivqen_reference::rqp::config::RqpConfig;
use rivqen_reference::rqp::respond;
use serde_json::Value;
use sha2::{Digest, Sha256};

const START: &[u8] = b"<script type=\"application/rivqen-manifest+json\">";
const END: &[u8] = b"</script>";

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rqp/protocol")
}

fn read_json(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn lines(list: &Value) -> Vec<Field> {
    list.as_array()
        .expect("header lines")
        .iter()
        .map(|pair| Field::new(pair[0].as_str().unwrap(), pair[1].as_str().unwrap()))
        .collect()
}

/// One server case.
struct Case {
    id: String,
    level: String,
    page: Vec<u8>,
    expected: Value,
    response: Response,
}

fn run(id: &str) -> Case {
    let case = read_json(&dir().join(id).join("case.json"));
    let expected = read_json(&dir().join(id).join("expected.json"));
    let page_id = case["page"].as_str().unwrap();
    let page = fs::read(dir().join("pages").join(format!("{page_id}.html"))).unwrap();
    let request = Request {
        method: expected["request"]["method"].as_str().unwrap().to_owned(),
        url: expected["request"]["target"].as_str().unwrap().to_owned(),
        fields: lines(&expected["request"]["headers"]),
    };
    let app = AppResponse {
        status: u16::try_from(case["app"]["status"].as_u64().unwrap()).unwrap(),
        fields: lines(&case["app"]["headers"]),
        body: page.clone(),
    };
    let empty = Value::Object(serde_json::Map::new());
    let config = RqpConfig::from_json(expected.get("server_config").unwrap_or(&empty))
        .unwrap_or_else(|e| panic!("{id}: server_config refused: {}", e.reason));
    let response = respond(&config, &request, &app);
    Case {
        id: id.to_owned(),
        level: expected["level"].as_str().unwrap_or("must").to_owned(),
        page,
        expected,
        response,
    }
}

fn combined(fields: &[Field], name: &str) -> Option<String> {
    let values: Vec<&str> = fields
        .iter()
        .filter(|f| f.is(name))
        .map(|f| f.value.as_str())
        .collect();
    (!values.is_empty()).then(|| values.join(", "))
}

fn vary_members(fields: &[Field]) -> Vec<String> {
    fields
        .iter()
        .filter(|f| f.is("vary"))
        .flat_map(|f| f.value.split(','))
        .map(|m| {
            m.trim_matches(|c| c == ' ' || c == '\t')
                .to_ascii_lowercase()
        })
        .filter(|m| !m.is_empty())
        .collect()
}

fn names(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().map(|s| s.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

/// The MUST failures and the SHOULD failures of one response.
fn compare(case: &Case) -> (Vec<String>, Vec<String>) {
    let e = &case.expected;
    let r = &case.response;
    let mut fail = Vec::new();
    if u64::from(r.status) != e["status"].as_u64().unwrap() {
        fail.push(format!("status {}, expected {}", r.status, e["status"]));
    }
    for (name, value) in e["headers_required"].as_object().unwrap() {
        let got = combined(&r.fields, name);
        if got.as_deref() != value.as_str() {
            fail.push(format!("header {name}: {got:?}, expected {value}"));
        }
    }
    for name in names(&e["headers_forbidden"]) {
        if r.fields.iter().any(|f| f.is(&name)) {
            fail.push(format!("header {name}: forbidden"));
        }
    }
    if let Some(optional) = e.get("headers_optional").and_then(Value::as_object) {
        for (name, value) in optional {
            let got: Vec<&Field> = r.fields.iter().filter(|f| f.is(name)).collect();
            if got.len() > 1
                || got
                    .first()
                    .is_some_and(|f| Some(f.value.as_str()) != value.as_str())
            {
                fail.push(format!(
                    "header {name}: {got:?}, expected at most one {value}"
                ));
            }
        }
    }
    // The writer owns the framing (model::Response).
    for name in ["content-length", "transfer-encoding", "connection"] {
        if r.fields.iter().any(|f| f.is(name)) {
            fail.push(format!("header {name}: set by the engine"));
        }
    }
    let kind = e["body"]["kind"].as_str().unwrap();
    let status = e["status"].as_u64().unwrap();
    let members = vary_members(&r.fields);
    let required = names(&e["vary"]["required"]);
    let should = names(&e["vary"]["should"]);
    let full_like = kind == "full" || status == 304 || (kind == "none" && status == 200);
    for m in &members {
        if (m.starts_with("rq-") && !should.contains(m)) || (full_like && m == "accept") {
            fail.push(format!("header vary: lists {m}"));
        }
    }
    for m in &required {
        if !members.contains(m) {
            fail.push(format!("header vary: does not list {m}"));
        }
    }
    let should_fail: Vec<String> = should
        .iter()
        .filter(|m| !members.contains(m))
        .map(|m| format!("header vary: does not list {m}"))
        .collect();
    match (kind, &r.body) {
        ("none", Body::None) => {}
        ("none", Body::Bytes(b)) if b.is_empty() => {}
        ("none", Body::Bytes(b)) => fail.push(format!("body: {} bytes, expected none", b.len())),
        (_, Body::None) => fail.push(format!("body: none, expected {kind}")),
        ("app", Body::Bytes(b)) => {
            if *b != case.page {
                fail.push("body: not the application body".to_owned());
            }
        }
        ("full", Body::Bytes(b)) => compare_full(&e["body"], &case.page, b, &mut fail),
        ("delta", Body::Bytes(b)) => compare_delta(&e["body"], b, &mut fail),
        (other, _) => fail.push(format!("unknown body kind {other}")),
    }
    if case.level == "should" {
        let mut all = fail;
        all.extend(should_fail);
        (Vec::new(), all)
    } else {
        (fail, should_fail)
    }
}

fn compare_full(expected: &Value, page: &[u8], body: &[u8], fail: &mut Vec<String>) {
    let at = usize::try_from(expected["insert_at"].as_u64().unwrap()).unwrap();
    if body.get(..at) != page.get(..at) {
        fail.push("body: the bytes before the manifest element differ".to_owned());
        return;
    }
    if body.get(at..at + START.len()) != Some(START) {
        fail.push("body: no manifest start tag at the insertion point".to_owned());
        return;
    }
    let text_start = at + START.len();
    let Some(lt) = body[text_start..]
        .iter()
        .position(|&b| b == b'<')
        .map(|p| p + text_start)
    else {
        fail.push("body: no </script>".to_owned());
        return;
    };
    if body.get(lt..lt + END.len()) != Some(END) {
        fail.push("body: the manifest text has \"<\"".to_owned());
        return;
    }
    if body.get(lt + END.len()..) != page.get(at..) {
        fail.push("body: the bytes after the manifest element differ".to_owned());
    }
    match strict_json(&body[text_start..lt]) {
        Err(why) => fail.push(format!(
            "body: the manifest is not a strict JSON text ({why})"
        )),
        Ok(value) => {
            if value != expected["manifest"] {
                fail.push(format!("body: the manifest value differs: {value}"));
            }
        }
    }
}

fn compare_delta(expected: &Value, body: &[u8], fail: &mut Vec<String>) {
    let env = match strict_json(body) {
        Ok(v) => v,
        Err(why) => {
            fail.push(format!("body: the delta is not a strict JSON text ({why})"));
            return;
        }
    };
    // The patch schema: exactly binding and operations; each operation exactly
    // block_id and content, 1 to 256 operations.
    let obj = env.as_object().unwrap();
    let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
    keys.sort_unstable();
    if keys != ["binding", "operations"] {
        fail.push(format!("body: envelope members {keys:?}"));
        return;
    }
    if env["binding"] != expected["binding"] {
        fail.push(format!(
            "body: binding {}, expected {} ({})",
            env["binding"], expected["binding"], expected["binding_inputs"]
        ));
    }
    let ops = env["operations"].as_array().unwrap();
    if ops.is_empty() || ops.len() > 256 {
        fail.push(format!("body: {} operations", ops.len()));
    }
    let want: Vec<&Value> = expected["blocks"].as_array().unwrap().iter().collect();
    let got: Vec<&str> = ops
        .iter()
        .map(|op| op["block_id"].as_str().unwrap_or(""))
        .collect();
    let want_ids: Vec<&str> = want.iter().map(|b| b["id"].as_str().unwrap()).collect();
    if got != want_ids {
        fail.push(format!(
            "body: changed blocks {got:?}, expected {want_ids:?}"
        ));
        return;
    }
    for (op, block) in ops.iter().zip(want) {
        let mut op_keys: Vec<&str> = op.as_object().unwrap().keys().map(String::as_str).collect();
        op_keys.sort_unstable();
        if op_keys != ["block_id", "content"] {
            fail.push(format!("body: operation members {op_keys:?}"));
        }
        let content = op["content"].as_str().unwrap_or("");
        let hash = base64_url(&Sha256::digest(content.as_bytes()));
        if hash != block["sha256"].as_str().unwrap() {
            fail.push(format!(
                "body: block {} content is not the current content",
                block["id"]
            ));
        }
    }
}

fn base64_url(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// JSON with the extra rules of manifest and patch §3.2 that a value parser
/// does not check: UTF-8 without a byte order mark and no duplicate member
/// names (checked with a second walk over the text).
fn strict_json(bytes: &[u8]) -> Result<Value, String> {
    if bytes.starts_with(b"\xEF\xBB\xBF") {
        return Err("byte order mark".to_owned());
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let mut pos = 0;
    no_duplicates(bytes, &mut pos)?;
    Ok(value)
}

fn skip_ws(b: &[u8], pos: &mut usize) {
    while *pos < b.len() && matches!(b[*pos], b' ' | b'\t' | b'\n' | b'\r') {
        *pos += 1;
    }
}

/// The raw text of a JSON string at `pos` (a valid text: `serde_json` parsed it).
fn raw_string(b: &[u8], pos: &mut usize) -> Vec<u8> {
    let start = *pos;
    *pos += 1;
    while b[*pos] != b'"' {
        *pos += if b[*pos] == b'\\' { 2 } else { 1 };
    }
    *pos += 1;
    b[start..*pos].to_vec()
}

/// Walk one JSON value of a text that `serde_json` accepted; fail on a
/// duplicate member name (compared by decoded value).
fn no_duplicates(b: &[u8], pos: &mut usize) -> Result<(), String> {
    skip_ws(b, pos);
    match b[*pos] {
        b'{' => {
            *pos += 1;
            let mut seen = std::collections::BTreeSet::new();
            loop {
                skip_ws(b, pos);
                if b[*pos] == b'}' {
                    *pos += 1;
                    return Ok(());
                }
                let key: String = serde_json::from_slice(&raw_string(b, pos)).unwrap();
                if !seen.insert(key) {
                    return Err("duplicate member name".to_owned());
                }
                skip_ws(b, pos);
                *pos += 1; // ':'
                no_duplicates(b, pos)?;
                skip_ws(b, pos);
                if b[*pos] == b',' {
                    *pos += 1;
                }
            }
        }
        b'[' => {
            *pos += 1;
            loop {
                skip_ws(b, pos);
                if b[*pos] == b']' {
                    *pos += 1;
                    return Ok(());
                }
                no_duplicates(b, pos)?;
                skip_ws(b, pos);
                if b[*pos] == b',' {
                    *pos += 1;
                }
            }
        }
        b'"' => {
            raw_string(b, pos);
            Ok(())
        }
        _ => {
            while *pos < b.len()
                && !matches!(b[*pos], b',' | b']' | b'}' | b' ' | b'\t' | b'\n' | b'\r')
            {
                *pos += 1;
            }
            Ok(())
        }
    }
}

#[test]
fn strict_json_finds_duplicate_members() {
    assert!(strict_json(br#"{"a":1,"b":{"c":[1,{"d":"x\"y"}]}}"#).is_ok());
    assert!(strict_json(br#"{"a":1,"a":2}"#).is_err());
    assert!(strict_json(br#"{"a":[{"b":1, "b" :2}]}"#).is_err());
    assert!(strict_json(br#"{"\u0061":1,"a":2}"#).is_err());
    assert!(strict_json(b"\xEF\xBB\xBF{}").is_err());
}

fn server_cases() -> Vec<String> {
    let mut ids: Vec<String> = fs::read_dir(dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.starts_with("FX-RQ-P"))
        .collect();
    ids.sort();
    ids
}

fn report(
    cases: &[Case],
    pick: impl Fn(&(Vec<String>, Vec<String>)) -> &Vec<String>,
) -> Vec<String> {
    let mut out = Vec::new();
    for case in cases {
        for f in pick(&compare(case)) {
            out.push(format!("{}: {f}", case.id));
        }
    }
    out
}

#[test]
fn rqp_fixtures_must() {
    let ids = server_cases();
    assert_eq!(ids.len(), 112, "the RQP suite has 112 server cases");
    let cases: Vec<Case> = ids.iter().map(|id| run(id)).collect();
    let failures = report(&cases, |(must, _)| must);
    assert!(
        failures.is_empty(),
        "{} MUST failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn rqp_fixtures_should() {
    let cases: Vec<Case> = server_cases().iter().map(|id| run(id)).collect();
    assert!(cases.iter().filter(|c| c.level == "should").count() >= 9);
    let failures = report(&cases, |(_, should)| should);
    assert!(
        failures.is_empty(),
        "{} SHOULD failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The comparison is not vacuous: the application response unchanged (the
/// stub of the contract) fails every case that expects F, D or a `304`.
#[test]
fn rqp_fixtures_compare_rejects_the_pass_through() {
    let mut failed = 0;
    let mut cases = 0;
    for id in server_cases() {
        let mut case = run(&id);
        if case.expected["body"]["kind"] == "app" {
            continue;
        }
        cases += 1;
        case.response = Response {
            status: 200,
            fields: vec![
                Field::new("content-type", "text/html; charset=utf-8"),
                Field::new("cache-control", "no-cache"),
            ],
            body: Body::Bytes(case.page.clone()),
        };
        let (must, should) = compare(&case);
        if !must.is_empty() || !should.is_empty() {
            failed += 1;
        }
    }
    assert_eq!(failed, cases);
    assert!(cases >= 96, "{cases}");
}
