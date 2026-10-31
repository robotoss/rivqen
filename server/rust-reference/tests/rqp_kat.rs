// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Known-answer tests from the examples of manifest and patch §1.1, §1.3, §2.1,
//! §2.2 and negotiation §9 (`spec/rqp/examples/kat/`, `kat.json`).

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
use rivqen_reference::rqp::select::manifest::{BlockLine, binding, page_revision};
use serde_json::{Value, json};

const URL: &str = "https://shop.example.com/catalog";
const R_A: &str = "r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8";
const R_B: &str = "r1.psbsc6whgs8ooESisNWp6E3PrQfkg4D69WmXAo12pbI";
const T: &str = "t1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM";
const HAVE: &str =
    "t=t1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM, b=:0Aqua3+/d6ReDehqKiXfLM8hC9ypGZS5:";
const NONCE: &str = ":AAECAwQFBgcICQoLDA0ODw==:";
const NONCE_BYTES: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/rqp/examples")
}

fn read(path: &str) -> Vec<u8> {
    fs::read(examples().join(path)).unwrap()
}

fn read_json(path: &str) -> Value {
    serde_json::from_slice(&read(path)).unwrap()
}

fn page_a() -> Vec<u8> {
    read("kat/page.html")
}

/// Page B of negotiation §9: page A with `130.00` in place of `120.00`.
fn page_b() -> Vec<u8> {
    String::from_utf8(page_a())
        .unwrap()
        .replace("120.00", "130.00")
        .into_bytes()
}

fn app(body: Vec<u8>) -> AppResponse {
    AppResponse {
        status: 200,
        fields: vec![
            Field::new("Content-Type", "text/html; charset=utf-8"),
            Field::new("Cache-Control", "no-cache"),
        ],
        body,
    }
}

fn get(url: &str, fields: &[(&str, &str)]) -> Request {
    Request {
        method: "GET".to_owned(),
        url: url.to_owned(),
        fields: fields.iter().map(|(n, v)| Field::new(n, v)).collect(),
    }
}

/// The request of negotiation §9.2 (a snapshot of A).
fn delta_request(url: &str) -> Request {
    get(
        url,
        &[
            ("Accept", "text/html, application/vnd.rivqen.delta+json"),
            ("Rq-Version", "1"),
            ("Rq-Capabilities", "(block-patch)"),
            ("If-None-Match", &format!("\"{R_A}\"")),
            ("Rq-Have", HAVE),
            ("Rq-Nonce", NONCE),
        ],
    )
}

fn value(r: &Response, name: &str) -> Option<String> {
    let v: Vec<&str> = r
        .fields
        .iter()
        .filter(|f| f.is(name))
        .map(|f| f.value.as_str())
        .collect();
    (!v.is_empty()).then(|| v.join(", "))
}

fn body(r: &Response) -> &[u8] {
    match &r.body {
        Body::Bytes(b) => b,
        Body::None => panic!("no body"),
    }
}

#[test]
fn rqp_kat_full_document_of_set_1() {
    // Negotiation §9.1; manifest and patch §1.1 and §1.7 (kat/full.html).
    let config = RqpConfig::default();
    let first = get(
        URL,
        &[
            ("Accept", "text/html"),
            ("Rq-Version", "1"),
            ("Rq-Capabilities", "(block-patch)"),
        ],
    );
    let r = respond(&config, &first, &app(page_a()));
    assert_eq!(r.status, 200);
    assert_eq!(
        value(&r, "content-type").as_deref(),
        Some("text/html; charset=utf-8")
    );
    assert_eq!(value(&r, "etag"), Some(format!("\"{R_A}\"")));
    assert_eq!(value(&r, "rq-version").as_deref(), Some("1"));
    assert_eq!(value(&r, "cache-control").as_deref(), Some("no-cache"));
    assert_eq!(body(&r), read("kat/full.html").as_slice());
    // The manifest is the value of valid/manifest/shop.json.
    let text = String::from_utf8(body(&r).to_vec()).unwrap();
    let m = text
        .split("+json\">")
        .nth(1)
        .unwrap()
        .split("</script>")
        .next()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(m).unwrap(),
        read_json("valid/manifest/shop.json")
    );
    // A browser without RQP gets the same response (H-51).
    assert_eq!(respond(&config, &get(URL, &[]), &app(page_a())), r);
}

