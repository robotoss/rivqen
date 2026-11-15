// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! Tests of the RQP markup fixture loader (WP-06 S1, T-02).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::fs;
use std::path::{Path, PathBuf};

use rivqen_testkit::fixtures::FixtureError;
use rivqen_testkit::fixtures_root;
use rivqen_testkit::markup::{ExpectedBlock, load, load_at};

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    fs::remove_dir_all(&dir).ok();
    fs::create_dir_all(&dir).unwrap();
    dir
}

const INVALID: &str = r#"{"valid":false,"error":"RQP_MARKUP_NESTED","blocks":[],"template_revision":null,"page_revision":null}"#;

fn fixture(root: &Path, id: &str, input: &[u8], expected: &str) {
    let dir = root.join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("input.html"), input).unwrap();
    fs::write(dir.join("expected.json"), expected).unwrap();
}

#[test]
fn markup_loader_reads_all_fixtures() {
    let all = load().unwrap();
    assert_eq!(all.len(), 362);
    let mut valid = 0;
    for f in &all {
        let expected = f.expected_result().unwrap();
        assert_eq!(expected.valid, expected.error.is_none(), "{}", f.id);
        assert_eq!(
            expected.valid,
            expected.template_revision.is_some(),
            "{}",
            f.id
        );
        assert_eq!(expected.valid, expected.page_revision.is_some(), "{}", f.id);
        if expected.valid {
            valid += 1;
        } else {
            assert!(expected.blocks.is_empty(), "{}", f.id);
        }
        // The input is the exact file.
        let on_disk = fs::metadata(f.dir.join("input.html")).unwrap().len();
        assert_eq!(f.input.len() as u64, on_disk, "{}", f.id);
    }
    assert_eq!(valid, 148);
    assert_eq!(all.len() - valid, 214);
    // The README in the root is not a fixture.
    assert!(all.iter().all(|f| f.id.starts_with("FX-RQ-MARKUP-")));
    assert_eq!(all[0].id, "FX-RQ-MARKUP-AFTER-HTML-END");
    assert_eq!(
        all[0].dir,
        fixtures_root()
            .join("rqp")
            .join("markup")
            .join("FX-RQ-MARKUP-AFTER-HTML-END")
    );
}

#[test]
fn markup_loader_orders_ids_by_bytes() {
    let all = load().unwrap();
    assert!(
        all.windows(2)
            .all(|w| w[0].id.as_bytes() < w[1].id.as_bytes())
    );

    let root = scratch("markup_order");
    // Names that differ in more than case (the file system can fold case).
    for id in ["b", "Z", "a", "_"] {
        fixture(&root, id, b"x", INVALID);
    }
    let ids: Vec<String> = load_at(&root).unwrap().into_iter().map(|f| f.id).collect();
    // Byte order: upper case before `_` before lower case.
    assert_eq!(ids, ["Z", "_", "a", "b"]);
}

#[test]
fn markup_loader_reads_exact_bytes_and_typed_blocks() {
    let root = scratch("markup_exact");
    fs::write(root.join("README.md"), "not a fixture").unwrap();
    let input = b"\xEF\xBB\xBF<p data-rq-block=\"a\">x\r\n</p>\xFF";
    fixture(
        &root,
        "FX-ONE",
        input,
        r#"{"valid":true,"error":null,"blocks":[{"id":"a","format":"html","start":24,"end":27,"sha256":"h"}],"template_revision":"t1.x","page_revision":"r1.y"}"#,
    );
    let all = load_at(&root).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].input, input);
    let expected = all[0].expected_result().unwrap();
    assert!(expected.valid);
    assert_eq!(expected.error, None);
    assert_eq!(
        expected.blocks,
        [ExpectedBlock {
            id: "a".to_owned(),
            format: "html".to_owned(),
            start: 24,
            end: 27,
            sha256: "h".to_owned(),
        }]
    );
    assert_eq!(expected.template_revision.as_deref(), Some("t1.x"));
    assert_eq!(expected.page_revision.as_deref(), Some("r1.y"));
    // Debug holds no input bytes.
    let debug = format!("{:?}", all[0]);
    let length = format!("input_len: {}", input.len());
    assert!(
        debug.contains("FX-ONE") && debug.contains(&length),
        "{debug}"
    );
    assert!(!debug.contains("data-rq-block"), "{debug}");
}

#[test]
fn markup_loader_names_the_path_of_a_missing_file() {
    let root = scratch("markup_missing");
    let dir = root.join("FX-NO-INPUT");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("expected.json"), INVALID).unwrap();
    match load_at(&root).unwrap_err() {
        FixtureError::Io { path, .. } => assert_eq!(path, dir.join("input.html")),
        other => panic!("{other}"),
    }

    let root = scratch("markup_missing_expected");
    let dir = root.join("FX-NO-EXPECTED");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("input.html"), "x").unwrap();
    let error = load_at(&root).unwrap_err();
    assert!(
        matches!(&error, FixtureError::Io { path, .. } if *path == dir.join("expected.json")),
        "{error}"
    );

    let missing = scratch("markup_no_root").join("absent");
    assert!(matches!(
        load_at(&missing).unwrap_err(),
        FixtureError::Io { path, .. } if path == missing
    ));
}

#[test]
fn markup_loader_reports_bad_json_and_bad_shape() {
    let root = scratch("markup_bad_json");
    fixture(&root, "FX-BAD", b"x", "{");
    assert!(matches!(
        load_at(&root).unwrap_err(),
        FixtureError::Json { path, .. } if path == root.join("FX-BAD").join("expected.json")
    ));

    let root = scratch("markup_bad_shape");
    for (id, expected) in [
        ("FX-VALID", r#"{"error":null,"blocks":[]}"#),
        ("FX-BLOCKS", r#"{"valid":true,"error":null,"blocks":{}}"#),
        (
            "FX-START",
            r#"{"valid":true,"error":null,"blocks":[{"id":"a","format":"html","start":-1,"end":2,"sha256":"h"}],"template_revision":"t","page_revision":"r"}"#,
        ),
        (
            "FX-ID",
            r#"{"valid":true,"error":null,"blocks":[{"format":"html","start":1,"end":2,"sha256":"h"}],"template_revision":"t","page_revision":"r"}"#,
        ),
        (
            "FX-ERROR",
            r#"{"valid":false,"error":7,"blocks":[],"template_revision":null,"page_revision":null}"#,
        ),
        ("FX-REVISION", r#"{"valid":false,"error":null,"blocks":[]}"#),
    ] {
        fixture(&root, id, b"x", expected);
    }
    let all = load_at(&root).unwrap();
    assert_eq!(all.len(), 6);
    for f in &all {
        match f.expected_result().unwrap_err() {
            FixtureError::Shape { path, .. } => {
                assert_eq!(path, f.dir.join("expected.json"), "{}", f.id);
            }
            other => panic!("{}: {other}", f.id),
        }
    }
}
