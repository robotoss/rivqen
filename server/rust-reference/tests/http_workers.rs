// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The worker pool and the deadlines of the server loop (WP-03 S2 §2
//! "Hardening" items 1 and 2; DL-097) over TCP on `127.0.0.1:0`, with a test
//! handler and with the conformance handler.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::similar_names
)]

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use rivqen_reference::control::{CASE_PATH, DEFAULT_ORIGIN, Server, Settings};
use rivqen_reference::http::handler::{Conn, Handler, Next, Outcome};
use rivqen_reference::http::reader::RequestHead;
use rivqen_reference::http::server::{ServeOptions, Timeouts, serve};
use rivqen_reference::model::{Body, Response};

/// Answers `200 ok` to every request; panics for the path `/panic`.
struct Ok200;

impl Handler for Ok200 {
    type Route = ();

    fn route(&self, _conn: &Conn, head: &RequestHead) -> Next<()> {
        assert_ne!(head.path(), "/panic", "test handler bug");
        Next::ReadBody(())
    }

    fn respond(&self, (): (), _conn: &Conn, _head: &RequestHead, _body: &[u8]) -> Outcome {
        Outcome::plain(Response {
            status: 200,
            fields: Vec::new(),
            body: Body::Bytes(b"ok".to_vec()),
        })
    }
}

fn start<H: Handler + Send + 'static>(handler: H, workers: usize, timeouts: Timeouts) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || serve(&listener, &handler, &ServeOptions { timeouts, workers }));
    port
}

fn connect(port: u16) -> TcpStream {
    let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    stream
}

/// Send `bytes`, close the write side, read the whole answer.
fn exchange(port: u16, bytes: &[u8]) -> Vec<u8> {
    let mut stream = connect(port);
    stream.write_all(bytes).unwrap();
    stream.shutdown(Shutdown::Write).ok();
    read_all(&mut stream)
}

fn read_all(stream: &mut TcpStream) -> Vec<u8> {
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    raw
}

fn status(raw: &[u8]) -> u16 {
    assert!(raw.len() >= 12, "{:?}", String::from_utf8_lossy(raw));
    std::str::from_utf8(&raw[9..12]).unwrap().parse().unwrap()
}

const GET: &[u8] = b"GET / HTTP/1.1\r\nHost: a\r\n\r\n";

/// Long deadlines: only the pool can answer the client in time.
fn long() -> Timeouts {
    Timeouts {
        first_byte: Duration::from_secs(30),
        head: Duration::from_secs(30),
        read: Duration::from_secs(60),
        ..Timeouts::default()
    }
}

#[test]
fn http_workers_idle_sockets_do_not_block() {
    let port = start(Ok200, 4, long());
    // Three idle sockets hold three of the four workers.
    let idle: Vec<TcpStream> = (0..3).map(|_| connect(port)).collect();
    thread::sleep(Duration::from_millis(200));
    let started = Instant::now();
    assert_eq!(status(&exchange(port, GET)), 200);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    // The idle sockets still wait: no answer, no close.
    for mut s in idle {
        s.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
        let mut b = [0u8; 1];
        let e = s.read(&mut b).unwrap_err();
        assert!(
            matches!(
                e.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ),
            "{e:?}"
        );
    }
}

#[test]
fn http_workers_one_worker_is_held_by_an_idle_socket_until_its_deadline() {
    let first_byte = Duration::from_millis(400);
    let port = start(
        Ok200,
        1,
        Timeouts {
            first_byte,
            ..long()
        },
    );
    let _idle = connect(port);
    thread::sleep(Duration::from_millis(50));
    let started = Instant::now();
    assert_eq!(status(&exchange(port, GET)), 200);
    let waited = started.elapsed();
    // One worker serves the connections in accept order: B waits for the
    // first-byte deadline of A, and no longer.
    assert!(waited >= Duration::from_millis(250), "{waited:?}");
    assert!(waited < Duration::from_secs(10), "{waited:?}");
}

#[test]
fn http_workers_survive_a_handler_panic() {
    let port = start(Ok200, 1, Timeouts::default());
    for _ in 0..3 {
        // The connection is closed without an answer.
        let raw = exchange(port, b"GET /panic HTTP/1.1\r\nHost: a\r\n\r\n");
        assert_eq!(raw, b"");
        // The only worker goes on.
        assert_eq!(status(&exchange(port, GET)), 200);
    }
}

/// Uses about `frames` × 64 KiB of stack (on purpose: the test measures the
/// stack of a worker).
#[inline(never)]
#[allow(clippy::large_stack_arrays)]
fn use_stack(frames: usize) -> usize {
    let buf = std::hint::black_box([1u8; 64 * 1024]);
    if frames == 0 {
        usize::from(buf[0])
    } else {
        use_stack(frames - 1) + usize::from(buf[frames % buf.len()])
    }
}

/// Answers `200` after it used 1.5 MiB of stack or more (24 frames of at least
/// 64 KiB; a debug frame can hold more than one copy of the array).
struct DeepStack;

impl Handler for DeepStack {
    type Route = ();

    fn route(&self, _conn: &Conn, _head: &RequestHead) -> Next<()> {
        Next::ReadBody(())
    }

