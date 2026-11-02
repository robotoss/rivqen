// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The limits of a case body (WP-03 S2 §2 "Hardening" item 3; DL-097;
//! CONTRACT.md §2): the pre-scan refuses a body before the JSON parse builds a
//! value. The note of the refusal shows which check refused it: a pre-scan note
//! cannot come from a check on a parsed value.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Shutdown, TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use proptest::prelude::*;
use rivqen_reference::control::{
    CASE_MEMBERS, CASE_PATH, CaseError, Conn, DEFAULT_ORIGIN, MAX_APP_HEADERS, MAX_CASE_DEPTH,
    MAX_CASE_TOKENS, Server, Settings, parse_case, prescan_case,
};
use rivqen_reference::http::reader::{RequestHead, Version};
use rivqen_reference::http::server::{ServeOptions, serve};
use rivqen_reference::model::Field;
use serde_json::{Value, json};

const TOKENS: CaseError = CaseError::Bad("the case has more than 65536 structural tokens");
const DEPTH: CaseError = CaseError::Bad("the case is nested deeper than 16");
const HEADERS: CaseError = CaseError::Bad("app.headers has more than 1024 lines");
const MEMBER: CaseError = CaseError::Bad("the case has a member that the contract does not name");

/// The structural tokens of the compact text of `v`.
fn tokens_of(v: &Value) -> usize {
    match v {
        Value::Object(m) => {
            2 + m.len() + m.len().saturating_sub(1) + m.values().map(tokens_of).sum::<usize>()
        }
        Value::Array(a) => 2 + a.len().saturating_sub(1) + a.iter().map(tokens_of).sum::<usize>(),
        _ => 0,
    }
}

fn depth_of(v: &Value) -> usize {
    match v {
        Value::Object(m) => 1 + m.values().map(depth_of).max().unwrap_or(0),
        Value::Array(a) => 1 + a.iter().map(depth_of).max().unwrap_or(0),
        _ => 0,
    }
}

