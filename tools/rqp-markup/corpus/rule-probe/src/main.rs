// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! `rqp-markup-rule-probe <fixtures-dir>`: for each subdirectory (byte order of
//! the name) with an `input.html`, prints one JSON line
//! `{"fixture":"<name>","valid":<bool>,"error":<code|null>,"rule":<id|null>}`.
//! Only names of ASCII letters, digits, `-` and `_` are read (the corpus
//! layout makes `p0001`, …), so they need no JSON escaping. Never prints
//! document content.

use std::io::{Read, Write};
use std::path::Path;
use std::process::ExitCode;

/// Reads at most `MAX_INPUT_BYTES + 1` bytes (M-01): a larger input stays
/// larger than the limit, so the library reports the limit error, and the
/// probe never holds an unbounded file in memory. A missing file is `None`
/// (CONTRACT.md section 2 item 2); every other error carries the path.
fn read_input(path: &Path) -> std::io::Result<Option<Vec<u8>>> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(with_path(path, e)),
    };
    let mut input = Vec::new();
    file.take(rqp_markup::MAX_INPUT_BYTES as u64 + 1)
        .read_to_end(&mut input)
        .map_err(|e| with_path(path, e))?;
    Ok(Some(input))
}

fn with_path(path: &Path, e: std::io::Error) -> std::io::Error {
    std::io::Error::new(e.kind(), format!("{}: {e}", path.display()))
}

fn run(dir: &Path) -> std::io::Result<()> {
    let mut names: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if let Some(name) = entry.file_name().to_str()
            && !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            names.push(name.to_string());
        }
    }
    names.sort_unstable();
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    for name in names {
        let path = dir.join(&name).join("input.html");
        let Some(input) = read_input(&path)? else {
            continue;
        };
        match rqp_markup::analyze(&input) {
            Ok(_) => writeln!(
                out,
                "{{\"fixture\":\"{name}\",\"valid\":true,\"error\":null,\"rule\":null}}"
            )?,
            Err(e) => writeln!(
                out,
                "{{\"fixture\":\"{name}\",\"valid\":false,\"error\":\"{}\",\"rule\":\"{}\"}}",
                e.code.as_str(),
                e.rule
            )?,
        }
    }
    out.flush()
}

fn fail(message: &str) -> ExitCode {
    // The exit code reports the failure also when stderr fails.
    writeln!(std::io::stderr(), "{message}").ok();
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let [dir] = args.as_slice() else {
        return fail("usage: rqp-markup-rule-probe <fixtures-dir>");
    };
    match run(Path::new(dir)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(&format!("rule-probe: {e}")),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("rule-probe-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_file_is_skipped() {
        let dir = temp_dir("missing");
        assert!(read_input(&dir.join("none.html")).unwrap().is_none());
    }

    #[test]
    fn other_read_error_is_returned_with_the_path() {
        let dir = temp_dir("dir");
        let e = read_input(&dir).unwrap_err();
        assert!(e.to_string().contains(&dir.display().to_string()));
    }

    #[test]
    fn read_stops_one_byte_after_the_limit() {
        let dir = temp_dir("big");
        let path = dir.join("input.html");
        std::fs::write(&path, vec![b'a'; rqp_markup::MAX_INPUT_BYTES + 10]).unwrap();
        let input = read_input(&path).unwrap().unwrap();
        assert_eq!(input.len(), rqp_markup::MAX_INPUT_BYTES + 1);
    }
}
