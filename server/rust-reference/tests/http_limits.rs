// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The limits of the HTTP layer over TCP (CONTRACT.md §3): the server loop on
//! `127.0.0.1:0` in a thread, a raw client.

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
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use rivqen_reference::control::{DEFAULT_ORIGIN, INFO_PATH, Server, Settings};
use rivqen_reference::http::server::{Timeouts, serve};

fn start(timeouts: Timeouts) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        let mut server = Server::new(Settings {
            conformance: true,
            lab_peer: false,
            origin: DEFAULT_ORIGIN.to_owned(),
        });
        serve(&listener, &mut server, &timeouts)
    });
    port
}

fn start_default() -> u16 {
    start(Timeouts::default())
}

#[derive(Debug)]
struct Reply {
    status: u16,
    fields: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Reply {
    fn field(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// Send `bytes`, close the write side, read the whole answer.
fn exchange(port: u16, bytes: &[u8]) -> Reply {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    // The server can answer before it reads everything (431, 413); a failed
    // write is then expected.
    if stream.write_all(bytes).is_ok() {
        stream.shutdown(Shutdown::Write).ok();
    }
    read_reply(&mut stream)
}

fn read_reply(stream: &mut TcpStream) -> Reply {
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    parse_reply(&raw)
}

fn parse_reply(raw: &[u8]) -> Reply {
    let end = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .unwrap_or_else(|| panic!("no head in {:?}", String::from_utf8_lossy(raw)));
    let head = std::str::from_utf8(&raw[..end]).unwrap();
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap();
    assert!(status_line.starts_with("HTTP/1.1 "), "{status_line}");
    let status = status_line[9..12].parse().unwrap();
    let fields = lines
        .map(|l| {
            let (n, v) = l.split_once(": ").unwrap();
            (n.to_owned(), v.to_owned())
        })
        .collect();
    Reply {
        status,
        fields,
        body: raw[end + 4..].to_vec(),
    }
}

fn info_request(port: u16, extra: &str) -> Vec<u8> {
    format!(
        "GET {INFO_PATH} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{extra}Connection: close\r\n\r\n"
    )
    .into_bytes()
}

#[test]
fn http_limits_8_kib_field_section_works() {
    let port = start_default();
    for size in [8 * 1024, 60 * 1024] {
        let pad = format!("X-Pad: {}\r\n", "p".repeat(size));
        let reply = exchange(port, &info_request(port, &pad));
        assert_eq!(reply.status, 200, "{size}");
    }
}

#[test]
fn http_limits_head_over_64_kib_is_431() {
    let port = start_default();
    let pad = format!("X-Pad: {}\r\n", "p".repeat(64 * 1024));
    let reply = exchange(port, &info_request(port, &pad));
    assert_eq!(reply.status, 431);
    assert_eq!(reply.body, br#"{"error":"FIELDS_TOO_LARGE"}"#);
    assert_eq!(reply.field("Connection"), Some("close"));
}

#[test]
fn http_limits_257_field_lines_is_431() {
    let port = start_default();
    let ok = "X: 1\r\n".repeat(254); // Host + 254 + Connection = 256 lines
    assert_eq!(exchange(port, &info_request(port, &ok)).status, 200);
    let over = "X: 1\r\n".repeat(255);
    assert_eq!(exchange(port, &info_request(port, &over)).status, 431);
}

#[test]
fn http_limits_long_request_line_is_414() {
    let port = start_default();
    let req = format!("GET /{} HTTP/1.1\r\nHost: a\r\n\r\n", "a".repeat(70 * 1024));
    assert_eq!(exchange(port, req.as_bytes()).status, 414);
}

#[test]
fn http_limits_body_over_8_mib_is_413_without_reading_it() {
    let port = start_default();
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    let started = Instant::now();
    // Only the head and a little of the announced body are sent.
    stream
        .write_all(b"POST /catalog HTTP/1.1\r\nHost: shop.example.com\r\nContent-Length: 8388609\r\n\r\nabc")
        .unwrap();
    let reply = read_reply(&mut stream);
    assert_eq!(reply.status, 413);
    assert_eq!(reply.body, br#"{"error":"TOO_LARGE"}"#);
    assert!(started.elapsed() < Duration::from_secs(5));
    // The server goes on.
    assert_eq!(exchange(port, &info_request(port, "")).status, 200);
}

#[test]
fn http_limits_body_of_8_mib_is_read() {
    let port = start_default();
    let mut req =
        b"POST /catalog HTTP/1.1\r\nHost: shop.example.com\r\nContent-Length: 8388608\r\n\r\n"
            .to_vec();
    req.resize(req.len() + 8 * 1024 * 1024, b'x');
    // No case is stored: the body is read, then 503.
    assert_eq!(exchange(port, &req).status, 503);
}

#[test]
fn http_limits_transfer_encoding_is_501() {
    let port = start_default();
    let reply = exchange(
        port,
        b"POST /catalog HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n0\r\n\r\n",
    );
    assert_eq!(reply.status, 501);
    assert_eq!(reply.body, br#"{"error":"NOT_IMPLEMENTED"}"#);
}

#[test]
fn http_limits_content_length_rules() {
    let port = start_default();
    let post = |cl: &str| format!("POST /catalog HTTP/1.1\r\nHost: a\r\n{cl}\r\n\r\nab");
    assert_eq!(
        exchange(port, post("Content-Length: 2, 3").as_bytes()).status,
        400
    );
    assert_eq!(
        exchange(
            port,
            post("Content-Length: 2\r\nContent-Length: 3").as_bytes()
        )
        .status,
        400
    );
    assert_eq!(
        exchange(port, post("Content-Length: -2").as_bytes()).status,
        400
    );
    // The same value repeated is one value; no case: 503 after the body.
    assert_eq!(
        exchange(port, post("Content-Length: 2, 2").as_bytes()).status,
        503
    );
    // A body shorter than Content-Length.
    assert_eq!(
        exchange(port, post("Content-Length: 5").as_bytes()).status,
        400
    );
}

#[test]
fn http_limits_bare_lf_is_400_at_once() {
    let port = start(Timeouts {
        read: Duration::from_secs(30),
        ..Timeouts::default()
    });
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    let started = Instant::now();
    // The write side stays open: the answer must not wait for more bytes.
    stream.write_all(b"GET / HTTP/1.1\nHost: a\n\n").unwrap();
    let reply = read_reply(&mut stream);
    assert_eq!(reply.status, 400);
    assert_eq!(reply.body, br#"{"error":"BAD_REQUEST"}"#);
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn http_limits_slow_request_is_408_after_the_deadline() {
    let port = start(Timeouts {
        read: Duration::from_millis(300),
        ..Timeouts::default()
    });
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    stream.write_all(b"GET / HTTP/1.1\r\nHost: a\r\n").unwrap();
    let reply = read_reply(&mut stream);
    assert_eq!(reply.status, 408);
    assert_eq!(reply.body, br#"{"error":"TIMEOUT"}"#);
    // The deadline covers the body too.
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    stream
        .write_all(b"POST / HTTP/1.1\r\nHost: a\r\nContent-Length: 10\r\n\r\nab")
        .unwrap();
    assert_eq!(read_reply(&mut stream).status, 408);
}

#[test]
fn http_limits_targets_that_are_not_origin_form_are_400() {
    let port = start_default();
    for target in [
        "http://shop.example.com/catalog",
        "*",
        "shop.example.com:443",
    ] {
        let req = format!("GET {target} HTTP/1.1\r\nHost: shop.example.com\r\n\r\n");
        assert_eq!(exchange(port, req.as_bytes()).status, 400, "{target}");
    }
    let req = "GET / HTTP/2.0\r\nHost: a\r\n\r\n";
    assert_eq!(exchange(port, req.as_bytes()).status, 505);
}

#[test]
fn http_limits_the_server_goes_on_after_bad_and_empty_connections() {
    let port = start_default();
    // A connection without a byte gets no answer.
    let mut empty = TcpStream::connect(("127.0.0.1", port)).unwrap();
    empty.shutdown(Shutdown::Write).unwrap();
    let mut raw = Vec::new();
    empty.read_to_end(&mut raw).unwrap();
    assert_eq!(raw, b"");
    for bad in [
        &b"\x00\x01\x02\r\n\r\n"[..],
        b"GET / HTTP/1.1\r\nHost: a\r\n X\r\n\r\n",
        b"GET",
    ] {
        assert_eq!(exchange(port, bad).status, 400);
    }
    assert_eq!(exchange(port, &info_request(port, "")).status, 200);
}

#[test]
fn http_limits_http_10_request_is_answered() {
    let port = start_default();
    let req = format!("GET {INFO_PATH} HTTP/1.0\r\nHost: localhost:{port}\r\n\r\n");
    let reply = exchange(port, req.as_bytes());
    assert_eq!(reply.status, 200);
    assert_eq!(reply.field("Content-Type"), Some("application/json"));
}

#[test]
fn http_limits_a_slow_reader_releases_the_server_after_the_write_deadline() {
    let port = start(Timeouts {
        write: Duration::from_millis(500),
        ..Timeouts::default()
    });
    // A case with a page of 4.5 MiB: more than the socket buffers hold.
    let groups = 3 * 512 * 1024;
    let json = format!(
        r#"{{"suite":"rqp","case_id":"x","page_base64":"{}","app":{{"status":200,"headers":[]}}}}"#,
        "YWFh".repeat(groups)
    );
    let post = format!(
        "POST /__rivqen/conformance/case HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{json}",
        json.len()
    );
    assert_eq!(exchange(port, post.as_bytes()).status, 204);
    // Client A reads 4 KiB every 50 ms: the server makes progress, slowly.
    let mut slow = TcpStream::connect(("127.0.0.1", port)).unwrap();
    slow.write_all(b"GET /catalog HTTP/1.1\r\nHost: shop.example.com\r\n\r\n")
        .unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_reader = Arc::clone(&stop);
    let reader = thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut total = 0usize;
        while !stop_reader.load(Ordering::Relaxed) {
            match slow.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => total += n,
            }
            thread::sleep(Duration::from_millis(50));
        }
        total
    });
    // Client B gets its answer soon after the write deadline of A. Without one
    // deadline over all writes, A would hold the server for about a minute.
    let started = Instant::now();
    assert_eq!(exchange(port, &info_request(port, "")).status, 200);
    let waited = started.elapsed();
    stop.store(true, Ordering::Relaxed);
    let got = reader.join().unwrap();
    assert!(waited < Duration::from_secs(8), "{waited:?}");
    assert!(got < groups * 3, "A got the whole page: {got}");
}
