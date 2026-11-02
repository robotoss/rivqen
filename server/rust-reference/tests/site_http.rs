// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The site mode through the binary over TCP (WP-03 S2 §2 "Command line" and
//! "Site mapping"; DL-096): RQP F then D, the legacy flow, the log lines, and
//! the start rules of the command line.

#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::many_single_char_names,
    clippy::format_push_string
)]

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use rivqen_reference::legacy::sha1_hex;
use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_rivqen-reference");

struct Temp(PathBuf);

impl Temp {
    fn new(tag: &str) -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let base = fs::canonicalize(std::env::temp_dir()).unwrap();
        let path = base.join(format!("rivqen-site-http-{tag}-{}-{n}", std::process::id()));
        if path.exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        fs::create_dir_all(&path).unwrap();
        Self(fs::canonicalize(&path).unwrap())
    }

    fn path(&self) -> &str {
        self.0.to_str().unwrap()
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

/// The server process; killed on drop.
struct Server {
    child: Option<Child>,
    port: u16,
}

impl Server {
    fn start(args: &[&str]) -> Self {
        let mut child = Command::new(BIN)
            .args(args)
            .args(["--port", "0"])
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
        Self {
            child: Some(child),
            port,
        }
    }

    /// Stop the server and return its stderr.
    fn stop(mut self) -> String {
        let mut child = self.child.take().unwrap();
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

    fn get(&self, method: &str, target: &str, fields: &[(&str, &str)]) -> Reply {
        let mut req = format!(
            "{method} {target} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n",
            self.port
        );
        for (n, v) in fields {
            req.push_str(&format!("{n}: {v}\r\n"));
        }
        req.push_str("\r\n");
        let mut s = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
        s.write_all(req.as_bytes()).unwrap();
        s.shutdown(Shutdown::Write).ok();
        let mut raw = Vec::new();
        s.read_to_end(&mut raw).unwrap();
        Reply::parse(&raw)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            child.kill().ok();
            child.wait().ok();
        }
    }
}

#[derive(Debug)]
struct Reply {
    status: u16,
    fields: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Reply {
    fn parse(raw: &[u8]) -> Self {
        let end = raw
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .unwrap_or_else(|| panic!("no head in {:?}", String::from_utf8_lossy(raw)));
        let head = std::str::from_utf8(&raw[..end]).unwrap();
        let mut lines = head.split("\r\n");
        let status = lines.next().unwrap()[9..12].parse().unwrap();
        let fields = lines
            .map(|l| {
                let (n, v) = l.split_once(": ").unwrap();
                (n.to_owned(), v.to_owned())
            })
            .collect();
        Self {
            status,
            fields,
            body: raw[end + 4..].to_vec(),
        }
    }

    fn field(&self, name: &str) -> Vec<&str> {
        self.fields
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .collect()
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

fn kat_page() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/rqp/examples/kat/page.html");
    fs::read(path).unwrap()
}

/// `Rq-Have` of the KAT page A (negotiation §3.4 example): `t` and `b` depend
/// only on the page, not on its URL.
const RQ_HAVE_A: &str =
    "t=t1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM, b=:0Aqua3+/d6ReDehqKiXfLM8hC9ypGZS5:";
const NONCE: &str = ":AAECAwQFBgcICQoLDA0ODw==:";

#[test]
fn site_rqp_full_then_delta() {
    let t = Temp::new("rqp");
    fs::write(t.0.join("catalog.html"), kat_page()).unwrap();
    let server = Server::start(&["--site", t.path(), "--protocol", "rqp"]);
    let first = [
        ("Rq-Version", "1"),
        ("Rq-Capabilities", "(block-patch)"),
        ("Accept", "text/html"),
    ];
    let f = server.get("GET", "/catalog.html", &first);
    assert_eq!(f.status, 200);
    assert_eq!(f.field("rq-version"), ["1"]);
    assert_eq!(f.field("content-type"), ["text/html; charset=utf-8"]);
    assert_eq!(f.field("cache-control"), ["no-cache"]);
    let etag = f.field("etag")[0].to_owned();
    assert!(etag.starts_with("\"r1.") && etag.len() == 48, "{etag}");
    let text = f.text();
    assert!(
        text.contains(r#"<script type="application/rivqen-manifest+json">{"protocol":"rqp/1","origin":"https://shop.example.com","path":"/catalog.html","page_revision":"#),
        "{text}"
    );
    assert!(
        text.contains(&etag[1..47]),
        "the manifest has the revision of the ETag"
    );
    let delta = |inm: &str| {
        vec![
            ("Rq-Version", "1".to_owned()),
            ("Rq-Capabilities", "(block-patch)".to_owned()),
            (
                "Accept",
                "text/html, application/vnd.rivqen.delta+json".to_owned(),
            ),
            ("If-None-Match", inm.to_owned()),
            ("Rq-Have", RQ_HAVE_A.to_owned()),
            ("Rq-Nonce", NONCE.to_owned()),
        ]
    };
    let fields = |v: &[(&'static str, String)]| -> Vec<(&'static str, String)> { v.to_vec() };
    let send = |v: Vec<(&'static str, String)>| {
        let refs: Vec<(&str, &str)> = v.iter().map(|(n, s)| (*n, s.as_str())).collect();
        server.get("GET", "/catalog.html", &refs)
    };
    // The same page: 304.
    let r = send(fields(&delta(&etag)));
    assert_eq!(r.status, 304);
    assert_eq!(r.body, b"");
    // The file changes: the next request gets a delta with the changed block.
    let changed = String::from_utf8(kat_page())
        .unwrap()
        .replace("120.00", "130.00");
    fs::write(t.0.join("catalog.html"), changed).unwrap();
    let d = send(fields(&delta(&etag)));
    assert_eq!(d.status, 200);
    assert_eq!(
        d.field("content-type"),
        ["application/vnd.rivqen.delta+json"]
    );
    assert_eq!(d.field("cache-control"), ["no-store"]);
    let v: Value = serde_json::from_slice(&d.body).unwrap();
    assert_eq!(
        v["operations"],
        json!([{ "block_id": "price", "content": "130.00" }])
    );
    assert!(v["binding"].as_str().unwrap().starts_with("d1."));
    // A client without a snapshot gets the new F.
    let f2 = server.get("GET", "/catalog.html", &first);
    assert_eq!(f2.status, 200);
    assert_ne!(f2.field("etag"), [etag.as_str()]);
    assert!(f2.text().contains("130.00"));
    drop(server);
}

const LEGACY_PAGE: &str = "<html><head><title>Shop</title></head><body>\
<!--sonicdiff-price--><p>1</p><!--sonicdiff-price-end-->\
<!--sonicdiff-stock--><p>2</p><!--sonicdiff-stock-end--></body></html>";
const LEGACY_TEMPLATE: &str = "<html><head>{title}</head><body>{price}{stock}</body></html>";

#[test]
fn site_legacy_flow() {
    let t = Temp::new("legacy");
    fs::write(t.0.join("catalog.html"), LEGACY_PAGE).unwrap();
    let server = Server::start(&[
        "--site",
        t.path(),
        "--protocol",
        "legacy",
        "--profile",
        "legacy-canonical",
    ]);
    let tag = sha1_hex(LEGACY_TEMPLATE.as_bytes());
    let etag = sha1_hex(LEGACY_PAGE.as_bytes());
    // First load: the full page with its tags.
    let first = server.get(
        "GET",
        "/catalog.html",
        &[("accept-diff", "true"), ("accept-encoding", "gzip")],
    );
    assert_eq!(first.status, 200);
    assert_eq!(first.field("template-change"), ["true"]);
    assert_eq!(first.field("etag"), [etag.as_str()]);
    assert_eq!(first.field("template-tag"), [tag.as_str()]);
    assert_eq!(first.body, LEGACY_PAGE.as_bytes());
    // The same page again: 304.
    let data_fields = [
        ("accept-diff", "true"),
        ("accept-encoding", "gzip"),
        ("template-tag", tag.as_str()),
    ];
    let mut with_inm = data_fields.to_vec();
    with_inm.push(("If-None-Match", etag.as_str()));
    let r = server.get("GET", "/catalog.html", &with_inm);
    assert_eq!(r.status, 304);
    assert_eq!(r.body, b"");
    // The data changes, the template stays: a data response.
    fs::write(
        t.0.join("catalog.html"),
        LEGACY_PAGE.replace("<p>1</p>", "<p>3</p>"),
    )
    .unwrap();
    let r = server.get("GET", "/catalog.html", &with_inm);
    assert_eq!(r.status, 200);
    assert_eq!(r.field("template-change"), ["false"]);
    let v: Value = serde_json::from_slice(&r.body).unwrap();
    assert_eq!(v["template-tag"], tag.as_str());
    assert_eq!(
        v["data"]["{price}"],
        "<!--sonicdiff-price--><p>3</p><!--sonicdiff-price-end-->"
    );
    drop(server);
}

#[test]
fn site_log_path_only_for_files() {
    let t = Temp::new("log");
    fs::write(t.0.join("a.html"), "<p>a</p>").unwrap();
    let big = fs::File::create(t.0.join("big.html")).unwrap();
    big.set_len(5 * 1024 * 1024 + 1).unwrap();
    let server = Server::start(&["--site", t.path(), "--protocol", "rqp"]);
    assert_eq!(server.get("GET", "/a.html?token=secret-q", &[]).status, 200);
    assert_eq!(server.get("GET", "/missing-secret-a.html", &[]).status, 404);
    assert_eq!(server.get("GET", "/../secret-b", &[]).status, 404);
    assert_eq!(server.get("GET", "/secret-c.md", &[]).status, 404);
    assert_eq!(server.get("POST", "/a.html", &[]).status, 405);
    assert_eq!(server.get("HEAD", "/a.html", &[]).status, 200);
    assert_eq!(server.get("GET", "/big.html", &[]).status, 404);
    let stderr = server.stop();
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(lines.len(), 7, "{stderr}");
    assert!(lines[0].starts_with("GET /a.html 200 "), "{stderr}");
    assert!(lines[1].starts_with("GET - 404 "), "{stderr}");
    assert!(lines[2].starts_with("GET - 404 "), "{stderr}");
    assert!(lines[3].starts_with("GET - 404 "), "{stderr}");
    assert!(lines[4].starts_with("POST - 405 "), "{stderr}");
    assert!(lines[5].starts_with("HEAD /a.html 200 "), "{stderr}");
    assert!(lines[6].starts_with("GET /big.html 404 "), "{stderr}");
    assert!(
        lines[6].ends_with("(the file is larger than its limit)"),
        "{stderr}"
    );
    assert!(!stderr.contains("secret"), "{stderr}");
    assert!(!stderr.contains("warning"), "{stderr}");
}

fn exit_of(args: &[&str]) -> (Option<i32>, String) {
    let out = Command::new(BIN).args(args).output().unwrap();
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn cli_site_excludes_conformance_and_lab_peer() {
    let t = Temp::new("cli1");
    for extra in ["--conformance", "--lab-peer"] {
        let (code, stderr) = exit_of(&["--site", t.path(), "--protocol", "rqp", extra]);
        assert_eq!(code, Some(2));
        assert!(
            stderr.contains("--site excludes --conformance and --lab-peer"),
            "{stderr}"
        );
        assert!(!stderr.contains(t.path()), "{stderr}");
    }
    let (code, stderr) = exit_of(&["--site", t.path()]);
    assert_eq!(code, Some(2));
    assert!(stderr.contains("--site needs --protocol"), "{stderr}");
    let (code, _) = exit_of(&["--site", t.path(), "--protocol", "legacy"]);
    assert_eq!(code, Some(2));
    let (code, _) = exit_of(&["--conformance", "--workers", "4"]);
    assert_eq!(code, Some(2));
}

#[test]
fn cli_site_directory_must_exist() {
    let t = Temp::new("cli2");
    fs::write(t.0.join("file.html"), "x").unwrap();
    for dir in [t.0.join("missing"), t.0.join("file.html")] {
        let d = dir.to_str().unwrap();
        let (code, stderr) = exit_of(&["--site", d, "--protocol", "rqp", "--port", "0"]);
        assert_eq!(code, Some(1), "{stderr}");
        assert!(
            stderr.contains("the --site directory cannot be used"),
            "{stderr}"
        );
        assert!(!stderr.contains(d), "{stderr}");
    }
}

#[test]
fn cli_site_config_file_rules() {
    let t = Temp::new("cli3");
    let site = t.0.join("site");
    fs::create_dir_all(&site).unwrap();
    let s = site.to_str().unwrap();
    for (name, body, text) in [
        ("bad.json", "{".to_owned(), "the --config file is not JSON"),
        (
            "unknown.json",
            r#"{"other":1}"#.to_owned(),
            "rivqen-reference: ",
        ),
        (
            "large.json",
            format!(r#"{{"policy":{{}}{}}}"#, " ".repeat(64 * 1024)),
            "the --config file is larger than 64 KiB",
        ),
    ] {
        let c = t.0.join(name);
        fs::write(&c, body).unwrap();
        let (code, stderr) = exit_of(&[
            "--site",
            s,
            "--protocol",
            "rqp",
            "--config",
            c.to_str().unwrap(),
            "--port",
            "0",
        ]);
        assert_eq!(code, Some(1), "{name}: {stderr}");
        assert!(stderr.contains(text), "{name}: {stderr}");
        assert_eq!(stderr.lines().count(), 1, "{name}: {stderr}");
    }
    let missing = t.0.join("missing.json");
    let (code, stderr) = exit_of(&[
        "--site",
        s,
        "--protocol",
        "rqp",
        "--config",
        missing.to_str().unwrap(),
    ]);
    assert_eq!(code, Some(1));
    assert!(
        stderr.contains("the --config file cannot be read"),
        "{stderr}"
    );
}

#[test]
fn cli_site_config_is_used() {
    let t = Temp::new("cli4");
    let site = t.0.join("site");
    fs::create_dir_all(&site).unwrap();
    fs::write(site.join("catalog.html"), kat_page()).unwrap();
    let c = t.0.join("config.json");
    // Exactly 64 KiB: at the limit.
    let config = r#"{"policy":{"ttl_seconds":60,"offline":true}}"#;
    fs::write(
        &c,
        format!("{config}{}", " ".repeat(64 * 1024 - config.len())),
    )
    .unwrap();
    assert_eq!(fs::metadata(&c).unwrap().len(), 64 * 1024);
    let server = Server::start(&[
        "--site",
        site.to_str().unwrap(),
        "--protocol",
        "rqp",
        "--config",
        c.to_str().unwrap(),
        "--origin",
        "http://127.0.0.1:9000",
    ]);
    let f = server.get("GET", "/catalog.html", &[]);
    assert_eq!(f.status, 200);
    let text = f.text();
    assert!(
        text.contains(r#""origin":"http://127.0.0.1:9000","path":"/catalog.html""#),
        "{text}"
    );
    assert!(
        text.contains(r#""policy":{"ttl_seconds":60,"offline":true}"#),
        "{text}"
    );
}

#[test]
fn cli_site_workers_serve_beside_an_idle_socket() {
    let t = Temp::new("cli5");
    fs::write(t.0.join("t.txt"), "x").unwrap();
    // With one worker an idle socket holds the server for the 5 s first-byte
    // deadline; with two the next client is answered at once.
    let server = Server::start(&["--site", t.path(), "--protocol", "rqp", "--workers", "2"]);
    let _idle = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    thread::sleep(Duration::from_millis(100));
    let started = Instant::now();
    assert_eq!(server.get("GET", "/t.txt", &[]).status, 200);
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    // The default is 8 workers.
    let server = Server::start(&["--site", t.path(), "--protocol", "rqp"]);
    let idle: Vec<TcpStream> = (0..7)
        .map(|_| TcpStream::connect(("127.0.0.1", server.port)).unwrap())
        .collect();
    thread::sleep(Duration::from_millis(100));
    let started = Instant::now();
    assert_eq!(server.get("GET", "/t.txt", &[]).status, 200);
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    drop(idle);
}
