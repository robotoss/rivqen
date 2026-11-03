// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The site mapping of WP-03 S2 §2 items 1 to 9 (DL-096), one test per item, in
//! the process: the site module of the binary is included by its path, and a
//! site directory is made in the temporary directory (canonical, because on
//! macOS `/var` is a link to `/private/var`).

#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

#[path = "../src/site/mod.rs"]
#[allow(dead_code)]
mod site;

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use rivqen_reference::http::handler::{Conn, Handler, LogPath, Next, Outcome};
use rivqen_reference::http::reader::{RequestHead, Version};
use rivqen_reference::model::{Body, Field, Profile};
use rivqen_reference::rqp::config::RqpConfig;
use site::{Engine, MAX_FILE_BYTES, MAX_HTML_BYTES, Site, map_path};

/// A directory that is removed at the end of the test.
struct Temp(PathBuf);

impl Temp {
    fn new(tag: &str) -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let base = fs::canonicalize(std::env::temp_dir()).unwrap();
        let path = base.join(format!("rivqen-site-{tag}-{}-{n}", std::process::id()));
        if path.exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        fs::create_dir_all(&path).unwrap();
        Self(fs::canonicalize(&path).unwrap())
    }

    fn file(&self, rel: &str, bytes: &[u8]) -> PathBuf {
        let p = self.0.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, bytes).unwrap();
        p
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

/// The site of the tests: `<temp>/site`, and `<temp>/outside/secret.html`.
fn layout(tag: &str) -> (Temp, PathBuf) {
    let t = Temp::new(tag);
    let site = t.0.join("site");
    for (rel, body) in [
        ("site/index.html", &b"<p>home</p>"[..]),
        ("site/a.html", b"<p>a</p>"),
        ("site/sub/index.html", b"<p>sub</p>"),
        ("site/sub/page.html", b"<p>page</p>"),
        ("site/.hidden.html", b"hidden"),
        ("site/.git/config.html", b"hidden dir"),
        ("site/readme.md", b"md"),
        ("site/noext", b"noext"),
        ("site/B.HTML", b"upper"),
        ("site/style.css", b"body{}"),
        ("site/app.js", b"1"),
        ("site/data.json", b"{}"),
        ("site/logo.svg", b"<svg/>"),
        ("site/i.png", b"png"),
        ("site/i.jpg", b"jpg"),
        ("site/i.jpeg", b"jpeg"),
        ("site/i.webp", b"webp"),
        ("site/favicon.ico", b"ico"),
        ("site/t.txt", b"txt"),
        ("site/a..b.txt", b"dots inside"),
        ("site/_u.txt", b"underscore first"),
        ("site/-d.txt", b"dash first"),
        ("site/9.txt", b"digit first"),
        ("outside/secret.html", b"SECRET-OUTSIDE"),
    ] {
        t.file(rel, body);
    }
    fs::create_dir_all(site.join("empty")).unwrap();
    symlink(site.join("a.html"), site.join("link-in.html")).unwrap();
    symlink(t.0.join("outside/secret.html"), site.join("link-out.html")).unwrap();
    symlink(site.join("sub"), site.join("linkdir")).unwrap();
    symlink(t.0.join("outside"), site.join("outdir")).unwrap();
    (t, site)
}

fn rqp_site(dir: &Path) -> Site {
    Site::new(
        dir,
        Engine::Rqp(RqpConfig::default()),
        "https://shop.example.com".to_owned(),
    )
    .unwrap()
}

fn head(method: &str, target: &str) -> RequestHead {
    RequestHead {
        method: method.to_owned(),
        target: target.to_owned(),
        version: Version::Http11,
        fields: vec![Field::new("Host", "127.0.0.1:8080")],
        content_length: 0,
    }
}

fn get(site: &Site, target: &str) -> Outcome {
    site.answer(&head("GET", target))
}

fn body(o: &Outcome) -> &[u8] {
    match &o.response.body {
        Body::Bytes(b) => b,
        Body::None => b"",
    }
}

fn field<'a>(o: &'a Outcome, name: &str) -> Vec<&'a str> {
    o.response
        .fields
        .iter()
        .filter(|f| f.is(name))
        .map(|f| f.value.as_str())
        .collect()
}

fn assert_404(o: &Outcome, target: &str) {
    assert_eq!(o.response.status, 404, "{target}");
    assert_eq!(body(o), b"Not Found\n", "{target}");
    assert_eq!(
        field(o, "content-type"),
        ["text/plain; charset=utf-8"],
        "{target}"
    );
}

