// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The control endpoint of CONTRACT.md §2 over TCP: every answer status, and the
//! case request that reaches the engine of the stored case. The peer check
//! (`403` for a peer that is not loopback) needs another address; its test is a
//! unit test in `src/control.rs`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::format_push_string,
    clippy::similar_names
)]

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

use rivqen_reference::control::{CASE_PATH, DEFAULT_ORIGIN, INFO_PATH, Server, Settings};
use rivqen_reference::http::server::{Timeouts, serve};

fn start() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        let mut server = Server::new(Settings {
            conformance: true,
            lab_peer: false,
            origin: DEFAULT_ORIGIN.to_owned(),
        });
        serve(&listener, &mut server, &Timeouts::default())
    });
    port
}

/// The raw answer bytes of one request.
fn raw_exchange(port: u16, bytes: &[u8]) -> Vec<u8> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    if stream.write_all(bytes).is_ok() {
        stream.shutdown(Shutdown::Write).ok();
    }
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    raw
}

fn status(raw: &[u8]) -> u16 {
    std::str::from_utf8(&raw[9..12]).unwrap().parse().unwrap()
}

fn body(raw: &[u8]) -> &[u8] {
    let end = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    &raw[end + 4..]
}

fn request(
    method: &str,
    target: &str,
    host: &str,
    fields: &[(&str, &str)],
    body: &[u8],
) -> Vec<u8> {
    let mut head = format!("{method} {target} HTTP/1.1\r\nHost: {host}\r\n");
    for (n, v) in fields {
        head.push_str(&format!("{n}: {v}\r\n"));
    }
    if !body.is_empty() {
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    head.push_str("Connection: close\r\n\r\n");
    let mut bytes = head.into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

fn post_case(port: u16, json: &str) -> Vec<u8> {
    raw_exchange(
        port,
        &request(
            "POST",
            CASE_PATH,
            &format!("127.0.0.1:{port}"),
            &[("Content-Type", "application/json")],
            json.as_bytes(),
        ),
    )
}

const LEGACY_CASE: &str = r#"{"suite":"legacy","case_id":"FX-LEG-T","profile":"legacy-php","page_base64":"PHA+aGk8L3A+","app":{"status":200,"headers":[["content-type","text/html; charset=utf-8"],["Set-Cookie","a=1"],["set-cookie","b=2"],["Content-Length","999"]]},"server_config":{}}"#;

#[test]
fn control_info_over_tcp() {
    let port = start();
    let raw = raw_exchange(
        port,
        &request("GET", INFO_PATH, &format!("127.0.0.1:{port}"), &[], b""),
    );
    let json = r#"{"contract":1,"profiles":["legacy-canonical","legacy-java","legacy-node","legacy-php"],"suites":["rqp","legacy"]}"#;
    assert_eq!(
        raw,
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}",
            json.len()
        )
        .as_bytes()
    );
    for host in [format!("localhost:{port}"), format!("[::1]:{port}")] {
        let raw = raw_exchange(port, &request("GET", INFO_PATH, &host, &[], b""));
        assert_eq!(status(&raw), 200, "{host}");
    }
}

#[test]
fn control_case_reaches_engine() {
    let port = start();
    assert_eq!(
        post_case(port, LEGACY_CASE),
        b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n"
    );
    // With the stub engines: the pass-through, field lines exactly as in `app`
    // without the framing fields of the application.
    let raw = raw_exchange(
        port,
        &request(
            "GET",
            "/catalog?x=1",
            "shop.example.com",
            &[("Accept-Diff", "true")],
            b"",
        ),
    );
    assert_eq!(
        raw,
        b"HTTP/1.1 200 OK\r\ncontent-type: text/html; charset=utf-8\r\nSet-Cookie: a=1\r\nset-cookie: b=2\r\nContent-Length: 9\r\nConnection: close\r\n\r\n<p>hi</p>"
    );
    // HEAD: no body.
    let raw = raw_exchange(
        port,
        &request("HEAD", "/catalog", "shop.example.com", &[], b""),
    );
    assert_eq!(status(&raw), 200);
    assert_eq!(body(&raw), b"");
    // The case stays for the next requests.
    let raw = raw_exchange(port, &request("GET", "/", "shop.example.com", &[], b""));
    assert_eq!(body(&raw), b"<p>hi</p>");
}

