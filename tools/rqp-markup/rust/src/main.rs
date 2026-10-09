// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Command line of the Rust RQP markup parser candidate
//! (`tools/rqp-markup/CONTRACT.md` section 2).
//!
//! ```text
//! rqp-markup <path-to-input.html>
//! rqp-markup --batch <fixtures-dir>
//! ```
//!
//! Exit code 0 when results are printed; 2 for a usage error or an I/O
//! error. Messages on stderr name paths and error kinds, never document
//! content.

#![forbid(unsafe_code)]

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rqp_markup::{MAX_INPUT_BYTES, analyze, result_json};

const USAGE: &str = "usage: rqp-markup <input.html> | rqp-markup --batch <fixtures-dir>";

/// A failure that ends the command with exit code 2.
#[derive(Debug)]
enum CliError {
    Usage,
    Io {
        op: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    Output(io::Error),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage => f.write_str(USAGE),
            Self::Io { op, path, source } => {
                write!(f, "rqp-markup: cannot {op} {}: {source}", path.display())
            }
            Self::Output(source) => write!(f, "rqp-markup: cannot write the result: {source}"),
        }
    }
}

/// Reads at most `MAX_INPUT_BYTES + 1` bytes: enough for M-01 to fail
/// without reading a larger file into memory.
fn read_input(path: &Path) -> Result<Vec<u8>, CliError> {
    let io_error = |op, source| CliError::Io {
        op,
        path: path.to_path_buf(),
        source,
    };
    let file = fs::File::open(path).map_err(|e| io_error("open", e))?;
    let limit = u64::try_from(MAX_INPUT_BYTES)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut bytes = Vec::new();
    file.take(limit)
        .read_to_end(&mut bytes)
        .map_err(|e| io_error("read", e))?;
    Ok(bytes)
}

fn write_line(out: &mut impl Write, line: &str) -> Result<(), CliError> {
    out.write_all(line.as_bytes())
        .and_then(|()| out.write_all(b"\n"))
        .map_err(CliError::Output)
}

fn single(path: &Path, out: &mut impl Write) -> Result<(), CliError> {
    let input = read_input(path)?;
    write_line(out, &result_json(&analyze(&input), None))
}

fn batch(dir: &Path, out: &mut impl Write) -> Result<(), CliError> {
    let io_error = |op, path: &Path, source| CliError::Io {
        op,
        path: path.to_path_buf(),
        source,
    };
    let mut names = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| io_error("list", dir, e))? {
        let entry = entry.map_err(|e| io_error("list", dir, e))?;
        if entry.path().is_dir() {
            names.push(entry.file_name());
        }
    }
    // Byte order of the name (CONTRACT.md section 2).
    names.sort_by(|a, b| a.as_encoded_bytes().cmp(b.as_encoded_bytes()));
    for name in names {
        let input_path = dir.join(&name).join("input.html");
        if !input_path.is_file() {
            continue;
        }
        let input = read_input(&input_path)?;
        let fixture = name.to_string_lossy();
        write_line(out, &result_json(&analyze(&input), Some(&fixture)))?;
    }
    Ok(())
}

fn run(args: &[String]) -> Result<(), CliError> {
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    match args {
        [flag, dir] if flag == "--batch" => batch(Path::new(dir), &mut out)?,
        [path] if !path.starts_with("--") => single(Path::new(path), &mut out)?,
        _ => return Err(CliError::Usage),
    }
    out.flush().map_err(CliError::Output)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // The only diagnostic output; it never contains document content.
            let mut stderr = io::stderr().lock();
            if writeln!(stderr, "{error}").is_err() {
                // Nothing else can report the failure; the exit code still does.
                return ExitCode::from(2);
            }
            ExitCode::from(2)
        }
    }
}