#[test]
fn site_mapping_item1_methods_get_and_head() {
    let (_t, dir) = layout("m1");
    let site = rqp_site(&dir);
    let o = get(&site, "/t.txt");
    assert_eq!((o.response.status, body(&o)), (200, &b"txt"[..]));
    let o = site.answer(&head("HEAD", "/t.txt"));
    assert_eq!(o.response.status, 200);
    for method in ["POST", "PUT", "DELETE", "OPTIONS", "PATCH", "get", "head"] {
        let o = site.answer(&head(method, "/t.txt"));
        assert_eq!(o.response.status, 405, "{method}");
        assert_eq!(field(&o, "allow"), ["GET, HEAD"]);
        assert_eq!(body(&o), b"Method Not Allowed\n");
        assert_eq!(field(&o, "content-type"), ["text/plain; charset=utf-8"]);
        assert_eq!(o.log_path, LogPath::Hidden);
    }
    // The site never reads a body: the handler answers from the head.
    let mut h = head("POST", "/t.txt");
    h.content_length = 10;
    let conn = Conn {
        peer: "127.0.0.1".parse().unwrap(),
        local_port: 8080,
    };
    // Next::ReadBody(Infallible) cannot exist: the pattern is irrefutable.
    let Next::Respond(o) = Handler::route(&site, &conn, &h);
    assert_eq!(o.response.status, 405);
}

#[test]
fn site_mapping_item2_path_pattern() {
    let (_t, dir) = layout("m2");
    let site = rqp_site(&dir);
    for target in [
        "/..",
        "/../outside/secret.html",
        "/%2e%2e/outside/secret.html",
        "/%2E%2E/outside/secret.html",
        "/sub/../a.html",
        "/./a.html",
        "/.hidden.html",
        "/.git/config.html",
        "//a.html",
        "/sub//page.html",
        "/a%2ehtml",
        "/%61.html",
        "/a html",
        "/\u{e9}.html",
        "/a\\b.html",
        "/a.html;x",
        "/~a.html",
        "",
        "a.html",
    ] {
        assert!(
            map_path(target.split('?').next().unwrap()).is_none(),
            "{target}"
        );
        let o = get(&site, target);
        assert_404(&o, target);
        assert_eq!(o.note, Some("the path maps to no file name"), "{target}");
        assert_eq!(o.log_path, LogPath::Hidden, "{target}");
    }
    // Allowed: letters, digits, `_`, `-`, and a dot that does not start a
    // segment; the query does not count.
    for (target, want) in [
        ("/a.html?x=../../outside", &b"<p>a</p>"[..]),
        ("/a..b.txt", b"dots inside"),
        ("/sub/page.html", b"<p>page</p>"),
        ("/_u.txt", b"underscore first"),
        ("/-d.txt", b"dash first"),
        ("/9.txt", b"digit first"),
    ] {
        let o = get(&site, target);
        assert_eq!(o.response.status, 200, "{target}");
        assert!(body(&o).ends_with(want), "{target}");
    }
}

#[test]
fn site_mapping_item3_index_for_a_directory_and_no_other_fallback() {
    let (_t, dir) = layout("m3");
    let site = rqp_site(&dir);
    for (target, want) in [("/", "<p>home</p>"), ("/sub/", "<p>sub</p>")] {
        let o = get(&site, target);
        assert_eq!(o.response.status, 200, "{target}");
        assert!(String::from_utf8_lossy(body(&o)).contains(want), "{target}");
    }
    // A directory without `/`, a directory without index.html, no `.html` guess.
    for target in ["/sub", "/empty/", "/a", "/sub/page", "/missing/"] {
        assert_404(&get(&site, target), target);
    }
    assert_eq!(
        map_path("/sub/").unwrap().segments,
        vec!["sub", "index.html"]
    );
    assert_eq!(map_path("/").unwrap().segments, vec!["index.html"]);
}

