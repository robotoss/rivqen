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
//! error; 70 when an internal defect was found for an input (the result for
//! that input fails closed, `CONTRACT.md` section 5, note). Messages on
//! stderr name paths, fixture names and error kinds, never document
//! content.

#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rqp_markup::{Document, MAX_INPUT_BYTES, MarkupError, analyze, result_json};

const USAGE: &str = "usage: rqp-markup <input.html> | rqp-markup --batch <fixtures-dir>";

/// Exit code when the analysis of an input found an internal defect.
const EXIT_INTERNAL: u8 = 70;

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

/// The error for an input path that is not a regular file.
fn not_a_file() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "not a regular file")
}

/// Reads at most `MAX_INPUT_BYTES + 1` bytes: enough for M-01 to fail
/// without reading a larger file into memory.
///
/// Only a regular file (also through a symbolic link) is an input. The type
/// is checked before the open: on Unix, the open of a FIFO for reading
/// waits for a writer, so the command would hang (round 2 L-2). A
/// directory, FIFO, socket or device is an I/O error (exit code 2). The open
/// file is checked again, so a path that changes to another type between
/// the two steps does not read a directory or a device; a change to a FIFO
/// in that window can still make the open wait.
fn read_input(path: &Path) -> Result<Vec<u8>, CliError> {
    let io_error = |op, source| CliError::Io {
        op,
        path: path.to_path_buf(),
        source,
    };
    let kind = fs::metadata(path).map_err(|e| io_error("open", e))?;
    if !kind.is_file() {
        return Err(io_error("open", not_a_file()));
    }
    let file = fs::File::open(path).map_err(|e| io_error("open", e))?;
    let kind = file.metadata().map_err(|e| io_error("open", e))?;
    if !kind.is_file() {
        return Err(io_error("open", not_a_file()));
    }
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

/// Where the outputs go, and whether an internal defect was found.
struct Sink<O, E> {
    out: O,
    err: E,
    defect: bool,
}

impl<O: Write, E: Write> Sink<O, E> {
    /// Analyzes one input and prints its result line.
    fn result(&mut self, input: &[u8], fixture: Option<&str>, label: &str) -> Result<(), CliError> {
        self.report(&analyze(input), fixture, label)
    }

    /// Prints the result line. An internal defect prints the fail-closed
    /// result, a message without document content, and sets `defect`.
    fn report(
        &mut self,
        result: &Result<Document, MarkupError>,
        fixture: Option<&str>,
        label: &str,
    ) -> Result<(), CliError> {
        if result.as_ref().is_err_and(MarkupError::is_internal) {
            self.defect = true;
            writeln!(
                self.err,
                "rqp-markup: internal error for {label}; the result fails closed"
            )
            .map_err(CliError::Output)?;
        }
        write_line(&mut self.out, &result_json(result, fixture))
    }
}

fn single(path: &Path, sink: &mut Sink<impl Write, impl Write>) -> Result<(), CliError> {
    let input = read_input(path)?;
    sink.result(&input, None, &path.display().to_string())
}

fn batch(dir: &Path, sink: &mut Sink<impl Write, impl Write>) -> Result<(), CliError> {
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
        // A missing input.html skips the directory. Any other entry that is
        // not a regular file is an I/O error (CONTRACT.md section 2, item 7);
        // read_input rejects it before the open, so a FIFO never blocks.
        if matches!(fs::metadata(&input_path), Err(e) if e.kind() == io::ErrorKind::NotFound) {
            continue;
        }
        let input = read_input(&input_path)?;
        let fixture = name.to_string_lossy();
        sink.result(&input, Some(&fixture), &format!("fixture {fixture}"))?;
    }
    Ok(())
}

/// Runs the command. Returns true when an internal defect was found.
/// Arguments are OS strings: a path that is not UTF-8 is a path like any
/// other (L-2: no panic on a non-UTF-8 argument).
fn run(args: &[OsString], err: impl Write) -> Result<bool, CliError> {
    let stdout = io::stdout();
    let mut sink = Sink {
        out: io::BufWriter::new(stdout.lock()),
        err,
        defect: false,
    };
    match args {
        [flag, dir] if flag == "--batch" => batch(Path::new(dir), &mut sink)?,
        [path] if !path.as_encoded_bytes().starts_with(b"--") => {
            single(Path::new(path), &mut sink)?;
        }
        _ => return Err(CliError::Usage),
    }
    sink.out.flush().map_err(CliError::Output)?;
    Ok(sink.defect)
}

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    match run(&args, io::stderr()) {
        Ok(false) => ExitCode::SUCCESS,
        Ok(true) => ExitCode::from(EXIT_INTERNAL),
        Err(error) => {
            // The only other diagnostic output; it never contains document
            // content.
            let mut stderr = io::stderr().lock();
            if writeln!(stderr, "{error}").is_err() {
                // Nothing else can report the failure; the exit code still does.
                return ExitCode::from(2);
            }
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn sink() -> Sink<Vec<u8>, Vec<u8>> {
        Sink {
            out: Vec::new(),
            err: Vec::new(),
            defect: false,
        }
    }

    #[test]
    fn results_without_a_defect_print_one_line_and_nothing_on_stderr() {
        let mut sink = sink();
        sink.result(b"<p data-rq-block=\"a\">x</p>", Some("f"), "fixture f")
            .unwrap();
        sink.result(b"<p data-rq-block=\"A\">x</p>", None, "in.html")
            .unwrap();
        assert!(!sink.defect);
        assert_eq!(sink.err, b"");
        let out = String::from_utf8(sink.out).unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("{\"fixture\":\"f\",\"valid\":true"));
        assert!(lines[1].contains("RQP_MARKUP_INVALID_ID"));
    }

    /// L-3: an internal defect gives the fail-closed result line, a message
    /// without content on stderr, and the defect flag (exit code 70).
    #[test]
    fn report_of_an_internal_defect() {
        // No input gives an internal defect today; the error is made here.
        let error = MarkupError {
            code: rqp_markup::ErrorCode::Structure,
            rule: rqp_markup::INTERNAL_RULE,
        };
        let mut sink = sink();
        sink.report(&Err(error), Some("secret-name"), "fixture x")
            .unwrap();
        assert!(sink.defect);
        let out = String::from_utf8(sink.out).unwrap();
        assert_eq!(
            out,
            "{\"fixture\":\"secret-name\",\"valid\":false,\"error\":\"RQP_MARKUP_STRUCTURE\",\
             \"blocks\":[],\"template_revision\":null,\"page_revision\":null}\n"
        );
        let err = String::from_utf8(sink.err).unwrap();
        assert_eq!(
            err,
            "rqp-markup: internal error for fixture x; the result fails closed\n"
        );
        // A rule error is not a defect.
        let mut sink = super::tests::sink();
        let rule = MarkupError {
            code: rqp_markup::ErrorCode::Structure,
            rule: "M-23",
        };
        sink.report(&Err(rule), None, "in.html").unwrap();
        assert!(!sink.defect);
        assert_eq!(sink.err, b"");
    }
}
