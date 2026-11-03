// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Site mode: `rivqen-reference --site <dir>` (WP-03 S2 §2 "Site mapping";
//! DL-096; H-114).
//!
//! The site serves the regular files of one directory and applies one protocol
//! engine to each answer: [`rivqen_reference::legacy::respond`] or
//! [`rivqen_reference::rqp::respond`]. The mapping fails closed: a request path
//! maps to a file only when every item of the mapping holds; any other request
//! gets `404` (`405` for a method other than `GET` and `HEAD`).
//!
//! 1. Methods `GET` and `HEAD` ([`ALLOW`]).
//! 2. The path (without the query) matches `^(/[A-Za-z0-9_-][A-Za-z0-9._-]*)*/?$`:
//!    no segment starts with `.`, so no `..`, no hidden file and no
//!    percent-encoding ([`map_path`]).
//! 3. A path that ends with `/` maps to `index.html` in that directory.
//! 4. Every component is a real directory, the last one a regular file; no
//!    component is a symbolic link; the canonical path is the walked path, under
//!    the canonical directory ([`Site::read`]).
//! 5. A fixed table of extensions ([`media_type`]).
//! 6. At most [`MAX_HTML_BYTES`] for `.html`, [`MAX_FILE_BYTES`] for the others.
//!
//! The file is read on every request (no cache). The site never reads a request
//! body: [`Site`] answers from the head ([`Handler::Route`] is [`Infallible`]).
//!
//! This module belongs to the binary (`main.rs`), as `cli.rs` does; it uses the
//! library by its crate name, so a test can include the same file.

use std::convert::Infallible;
use std::fs::{self, File, Metadata};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use rivqen_reference::http::handler::{Conn, Handler, LogPath, Next, Outcome, guard};
use rivqen_reference::http::reader::RequestHead;
use rivqen_reference::http::writer::check_response;
use rivqen_reference::model::{AppResponse, Body, Field, Profile, Request, Response, pass_through};
use rivqen_reference::rqp::config::RqpConfig;
use rivqen_reference::rqp::select::{self, Full, Outcome as RqpOutcome};

/// The largest `.html` file (M-01: the largest document of the markup rules).
pub const MAX_HTML_BYTES: u64 = 5 * 1024 * 1024;
/// The largest file of another type.
pub const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
/// The value of `Allow` in a `405`.
pub const ALLOW: &str = "GET, HEAD";
/// The file of a path that ends with `/`.
pub const INDEX: &str = "index.html";

/// The protocol engine of the site (one per process).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Engine {
    /// The legacy engine in this profile.
    Legacy(Profile),
    /// The RQP engine with this configuration.
    Rqp(RqpConfig),
}

/// The media type of a file name by its extension (item 5); `None` for any
/// other extension and for a name without one.
#[must_use]
pub fn media_type(name: &str) -> Option<&'static str> {
    let (_, extension) = name.rsplit_once('.')?;
    Some(match extension {
        "html" => HTML,
        "css" => "text/css",
        "js" => "text/javascript",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "ico" => "image/vnd.microsoft.icon",
        "txt" => "text/plain; charset=utf-8",
        _ => return None,
    })
}

/// The media type of `.html`.
pub const HTML: &str = "text/html; charset=utf-8";

/// The size limit of a file of `media_type` (item 6).
#[must_use]
pub fn size_limit(media_type: &str) -> u64 {
    if media_type == HTML {
        MAX_HTML_BYTES
    } else {
        MAX_FILE_BYTES
    }
}

/// `[A-Za-z0-9_-][A-Za-z0-9._-]*`.
fn is_segment(segment: &str) -> bool {
    let mut bytes = segment.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.')
}

/// A request path that maps to a file name (items 2, 3 and 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapped<'a> {
    /// The path segments below the site directory; the last one is the file
    /// name (`index.html` for a path that ends with `/`).
    pub segments: Vec<&'a str>,
    /// The media type of the file name.
    pub media_type: &'static str,
    /// The size limit of the file.
    pub limit: u64,
}