fn header_lines_of(v: &Value) -> usize {
    v.get("app")
        .filter(|a| a.is_object())
        .and_then(|a| a.get("headers"))
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

fn names_ok(v: &Value) -> bool {
    v.as_object()
        .is_none_or(|m| m.keys().all(|k| CASE_MEMBERS.contains(&k.as_str())))
}

/// The verdict of the limits of DL-097 on a parsed value.
fn within_limits(v: &Value) -> bool {
    tokens_of(v) <= MAX_CASE_TOKENS
        && depth_of(v) <= MAX_CASE_DEPTH
        && header_lines_of(v) <= MAX_APP_HEADERS
        && names_ok(v)
}

fn rqp_case() -> Value {
    json!({
        "suite": "rqp",
        "case_id": "FX-RQ-CAPS",
        "page_base64": "PHA+aGk8L3A+",
        "app": { "status": 200, "headers": [["Content-Type", "text/html"]] },
        "server_config": {}
    })
}

fn legacy_case() -> Value {
    json!({
        "suite": "legacy",
        "case_id": "FX-LEG-CAPS",
        "profile": "legacy-canonical",
        "page_base64": "PHA+aGk8L3A+",
        "app": { "status": 200, "headers": [["Content-Type", "text/html"]] }
    })
}

fn bad(body: &str) -> CaseError {
    parse_case(body.as_bytes()).map(|_| ()).unwrap_err()
}

/// `name` as a JSON string with its first character as a `\uXXXX` escape.
fn escaped_name(name: &str) -> String {
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    let backslash = char::from(0x5c);
    format!("\"{backslash}u{:04x}{}\"", u32::from(first), chars.as_str())
}

#[test]
fn control_case_caps_escaped_name_helper() {
    let e = escaped_name("suite");
    assert_eq!(e.len(), "\"suite\"".len() + 5);
    assert_eq!(e.as_bytes()[1], 0x5c);
    assert_eq!(&e[2..7], "u0073");
    assert_eq!(serde_json::from_str::<String>(&e).unwrap(), "suite");
}

fn with_headers(lines: usize) -> String {
    let mut v = legacy_case();
    v["app"]["headers"] = Value::Array((0..lines).map(|i| json!(["X-N", i.to_string()])).collect());
    v.to_string()
}

#[test]
fn control_case_caps_tokens() {
    let body = |n: usize| {
        let mut v = rqp_case();
        v["server_config"] = json!({ "x": vec![0; n] });
        v
    };
    // tokens(n) = tokens(1) + (n - 1)
    let n = MAX_CASE_TOKENS - tokens_of(&body(1)) + 1;
    let at = body(n);
    assert_eq!(tokens_of(&at), MAX_CASE_TOKENS);
    let text = at.to_string();
    assert_eq!(prescan_case(text.as_bytes()), Ok(()));
    // The value is built and the configuration refuses the member x: another note.
    let note = bad(&text);
    assert_ne!(note, TOKENS);
    let over = body(n + 1).to_string();
    assert_eq!(prescan_case(over.as_bytes()), Err(TOKENS));
    assert_eq!(bad(&over), TOKENS);
    // Pretty printing adds white space, no token.
    let pretty = serde_json::to_string_pretty(&body(n + 1)).unwrap();
    assert_eq!(bad(&pretty), TOKENS);
}

#[test]
fn control_case_caps_tokens_inside_strings_do_not_count() {
    let mut v = legacy_case();
    v["case_id"] = Value::String("{[,:]}\"\\".repeat(20_000));
    let text = v.to_string();
    let structural = text.matches(['{', '}', '[', ']', ',', ':']).count();
    assert!(structural > MAX_CASE_TOKENS, "{structural}");
    assert!(parse_case(text.as_bytes()).is_ok());
}

#[test]
fn control_case_caps_depth() {
    let nested = |levels: usize| {
        let mut inner = json!(1);
        for _ in 0..levels {
            inner = json!({ "a": inner });
        }
        let mut v = rqp_case();
        v["server_config"] = inner;
        v
    };
    // Root (1) + server_config and its 15 inner objects = 16.
    let at = nested(15);
    assert_eq!(depth_of(&at), MAX_CASE_DEPTH);
    assert_eq!(prescan_case(at.to_string().as_bytes()), Ok(()));
    assert_ne!(bad(&at.to_string()), DEPTH);
    let over = nested(16);
    assert_eq!(depth_of(&over), MAX_CASE_DEPTH + 1);
    assert_eq!(bad(&over.to_string()), DEPTH);
    // Arrays count as well, and the scan stops at the 17th opening bracket.
    let deep = "[".repeat(MAX_CASE_DEPTH + 1);
    assert_eq!(prescan_case(deep.as_bytes()), Err(DEPTH));
    assert_eq!(prescan_case("[".repeat(MAX_CASE_DEPTH).as_bytes()), Ok(()));
}

#[test]
fn control_case_caps_app_headers() {
    let at = with_headers(MAX_APP_HEADERS);
    let case = parse_case(at.as_bytes()).unwrap();
    assert_eq!(case.app.fields.len(), MAX_APP_HEADERS);
    assert_eq!(bad(&with_headers(MAX_APP_HEADERS + 1)), HEADERS);
    // Only app.headers has the limit: an array of 2000 elsewhere is not lines.
    let mut v = rqp_case();
    v["server_config"] = json!({ "headers": vec![0; 2000] });
    assert_ne!(bad(&v.to_string()), HEADERS);
    let mut v = legacy_case();
    v["case_id"] = json!("headers");
    v["app"]["other"] = json!({ "headers": vec![0; 2000] });
    assert!(parse_case(v.to_string().as_bytes()).is_ok());
}

#[test]
fn control_case_caps_unknown_member() {
    let mut v = legacy_case();
    v["extra"] = json!(1);
    assert_eq!(bad(&v.to_string()), MEMBER);
    // Every named member is allowed (profile also for rqp, as in S1).
    let mut v = rqp_case();
    v["profile"] = json!("legacy-java");
    assert!(parse_case(v.to_string().as_bytes()).is_ok());
    // A member of app or of a nested object is not a top-level member.
    let mut v = legacy_case();
    v["app"]["extra"] = json!({ "extra": 1 });
    assert!(parse_case(v.to_string().as_bytes()).is_ok());
    // The member name is compared after its escapes.
    let escaped = legacy_case()
        .to_string()
        .replace("\"suite\"", &escaped_name("suite"));
    assert!(escaped.contains(&escaped_name("suite")));
    assert!(parse_case(escaped.as_bytes()).is_ok());
    let escaped = legacy_case()
        .to_string()
        .replace("\"suite\"", &escaped_name("suitex"));
    assert!(escaped.contains(&escaped_name("suitex")));
    assert_eq!(bad(&escaped), MEMBER);
    // Every member name with an escape is still a member name.
    for name in CASE_MEMBERS {
        let body = format!("{{{}:1}}", escaped_name(name));
        assert_eq!(prescan_case(body.as_bytes()), Ok(()), "{name}");
    }
    // A long name with escapes is no member name, also when it decodes to one:
    // 13 escapes of 6 bytes, the longest name with every byte escaped, still fit.
    let all_escaped: String = "server_config"
        .chars()
        .map(|c| escaped_name(&c.to_string()).trim_matches('"').to_owned())
        .collect();
    let body = format!("{{\"{all_escaped}\":1}}");
    assert_eq!(prescan_case(body.as_bytes()), Ok(()));
    let long = format!("{{{}:1}}", escaped_name(&"s".repeat(100)));
    assert_eq!(prescan_case(long.as_bytes()), Err(MEMBER));
    // An invalid escape in a member name is not JSON.
    assert_eq!(
        prescan_case(br#"{"\uZZZZ":1}"#),
        Err(CaseError::Bad("the body is not JSON"))
    );
    // app with escapes keeps the header limit.
    let headers = with_headers(MAX_APP_HEADERS + 1).replace("\"app\"", &escaped_name("app"));
    assert!(headers.contains(&escaped_name("app")));
    assert_eq!(bad(&headers), HEADERS);
    let headers =
        with_headers(MAX_APP_HEADERS + 1).replace("\"headers\"", &escaped_name("headers"));
    assert!(headers.contains(&escaped_name("headers")));
    assert_eq!(bad(&headers), HEADERS);
}

#[test]
fn control_case_caps_invalid_json_is_left_to_the_parse() {
    for body in [
        &b""[..],
        b"not json",
        b"}",
        b"]]",
        b"{\"suite\"",
        b"{\"suite\":\"rqp",
        b"{\"suite\":\"rqp\\",
        b"\"",
    ] {
        let r = prescan_case(body);
        assert!(
            r.is_ok() || r == Err(CaseError::Bad("the body is not JSON")),
            "{body:?}: {r:?}"
        );
        assert_eq!(
            parse_case(body).map(|_| ()),
            Err(CaseError::Bad("the body is not JSON")),
            "{body:?}"
        );
    }
}

#[test]
fn control_case_caps_the_scan_is_linear_on_8_mib() {
    let size = 8 * 1024 * 1024;
    let inputs: Vec<Vec<u8>> = vec![
        // One long string of escapes.
        [b"{\"case_id\":\"".to_vec(), b"\\\"".repeat(size / 2)].concat(),
        // Bytes that are no token.
        vec![b'0'; size],
        // Many short member names at the top level (stops at the token limit).
        [b"{".to_vec(), b"\"app\":1,".repeat(size / 8)].concat(),
        // Many escaped member names.
        [
            b"{".to_vec(),
            format!("{}:1,", escaped_name("app"))
                .into_bytes()
                .repeat(size / 13),
        ]
        .concat(),
        // The deepest nesting, then commas.
        [b"[".repeat(16), b",".repeat(size)].concat(),
        // Unterminated strings.
        b"\"\\".repeat(size / 2),
    ];
    for input in inputs {
        let started = Instant::now();
        let r = prescan_case(&input);
        let took = started.elapsed();
        // A quadratic scan of 8 MiB takes hours; a linear one takes milliseconds.
        assert!(took < Duration::from_secs(5), "{took:?} {r:?}");
    }
}

#[test]
fn control_case_caps_answer_400_bad_case_over_tcp() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        let server = Server::new(Settings {
            conformance: true,
            lab_peer: false,
            origin: DEFAULT_ORIGIN.to_owned(),
        });
        serve(&listener, &server, &ServeOptions::default())
    });
    let send = |json: &str| {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
        let head = format!(
            "POST {CASE_PATH} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
            json.len()
        );
        s.write_all(head.as_bytes()).unwrap();
        s.write_all(json.as_bytes()).unwrap();
        s.shutdown(Shutdown::Write).ok();
        let mut raw = Vec::new();
        s.read_to_end(&mut raw).unwrap();
        raw
    };
    for body in [
        with_headers(MAX_APP_HEADERS + 1),
        "[".repeat(17),
        r#"{"suite":"rqp","x":1}"#.to_owned(),
    ] {
        let raw = send(&body);
        assert!(raw.starts_with(b"HTTP/1.1 400 "), "{body:.40}");
        assert!(raw.ends_with(br#"{"error":"BAD_CASE"}"#));
    }
    assert!(send(&with_headers(MAX_APP_HEADERS)).starts_with(b"HTTP/1.1 204 "));
}

#[test]
fn control_case_caps_keep_the_old_case() {
    let server = Server::new(Settings {
        conformance: true,
        lab_peer: false,
        origin: DEFAULT_ORIGIN.to_owned(),
    });
    let conn = Conn {
        peer: IpAddr::V4(Ipv4Addr::LOCALHOST),
        local_port: 18080,
    };
    let post = |body: &str| {
        let head = RequestHead {
            method: "POST".to_owned(),
            target: CASE_PATH.to_owned(),
            version: Version::Http11,
            fields: vec![
                Field::new("Host", "127.0.0.1:18080"),
                Field::new("Content-Type", "application/json"),
            ],
            content_length: body.len() as u64,
        };
        server.handle(&conn, &head, body.as_bytes())
    };
    assert_eq!(post(&legacy_case().to_string()).response.status, 204);
    let o = post(&with_headers(MAX_APP_HEADERS + 1));
    assert_eq!(o.response.status, 400);
    assert_eq!(o.note, Some("app.headers has more than 1024 lines"));
    assert_eq!(server.case().unwrap().case_id, "FX-LEG-CAPS");
}

/// JSON values with member names from the contract and others, nested up to 20
/// levels.
fn json_value() -> impl Strategy<Value = Value> {
    let name = prop_oneof![
        prop::sample::select(CASE_MEMBERS.to_vec()).prop_map(str::to_owned),
        Just("headers".to_owned()),
        "[a-z{}\\[\\],:\"\\\\]{0,6}",
    ];
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i32>().prop_map(|n| json!(n)),
        "[ -~]{0,8}".prop_map(Value::String),
    ];
    leaf.prop_recursive(20, 96, 6, move |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..6).prop_map(Value::Array),
            prop::collection::vec((name.clone(), inner), 0..6)
                .prop_map(|pairs| Value::Object(pairs.into_iter().collect())),
        ]
    })
}

