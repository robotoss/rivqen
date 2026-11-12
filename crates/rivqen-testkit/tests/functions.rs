// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! Tests of the legacy function fixture loader (WP-06 S1, T-03).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::fs;
use std::path::PathBuf;

use rivqen_testkit::fixtures::FixtureError;
use rivqen_testkit::functions::{load, load_at};

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    fs::remove_dir_all(&dir).ok();
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &std::path::Path, cases: &str, results: &str) {
    fs::write(dir.join("cases.json"), cases).unwrap();
    fs::write(dir.join("legacy-t.json"), results).unwrap();
}

fn shape(error: FixtureError) -> (PathBuf, String) {
    match error {
        FixtureError::Shape { path, what } => (path, what),
        other => panic!("{other}"),
    }
}

#[test]
fn functions_loader_reads_the_committed_profiles() {
    let canonical = load("canonical").unwrap();
    assert_eq!(canonical.len(), 214);
    assert_eq!(canonical[0].id, "FN-ID-01");
    assert_eq!(canonical[0].function, "session_id");
    assert_eq!(canonical[0].divergences, ["D-20"]);
    assert!(canonical[0].result.is_none());
    assert_eq!(
        canonical.iter().filter(|f| f.function == "split").count(),
        35
    );
    for profile in ["android", "ios"] {
        let all = load(profile).unwrap();
        assert_eq!(all.len(), canonical.len(), "{profile}");
        let ids: Vec<_> = all.iter().map(|f| &f.id).collect();
        let want: Vec<_> = canonical.iter().map(|f| &f.id).collect();
        assert_eq!(ids, want, "{profile}");
    }
}

#[test]
fn functions_loader_keeps_the_file_order_of_data_members() {
    let dir = scratch("functions_order");
    write(
        &dir,
        r#"[{"id":"A","fn":"split","input":{}},{"id":"B","fn":"split","input":{}}]"#,
        r#"{"results":[
          {"id":"B","fn":"split","evidence":"rule","result":{"template":"t","data":{"{z}":"1, \"x\"","{a}":"[2]","{m}":{"k":[1,2]}}}},
          {"id":"A","fn":"split","evidence":"rule","error":"duplicate-block-name"}]}"#,
    );
    let all = load_at(&dir, "t").unwrap();
    // The order of the cases file, not of the results file.
    let ids: Vec<_> = all.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, ["A", "B"]);
    assert_eq!(all[0].error.as_deref(), Some("duplicate-block-name"));
    assert_eq!(all[0].data_keys, None);
    assert_eq!(all[1].data_keys.as_deref().unwrap(), ["{z}", "{a}", "{m}"]);
    assert_eq!(all[1].result.as_ref().unwrap()["template"], "t");
}

#[test]
fn functions_loader_case_without_result_fails() {
    let dir = scratch("functions_no_result");
    write(
        &dir,
        r#"[{"id":"A","fn":"x","input":1},{"id":"B","fn":"x","input":1}]"#,
        r#"{"results":[{"id":"A","fn":"x","evidence":"none","result":null}]}"#,
    );
    let (path, what) = shape(load_at(&dir, "t").unwrap_err());
    assert_eq!(path, dir.join("legacy-t.json"));
    assert!(what.contains("`B` has no result"), "{what}");
}

#[test]
fn functions_loader_result_without_case_fails() {
    let dir = scratch("functions_no_case");
    write(
        &dir,
        r#"[{"id":"A","fn":"x","input":1}]"#,
        r#"{"results":[{"id":"A","fn":"x","evidence":"none"},{"id":"Z","fn":"x","evidence":"none"}]}"#,
    );
    let (path, what) = shape(load_at(&dir, "t").unwrap_err());
    assert_eq!(path, dir.join("legacy-t.json"));
    assert!(what.contains("`Z` has no case"), "{what}");
}

#[test]
fn functions_loader_rejects_repeated_ids_and_changed_function() {
    let dir = scratch("functions_repeat");
    write(
        &dir,
        r#"[{"id":"A","fn":"x"},{"id":"A","fn":"x"}]"#,
        r#"{"results":[{"id":"A","fn":"x","evidence":"none"}]}"#,
    );
    let (path, what) = shape(load_at(&dir, "t").unwrap_err());
    assert_eq!(path, dir.join("cases.json"));
    assert!(what.contains("twice"), "{what}");

    write(
        &dir,
        r#"[{"id":"A","fn":"x"}]"#,
        r#"{"results":[{"id":"A","fn":"x","evidence":"none"},{"id":"A","fn":"x","evidence":"none"}]}"#,
    );
    let (path, what) = shape(load_at(&dir, "t").unwrap_err());
    assert_eq!(path, dir.join("legacy-t.json"));
    assert!(what.contains("twice"), "{what}");

    write(
        &dir,
        r#"[{"id":"A","fn":"x"}]"#,
        r#"{"results":[{"id":"A","fn":"y","evidence":"none"}]}"#,
    );
    let (_, what) = shape(load_at(&dir, "t").unwrap_err());
    assert!(what.contains("`fn` differs"), "{what}");
}

#[test]
fn functions_loader_names_the_path_of_a_bad_file() {
    let dir = scratch("functions_bad_file");
    fs::write(dir.join("cases.json"), "[]").unwrap();
    match load_at(&dir, "t").unwrap_err() {
        FixtureError::Io { path, .. } => assert_eq!(path, dir.join("legacy-t.json")),
        other => panic!("{other}"),
    }
    fs::write(dir.join("legacy-t.json"), "{").unwrap();
    match load_at(&dir, "t").unwrap_err() {
        FixtureError::Json { path, .. } => assert_eq!(path, dir.join("legacy-t.json")),
        other => panic!("{other}"),
    }
    fs::write(dir.join("legacy-t.json"), r#"{"results":5}"#).unwrap();
    let (path, _) = shape(load_at(&dir, "t").unwrap_err());
    assert_eq!(path, dir.join("legacy-t.json"));
}

#[test]
fn functions_loader_profile_is_a_plain_name() {
    for bad in ["", "../x", "a/b", "A", "x.y"] {
        let (_, what) = shape(load(bad).unwrap_err());
        assert!(what.contains("profile name"), "{bad}: {what}");
    }
}
