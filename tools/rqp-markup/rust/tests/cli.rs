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

/// L-2: an argument that is not UTF-8 is a path like any other. A missing
/// path is an I/O error (exit code 2), never a panic.
#[cfg(unix)]
#[test]
fn non_utf8_arguments_are_paths() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt as _;
    let dir = temp_dir();
    let missing = dir.join(OsStr::from_bytes(b"in-\xFF.html"));
    for args in [
        vec![missing.as_os_str()],
        vec![OsStr::new("--batch"), missing.as_os_str()],
    ] {
        let output = bin().args(&args).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("cannot"));
    }
    // A flag that is not UTF-8 is a usage error.
    let output = bin().arg(OsStr::from_bytes(b"--\xFF")).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("usage:"));
    // An existing file with such a name is read.
    let file = dir.join(OsStr::from_bytes(b"ok-\xFF.html"));
    if fs::write(&file, b"<p data-rq-block=\"a\">x</p>").is_ok() {
        let output = bin().arg(&file).output().unwrap();
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(lines(&output)[0]["blocks"][0]["id"], "a");
    }
}

/// Runs the command on `fifo` (a FIFO with no writer). The command must end
/// without a writer. If it waits longer than the watchdog time, the test
/// opens the FIFO for writing, so the command can end, and fails.
#[cfg(unix)]
fn run_on_fifo(args: &[&Path], fifo: &Path) -> Output {
    use std::process::Stdio;
    use std::sync::mpsc;
    use std::time::Duration;
    let child = bin()
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let (done, wait) = mpsc::channel();
    let waiter = std::thread::spawn(move || done.send(child.wait_with_output().unwrap()));
    let Ok(output) = wait.recv_timeout(Duration::from_secs(60)) else {
        // The command waits in the open of the FIFO; a writer ends that.
        drop(fs::OpenOptions::new().write(true).open(fifo).unwrap());
        waiter.join().unwrap().unwrap();
        panic!("the command waited for a writer of the FIFO");
    };
    waiter.join().unwrap().unwrap();
    output
}

/// Round 2 L-2: single mode does not open a FIFO (the open would wait for a
/// writer). A path that is not a regular file is an I/O error (exit code 2).
#[cfg(unix)]
#[test]
fn inputs_that_are_not_regular_files_do_not_block() {
    let dir = temp_dir();
    let fifo = dir.join("fifo.html");
    let made = Command::new("mkfifo").arg(&fifo).status();
    if !made.is_ok_and(|status| status.success()) {
        // No `mkfifo` command: nothing to check on this system.
        return;
    }
    let output = run_on_fifo(&[&fifo], &fifo);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not a regular file"), "{stderr}");

    // A symbolic link to a FIFO is not a regular file either.
    let link = dir.join("link.html");
    std::os::unix::fs::symlink(&fifo, &link).unwrap();
    let output = run_on_fifo(&[&link], &fifo);
    assert_eq!(output.status.code(), Some(2));

    // A symbolic link to a regular file is read.
    let file = dir.join("in.html");
    fs::write(&file, b"<p data-rq-block=\"a\">x</p>").unwrap();
    let link = dir.join("file-link.html");
    std::os::unix::fs::symlink(&file, &link).unwrap();
    let output = run(&[path_str(&link)]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(lines(&output)[0]["blocks"][0]["id"], "a");
}

/// Makes a batch directory with fixture "a" (input.html made by `make`) and
/// a valid fixture "b".
#[cfg(unix)]
fn batch_with_bad_a(make: impl FnOnce(&Path) -> bool) -> Option<PathBuf> {
    let fixtures = temp_dir();
    for name in ["a", "b"] {
        fs::create_dir(fixtures.join(name)).unwrap();
    }
    fs::write(fixtures.join("b").join("input.html"), b"<p>x</p>").unwrap();
    make(&fixtures.join("a").join("input.html")).then_some(fixtures)
}

#[cfg(unix)]
fn mkfifo(path: &Path) -> bool {
    Command::new("mkfifo")
        .arg(path)
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(unix)]
fn assert_batch_io_error(output: &Output) {
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not a regular file"), "{stderr}");
    assert!(stderr.contains("input.html"), "{stderr}");
    assert!(!stderr.contains("<p>"), "{stderr}");
}

/// CONTRACT section 2 item 7: in batch mode an `input.html` that is a FIFO
/// is an I/O error (exit code 2), and the command does not wait.
#[cfg(unix)]
#[test]
fn batch_mode_fifo_input_is_an_io_error() {
    let Some(fixtures) = batch_with_bad_a(mkfifo) else {
        return; // No `mkfifo` command on this system.
    };
    let fifo = fixtures.join("a").join("input.html");
    let output = run_on_fifo(&[Path::new("--batch"), &fixtures], &fifo);
    assert_batch_io_error(&output);
}

#[cfg(unix)]
#[test]
fn batch_mode_symlink_to_fifo_is_an_io_error() {
    let Some(fixtures) = batch_with_bad_a(|input| {
        let target = input.with_file_name("real-fifo");
        mkfifo(&target) && std::os::unix::fs::symlink(&target, input).is_ok()
    }) else {
        return;
    };
    let fifo = fixtures.join("a").join("real-fifo");
    let output = run_on_fifo(&[Path::new("--batch"), &fixtures], &fifo);
    assert_batch_io_error(&output);
}

#[cfg(unix)]
#[test]
fn batch_mode_directory_named_input_is_an_io_error() {
    let fixtures = batch_with_bad_a(|input| fs::create_dir(input).is_ok()).unwrap();
    let output = run(&["--batch", path_str(&fixtures)]);
    assert_batch_io_error(&output);
}

#[test]
fn batch_mode_sorts_by_name_and_skips_directories_without_input() {
    let dir = temp_dir();
    for (name, input) in [
        ("b", &b"<p data-rq-block=\"b\">x</p>"[..]),
        ("a", b"<p data-rq-block=\"a\">x</p>"),
        // "Z" and not "B": on a case-insensitive file system "B" and "b" are one directory.
        ("Z", b"\xFF"),
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
    assert_eq!(fixtures, ["Z", "a", "a\"q", "b"]);
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
