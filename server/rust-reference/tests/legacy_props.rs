// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Property tests of the legacy engine:
//!
//! 1. `respond` never panics on any page bytes and field lines, in every
//!    profile, and keeps the response contract of `model.rs` (seeds:
//!    `fixtures/malformed/seeds`).
//! 2. A data response rebuilds the page: each placeholder of the template
//!    replaced once by its value gives the body of A (markers §5).
//! 3. The forward scans give the result of the regular expressions of C-06
//!    and C-07. The oracle below is a direct backtracking reading of the two
//!    expressions (quadratic, small inputs only), written apart from the
//!    engine.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use proptest::prelude::*;
use rivqen_reference::legacy::{self, Split, sha1_hex};
use rivqen_reference::model::{AppResponse, Body, Field, Profile, Request, Response};
use serde_json::Value;

// ------------------------------------------------------------------ seeds

fn seed_dir(kind: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/malformed/seeds")
        .join(kind)
}

fn seed_files(kind: &str) -> Vec<Vec<u8>> {
    let mut paths: Vec<_> = fs::read_dir(seed_dir(kind))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    paths.sort();
    paths.into_iter().map(|p| fs::read(p).unwrap()).collect()
}

fn seed_values() -> Vec<String> {
    let mut v: Vec<String> = seed_files("fields")
        .into_iter()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .collect();
    v.extend(
        [
            "true",
            "false",
            "store",
            "http",
            "gzip",
            "identity",
            "no-store",
            "*",
            "",
            "text/html",
            "text/html; charset=utf-8",
        ]
        .map(String::from),
    );
    v
}

const NAMES: [&str; 14] = [
    "accept-diff",
    "Accept-Diff",
    "if-none-match",
    "template-tag",
    "accept-encoding",
    "cache-offline",
    "cache-control",
    "vary",
    "content-type",
    "etag",
    "content-length",
    "transfer-encoding",
    "cdn-cache-control",
    "x-other",
];

// ------------------------------------------------------------------ the regex oracle

fn ci_at(h: &[u8], at: usize, pat: &[u8]) -> bool {
    h.len() >= at + pat.len() && h[at..at + pat.len()].eq_ignore_ascii_case(pat)
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// `<title([^\n]*?)</title>`, leftmost, lazy, ASCII case-insensitive.
fn oracle_title(h: &[u8]) -> Option<(usize, usize)> {
    for o in 0..h.len() {
        if !ci_at(h, o, b"<title") {
            continue;
        }
        let mut c = o + 6;
        loop {
            if ci_at(h, c, b"</title>") {
                return Some((o, c + 8));
            }
            if c >= h.len() || h[c] == b'\n' {
                break;
            }
            c += 1;
        }
    }
    None
}

/// Every parse of `<!--sonicdiff-?(\w*)<tail>` at `at`, in backtracking order
/// (`-?` with the dash first, then `\w*` from longest to shortest).
fn oracle_marker(h: &[u8], at: usize, tail: &[u8]) -> Vec<(usize, usize, usize)> {
    let mut out = Vec::new();
    if !ci_at(h, at, b"<!--sonicdiff") {
        return out;
    }
    let p = at + 13;
    for dash in [true, false] {
        let s = if dash {
            if h.get(p) != Some(&b'-') {
                continue;
            }
            p + 1
        } else {
            p
        };
        let run = h[s.min(h.len())..]
            .iter()
            .take_while(|b| is_word(**b))
            .count();
        for len in (0..=run).rev() {
            if ci_at(h, s + len, tail) {
                out.push((s, s + len, s + len + tail.len()));
            }
        }
    }
    out
}

/// `<!--sonicdiff-?(\w*)-->([\s\S]+?)<!--sonicdiff-?(\w*)-end-->`, global.
fn oracle_blocks(h: &[u8]) -> Vec<(usize, usize, usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    'scan: while i < h.len() {
        for (ns, ne, oe) in oracle_marker(h, i, b"-->") {
            for k in oe + 1..h.len() {
                if let Some(&(_, _, end)) = oracle_marker(h, k, b"-end-->").first() {
                    out.push((i, ns, ne, end));
                    i = end;
                    continue 'scan;
                }
            }
        }
        i += 1;
    }
    out
}