/// Map a request path to file segments, without any file system access. `None`
/// when the path does not match item 2 or the file name has no media type.
#[must_use]
pub fn map_path(path: &str) -> Option<Mapped<'_>> {
    let rest = path.strip_prefix('/')?;
    let (body, directory) = match rest.strip_suffix('/') {
        Some(body) => (body, true),
        None => (rest, rest.is_empty()),
    };
    let mut segments: Vec<&str> = Vec::new();
    if !body.is_empty() {
        for segment in body.split('/') {
            if !is_segment(segment) {
                return None;
            }
            segments.push(segment);
        }
    }
    if directory {
        segments.push(INDEX);
    }
    let name = *segments.last()?;
    let media_type = media_type(name)?;
    Some(Mapped {
        media_type,
        limit: size_limit(media_type),
        segments,
    })
}

/// Why a mapped path gives no file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Miss {
    /// No regular file at the path under the rules of item 4.
    NotFound,
    /// The file is larger than its limit (item 6).
    TooLarge,
    /// The file could not be read.
    Unreadable,
}

/// The same file: on Unix the device and the inode.
#[cfg(unix)]
fn same_file(a: &Metadata, b: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.dev() == b.dev() && a.ino() == b.ino()
}

#[cfg(not(unix))]
fn same_file(a: &Metadata, b: &Metadata) -> bool {
    a.len() == b.len() && a.file_type() == b.file_type()
}

/// The site: the canonical directory, the engine and the origin of the
/// request URL.
#[derive(Debug)]
pub struct Site {
    root: PathBuf,
    engine: Engine,
    origin: String,
}

impl Site {
    /// A site over `dir`.
    ///
    /// # Errors
    /// The I/O error of the canonical path; `NotADirectory` when `dir` is no
    /// directory.
    pub fn new(dir: &Path, engine: Engine, origin: String) -> io::Result<Self> {
        let root = fs::canonicalize(dir)?;
        if !fs::metadata(&root)?.is_dir() {
            return Err(io::ErrorKind::NotADirectory.into());
        }
        Ok(Self {
            root,
            engine,
            origin,
        })
    }

    /// The path of the file of `mapped` under the rules of item 4, and its
    /// metadata (not followed through a link).
    ///
    /// # Errors
    /// [`Miss::NotFound`].
    pub fn resolve(&self, mapped: &Mapped<'_>) -> Result<(PathBuf, Metadata), Miss> {
        let mut path = self.root.clone();
        let mut last: Option<Metadata> = None;
        let count = mapped.segments.len();
        for (i, segment) in mapped.segments.iter().enumerate() {
            path.push(segment);
            let meta = fs::symlink_metadata(&path).map_err(|_| Miss::NotFound)?;
            let kind = meta.file_type();
            let is_last = i.checked_add(1) == Some(count);
            let fits = if is_last {
                kind.is_file()
            } else {
                kind.is_dir()
            };
            if kind.is_symlink() || !fits {
                return Err(Miss::NotFound);
            }
            last = Some(meta);
        }
        let meta = last.ok_or(Miss::NotFound)?;
        // No component is a link, so the canonical path is the walked path; a
        // difference (a link created meanwhile, a case-insensitive name) fails
        // closed.
        let canonical = fs::canonicalize(&path).map_err(|_| Miss::NotFound)?;
        if canonical != path || !canonical.starts_with(&self.root) {
            return Err(Miss::NotFound);
        }
        Ok((path, meta))
    }

    /// The bytes of the file of `mapped` (items 4 and 6).
    ///
    /// # Errors
    /// [`Miss`].
    pub fn read(&self, mapped: &Mapped<'_>) -> Result<Vec<u8>, Miss> {
        let (path, meta) = self.resolve(mapped)?;
        if meta.len() > mapped.limit {
            return Err(Miss::TooLarge);
        }
        let file = File::open(&path).map_err(|_| Miss::NotFound)?;
        let opened = file.metadata().map_err(|_| Miss::Unreadable)?;
        // The file that was opened is the file that was checked.
        if !opened.is_file() || !same_file(&meta, &opened) {
            return Err(Miss::NotFound);
        }
        let capacity = usize::try_from(opened.len().min(mapped.limit)).unwrap_or(0);
        let mut bytes = Vec::with_capacity(capacity);
        file.take(mapped.limit.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|_| Miss::Unreadable)?;
        // The file can grow after the check.
        if u64::try_from(bytes.len()).map_or(true, |n| n > mapped.limit) {
            return Err(Miss::TooLarge);
        }
        Ok(bytes)
    }

