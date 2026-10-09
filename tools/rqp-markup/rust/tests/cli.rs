// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The command line of `CONTRACT.md` section 2: modes, exit codes, batch
//! order, and no document content on stderr.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::format_collect
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use rqp_markup::MAX_INPUT_BYTES;
use serde_json::Value;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rqp-markup"))
}

/// A new empty directory below the target directory.
fn temp_dir() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "cli-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&str]) -> Output {
    bin().args(args).output().unwrap()
}

fn lines(output: &Output) -> Vec<Value> {
    String::from_utf8(output.stdout.clone())
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn path_str(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn usage_errors_exit_2() {
    for args in [
        &[][..],
        &["--batch"],
        &["a", "b"],
        &["--batch", "a", "b"],
        &["--help"],
    ] {
        let output = run(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("usage:"),
            "{args:?}"
        );
    }
}

#[test]
fn single_mode_prints_one_result_line() {
    let dir = temp_dir();
    let file = dir.join("in.html");
    fs::write(&file, b"<p data-rq-block=\"price\">120.00</p>").unwrap();
    let output = run(&[path_str(&file)]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    assert!(output.stdout.ends_with(b"}\n"));
    let results = lines(&output);
    assert_eq!(results.len(), 1);
    let result = &results[0];
    assert_eq!(result["valid"], true);
    assert_eq!(result["error"], Value::Null);
    assert_eq!(result["blocks"][0]["id"], "price");
    assert_eq!(result["blocks"][0]["format"], "html");
    assert_eq!(result["blocks"][0]["start"], 25);
    assert_eq!(result["blocks"][0]["end"], 31);
    assert_eq!(result["blocks"][0]["sha256"].as_str().unwrap().len(), 43);
    assert!(result.get("fixture").is_none());
}

#[test]
fn invalid_documents_exit_0_and_print_no_content() {
    let dir = temp_dir();
    let file = dir.join("in.html");
    fs::write(
        &file,
        b"<p data-rq-block=\"secret-value\">x</p><p data-rq-block=\"secret-value\">y</p>",
    )
    .unwrap();
    let output = run(&[path_str(&file)]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let result = &lines(&output)[0];
    assert_eq!(result["valid"], false);
    assert_eq!(result["error"], "RQP_MARKUP_DUPLICATE");
    assert_eq!(result["blocks"], Value::Array(Vec::new()));
    assert_eq!(result["template_revision"], Value::Null);
    assert_eq!(result["page_revision"], Value::Null);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("secret"));
}

#[test]
fn io_errors_exit_2() {
    let dir = temp_dir();
    let missing = dir.join("missing.html");
    let output = run(&[path_str(&missing)]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot open"));

    let output = run(&["--batch", path_str(&missing)]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot list"));

    // A directory is not a readable input file.
    let output = run(&[path_str(&dir)]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn batch_mode_sorts_by_name_and_skips_directories_without_input() {
    let dir = temp_dir();
    for (name, input) in [
        ("b", &b"<p data-rq-block=\"b\">x</p>"[..]),
        ("a", b"<p data-rq-block=\"a\">x</p>"),
        ("B", b"\xFF"),
        ("a\"q", b""),
    ] {
        fs::create_dir(dir.join(name)).unwrap();
        fs::write(dir.join(name).join("input.html"), input).unwrap();
    }
    fs::create_dir(dir.join("c-no-input")).unwrap();
    fs::write(dir.join("c-no-input").join("other.html"), b"<p>").unwrap();
    fs::write(dir.join("plain-file"), b"x").unwrap();

    let output = run(&["--batch", path_str(&dir)]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let results = lines(&output);
    let fixtures: Vec<&str> = results
        .iter()
        .map(|r| r["fixture"].as_str().unwrap())
        .collect();
    assert_eq!(fixtures, ["B", "a", "a\"q", "b"]);
    assert_eq!(results[0]["error"], "RQP_MARKUP_ENCODING");
    assert_eq!(results[1]["blocks"][0]["id"], "a");
    assert_eq!(results[2]["valid"], true);
    assert_eq!(results[3]["blocks"][0]["id"], "b");
}

#[test]
fn oversized_input_is_a_limit_result() {
    let dir = temp_dir();
    let file = dir.join("big.html");
    fs::write(&file, vec![b'a'; MAX_INPUT_BYTES + 10]).unwrap();
    let output = run(&[path_str(&file)]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(lines(&output)[0]["error"], "RQP_MARKUP_LIMIT");

    fs::write(&file, vec![b'a'; MAX_INPUT_BYTES]).unwrap();
    let output = run(&[path_str(&file)]);
    assert_eq!(lines(&output)[0]["valid"], true);
}