#[test]
fn control_rqp_case_reaches_engine() {
    let port = start();
    let rqp = r#"{"suite":"rqp","case_id":"FX-RQ-P01","page_base64":"YWJj","app":{"status":404,"headers":[["ETag","\"r1.x\""]]},"server_config":{}}"#;
    assert_eq!(status(&post_case(port, rqp)), 204);
    let raw = raw_exchange(
        port,
        &request("GET", "/catalog", "shop.example.com", &[], b""),
    );
    assert_eq!(
        raw,
        b"HTTP/1.1 404 Not Found\r\nETag: \"r1.x\"\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc"
    );
}

#[test]
fn control_case_request_before_a_case_is_503() {
    let port = start();
    let raw = raw_exchange(
        port,
        &request("GET", "/catalog", "shop.example.com", &[], b""),
    );
    assert_eq!(status(&raw), 503);
    assert_eq!(body(&raw), br#"{"error":"NO_CASE"}"#);
}

#[test]
fn control_bad_case_is_400_and_keeps_the_old_case() {
    let port = start();
    assert_eq!(status(&post_case(port, LEGACY_CASE)), 204);
    for bad in [
        "{",
        r#"{"suite":"other","case_id":"x","page_base64":"","app":{"status":200,"headers":[]}}"#,
        r#"{"suite":"rqp","case_id":"x","page_base64":"","app":{"status":200,"headers":[]},"server_config":"x"}"#,
    ] {
        let raw = post_case(port, bad);
        assert_eq!(status(&raw), 400, "{bad}");
        assert_eq!(body(&raw), br#"{"error":"BAD_CASE"}"#);
    }
    let raw = raw_exchange(port, &request("GET", "/", "shop.example.com", &[], b""));
    assert_eq!(body(&raw), b"<p>hi</p>");
}

#[test]
fn control_unknown_profile_is_422() {
    let port = start();
    let raw = post_case(port, &LEGACY_CASE.replace("legacy-php", "legacy-go"));
    assert_eq!(status(&raw), 422);
    assert_eq!(body(&raw), br#"{"error":"UNSUPPORTED"}"#);
}

#[test]
fn control_case_over_8_mib_is_413() {
    let port = start();
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    let head = format!(
        "POST {CASE_PATH} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: 8388609\r\n\r\n{{"
    );
    stream.write_all(head.as_bytes()).unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    assert_eq!(status(&raw), 413);
    assert_eq!(body(&raw), br#"{"error":"TOO_LARGE"}"#);
}

#[test]
fn control_case_without_json_content_type_is_415() {
    let port = start();
    let raw = raw_exchange(
        port,
        &request(
            "POST",
            CASE_PATH,
            &format!("127.0.0.1:{port}"),
            &[("Content-Type", "text/plain")],
            LEGACY_CASE.as_bytes(),
        ),
    );
    assert_eq!(status(&raw), 415);
    assert_eq!(body(&raw), br#"{"error":"UNSUPPORTED_MEDIA_TYPE"}"#);
}

#[test]
fn control_host_that_is_not_loopback_is_403() {
    let port = start();
    for host in [
        "shop.example.com".to_owned(),
        format!("127.0.0.1:{}", port.wrapping_add(1)),
        "127.0.0.1".to_owned(),
    ] {
        let raw = raw_exchange(port, &request("GET", INFO_PATH, &host, &[], b""));
        assert_eq!(status(&raw), 403, "{host}");
        assert_eq!(body(&raw), br#"{"error":"FORBIDDEN"}"#);
    }
}

#[test]
fn control_unknown_path_is_404() {
    let port = start();
    let raw = raw_exchange(
        port,
        &request(
            "GET",
            "/__rivqen/conformance/cases",
            &format!("127.0.0.1:{port}"),
            &[],
            b"",
        ),
    );
    assert_eq!(status(&raw), 404);
    assert_eq!(body(&raw), br#"{"error":"NOT_FOUND"}"#);
}

#[test]
fn control_wrong_method_is_405() {
    let port = start();
    let host = format!("127.0.0.1:{port}");
    let raw = raw_exchange(port, &request("POST", INFO_PATH, &host, &[], b"{}"));
    assert_eq!(status(&raw), 405);
    assert_eq!(body(&raw), br#"{"error":"METHOD_NOT_ALLOWED"}"#);
    let raw = raw_exchange(port, &request("GET", CASE_PATH, &host, &[], b""));
    assert_eq!(status(&raw), 405);
    let text = String::from_utf8_lossy(&raw).into_owned();
    assert!(text.contains("\r\nAllow: POST\r\n"), "{text}");
}
