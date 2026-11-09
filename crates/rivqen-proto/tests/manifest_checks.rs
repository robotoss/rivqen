// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Manifest checks of manifest-patch section 1.8, items 1, 2 and 4, against
//! the schema examples of `spec/rqp/examples`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::arithmetic_side_effects
)]

use std::fs;
use std::path::{Path, PathBuf};

use rivqen_integrity::PageRevision;
use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_proto::json::{JsonRule, SchemaKeyword};
use rivqen_proto::manifest::{ManifestError, check_url, parse_manifest, read_manifest};
use rivqen_proto::types::{Limits, MAX_MANIFEST_BYTES, Mode, Policy};

const SHOP_REVISION: &str = "r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8";

fn examples(kind: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/rqp/examples")
        .join(kind)
        .join("manifest")
}

fn json_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    files
}

fn manifest(members: &str) -> String {
    format!(
        r#"{{"protocol":"rqp/1","origin":"https://shop.example.com","path":"/catalog","page_revision":"{SHOP_REVISION}"{members}}}"#
    )
}

fn schema_error(text: &str) -> (SchemaKeyword, String) {
    match parse_manifest(text.as_bytes()) {
        Err(ManifestError::Schema(v)) => (v.keyword(), v.instance_path()),
        other => panic!("schema error expected, got {other:?}"),
    }
}

#[test]
fn manifest_examples() {
    let valid = json_files(&examples("valid"));
    assert_eq!(valid.len(), 5);
    for file in &valid {
        let bytes = fs::read(file).unwrap();
        let m = parse_manifest(&bytes).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        assert_eq!(m.page_revision.to_string().len(), 46);
    }

    // The two numbers that rule 4 of section 3.2 rejects before the schema.
    let json_rule_first = ["policy-ttl-fraction.json", "policy-ttl-negative.json"];
    let invalid = json_files(&examples("invalid"));
    assert_eq!(invalid.len(), 35);
    for file in &invalid {
        let name = file.file_name().unwrap().to_str().unwrap();
        let wrapper: serde_json::Value = serde_json::from_slice(&fs::read(file).unwrap()).unwrap();
        // Every defect of these instances survives a serde_json round trip:
        // no duplicate names, and serde_json writes -1 and 1.5 as they are.
        let text = serde_json::to_vec(&wrapper["instance"]).unwrap();
        let error = parse_manifest(&text).expect_err(name);
        assert_eq!(error.code(), ErrorCode::ManifestInvalid, "{name}");
        if json_rule_first.contains(&name) {
            assert_eq!(error, ManifestError::Json(json_error(&text)), "{name}");
            continue;
        }
        let ManifestError::Schema(v) = error else {
            panic!("{name}: schema error expected, got {error:?}");
        };
        assert_eq!(
            v.keyword().as_str(),
            wrapper["expect"]["keyword"].as_str().unwrap(),
            "{name}"
        );
        assert_eq!(
            v.instance_path(),
            wrapper["expect"]["instancePath"].as_str().unwrap(),
            "{name}"
        );
    }
}

fn json_error(text: &[u8]) -> rivqen_proto::json::JsonError {
    let e = rivqen_proto::json::parse(text).unwrap_err();
    assert_eq!(e.rule(), JsonRule::Integer);
    e
}

#[test]
fn manifest_reads_the_members() {
    let path = examples("valid").join("bounds.json");
    let m = parse_manifest(&fs::read(path).unwrap()).unwrap();
    assert_eq!(m.origin, "http://[::1]:8080");
    assert_eq!(m.path, "/a/b%20c/;x=1/~!$&'()*+,=@:_.-");
    assert_eq!(
        m.page_revision,
        PageRevision::parse("r1.GKw-c0PwFokMUQ6T-TUmEWnZ4_VlQ2Qpgw-vCTT0-OQ").unwrap()
    );
    assert_eq!(
        m.policy,
        Some(Policy {
            mode: Some(Mode::Public),
            ttl_seconds: Some(31_536_000),
            offline: Some(true),
        })
    );
    assert_eq!(
        m.limits,
        Some(Limits {
            max_patch_bytes: Some(1024)
        })
    );
}

