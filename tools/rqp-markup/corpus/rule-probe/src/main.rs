// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! `rqp-markup-rule-probe <fixtures-dir>`: for each subdirectory (byte order of
//! the name) with an `input.html`, prints one JSON line
//! `{"fixture":"<name>","valid":<bool>,"error":<code|null>,"rule":<id|null>}`.
//! Only names of ASCII letters, digits, `-` and `_` are read (the corpus
//! layout makes `p0001`, …), so they need no JSON escaping. Never prints
//! document content.

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

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
        let Ok(input) = std::fs::read(&path) else {
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

fn main() -> ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let [dir] = args.as_slice() else {
        // The exit code reports the usage error also when stderr fails.
        if writeln!(
            std::io::stderr(),
            "usage: rqp-markup-rule-probe <fixtures-dir>"
        )
        .is_err()
        {
            return ExitCode::from(2);
        }
        return ExitCode::from(2);
    };
    match run(Path::new(dir)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // The exit code reports the failure also when stderr fails.
            if writeln!(std::io::stderr(), "rule-probe: {e}").is_err() {
                return ExitCode::from(2);
            }
            ExitCode::from(2)
        }
    }
}
