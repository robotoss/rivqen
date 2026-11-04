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
//! Before the mapping, the `Host` of the request must be one field line equal
//! (ASCII case-insensitive) to the authority of `--origin` or to a loopback
//! name with the local port; else `421` (security review S2 L-4, DL-098).
//!
//! The file is read on every request (no cache). The site never reads a request
//! body: [`Site`] answers from the head ([`Handler::Route`] is [`Infallible`]).
//!
//! Between the walk and the read (security review S2 L-1, L-2; DL-098): the file
//! is opened with `O_NONBLOCK`, so a FIFO swapped in after the walk cannot block
//! a worker; the handle must be a regular file with the device and inode of the
//! walked file; on Linux the path of the handle (`/proc/self/fd/<fd>`) must be
//! the walked path. Residual race on other Unix systems: a directory component
//! swapped for a link between the walk and the open is not seen; it needs write
//! access to the site directory.
//!
//! This module belongs to the binary (`main.rs`), as `cli.rs` does, and only
//! on Unix (I-2); it uses the library by its crate name, so a test can include
//! the same file.

use std::convert::Infallible;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use rivqen_reference::control::is_control_host;
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

/// `O_NONBLOCK` of the target (L-2). FACT: Linux `0o4000` (2048) on these
/// architectures (libc 0.2.191 `src/unix/linux_like/linux/{gnu,musl}/…`, from the
/// kernel `asm-generic/fcntl.h`; mips and sparc differ); macOS `0x0004` (SDK
/// `sys/fcntl.h`). Any other target: NOT FOUND, so the build fails.
#[cfg(all(
    target_os = "linux",
    any(
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "arm",
        target_arch = "aarch64",
        target_arch = "riscv64",
        target_arch = "powerpc64",
        target_arch = "s390x",
        target_arch = "loongarch64"
    )
))]
const O_NONBLOCK: i32 = 0o4000;
#[cfg(target_os = "macos")]
const O_NONBLOCK: i32 = 0x0004;
#[cfg(not(any(
    target_os = "macos",
    all(
        target_os = "linux",
        any(
            target_arch = "x86",
            target_arch = "x86_64",
            target_arch = "arm",
            target_arch = "aarch64",
            target_arch = "riscv64",
            target_arch = "powerpc64",
            target_arch = "s390x",
            target_arch = "loongarch64"
        )
    )
)))]
compile_error!("the O_NONBLOCK value of this target is not verified (WP-03 S2 L-2): NOT FOUND");

/// Open `path` for reading without blocking: a FIFO does not wait for a writer
/// (L-2). A regular file reads as without the flag.
///
/// # Errors
/// The I/O error of the open.
pub fn open_nonblocking(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(O_NONBLOCK)
        .open(path)
}

/// The same file: the device and the inode.
/// True when two authorities are the same after one explicit default port is
/// removed from each (`a.example` and `a.example:443` for `https`), compared ASCII
/// case-insensitively (RFC 9110 §4.2.3).
fn same_authority(host: &str, authority: &str, default_port: &str) -> bool {
    let strip = |text: &str| -> String {
        let lower = text.to_ascii_lowercase();
        match lower.rsplit_once(':') {
            Some((name, port)) if port == default_port && !name.is_empty() => name.to_owned(),
            _ => lower,
        }
    };
    strip(host) == strip(authority)
}

#[cfg(test)]
mod authority_tests {
    use super::same_authority;

    #[test]
    fn site_host_default_port_is_the_same_authority() {
        assert!(same_authority("a.example", "a.example:443", "443"));
        assert!(same_authority("A.Example:443", "a.example", "443"));
        assert!(same_authority("a.example:8443", "a.example:8443", "443"));
        assert!(!same_authority("a.example:8443", "a.example", "443"));
        assert!(!same_authority("a.example:80", "a.example", "443"));
        assert!(same_authority("a.example:80", "a.example", "80"));
        assert!(!same_authority("b.example", "a.example", "443"));
        assert!(!same_authority(":443", "", "443"));
    }
}

fn same_file(a: &Metadata, b: &Metadata) -> bool {
    a.dev() == b.dev() && a.ino() == b.ino()
}

/// The handle is at the walked path (L-1): on Linux the path of the handle in
/// `/proc/self/fd`; an error fails closed.
#[cfg(target_os = "linux")]
fn opened_at(file: &File, path: &Path) -> bool {
    use std::os::fd::AsRawFd;
    fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd())).is_ok_and(|p| p == path)
}

/// Other Unix systems have no such check without a new dependency: the device
/// and inode check covers the last component only (residual race, module doc).
#[cfg(not(target_os = "linux"))]
fn opened_at(_file: &File, _path: &Path) -> bool {
    true
}

/// The site: the canonical directory, the engine and the origin of the
/// request URL.
#[derive(Debug)]
pub struct Site {
    root: PathBuf,
    engine: Engine,
    origin: String,
    /// The authority of `origin` (`host[:port]` as given); `None` when the
    /// origin has none, so only a loopback `Host` is answered.
    authority: Option<String>,
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
        let authority = origin
            .split_once("://")
            .map(|(_, authority)| authority.to_owned())
            .filter(|authority| !authority.is_empty());
        Ok(Self {
            root,
            engine,
            origin,
            authority,
        })
    }

    /// `Host` is one field line, equal (ASCII case-insensitive) to the authority
    /// of the origin or to a loopback name with the local port (L-4).
    #[must_use]
    pub fn host_allowed(&self, conn: &Conn, head: &RequestHead) -> bool {
        let mut hosts = head.values("host");
        let (Some(host), None) = (hosts.next(), hosts.next()) else {
            return false;
        };
        self.authority
            .as_deref()
            .is_some_and(|a| same_authority(host, a, self.default_port()))
            || is_control_host(host, conn.local_port)
    }

    /// The path of the file of `mapped` under the rules of item 4, and its
    /// metadata (not followed through a link).
    ///
    /// # Errors
    /// [`Miss::NotFound`].
    /// The default port of the scheme of `--origin` (RFC 9110 §4.2).
    fn default_port(&self) -> &'static str {
        if self.origin.starts_with("https://") {
            "443"
        } else {
            "80"
        }
    }

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
        // O_NONBLOCK: a FIFO swapped in after the walk does not block (L-2).
        let file = open_nonblocking(&path).map_err(|_| Miss::NotFound)?;
        let opened = file.metadata().map_err(|_| Miss::Unreadable)?;
        // The file that was opened is the file that was checked, at the walked
        // path (L-1). These checks fail only in a race with a local writer.
        if !opened.is_file() || !same_file(&meta, &opened) || !opened_at(&file, &path) {
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
    pub fn answer(&self, conn: &Conn, head: &RequestHead) -> Outcome {
        if !self.host_allowed(conn, head) {
            return Outcome::noted(
                text(421, "Misdirected Request\n"),
                "the Host is not the origin or a loopback name with the port",
            );
        }
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

    fn route(&self, conn: &Conn, head: &RequestHead) -> Next<Infallible> {
        Next::Respond(self.answer(conn, head))
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