/// The split of legacy wire C-06…C-08 and P-09, P-10 over the oracle matches.
fn oracle_split(profile: Profile, text: &str) -> Option<Split> {
    let (stage, title) = match oracle_title(text.as_bytes()) {
        Some((s, e)) => (
            format!("{}{{title}}{}", &text[..s], &text[e..]),
            text[s..e].to_owned(),
        ),
        None => (text.to_owned(), String::new()),
    };
    let mut data: Vec<(String, String)> = vec![("{title}".into(), title)];
    let mut template = String::new();
    let mut last = 0;
    let mut auto = 0;
    let mut blocks = 0;
    for (start, ns, ne, end) in oracle_blocks(stage.as_bytes()) {
        let name = &stage[ns..ne];
        let key = match profile {
            Profile::Canonical if name.is_empty() || name.len() > 128 => continue,
            Profile::Node if name.is_empty() => {
                auto += 1;
                format!("{{auto{}}}", auto - 1)
            }
            _ => format!("{{{name}}}"),
        };
        let value = stage[start..end].to_owned();
        match data.iter_mut().find(|(k, _)| *k == key) {
            Some(_) if profile == Profile::Canonical => return None,
            Some(slot) => slot.1 = value,
            None => data.push((key.clone(), value)),
        }
        template.push_str(&stage[last..start]);
        template.push_str(&key);
        last = end;
        blocks += 1;
    }
    template.push_str(&stage[last..]);
    Some(Split {
        template,
        data,
        blocks,
    })
}

const TOKENS: [&str; 30] = [
    "<title",
    "<TITLE>",
    "<title>",
    "</title>",
    "</TiTlE>",
    "\n",
    "\r",
    "x",
    "é",
    "<!--sonicdiff",
    "<!--SONICDIFF",
    "<!--sonicdiff-",
    "-",
    "--",
    "-->",
    "-end-->",
    "-END-->",
    "a",
    "b",
    "_",
    "title",
    "end",
    " ",
    "<",
    ">",
    "{",
    "}",
    "<!--sonicdiff-a-->",
    "<!--sonicdiff-a-end-->",
    "<!--sonicdiff-->",
];

fn token_text() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(&TOKENS[..]), 0..40).prop_map(|t| t.concat())
}

#[test]
fn legacy_split_equals_the_expressions_on_the_seed_pages() {
    let mut checked = 0;
    for page in seed_files("pages") {
        let Ok(text) = std::str::from_utf8(&page) else {
            continue;
        };
        if text.len() > 64 * 1024 {
            continue; // the oracle is quadratic
        }
        for p in Profile::ALL {
            assert_eq!(
                legacy::split(p, text),
                oracle_split(p, text),
                "{}",
                p.name()
            );
        }
        checked += 1;
    }
    assert!(checked >= 20, "{checked}");
}

// ------------------------------------------------------------------ properties

fn assert_contract(req: &Request, app: &AppResponse, r: &Response) {
    for f in &r.fields {
        assert!(
            !f.is("content-length") && !f.is("transfer-encoding") && !f.is("connection"),
            "{f:?}"
        );
    }
    // A legacy 304 has no body (C-05). A 304 of the application is the
    // pass-through of model.rs; the writer drops its body.
    if r.status == 304 && app.status != 304 {
        assert_eq!(r.body, Body::None);
    }
    if r.fields
        .iter()
        .any(|f| f.is("template-change") && f.value == "false")
        && req.method != "HEAD"
    {
        let Body::Bytes(b) = &r.body else {
            panic!("data without body")
        };
        let v: Value = serde_json::from_slice(b).expect("data body is JSON");
        assert_eq!(v.as_object().map(serde_json::Map::len), Some(4));
    }
}

fn page_strategy(seeds: Vec<Vec<u8>>) -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        prop::sample::select(seeds),
        token_text().prop_map(String::into_bytes),
        prop::collection::vec(any::<u8>(), 0..200),
        (token_text(), prop::collection::vec(any::<u8>(), 0..4)).prop_map(|(t, b)| {
            let mut v = t.into_bytes();
            v.extend(b);
            v
        }),
    ]
}

fn field_strategy(values: Vec<String>) -> impl Strategy<Value = Vec<Field>> {
    let value = prop_oneof![
        prop::sample::select(values),
        ".{0,20}",
        "[ \t]*(true|gzip|W/\"?[0-9a-f]{0,40}\"?)[ \t]*"
    ];
    prop::collection::vec((prop::sample::select(&NAMES[..]), value), 0..8)
        .prop_map(|v| v.into_iter().map(|(n, v)| Field::new(n, &v)).collect())
}