    /// The answer to a request head.
    #[must_use]
    pub fn answer(&self, head: &RequestHead) -> Outcome {
        if head.method != "GET" && head.method != "HEAD" {
            let mut response = text(405, "Method Not Allowed\n");
            response.fields.push(Field::new("Allow", ALLOW));
            return Outcome::noted(response, "a method other than GET and HEAD");
        }
        let Some(mapped) = map_path(head.path()) else {
            return Outcome::noted(text(404, NOT_FOUND), "the path maps to no file name");
        };
        let body = match self.read(&mapped) {
            Ok(body) => body,
            Err(Miss::NotFound) => {
                return Outcome::noted(text(404, NOT_FOUND), "no regular file at the path");
            }
            // The path maps to a file of the site: the log line shows it.
            Err(Miss::TooLarge) => {
                return Outcome::noted(text(404, NOT_FOUND), "the file is larger than its limit")
                    .with_log_path(LogPath::Request);
            }
            Err(Miss::Unreadable) => {
                return Outcome::noted(text(404, NOT_FOUND), "the file cannot be read")
                    .with_log_path(LogPath::Request);
            }
        };
        let app = AppResponse {
            status: 200,
            fields: vec![
                Field::new("Content-Type", mapped.media_type),
                Field::new("Cache-Control", "no-cache"),
            ],
            body,
        };
        let request = Request {
            method: head.method.clone(),
            url: format!("{}{}", self.origin, head.target),
            fields: head.fields.clone(),
        };
        self.apply(&request, &app).with_log_path(LogPath::Request)
    }

    /// The engine over the application response A (item 7). An engine defect
    /// gives A unchanged (ERR-07).
    fn apply(&self, request: &Request, app: &AppResponse) -> Outcome {
        let result = guard(|| match &self.engine {
            Engine::Legacy(profile) => (
                rivqen_reference::legacy::respond(*profile, request, app),
                None,
            ),
            Engine::Rqp(config) => {
                let (response, outcome) = select::select(config, request, app);
                (response, passthrough_note(outcome))
            }
        });
        match result {
            Ok((response, note)) if check_response(&response).is_ok() => Outcome {
                response,
                note,
                log_path: LogPath::Hidden,
            },
            Ok(_) => Outcome::noted(
                pass_through(request, app),
                "SERVER_PASSTHROUGH: the engine answer cannot be written, A sent",
            ),
            Err(()) => Outcome::noted(
                pass_through(request, app),
                "SERVER_PASSTHROUGH: the engine panicked, A sent",
            ),
        }
    }
}

const NOT_FOUND: &str = "Not Found\n";

/// A small `text/plain` answer without input values (item 8).
fn text(status: u16, body: &str) -> Response {
    Response {
        status,
        fields: vec![Field::new("Content-Type", "text/plain; charset=utf-8")],
        body: Body::Bytes(body.as_bytes().to_vec()),
    }
}

/// The log note of an RQP answer that hides an internal defect (ERR-01).
fn passthrough_note(outcome: RqpOutcome) -> Option<&'static str> {
    match outcome {
        RqpOutcome::Internal => Some("SERVER_PASSTHROUGH: internal defect, A sent"),
        RqpOutcome::Full(Full::Defect) => Some("SERVER_PASSTHROUGH: internal defect, F sent"),
        _ => None,
    }
}

impl Handler for Site {
    type Route = Infallible;

    fn route(&self, _conn: &Conn, head: &RequestHead) -> Next<Infallible> {
        Next::Respond(self.answer(head))
    }

    fn respond(&self, route: Infallible, _: &Conn, _: &RequestHead, _: &[u8]) -> Outcome {
        match route {}
    }
}

#[cfg(test)]
mod tests {
    use super::passthrough_note;
    use rivqen_reference::rqp::select::{Full, Invalid, Outcome};

    #[test]
    fn site_only_internal_defects_get_a_note() {
        assert_eq!(
            passthrough_note(Outcome::Internal),
            Some("SERVER_PASSTHROUGH: internal defect, A sent")
        );
        assert_eq!(
            passthrough_note(Outcome::Full(Full::Defect)),
            Some("SERVER_PASSTHROUGH: internal defect, F sent")
        );
        assert_eq!(passthrough_note(Outcome::Full(Full::PatchContent)), None);
        assert_eq!(passthrough_note(Outcome::Invalid(Invalid::Markup)), None);
        assert_eq!(passthrough_note(Outcome::Delta), None);
        assert_eq!(passthrough_note(Outcome::NotModified), None);
    }
}