#[test]
fn rqp_kat_full_document_of_set_2_with_policy_and_limits() {
    // Manifest and patch §1.3, second set (kat/full-policy.html).
    let config = RqpConfig::from_json(&json!({
        "policy": {"ttl_seconds": 60, "offline": true},
        "limits": {"max_patch_bytes": 65536}
    }))
    .unwrap();
    let r = respond(&config, &get(URL, &[]), &app(page_a()));
    assert_eq!(
        value(&r, "etag").as_deref(),
        Some("\"r1.pONNMoySlMlYw50BpgdvbUlGESnMzZBCOmQ4SlebDxs\"")
    );
    assert_eq!(body(&r), read("kat/full-policy.html").as_slice());
    assert_eq!(
        config.policy_line_text().as_deref(),
        Some("policy;ttl_seconds=60;offline=true;limits;max_patch_bytes=65536")
    );
}

#[test]
fn rqp_kat_page_revision_preimages_of_1_3() {
    let lines = [
        BlockLine {
            id: "title",
            format: "html",
            sha256: "0Aqua3-_d6RWbY3zaAXXrpc4lNqfUz7JZRazHgKLces",
        },
        BlockLine {
            id: "price",
            format: "html",
            sha256: "Xg3oaiol3ywUdxTQZNp7JJJu7_R8kUL-HK4accfrDKs",
        },
        BlockLine {
            id: "cart",
            format: "json",
            sha256: "zyEL3KkZlLmAgTTFnonr3pdX-DnDM81i9xFoREUEaX4",
        },
    ];
    assert_eq!(page_revision(T, None, &lines), R_A);
    let pl = "policy;ttl_seconds=60;offline=true;limits;max_patch_bytes=65536";
    assert_eq!(
        page_revision(T, Some(pl), &lines),
        "r1.pONNMoySlMlYw50BpgdvbUlGESnMzZBCOmQ4SlebDxs"
    );
    let mut b = lines;
    b[1].sha256 = "bB9ScYcQQzKWqPCBW62MTsc9Ff_djve7Denf3YMoCUg";
    assert_eq!(page_revision(T, None, &b), R_B);
    assert_eq!(
        page_revision(T, Some(pl), &b),
        "r1.kX-KQFiXmNtLNzjvXP7GR3K-_79Fc1l4WEtReCgqwQQ"
    );
}

#[test]
fn rqp_kat_bindings_of_kat_json() {
    let kat = read_json("kat/kat.json");
    let mut all = kat["bindings"].as_array().unwrap().clone();
    all.extend(
        kat["with_policy_limits"]["bindings"]
            .as_array()
            .unwrap()
            .iter()
            .cloned(),
    );
    all.push(kat["binding_with_query"].clone());
    for b in &all {
        assert_eq!(b["nonce"], "AAECAwQFBgcICQoLDA0ODw");
        let got = binding(
            b["url"].as_str().unwrap(),
            b["base_revision"].as_str().unwrap(),
            &NONCE_BYTES,
            b["next_revision"].as_str().unwrap(),
        );
        assert_eq!(got, b["binding"].as_str().unwrap());
    }
}

#[test]
fn rqp_kat_not_modified_of_9_2() {
    let r = respond(&RqpConfig::default(), &delta_request(URL), &app(page_a()));
    assert_eq!(r.status, 304);
    assert_eq!(r.body, Body::None);
    assert_eq!(value(&r, "etag"), Some(format!("\"{R_A}\"")));
    assert_eq!(value(&r, "cache-control").as_deref(), Some("no-cache"));
    assert_eq!(value(&r, "content-type"), None);
}

