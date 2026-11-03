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

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

const TOKEN: &str = "Tok3n_of-the-test-run_0123456789";

/// A token file in the temporary directory, removed at the end.
struct TokenFile(PathBuf);

impl TokenFile {
    fn new(tag: &str, content: &str) -> Self {
        let base = fs::canonicalize(std::env::temp_dir()).unwrap();
        let path = base.join(format!("rivqen-token-{tag}-{}.txt", std::process::id()));
        fs::write(&path, content).unwrap();
        Self(path)
    }

    fn path(&self) -> &str {
        self.0.to_str().unwrap()
    }
}

impl Drop for TokenFile {
    fn drop(&mut self) {
        fs::remove_file(&self.0).ok();
    }
}

/// `GET /info`, with the token field when `token` is given; the raw answer.
fn info(port: u16, token: Option<&str>) -> Vec<u8> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    let field = token.map_or(String::new(), |t| {
        format!("Rivqen-Conformance-Token: {t}\r\n")
    });
    stream
        .write_all(
            format!(
                "GET /__rivqen/conformance/info HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{field}\r\n"
            )
            .as_bytes(),
        )
        .unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    raw
}

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
    assert!(
        stderr.contains("--conformance or --site <dir> is required"),
        "{stderr}"
    );
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
    let token = TokenFile::new("lab", &format!("{TOKEN}\n"));
    let (child, port) = spawn(&[
        "--conformance",
        "--port",
        "0",
        "--lab-peer",
        "--control-token-file",
        token.path(),
    ]);
    // A loopback peer is still allowed, with the token.
    let raw = info(port, Some(TOKEN));
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
    assert!(!stderr.contains(TOKEN), "{stderr}");
}

#[test]
fn control_token_over_tcp() {
    // Surrounding white space of the file is removed.
    let token = TokenFile::new("tcp", &format!("  {TOKEN}\r\n\n"));
    let (child, port) = spawn(&[
        "--conformance",
        "--port",
        "0",
        "--control-token-file",
        token.path(),
    ]);
    let refused = info(port, None);
    assert!(refused.starts_with(b"HTTP/1.1 403 Forbidden\r\n"));
    assert!(refused.ends_with(br#"{"error":"FORBIDDEN"}"#));
    let wrong = info(port, Some(&TOKEN.replace('T', "t")));
    assert!(wrong.starts_with(b"HTTP/1.1 403 "));
    let right = info(port, Some(TOKEN));
    assert!(right.starts_with(b"HTTP/1.1 200 OK\r\n"));
    assert!(!String::from_utf8_lossy(&right).contains(TOKEN));
    let stderr = stop(child);
    assert!(!stderr.contains(TOKEN), "{stderr}");
    assert!(
        stderr.contains("(the control token is missing or wrong)"),
        "{stderr}"
    );
    assert!(
        stderr.contains("GET /__rivqen/conformance/info 200 "),
        "{stderr}"
    );
}

#[test]
fn control_token_file_rules() {
    let cases = [
        ("short", "short-token".to_owned(), "does not hold a token"),
        ("pad", format!("{TOKEN}=="), "does not hold a token"),
        ("two", format!("{TOKEN} {TOKEN}"), "does not hold a token"),
        ("empty", String::new(), "does not hold a token"),
        (
            "large",
            format!("{TOKEN}{}", " ".repeat(1024)),
            "larger than 1 KiB",
        ),
    ];
    for (tag, content, text) in cases {
        let file = TokenFile::new(tag, &content);
        let (code, stdout, stderr) = exit_of(&[
            "--conformance",
            "--port",
            "0",
            "--control-token-file",
            file.path(),
        ]);
        assert_eq!(code, Some(1), "{tag}: {stderr}");
        assert_eq!(stdout, "", "{tag}");
        assert_eq!(stderr.lines().count(), 1, "{tag}: {stderr}");
        assert!(stderr.contains(text), "{tag}: {stderr}");
        assert!(!stderr.contains(file.path()), "{tag}: {stderr}");
        assert!(
            !stderr.contains(TOKEN) && !stderr.contains("short-token"),
            "{tag}"
        );
    }
    let missing = TokenFile::new("gone", "");
    fs::remove_file(&missing.0).unwrap();
    let (code, _, stderr) = exit_of(&["--conformance", "--control-token-file", missing.path()]);
    assert_eq!(code, Some(1));
    assert!(
        stderr.contains("the --control-token-file cannot be read"),
        "{stderr}"
    );
    // The command-line rules are usage errors (exit 2).
    let (code, _, stderr) = exit_of(&["--conformance", "--lab-peer"]);
    assert_eq!(code, Some(2));
    assert!(
        stderr.contains("--lab-peer needs --control-token-file <file>"),
        "{stderr}"
    );
}

#[test]
fn cli_idle_connection_writes_one_log_line() {
    // The default first-byte deadline is 5 s.
    let (child, port) = spawn(&["--conformance", "--port", "0"]);
    let mut idle = TcpStream::connect(("127.0.0.1", port)).unwrap();
    idle.set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    let mut raw = Vec::new();
    idle.read_to_end(&mut raw).unwrap();
    assert_eq!(raw, b"", "closed without an answer");
    // The line is written before the connection is closed.
    let stderr = stop(child);
    assert_eq!(stderr.lines().count(), 1, "{stderr}");
    assert!(stderr.starts_with("- - - "), "{stderr}");
    assert!(
        stderr
            .trim_end()
            .ends_with("ms (no request byte in time; closed)"),
        "{stderr}"
    );
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
