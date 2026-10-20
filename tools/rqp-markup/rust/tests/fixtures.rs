// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Runs every golden fixture of `fixtures/rqp/markup/` and compares the
//! result object with `expected.json` (the same comparison as
//! `tools/rqp-markup/diff.mjs`).
//!
//! The fixture directory is `$RQP_MARKUP_FIXTURES` when it is set (for
//! example for `cargo mutants`, which builds a copy of the package), else
//! `../../../fixtures/rqp/markup` from this package.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::format_collect
)]

use std::path::PathBuf;

use rqp_markup::{analyze, result_json};
use serde_json::Value;

fn fixtures_dir() -> PathBuf {
    std::env::var_os("RQP_MARKUP_FIXTURES").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/rqp/markup"),
        PathBuf::from,
    )
}

#[test]
fn all_markup_fixtures_match_expected_json() {
    let dir = fixtures_dir();
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot list {}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.join("input.html").is_file())
        .collect();
    names.sort();
    assert!(
        names.len() >= 117,
        "only {} fixtures in {}",
        names.len(),
        dir.display()
    );

    let mut failures = Vec::new();
    for path in &names {
        let input = std::fs::read(path.join("input.html")).unwrap();
        let expected: Value =
            serde_json::from_slice(&std::fs::read(path.join("expected.json")).unwrap()).unwrap();
        let got: Value = serde_json::from_str(&result_json(&analyze(&input), None)).unwrap();
        for field in [
            "valid",
            "error",
            "blocks",
            "template_revision",
            "page_revision",
        ] {
            if got[field] != expected[field] {
                failures.push(format!(
                    "{} {field}: expected {}, got {}",
                    path.file_name().unwrap().to_string_lossy(),
                    expected[field],
                    got[field]
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