/// A value inside a chain of 0 to 24 arrays and objects: depths around the limit.
fn deep_value() -> impl Strategy<Value = Value> {
    let wrapper = (
        any::<bool>(),
        prop::sample::select(vec!["app", "headers", "a"]),
    );
    (json_value(), prop::collection::vec(wrapper, 0..24)).prop_map(|(v, chain)| {
        chain.into_iter().fold(v, |inner, (array, name)| {
            if array {
                json!([1, inner])
            } else {
                json!({ name: inner })
            }
        })
    })
}

/// A case-like value: a top-level object, sometimes with an app.headers list.
fn case_value() -> impl Strategy<Value = Value> {
    (
        prop_oneof![json_value(), deep_value()],
        0..4usize,
        any::<bool>(),
    )
        .prop_map(|(v, lines, app)| {
            let mut root = match v {
                Value::Object(m) => Value::Object(m),
                other => json!({ "server_config": other }),
            };
            if app {
                root["app"] = json!({ "headers": vec![json!(["a", "b"]); lines] });
            }
            root
        })
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 2000,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    /// The pre-scan never panics on any bytes.
    #[test]
    fn control_case_prescan_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..2048)) {
        let r = prescan_case(&bytes);
        prop_assert!(matches!(r, Ok(()) | Err(CaseError::Bad(_))));
    }

    /// Nor on JSON text with one byte changed, the inputs closest to a case.
    #[test]
    fn control_case_prescan_never_panics_on_damaged_json(
        v in case_value(), at in any::<prop::sample::Index>(), byte in any::<u8>()
    ) {
        let mut text = v.to_string().into_bytes();
        let i = at.index(text.len());
        text[i] = byte;
        let r = prescan_case(&text);
        prop_assert!(matches!(r, Ok(()) | Err(CaseError::Bad(_))));
    }

    /// On valid JSON the pre-scan agrees with the limits measured on the value,
    /// compact and pretty.
    #[test]
    fn control_case_prescan_agrees_with_the_value(v in prop_oneof![case_value(), json_value(), deep_value()]) {
        let expected = within_limits(&v);
        let compact = v.to_string();
        prop_assert_eq!(prescan_case(compact.as_bytes()).is_ok(), expected, "{}", compact);
        let pretty = serde_json::to_string_pretty(&v).unwrap();
        prop_assert_eq!(prescan_case(pretty.as_bytes()).is_ok(), expected, "{}", pretty);
    }
}
