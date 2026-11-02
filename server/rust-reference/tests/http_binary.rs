// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The binary: exit codes of the command line, the `listening <port>` line, and
//! one exchange.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::time::Duration;

const BIN: &str = env!("CARGO_BIN_EXE_rivqen-reference");

fn exit_of(args: &[&str]) -> (Option<i32>, String, String) {
    let out = Command::new(BIN).args(args).output().unwrap();
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn cli_rejects_a_start_without_conformance() {
    let (code, stdout, stderr) = exit_of(&[]);
    assert_eq!(code, Some(2));
    assert_eq!(stdout, "");
    assert!(stderr.contains("--conformance is required"), "{stderr}");
    assert!(
        stderr.contains("usage: rivqen-reference --conformance"),
        "{stderr}"
    );
}

#[test]
fn cli_rejects_non_loopback_listen_without_lab_peer() {
    let (code, _, stderr) = exit_of(&["--conformance", "--listen", "0.0.0.0", "--port", "0"]);
    assert_eq!(code, Some(2));
    assert!(stderr.contains("needs --lab-peer"), "{stderr}");
}

#[test]
fn cli_rejects_unknown_options_without_echo() {
    let (code, _, stderr) = exit_of(&["--conformance", "--secret-value-xyz"]);
    assert_eq!(code, Some(2));
    assert!(!stderr.contains("secret-value-xyz"), "{stderr}");
}

#[test]
fn cli_help_exits_0() {
    let (code, stdout, _) = exit_of(&["--help"]);
    assert_eq!(code, Some(0));
    assert!(stdout.starts_with("usage: rivqen-reference"), "{stdout}");
}

/// Start the binary, read its `listening <port>` line, return the child and the
/// port.
fn spawn(args: &[&str]) -> (std::process::Child, u16) {
    let mut child = Command::new(BIN)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let port = line
        .strip_prefix("listening ")
        .and_then(|p| p.strip_suffix('\n'))
        .unwrap_or_else(|| panic!("first line {line:?}"))
        .parse()
        .unwrap();
    (child, port)
}

fn stop(mut child: std::process::Child) -> String {
    child.kill().unwrap();
    child.wait().unwrap();
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    stderr
}

#[test]
fn control_lab_peer_writes_one_warning_line_at_start() {
    let (child, port) = spawn(&["--conformance", "--port", "0", "--lab-peer"]);
    // A loopback peer is still allowed.
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    stream
        .write_all(
            format!("GET /__rivqen/conformance/info HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n")
                .as_bytes(),
        )
        .unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    assert!(raw.starts_with(b"HTTP/1.1 200 OK\r\n"));
    let stderr = stop(child);
    let mut lines = stderr.lines();
    assert_eq!(
        lines.next(),
        Some(
            "rivqen-reference: warning: --lab-peer is on: control requests are accepted from loopback and private peers (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, fc00::/7, fe80::/10); publish the port on 127.0.0.1 only"
        )
    );
    assert!(
        lines
            .next()
            .unwrap()
            .starts_with("GET /__rivqen/conformance/info 200 "),
        "{stderr}"
    );
    assert_eq!(stderr.matches("--lab-peer").count(), 1, "{stderr}");
}

#[test]
fn cli_without_lab_peer_writes_no_warning() {
    let (child, port) = spawn(&["--conformance", "--port", "0"]);
    // A case request with a path that must not reach the log.
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    stream
        .write_all(b"GET /reset/secret-path-token HTTP/1.1\r\nHost: shop.example.com\r\n\r\n")
        .unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    assert!(raw.starts_with(b"HTTP/1.1 503 "));
    let stderr = stop(child);
    assert!(!stderr.contains("warning"), "{stderr}");
    assert!(!stderr.contains("secret"), "{stderr}");
    assert!(stderr.starts_with("GET - 503 "), "{stderr}");
}

#[test]
fn cli_prints_listening_port_first_and_serves() {
    let mut child = Command::new(BIN)
        .args(["--conformance", "--port", "0"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let port: u16 = line
        .strip_prefix("listening ")
        .and_then(|p| p.strip_suffix('\n'))
        .unwrap_or_else(|| panic!("first line {line:?}"))
        .parse()
        .unwrap();
    assert_ne!(port, 0);
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    stream
        .write_all(
            format!("GET /__rivqen/conformance/info?q=secret HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-Token: secret-field\r\n\r\n")
                .as_bytes(),
        )
        .unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    assert!(raw.starts_with(b"HTTP/1.1 200 OK\r\n"));
    child.kill().unwrap();
    child.wait().unwrap();
    // One log line: method, path without query, status, duration.
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(
        stderr.starts_with("GET /__rivqen/conformance/info 200 "),
        "{stderr}"
    );
    assert!(!stderr.contains("secret"), "{stderr}");
}