#[test]
fn rqp_kat_delta_of_9_3_is_shop_price() {
    let r = respond(&RqpConfig::default(), &delta_request(URL), &app(page_b()));
    assert_eq!(r.status, 200);
    assert_eq!(
        value(&r, "content-type").as_deref(),
        Some("application/vnd.rivqen.delta+json")
    );
    assert_eq!(
        value(&r, "x-content-type-options").as_deref(),
        Some("nosniff")
    );
    assert_eq!(value(&r, "cache-control").as_deref(), Some("no-store"));
    assert_eq!(value(&r, "rq-version").as_deref(), Some("1"));
    assert_eq!(value(&r, "vary").as_deref(), Some("Rq-Nonce"));
    assert_eq!(value(&r, "etag"), None);
    let d: Value = serde_json::from_slice(body(&r)).unwrap();
    assert_eq!(d, read_json("valid/patch/shop-price.json"));
}

#[test]
fn rqp_kat_all_blocks_delta_without_b_is_shop_all() {
    // Manifest and patch §2.5 item 2 (H-7): without `b`, every block.
    let page = String::from_utf8(page_a())
        .unwrap()
        .replace(">Shop<", "><")
        .replace("120.00", "130.00")
        .replace("{\"items\":3}", "{\"items\":4}")
        .into_bytes();
    let mut request = delta_request(URL);
    for f in &mut request.fields {
        if f.is("rq-have") {
            f.value = format!("t={T}");
        }
    }
    let r = respond(&RqpConfig::default(), &request, &app(page));
    let d: Value = serde_json::from_slice(body(&r)).unwrap();
    assert_eq!(d, read_json("valid/patch/shop-all.json"));
}

#[test]
fn rqp_kat_delta_with_policy_is_shop_policy_price() {
    let config = RqpConfig::from_json(&json!({
        "policy": {"ttl_seconds": 60, "offline": true},
        "limits": {"max_patch_bytes": 65536}
    }))
    .unwrap();
    let mut request = delta_request(URL);
    for f in &mut request.fields {
        if f.is("if-none-match") {
            f.value = "\"r1.pONNMoySlMlYw50BpgdvbUlGESnMzZBCOmQ4SlebDxs\"".to_owned();
        }
    }
    let r = respond(&config, &request, &app(page_b()));
    let d: Value = serde_json::from_slice(body(&r)).unwrap();
    assert_eq!(d, read_json("valid/patch/shop-policy-price.json"));
}

#[test]
fn rqp_kat_delta_binding_with_query() {
    // kat.json binding_with_query: U is the WHATWG serialization without the
    // fragment.
    let kat = read_json("kat/kat.json");
    let q = &kat["binding_with_query"];
    let r = respond(
        &RqpConfig::default(),
        &delta_request(q["url_input"].as_str().unwrap()),
        &app(page_b()),
    );
    let d: Value = serde_json::from_slice(body(&r)).unwrap();
    assert_eq!(d["binding"], q["binding"]);
}

#[test]
fn rqp_kat_invalid_page_of_9_4() {
    // A second block `price` (M-21): the application response unchanged, the
    // Rq-* field of the application removed (DL-030).
    let page = String::from_utf8(page_a())
        .unwrap()
        .replace("<header>", "<p data-rq-block=\"price\">1</p><header>")
        .into_bytes();
    let mut a = app(page.clone());
    a.fields.push(Field::new("Rq-Version", "1"));
    let r = respond(&RqpConfig::default(), &get(URL, &[("Rq-Version", "1")]), &a);
    assert_eq!(r.status, 200);
    assert_eq!(r.body, Body::Bytes(page));
    assert_eq!(
        r.fields,
        vec![
            Field::new("Content-Type", "text/html; charset=utf-8"),
            Field::new("Cache-Control", "no-cache"),
        ]
    );
}