/// 512 cases by default; `PROPTEST_CASES` raises it (the long runs of the sprint close).
fn cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(512)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: cases(), ..ProptestConfig::default() })]

    #[test]
    fn legacy_prop_respond_never_panics(
        page in page_strategy(seed_files("pages")),
        req_fields in field_strategy(seed_values()),
        app_fields in field_strategy(seed_values()),
        html in any::<bool>(),
        method in prop::sample::select(&["GET", "HEAD", "POST", "get"][..]),
        status in prop::sample::select(&[200u16, 304, 404][..]),
    ) {
        let mut fields = app_fields;
        if html {
            fields.insert(0, Field::new("content-type", "text/html; charset=utf-8"));
        }
        let app = AppResponse { status, fields, body: page };
        let req = Request { method: method.into(), url: "https://shop.example.com/".into(), fields: req_fields };
        for p in Profile::ALL {
            let r = legacy::respond(p, &req, &app);
            assert_contract(&req, &app, &r);
        }
    }

    #[test]
    fn legacy_prop_scan_equals_the_expressions(text in token_text()) {
        for p in Profile::ALL {
            prop_assert_eq!(legacy::split(p, &text), oracle_split(p, &text), "{}", p.name());
        }
    }

    #[test]
    fn legacy_prop_data_response_rebuilds_the_page(page in structured_page()) {
        let app = AppResponse {
            status: 200,
            fields: vec![Field::new("content-type", "text/html; charset=utf-8")],
            body: page.clone().into_bytes(),
        };
        for p in Profile::ALL {
            let s = legacy::split(p, &page).unwrap();
            let tag = sha1_hex(s.template.as_bytes());
            let req = Request {
                method: "GET".into(),
                url: "https://shop.example.com/".into(),
                fields: vec![
                    Field::new("accept-encoding", "gzip"),
                    Field::new("accept-diff", "true"),
                    Field::new("template-tag", &tag),
                ],
            };
            let r = legacy::respond(p, &req, &app);
            if p == Profile::Node && s.blocks == 0 {
                prop_assert_eq!(&r.body, &Body::Bytes(app.body.clone()));
                continue;
            }
            let Body::Bytes(b) = &r.body else { panic!("no body") };
            let v: Value = serde_json::from_slice(b).unwrap();
            let data: HashMap<&str, &str> = v["data"].as_object().unwrap().iter().map(|(k, v)| (k.as_str(), v.as_str().unwrap())).collect();
            let rebuilt = rebuild(&s.template, &data);
            prop_assert_eq!(&rebuilt, &page, "{}", p.name());
            prop_assert_eq!(v["html-sha1"].as_str().unwrap(), sha1_hex(rebuilt.as_bytes()));
        }
    }
}

/// Markers §5: each placeholder of the template, in template order, replaced
/// once by its value. The page text has no `{`, so each `{…}` of the template
/// is a placeholder.
fn rebuild(template: &str, data: &HashMap<&str, &str>) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let close = open + rest[open..].find('}').unwrap();
        let key = &rest[open..=close];
        out.push_str(
            data.get(key)
                .unwrap_or_else(|| panic!("no value for {key}")),
        );
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
}

/// A page of plain text, at most one title, and blocks with distinct names
/// (one may be empty); the text has no `{`, `}` or `<`.
fn structured_page() -> impl Strategy<Value = String> {
    let text = "[a-z \n\r\t>é!-]{0,12}";
    let title = (
        prop::sample::select(&["<title>", "<TITLE lang=x>", "<title"][..]),
        "[a-z \r>é-]{0,8}",
        any::<bool>(),
    )
        .prop_map(|(open, body, lf)| format!("{open}{body}{}</title>", if lf { "\n" } else { "" }));
    let block = (
        any::<bool>(),
        "[a-z \n>é-]{1,12}",
        prop::sample::select(&["-end-->", "-END-->"][..]),
        any::<bool>(),
    );
    (
        text,
        prop::option::of(title),
        prop::collection::vec((block, "[a-z \n>é!-]{0,6}"), 0..6),
    )
        .prop_map(|(head, title, blocks)| {
            let mut page = head;
            if let Some(t) = title {
                page.push_str(&t);
            }
            let mut had_empty = false;
            for (i, ((empty, content, end, upper), gap)) in blocks.into_iter().enumerate() {
                let name = if empty && !had_empty {
                    had_empty = true;
                    String::new()
                } else {
                    format!("n{i}")
                };
                let open = if upper {
                    "<!--SONICDIFF-"
                } else {
                    "<!--sonicdiff-"
                };
                page.push_str(
                    &[
                        open,
                        &name,
                        "-->",
                        &content,
                        "<!--sonicdiff-",
                        &name,
                        end,
                        &gap,
                    ]
                    .concat(),
                );
            }
            page
        })
}