#[test]
fn site_mapping_item4_symbolic_links_and_files_outside_are_refused() {
    let (t, dir) = layout("m4");
    let site = rqp_site(&dir);
    for target in [
        "/link-in.html",
        "/link-out.html",
        "/linkdir/page.html",
        "/linkdir/",
        "/outdir/secret.html",
    ] {
        let o = get(&site, target);
        assert_404(&o, target);
        assert_eq!(o.note, Some("no regular file at the path"), "{target}");
        assert_eq!(o.log_path, LogPath::Hidden, "{target}");
        assert!(!String::from_utf8_lossy(body(&o)).contains("SECRET"));
    }
    // A directory with the name of a file, and a file inside a file.
    fs::create_dir_all(dir.join("dir.html")).unwrap();
    assert_404(&get(&site, "/dir.html"), "/dir.html");
    for target in ["/a.html/x.html", "/a.html/"] {
        assert!(map_path(target).is_some(), "{target}");
        let o = get(&site, target);
        assert_404(&o, target);
        assert_eq!(o.note, Some("no regular file at the path"), "{target}");
    }
    // On a case-insensitive volume (the macOS default), a name that differs from
    // the file name only in case reaches the file, but its canonical path has the
    // name of the file: that fails closed. A case-sensitive volume has no such
    // file at all.
    fs::write(dir.join("case.txt"), b"case").unwrap();
    let o = get(&site, "/CASE.txt");
    assert_404(&o, "/CASE.txt");
    if dir.join("CASE.txt").exists() {
        assert_eq!(o.note, Some("no regular file at the path"));
    }
    assert_eq!(body(&get(&site, "/case.txt")), b"case");
    // A FIFO is not a regular file (and the answer does not wait on it).
    let fifo = dir.join("fifo.txt");
    if Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .is_ok_and(|s| s.success())
    {
        assert_404(&get(&site, "/fifo.txt"), "/fifo.txt");
    }
    // The site directory itself may be given through a link: the site uses its
    // canonical path, and the files below it are served.
    let via = t.0.join("via");
    symlink(&dir, &via).unwrap();
    let site = rqp_site(&via);
    assert_eq!(get(&site, "/t.txt").response.status, 200);
    assert_404(&get(&site, "/link-in.html"), "/link-in.html");
    // A file that is not a directory cannot be the site.
    let e = Site::new(
        &dir.join("t.txt"),
        Engine::Rqp(RqpConfig::default()),
        String::new(),
    )
    .unwrap_err();
    assert_eq!(e.kind(), std::io::ErrorKind::NotADirectory);
    assert!(
        Site::new(
            &t.0.join("missing"),
            Engine::Rqp(RqpConfig::default()),
            String::new()
        )
        .is_err()
    );
}

#[test]
fn site_mapping_item5_media_types_and_unknown_extensions() {
    let (_t, dir) = layout("m5");
    let site = rqp_site(&dir);
    for (target, media) in [
        ("/style.css", "text/css"),
        ("/app.js", "text/javascript"),
        ("/data.json", "application/json"),
        ("/logo.svg", "image/svg+xml"),
        ("/i.png", "image/png"),
        ("/i.jpg", "image/jpeg"),
        ("/i.jpeg", "image/jpeg"),
        ("/i.webp", "image/webp"),
        ("/favicon.ico", "image/vnd.microsoft.icon"),
        ("/t.txt", "text/plain; charset=utf-8"),
    ] {
        let o = get(&site, target);
        assert_eq!(o.response.status, 200, "{target}");
        assert_eq!(field(&o, "content-type"), [media], "{target}");
        assert_eq!(field(&o, "cache-control"), ["no-cache"], "{target}");
    }
    let o = get(&site, "/a.html");
    assert_eq!(field(&o, "content-type"), ["text/html; charset=utf-8"]);
    for target in [
        "/readme.md",
        "/noext",
        "/A.HTML",
        "/a.htm",
        "/a.html.bak",
        "/x.",
    ] {
        let o = get(&site, target);
        assert_404(&o, target);
        assert_eq!(o.note, Some("the path maps to no file name"), "{target}");
    }
}

#[test]
fn site_mapping_item6_size_limits() {
    let (_t, dir) = layout("m6");
    let site = rqp_site(&dir);
    let sized = |name: &str, len: u64| {
        let f = fs::File::create(dir.join(name)).unwrap();
        f.set_len(len).unwrap();
    };
    sized("big.html", MAX_HTML_BYTES + 1);
    sized("big.css", MAX_FILE_BYTES + 1);
    sized("max.txt", MAX_FILE_BYTES);
    sized("max.css", MAX_HTML_BYTES + 1);
    for target in ["/big.html", "/big.css"] {
        let o = get(&site, target);
        assert_404(&o, target);
        assert_eq!(
            o.note,
            Some("the file is larger than its limit"),
            "{target}"
        );
        // The path maps to a file of the site: the log line shows it.
        assert_eq!(o.log_path, LogPath::Request, "{target}");
    }
    // At the limit: served. A non-HTML file may be larger than the HTML limit.
    for (target, len) in [
        ("/max.txt", MAX_FILE_BYTES),
        ("/max.css", MAX_HTML_BYTES + 1),
    ] {
        let o = get(&site, target);
        assert_eq!(o.response.status, 200, "{target}");
        assert_eq!(body(&o).len() as u64, len, "{target}");
    }
    let html = format!(
        "<p>{}</p>",
        "a".repeat(usize::try_from(MAX_HTML_BYTES).unwrap() - 7)
    );
    fs::write(dir.join("max.html"), &html).unwrap();
    assert_eq!(html.len() as u64, MAX_HTML_BYTES);
    assert_eq!(get(&site, "/max.html").response.status, 200);
    fs::write(dir.join("max.html"), format!("{html} ")).unwrap();
    assert_404(&get(&site, "/max.html"), "/max.html");
    assert_eq!(MAX_HTML_BYTES, 5 * 1024 * 1024);
    assert_eq!(MAX_FILE_BYTES, 8 * 1024 * 1024);
}