    fn respond(&self, (): (), _conn: &Conn, _head: &RequestHead, _body: &[u8]) -> Outcome {
        let n = use_stack(24);
        Outcome::plain(Response {
            status: 200,
            fields: Vec::new(),
            body: Body::Bytes(n.to_string().into_bytes()),
        })
    }
}

#[test]
fn http_workers_have_an_8_mib_stack() {
    // The S1 loop ran on the main thread (8 MiB); a worker has the same stack,
    // so a deep handler does not overflow (an overflow aborts the process).
    let port = start(DeepStack, 1, Timeouts::default());
    let raw = exchange(port, GET);
    assert_eq!(status(&raw), 200);
    assert!(raw.ends_with(b"25"));
}

#[test]
fn http_workers_a_value_outside_1_to_64_is_clamped() {
    for workers in [0, 1000] {
        let port = start(Ok200, workers, Timeouts::default());
        assert_eq!(status(&exchange(port, GET)), 200);
    }
}

#[test]
fn http_workers_many_clients_at_once_are_all_answered() {
    let port = start(Ok200, 4, Timeouts::default());
    let clients: Vec<_> = (0..100)
        .map(|_| thread::spawn(move || status(&exchange(port, GET))))
        .collect();
    for c in clients {
        assert_eq!(c.join().unwrap(), 200);
    }
}

fn conformance(port_workers: usize) -> u16 {
    let server = Server::new(Settings {
        conformance: true,
        lab_peer: false,
        origin: DEFAULT_ORIGIN.to_owned(),
    });
    start(server, port_workers, Timeouts::default())
}

#[test]
fn http_workers_conformance_mode_answers_in_case_order() {
    let port = conformance(1);
    let case = r#"{"suite":"legacy","case_id":"x","profile":"legacy-canonical","page_base64":"PHA+aGk8L3A+","app":{"status":200,"headers":[["Content-Type","text/html"]]}}"#;
    for round in 0..5 {
        // A: the whole POST is sent, the answer is not read yet.
        let mut a = connect(port);
        let post = format!(
            "POST {CASE_PATH} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{case}",
            case.len()
        );
        a.write_all(post.as_bytes()).unwrap();
        // B connects after A: one worker answers B after A, so B sees the case.
        let raw = exchange(
            port,
            b"GET /catalog HTTP/1.1\r\nHost: shop.example.com\r\n\r\n",
        );
        assert_eq!(status(&raw), 200, "{round}");
        assert!(raw.ends_with(b"<p>hi</p>"), "{round}");
        assert_eq!(status(&read_all(&mut a)), 204);
    }
}

#[test]
fn http_deadline_a_socket_that_sends_nothing_is_closed_after_the_first_byte_deadline() {
    let port = start(
        Ok200,
        1,
        Timeouts {
            first_byte: Duration::from_millis(300),
            ..long()
        },
    );
    let mut silent = connect(port);
    let started = Instant::now();
    // Closed without an answer (no 408 to a client that sent nothing).
    assert_eq!(read_all(&mut silent), b"");
    let waited = started.elapsed();
    assert!(waited >= Duration::from_millis(200), "{waited:?}");
    assert!(waited < Duration::from_secs(10), "{waited:?}");
    // The worker is free again.
    assert_eq!(status(&exchange(port, GET)), 200);
}

#[test]
fn http_deadline_the_rest_of_the_head_has_the_head_deadline() {
    let port = start(
        Ok200,
        1,
        Timeouts {
            head: Duration::from_millis(300),
            ..long()
        },
    );
    let mut slow = connect(port);
    let started = Instant::now();
    slow.write_all(b"GET / HT").unwrap();
    assert_eq!(status(&read_all(&mut slow)), 408);
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn http_deadline_the_first_byte_starts_the_head_deadline() {
    // The first-byte deadline is short, the head deadline is long: a head that
    // started in time may take longer than the first-byte deadline.
    let port = start(
        Ok200,
        1,
        Timeouts {
            first_byte: Duration::from_millis(300),
            head: Duration::from_secs(10),
            ..long()
        },
    );
    let mut stream = connect(port);
    stream.write_all(b"G").unwrap();
    thread::sleep(Duration::from_millis(700));
    stream.write_all(&GET[1..]).unwrap();
    assert_eq!(status(&read_all(&mut stream)), 200);
}

#[test]
fn http_deadline_the_request_deadline_bounds_the_head_deadline() {
    let port = start(
        Ok200,
        1,
        Timeouts {
            first_byte: Duration::from_secs(30),
            head: Duration::from_secs(30),
            read: Duration::from_millis(300),
            ..Timeouts::default()
        },
    );
    // Nothing is sent: the request deadline is earlier than the first-byte
    // deadline, so the connection is closed after it.
    let mut silent = connect(port);
    let started = Instant::now();
    assert_eq!(read_all(&mut silent), b"");
    assert!(started.elapsed() < Duration::from_secs(10));
    // One byte, then nothing: the request deadline, not the head deadline.
    let mut slow = connect(port);
    let started = Instant::now();
    slow.write_all(b"G").unwrap();
    assert_eq!(status(&read_all(&mut slow)), 408);
    assert!(started.elapsed() < Duration::from_secs(10));
}