#[test]
fn manifest_policy_presence() {
    // Absent members.
    let m = parse_manifest(manifest("").as_bytes()).unwrap();
    assert_eq!((m.policy, m.limits), (None, None));
    // Empty objects are present and have no members.
    let m = parse_manifest(manifest(r#","policy":{},"limits":{}"#).as_bytes()).unwrap();
    assert_eq!(m.policy, Some(Policy::default()));
    assert_eq!(m.limits, Some(Limits::default()));
    // Default members are accepted and stay present.
    let m = parse_manifest(
        manifest(r#","policy":{"mode":"private","ttl_seconds":0,"offline":false}"#).as_bytes(),
    )
    .unwrap();
    assert_eq!(
        m.policy,
        Some(Policy {
            mode: Some(Mode::Private),
            ttl_seconds: Some(0),
            offline: Some(false),
        })
    );
    // One member: only that member is present.
    let m = parse_manifest(manifest(r#","policy":{"offline":true}"#).as_bytes()).unwrap();
    assert_eq!(
        m.policy,
        Some(Policy {
            offline: Some(true),
            ..Policy::default()
        })
    );
    // The bounds of the ranges.
    let m =
        parse_manifest(manifest(r#","limits":{"max_patch_bytes":5242880}"#).as_bytes()).unwrap();
    assert_eq!(m.limits.unwrap().max_patch_bytes, Some(5_242_880));
}

#[test]
fn manifest_size_limit() {
    // At the limit: white space between tokens is allowed (section 3.2).
    let mut text = manifest("").into_bytes();
    text.resize(MAX_MANIFEST_BYTES, b' ');
    assert!(parse_manifest(&text).is_ok());
    // One byte more fails before the JSON parse.
    text.push(b' ');
    assert_eq!(parse_manifest(&text), Err(ManifestError::TooLarge));
    // A text that is not JSON gives the size error too: the size comes first.
    let garbage = vec![0xff_u8; MAX_MANIFEST_BYTES + 1];
    let e = parse_manifest(&garbage).unwrap_err();
    assert_eq!(e, ManifestError::TooLarge);
    assert_eq!(e.code(), ErrorCode::ManifestInvalid);
}

#[test]
fn manifest_url_mismatch() {
    let text = manifest("");
    let m = read_manifest(text.as_bytes(), "https://shop.example.com", "/catalog").unwrap();
    assert_eq!(m.path, "/catalog");
    for (origin, path) in [
        ("https://shop.example.com:443", "/catalog"),
        ("http://shop.example.com", "/catalog"),
        ("https://SHOP.example.com", "/catalog"),
        ("https://shop.example.com", "/catalog/"),
        ("https://shop.example.com", "/Catalog"),
        ("https://shop.example.com", "/catalog?x=1"),
        ("https://shop.example.co", "/catalog"),
        ("https://shop.example.com", ""),
    ] {
        assert_eq!(
            read_manifest(text.as_bytes(), origin, path),
            Err(ManifestError::UrlMismatch),
            "{origin} {path}"
        );
    }
    assert_eq!(
        check_url(&m, "https://shop.example.com", "/catalog"),
        Ok(())
    );
    assert_eq!(
        check_url(&m, "https://shop.example.com", "/other")
            .unwrap_err()
            .code(),
        ErrorCode::ManifestInvalid
    );
    // The size, JSON and schema checks come before the URL check.
    assert_eq!(
        read_manifest(b"[]", "https://shop.example.com", "/other"),
        Err(parse_manifest(b"[]").unwrap_err())
    );
}

#[test]
fn manifest_old_form() {
    let blocks = r#","blocks":[{"id":"price","format":"html","sha256":"Xg3oaiol3ywUdxTQZNp7JJJu7_R8kUL-HK4accfrDKs"}]"#;
    let template = r#","template_revision":"t1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM""#;
    for members in [
        blocks,
        template,
        r#","page_id":"catalog""#,
        r#","sequence":1"#,
    ] {
        assert_eq!(
            schema_error(&manifest(members)),
            (SchemaKeyword::AdditionalProperties, String::new()),
            "{members}"
        );
    }
}

#[test]
fn manifest_json_rules_apply() {
    let duplicate = manifest(r#","origin":"https://shop.example.com""#);
    let e = parse_manifest(duplicate.as_bytes()).unwrap_err();
    assert!(matches!(e, ManifestError::Json(j) if j.rule() == JsonRule::Duplicate));
    let nested = manifest(r#","policy":{"offline":true,"offline":true}"#);
    let e = parse_manifest(nested.as_bytes()).unwrap_err();
    assert!(matches!(e, ManifestError::Json(j) if j.rule() == JsonRule::Duplicate));
    let mut bom = "\u{feff}".as_bytes().to_vec();
    bom.extend_from_slice(manifest("").as_bytes());
    let e = parse_manifest(&bom).unwrap_err();
    assert!(matches!(e, ManifestError::Json(j) if j.rule() == JsonRule::Bom));
    for ttl in ["60.0", "6e1"] {
        let text = manifest(&format!(r#","policy":{{"ttl_seconds":{ttl}}}"#));
        let e = parse_manifest(text.as_bytes()).unwrap_err();
        assert!(
            matches!(e, ManifestError::Json(j) if j.rule() == JsonRule::Integer),
            "{ttl}"
        );
    }
    // Text after the object.
    let e = parse_manifest(format!("{} {{}}", manifest("")).as_bytes()).unwrap_err();
    assert!(matches!(e, ManifestError::Json(j) if j.rule() == JsonRule::Syntax));
    // An escaped member name is the same member.
    // `~` stands for a backslash: "p~u0061th" is the JSON text of "path".
    let escaped = manifest("").replace(r#""path""#, &r#""p~u0061th""#.replace('~', "\\"));
    assert!(escaped.contains(r"p\u0"));
    assert_eq!(parse_manifest(escaped.as_bytes()).unwrap().path, "/catalog");
}

#[test]
fn manifest_origin_follows_the_schema_pattern() {
    let origin = |o: &str| {
        format!(
            r#"{{"protocol":"rqp/1","origin":"{o}","path":"/","page_revision":"{SHOP_REVISION}"}}"#
        )
    };
    for good in [
        "https://a",
        "http://a.b-c_d.0",
        "https://[::1]",
        "https://[0:a.f]:1",
        "https://shop.example.com:99999",
        "https://x:10",
    ] {
        assert!(parse_manifest(origin(good).as_bytes()).is_ok(), "{good}");
    }
    for bad in [
        "https://",
        "https:/a",
        "HTTPS://a",
        "https://a:",
        "https://a:0",
        "https://a:123456",
        "https://a:1a",
        "https://[]",
        "https://[::G]",
        "https://[::1",
        "https://[::1]x",
        "https://a b",
        "https://a/",
        "wss://a",
        "https://é",
    ] {
        assert_eq!(
            schema_error(&origin(bad)),
            (SchemaKeyword::Pattern, "/origin".to_owned()),
            "{bad}"
        );
    }
    let long = format!("https://{}", "a".repeat(512 - 8));
    assert!(parse_manifest(origin(&long).as_bytes()).is_ok());
    let longer = format!("https://{}", "a".repeat(512 - 7));
    assert_eq!(
        schema_error(&origin(&longer)),
        (SchemaKeyword::MaxLength, "/origin".to_owned())
    );
}

#[test]
fn manifest_path_follows_the_schema_pattern() {
    let path = |p: &str| {
        format!(
            r#"{{"protocol":"rqp/1","origin":"https://a","path":"{p}","page_revision":"{SHOP_REVISION}"}}"#
        )
    };
    assert!(parse_manifest(path(&format!("/{}", "a".repeat(4095))).as_bytes()).is_ok());
    assert_eq!(
        schema_error(&path(&format!("/{}", "a".repeat(4096)))),
        (SchemaKeyword::MaxLength, "/path".to_owned())
    );
    assert_eq!(
        schema_error(&path("")),
        (SchemaKeyword::MinLength, "/path".to_owned())
    );
    assert!(parse_manifest(path("/^`{}|").as_bytes()).is_ok());
    for bad in ["/a>b", "/a\\\"b", "/a\\u007f", "/a<", "/?", "/#", "a/"] {
        assert_eq!(
            schema_error(&path(bad)),
            (SchemaKeyword::Pattern, "/path".to_owned()),
            "{bad}"
        );
    }
}

#[test]
fn manifest_members_follow_the_schema() {
    for (members, keyword, at) in [
        (r#","policy":[]"#, SchemaKeyword::Type, "/policy"),
        (r#","limits":null"#, SchemaKeyword::Type, "/limits"),
        (
            r#","policy":{"mode":1}"#,
            SchemaKeyword::Enum,
            "/policy/mode",
        ),
        (
            r#","policy":{"mode":"Public"}"#,
            SchemaKeyword::Enum,
            "/policy/mode",
        ),
        (
            r#","policy":{"ttl_seconds":31536001}"#,
            SchemaKeyword::Maximum,
            "/policy/ttl_seconds",
        ),
        (
            r#","policy":{"ttl_seconds":true}"#,
            SchemaKeyword::Type,
            "/policy/ttl_seconds",
        ),
        (
            r#","policy":{"offline":0}"#,
            SchemaKeyword::Type,
            "/policy/offline",
        ),
        (
            r#","limits":{"max_patch_bytes":1023}"#,
            SchemaKeyword::Minimum,
            "/limits/max_patch_bytes",
        ),
        (
            r#","limits":{"max_patch_bytes":5242881}"#,
            SchemaKeyword::Maximum,
            "/limits/max_patch_bytes",
        ),
        (
            r#","limits":{"max_patch_bytes":"2048"}"#,
            SchemaKeyword::Type,
            "/limits/max_patch_bytes",
        ),
        (
            r#","limits":{"max_patch_bytes":18446744073709551615}"#,
            SchemaKeyword::Maximum,
            "/limits/max_patch_bytes",
        ),
    ] {
        assert_eq!(
            schema_error(&manifest(members)),
            (keyword, at.to_owned()),
            "{members}"
        );
    }
    assert!(parse_manifest(manifest(r#","limits":{"max_patch_bytes":1024}"#).as_bytes()).is_ok());
    assert!(parse_manifest(manifest(r#","policy":{"ttl_seconds":31536000}"#).as_bytes()).is_ok());
}

#[test]
fn manifest_requires_its_members_with_their_types() {
    let no_protocol = manifest("").replace(r#""protocol":"rqp/1","#, "");
    assert_eq!(
        schema_error(&no_protocol),
        (SchemaKeyword::Required, String::new())
    );
    let no_origin = manifest("").replace(r#""origin":"https://shop.example.com","#, "");
    assert_eq!(
        schema_error(&no_origin),
        (SchemaKeyword::Required, String::new())
    );
    let no_path = manifest("").replace(r#""path":"/catalog","#, "");
    assert_eq!(
        schema_error(&no_path),
        (SchemaKeyword::Required, String::new())
    );
    assert_eq!(schema_error("{}"), (SchemaKeyword::Required, String::new()));
    for (from, to, keyword, at) in [
        (
            r#""origin":"https"#,
            r#""origin":["https"#,
            SchemaKeyword::Type,
            "/origin",
        ),
        (
            r#""path":"/catalog""#,
            r#""path":7"#,
            SchemaKeyword::Type,
            "/path",
        ),
        (
            r#""page_revision":""#,
            r#""page_revision":{"a":""#,
            SchemaKeyword::Type,
            "/page_revision",
        ),
        (
            r#""protocol":"rqp/1""#,
            r#""protocol":"RQP/1""#,
            SchemaKeyword::Const,
            "/protocol",
        ),
        (
            r#""protocol":"rqp/1""#,
            r#""protocol":null"#,
            SchemaKeyword::Const,
            "/protocol",
        ),
    ] {
        let text = manifest("").replacen(from, to, 1);
        assert_eq!(schema_error(&text), (keyword, at.to_owned()), "{to}");
    }
    assert_eq!(
        schema_error("\"rqp/1\""),
        (SchemaKeyword::Type, String::new())
    );
}

#[test]
fn manifest_errors_hold_no_content() {
    let marker = "SECRETMARKER";
    let texts = [
        manifest(&format!(r#","{marker}":1"#)),
        manifest("").replace("/catalog", &format!("/{marker} x")),
        manifest(&format!(r#","policy":{{"mode":"{marker}"}}"#)),
    ];
    for text in &texts {
        let e = parse_manifest(text.as_bytes()).unwrap_err();
        assert!(!format!("{e} {e:?}").contains(marker), "{e}");
    }
    let e = read_manifest(manifest("").as_bytes(), marker, marker).unwrap_err();
    assert!(!format!("{e} {e:?}").contains(marker));
    assert_eq!(
        e.to_string(),
        "MANIFEST_INVALID: manifest origin or path is not the final URL"
    );
    let e = parse_manifest(br#"{"protocol":1.5}"#).unwrap_err();
    assert!(core::error::Error::source(&e).is_some());
    assert_eq!(
        e.to_string(),
        "MANIFEST_INVALID: manifest text breaks a JSON rule: number is not an integer of digits only (manifest-patch 3.2 rule 4) at byte 12"
    );
    // The check stops at the first failure in text order: here the schema
    // type of the top-level value, before the number in it.
    assert!(matches!(
        parse_manifest(b"[1.5]"),
        Err(ManifestError::Schema(_))
    ));
    let e = parse_manifest(b"[]").unwrap_err();
    assert_eq!(
        e.to_string(),
        "MANIFEST_INVALID: manifest fails the schema: schema keyword type fails at ''"
    );
    assert!(core::error::Error::source(&ManifestError::TooLarge).is_none());
}