#[test]
fn site_mapping_item7_application_response_and_engine() {
    let (_t, dir) = layout("m7");
    let site = rqp_site(&dir);
    // A non-HTML file goes through the engine unchanged.
    let o = get(&site, "/style.css");
    assert_eq!(o.response.status, 200);
    assert_eq!(
        o.response.fields,
        vec![
            Field::new("Content-Type", "text/css"),
            Field::new("Cache-Control", "no-cache"),
        ]
    );
    assert_eq!(body(&o), b"body{}");
    // The file is read on every request.
    fs::write(dir.join("style.css"), b"p{}").unwrap();
    assert_eq!(body(&get(&site, "/style.css")), b"p{}");
    // An HTML page gets F of RQP: the manifest and Rq-Version.
    fs::write(
        dir.join("page.html"),
        "<!DOCTYPE html><html><head><title>P</title></head><body><p data-rq-block=\"x\">1</p></body></html>",
    )
    .unwrap();
    let o = get(&site, "/page.html?q=1");
    assert_eq!(o.response.status, 200);
    assert_eq!(field(&o, "rq-version"), ["1"]);
    let text = String::from_utf8_lossy(body(&o)).into_owned();
    assert!(
        text.contains(r#""origin":"https://shop.example.com","path":"/page.html""#),
        "{text}"
    );
    assert_eq!(o.log_path, LogPath::Request);
    assert_eq!(o.note, None);
    // The legacy engine with a legacy request: the full response of C-12.
    let legacy = Site::new(
        &dir,
        Engine::Legacy(Profile::Canonical),
        "https://x.example".to_owned(),
    )
    .unwrap();
    let mut h = head("GET", "/a.html");
    h.fields.push(Field::new("accept-diff", "true"));
    let o = legacy.answer(&h);
    assert_eq!(o.response.status, 200);
    assert_eq!(field(&o, "template-change"), ["true"]);
    assert_eq!(body(&o), b"<p>a</p>");
}

#[test]
fn site_mapping_item8_error_bodies_have_no_input_values() {
    let (_t, dir) = layout("m8");
    let site = rqp_site(&dir);
    for target in [
        "/secret-token-1.html",
        "/../secret-token-2",
        "/secret-token-3.md",
    ] {
        let o = get(&site, target);
        assert_404(&o, target);
        assert!(!o.response.fields.iter().any(|f| f.value.contains("secret")));
    }
    // The site mode has no control endpoint (DL-086): its paths are 404.
    for target in ["/__rivqen/conformance/info", "/__rivqen/conformance/case"] {
        assert_404(&get(&site, target), target);
        assert_eq!(site.answer(&head("POST", target)).response.status, 405);
    }
    let o = site.answer(&head("SECRETMETHOD", "/secret-token-4.html"));
    assert_eq!(body(&o), b"Method Not Allowed\n");
    assert!(!o.response.fields.iter().any(|f| f.value.contains("SECRET")));
}

#[test]
fn site_mapping_item9_log_path_only_for_a_file_of_the_site() {
    let (_t, dir) = layout("m9");
    let site = rqp_site(&dir);
    assert_eq!(get(&site, "/a.html").log_path, LogPath::Request);
    assert_eq!(get(&site, "/").log_path, LogPath::Request);
    for target in [
        "/missing.html",
        "/../x",
        "/readme.md",
        "/link-out.html",
        "/empty/",
    ] {
        assert_eq!(get(&site, target).log_path, LogPath::Hidden, "{target}");
    }
    assert_eq!(
        site.answer(&head("POST", "/a.html")).log_path,
        LogPath::Hidden
    );
}
