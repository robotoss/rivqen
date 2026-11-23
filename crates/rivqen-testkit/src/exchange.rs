// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Loader of the stored exchanges of the canonical legacy client fixtures
//! (`fixtures/legacy/FX-LEG-CL-*/exchanges.json`, DL-216; format:
//! `fixtures/legacy/README.md` "Client `exchanges.json`").
//!
//! The file holds what the canonical client of the JavaScript model gets in
//! one run of the fixture: every request with its delivered response and its
//! stop (a cut or a refused connection), and the expanded CL-38 record of
//! each `cache_before` entry. With `case.json` and `expected.json`, a Rust
//! test runs the fixture without Node.js.
//!
//! Every value is checked: the members (no unknown member), the `format`,
//! the exchange numbers (1, 2, 3, … in entry order; `null` only for a refused
//! connection), the body forms (exactly one of `page` and `base64`; strict
//! base64; `len` equals the bytes, also for a page of `pages/`), the stop
//! inside the body, and the limits [`MAX_EXCHANGES`], [`MAX_RECORDS`],
//! [`MAX_BODY_BYTES`] and [`MAX_FILE_BYTES`]. A file that fails a check is a
//! [`FixtureError`] with its path; nothing panics. The SHA-256 of a body is
//! kept as text: the generator check (`gen.mjs --check`) binds it to the
//! bytes.
//!
//! `serde_json` here keeps object members sorted by key, so the format puts
//! every ordered list (field lines, data, Vary values) in arrays.
//!
//! [`fmt::Debug`] of a body, a record and a field line has lengths, never the
//! bytes of a page, a template or a data value.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use rivqen_model::http::FieldLine;
use serde_json::{Map, Value};

use crate::fixtures::{FixtureError, load_json};
use crate::fixtures_root;

/// The file name in a canonical client fixture directory.
pub const FILE_NAME: &str = "exchanges.json";
/// The only `format` that this loader reads.
pub const FORMAT: u64 = 1;
/// At most this many exchanges in one file (one per step, case limit).
pub const MAX_EXCHANGES: usize = 256;
/// At most this many `cache_before` records (case limit).
pub const MAX_RECORDS: usize = 16;
/// At most this many bytes in one body (case limit of `bytes`, 16 MiB).
pub const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;
/// At most this many bytes in one `exchanges.json` file.
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// Where the bytes of a [`Body`] come from in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum BodySource {
    /// The bytes of `pages/<id>.html` (the page of the application response).
    Page(String),
    /// Base64 text in the file (a data response, a changed body).
    Inline,
}

/// The checked bytes of a body: their length equals `len` of the file.
#[derive(Clone, PartialEq, Eq)]
pub struct Body {
    source: BodySource,
    bytes: Vec<u8>,
    sha256: String,
}

impl Body {
    /// The body bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Where the bytes come from.
    #[must_use]
    pub const fn source(&self) -> &BodySource {
        &self.source
    }

    /// The lowercase hex SHA-256 of the bytes, as the file gives it.
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// The number of bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// True when the body has no bytes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

impl fmt::Debug for Body {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Body")
            .field("source", &self.source)
            .field("len", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

/// The request that the canonical client sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The method (`GET`).
    pub method: String,
    /// The path and the query.
    pub target: String,
    /// Every sent field line, in order.
    pub fields: Vec<FieldLine>,
}

/// The delivered response (after the field and body faults of the schedule).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// The status code.
    pub status: u16,
    /// Every delivered field line, in order.
    pub fields: Vec<FieldLine>,
    /// The body; `None` when the response has none (a `304`).
    pub body: Option<Body>,
}

/// How the body of an exchange ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Stop {
    /// The body ends complete.
    Complete,
    /// The connection closes after the first `at` body bytes (`cut`).
    Cut {
        /// The number of body bytes that the client gets.
        at: usize,
    },
    /// The connection is refused before a response head (`server_down`).
    Refused,
}

/// One request of the client and what it got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exchange {
    /// The 1-based exchange number; `None` for a refused connection.
    pub number: Option<u32>,
    /// The number of the step that sent the request.
    pub step: u32,
    /// The 1-based index of the session that sent it.
    pub session: u32,
    /// The request.
    pub request: Request,
    /// The response; `None` for a refused connection.
    pub response: Option<Response>,
    /// How the body ends.
    pub stop: Stop,
}

/// The cache record of CL-38 that a `cache_before` entry expands into.
#[derive(Clone, PartialEq, Eq)]
pub struct CacheRecord {
    /// The partition (the account); `None` for the `null` partition.
    pub partition: Option<String>,
    /// The page URL.
    pub url: String,
    /// The Vary values: lowercase names, `None` for an absent field.
    pub vary: Vec<(String, Option<String>)>,
    /// The stored etag.
    pub etag: String,
    /// The stored template tag.
    pub template_tag: String,
    /// The stored page.
    pub page: Body,
    /// The template text.
    pub template: String,
    /// The data entries in stored order.
    pub data: Vec<(String, String)>,
    /// The stored page fields in stored order.
    pub fields: Vec<FieldLine>,
}

impl fmt::Debug for CacheRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CacheRecord")
            .field("partition", &self.partition)
            .field("url", &self.url)
            .field("vary_len", &self.vary.len())
            .field("page", &self.page)
            .field("template_len", &self.template.len())
            .field("data_len", &self.data.len())
            .field("fields", &self.fields)
            .finish_non_exhaustive()
    }
}

/// The stored exchanges of one canonical client fixture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientExchanges {
    /// The fixture id (the directory name).
    pub id: String,
    /// The directory of the fixture.
    pub dir: PathBuf,
    /// The `cache_before` records in entry order.
    pub cache_before: Vec<CacheRecord>,
    /// The exchanges in send order.
    pub exchanges: Vec<Exchange>,
}

/// Load the file of every canonical client fixture of `fixtures/legacy`.
///
/// # Errors
/// [`FixtureError`] with the path of the file that failed. See [`load_all_at`].
pub fn load_all() -> Result<Vec<ClientExchanges>, FixtureError> {
    load_all_at(&fixtures_root().join("legacy"))
}

/// Load the file of every canonical client fixture below `root` (pages in
/// `root/pages`), sorted by id. A canonical client fixture is a directory
/// `FX-LEG-*` whose `case.json` has `kind: "client"` and
/// `profile: "legacy-canonical"`.
///
/// # Errors
/// [`FixtureError::Io`] when a directory or file cannot be read (also a
/// missing `exchanges.json`), [`FixtureError::Json`] when a file is not JSON,
/// [`FixtureError::Shape`] when a file breaks a rule of the format.
pub fn load_all_at(root: &Path) -> Result<Vec<ClientExchanges>, FixtureError> {
    let pages = root.join("pages");
    let mut dirs = Vec::new();
    for entry in fs::read_dir(root).map_err(io_error(root))? {
        let entry = entry.map_err(io_error(root))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("FX-LEG-") || !entry.file_type().map_err(io_error(root))?.is_dir() {
            continue;
        }
        let case_path = entry.path().join("case.json");
        if !case_path.is_file() {
            continue;
        }
        let case = load_json(&case_path)?;
        let canonical_client = case.get("kind").and_then(Value::as_str) == Some("client")
            && case.get("profile").and_then(Value::as_str) == Some("legacy-canonical");
        if canonical_client {
            dirs.push(entry.path());
        }
    }
    dirs.sort();
    dirs.iter().map(|dir| load_dir(dir, &pages)).collect()
}

/// Load `exchanges.json` of the fixture directory `dir`; page references are
/// read from `pages`. The `id` of the file must equal the directory name.
///
/// # Errors
/// As [`load_all_at`].
pub fn load_dir(dir: &Path, pages: &Path) -> Result<ClientExchanges, FixtureError> {
    let path = dir.join(FILE_NAME);
    let size = fs::metadata(&path).map_err(io_error(&path))?.len();
    let reader = Reader { path: &path };
    if size > MAX_FILE_BYTES {
        return Err(reader.shape(format!("the file is over {MAX_FILE_BYTES} bytes")));
    }
    let json = load_json(&path)?;
    let top = reader.object(
        &json,
        "the file",
        &["id", "format", "cache_before", "exchanges"],
    )?;
    let id = reader.string(top, "id")?;
    let dir_name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if id != dir_name {
        return Err(reader.shape(format!(
            "`id` is `{id}`, not the directory name `{dir_name}`"
        )));
    }
    if top.get("format").and_then(Value::as_u64) != Some(FORMAT) {
        return Err(reader.shape(format!("`format` is not {FORMAT}")));
    }
    let records = reader.list(top, "cache_before", MAX_RECORDS)?;
    let cache_before = records
        .iter()
        .enumerate()
        .map(|(i, r)| reader.record(r, &format!("cache_before[{i}]"), pages))
        .collect::<Result<Vec<_>, _>>()?;
    let items = reader.list(top, "exchanges", MAX_EXCHANGES)?;
    let mut exchanges = Vec::with_capacity(items.len());
    let mut next: u32 = 1;
    for (i, item) in items.iter().enumerate() {
        let where_ = format!("exchanges[{i}]");
        let exchange = reader.exchange(item, &where_, pages)?;
        if let Some(n) = exchange.number {
            if n != next {
                return Err(reader.shape(format!("{where_}: exchange {n}, expected {next}")));
            }
            next = next
                .checked_add(1)
                .ok_or_else(|| reader.shape(format!("{where_}: too many exchanges")))?;
        }
        exchanges.push(exchange);
    }
    Ok(ClientExchanges {
        id,
        dir: dir.to_path_buf(),
        cache_before,
        exchanges,
    })
}

fn io_error(path: &Path) -> impl Fn(std::io::Error) -> FixtureError + '_ {
    move |source| FixtureError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// The checks of one file; every error names the file.
struct Reader<'p> {
    path: &'p Path,
}

impl Reader<'_> {
    fn shape(&self, what: impl Into<String>) -> FixtureError {
        FixtureError::Shape {
            path: self.path.to_path_buf(),
            what: what.into(),
        }
    }

    /// The object `value` with exactly the members `allowed` (all required).
    fn object<'v>(
        &self,
        value: &'v Value,
        where_: &str,
        allowed: &[&str],
    ) -> Result<&'v Map<String, Value>, FixtureError> {
        let object = value
            .as_object()
            .ok_or_else(|| self.shape(format!("{where_} is not an object")))?;
        if let Some(extra) = object.keys().find(|k| !allowed.contains(&k.as_str())) {
            return Err(self.shape(format!("{where_}: unknown member `{extra}`")));
        }
        if let Some(missing) = allowed.iter().find(|k| !object.contains_key(**k)) {
            return Err(self.shape(format!("{where_}: member `{missing}` is missing")));
        }
        Ok(object)
    }

    fn string(&self, object: &Map<String, Value>, key: &str) -> Result<String, FixtureError> {
        object
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| self.shape(format!("`{key}` is not a string")))
    }

    fn number(&self, value: Option<&Value>, what: &str, min: u64) -> Result<u64, FixtureError> {
        value
            .and_then(Value::as_u64)
            .filter(|n| *n >= min)
            .ok_or_else(|| self.shape(format!("{what} is not an integer of at least {min}")))
    }

    fn u32_member(
        &self,
        object: &Map<String, Value>,
        key: &str,
        where_: &str,
    ) -> Result<u32, FixtureError> {
        let n = self.number(object.get(key), &format!("{where_}.{key}"), 1)?;
        u32::try_from(n).map_err(|_| self.shape(format!("{where_}.{key} is too large")))
    }

    fn list<'v>(
        &self,
        object: &'v Map<String, Value>,
        key: &str,
        max: usize,
    ) -> Result<&'v Vec<Value>, FixtureError> {
        let list = object
            .get(key)
            .and_then(Value::as_array)
            .ok_or_else(|| self.shape(format!("`{key}` is not a list")))?;
        if list.len() > max {
            return Err(self.shape(format!("`{key}` has more than {max} entries")));
        }
        Ok(list)
    }

    /// A list of `[string, string]` pairs.
    fn pairs(
        &self,
        value: Option<&Value>,
        what: &str,
    ) -> Result<Vec<(String, String)>, FixtureError> {
        let list = value
            .and_then(Value::as_array)
            .ok_or_else(|| self.shape(format!("{what} is not a list")))?;
        list.iter()
            .map(|pair| match pair.as_array().map(Vec::as_slice) {
                Some([Value::String(a), Value::String(b)]) => Ok((a.clone(), b.clone())),
                _ => Err(self.shape(format!("{what}: an entry is not a pair of strings"))),
            })
            .collect()
    }

    fn fields(&self, value: Option<&Value>, what: &str) -> Result<Vec<FieldLine>, FixtureError> {
        Ok(self
            .pairs(value, what)?
            .into_iter()
            .map(|(name, value)| FieldLine::new(name, value))
            .collect())
    }

    fn body(&self, value: &Value, what: &str, pages: &Path) -> Result<Body, FixtureError> {
        let object = value
            .as_object()
            .ok_or_else(|| self.shape(format!("{what} is not an object")))?;
        let len = self.number(object.get("len"), &format!("{what}.len"), 0)?;
        let len = usize::try_from(len)
            .ok()
            .filter(|n| *n <= MAX_BODY_BYTES)
            .ok_or_else(|| self.shape(format!("{what}.len is over {MAX_BODY_BYTES}")))?;
        let sha256 = object
            .get("sha256")
            .and_then(Value::as_str)
            .filter(|s| {
                s.len() == 64
                    && s.bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
            .ok_or_else(|| self.shape(format!("{what}.sha256 is not 64 lowercase hex digits")))?
            .to_owned();
        let (source, bytes) = match (object.get("page"), object.get("base64")) {
            (Some(page), None) => {
                self.object(value, what, &["page", "len", "sha256"])?;
                let id = page.as_str().filter(|p| valid_page_id(p)).ok_or_else(|| {
                    self.shape(format!("{what}.page is not a page id (a-z, 0-9, `_`, `-`)"))
                })?;
                let file = pages.join(format!("{id}.html"));
                let size = fs::metadata(&file).map_err(io_error(&file))?.len();
                if usize::try_from(size).ok() != Some(len) {
                    return Err(self.shape(format!(
                        "{what}: pages/{id}.html has {size} bytes, not {len}"
                    )));
                }
                let bytes = fs::read(&file).map_err(io_error(&file))?;
                (BodySource::Page(id.to_owned()), bytes)
            }
            (None, Some(text)) => {
                self.object(value, what, &["base64", "len", "sha256"])?;
                let text = text
                    .as_str()
                    .ok_or_else(|| self.shape(format!("{what}.base64 is not a string")))?;
                let bytes = decode_base64(text, MAX_BODY_BYTES)
                    .map_err(|e| self.shape(format!("{what}.base64: {e}")))?;
                (BodySource::Inline, bytes)
            }
            _ => {
                return Err(self.shape(format!(
                    "{what} must have exactly one of `page` and `base64`"
                )));
            }
        };
        if bytes.len() != len {
            return Err(self.shape(format!("{what}: {} bytes, not len {len}", bytes.len())));
        }
        Ok(Body {
            source,
            bytes,
            sha256,
        })
    }

    fn record(
        &self,
        value: &Value,
        where_: &str,
        pages: &Path,
    ) -> Result<CacheRecord, FixtureError> {
        let r = self.object(
            value,
            where_,
            &[
                "partition",
                "url",
                "vary",
                "etag",
                "template_tag",
                "page",
                "template",
                "data",
                "fields",
            ],
        )?;
        let text = |key: &str| {
            r.get(key)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| self.shape(format!("{where_}.{key} is not a string")))
        };
        let partition = match r.get("partition") {
            Some(Value::Null) => None,
            Some(Value::String(p)) => Some(p.clone()),
            _ => return Err(self.shape(format!("{where_}.partition is not a string or null"))),
        };
        let vary = r
            .get("vary")
            .and_then(Value::as_array)
            .ok_or_else(|| self.shape(format!("{where_}.vary is not a list")))?
            .iter()
            .map(|pair| match pair.as_array().map(Vec::as_slice) {
                Some([Value::String(n), Value::String(v)]) => Ok((n.clone(), Some(v.clone()))),
                Some([Value::String(n), Value::Null]) => Ok((n.clone(), None)),
                _ => Err(self.shape(format!(
                    "{where_}.vary: an entry is not [name, value or null]"
                ))),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let page = r
            .get("page")
            .ok_or_else(|| self.shape(format!("{where_}.page is missing")))?;
        Ok(CacheRecord {
            partition,
            url: text("url")?,
            vary,
            etag: text("etag")?,
            template_tag: text("template_tag")?,
            page: self.body(page, &format!("{where_}.page"), pages)?,
            template: text("template")?,
            data: self.pairs(r.get("data"), &format!("{where_}.data"))?,
            fields: self.fields(r.get("fields"), &format!("{where_}.fields"))?,
        })
    }

    fn exchange(
        &self,
        value: &Value,
        where_: &str,
        pages: &Path,
    ) -> Result<Exchange, FixtureError> {
        let e = self.object(
            value,
            where_,
            &["exchange", "step", "session", "request", "response", "stop"],
        )?;
        let number = match e.get("exchange") {
            Some(Value::Null) => None,
            n => {
                let n = self.number(n, &format!("{where_}.exchange"), 1)?;
                Some(
                    u32::try_from(n)
                        .map_err(|_| self.shape(format!("{where_}.exchange is too large")))?,
                )
            }
        };
        let step = self.u32_member(e, "step", where_)?;
        let session = self.u32_member(e, "session", where_)?;
        let request_value = e
            .get("request")
            .ok_or_else(|| self.shape(format!("{where_}.request is missing")))?;
        let q = self.object(
            request_value,
            &format!("{where_}.request"),
            &["method", "target", "fields"],
        )?;
        let request = Request {
            method: self.string(q, "method")?,
            target: self.string(q, "target")?,
            fields: self.fields(q.get("fields"), &format!("{where_}.request.fields"))?,
        };
        let response = match e.get("response") {
            Some(Value::Null) => None,
            Some(r) => Some(self.response(r, &format!("{where_}.response"), pages)?),
            None => return Err(self.shape(format!("{where_}.response is missing"))),
        };
        let ending = self.stop(e.get("stop"), where_)?;
        let body_len = response
            .as_ref()
            .and_then(|r| r.body.as_ref())
            .map_or(0, Body::len);
        match (number, &response, ending) {
            (None, None, Stop::Refused) => {}
            (None, _, _) | (_, None, _) | (_, _, Stop::Refused) => {
                return Err(self.shape(format!(
                    "{where_}: `exchange` null, `response` null and stop `refused` go together"
                )));
            }
            (Some(_), Some(_), Stop::Cut { at }) if at > body_len => {
                return Err(self.shape(format!(
                    "{where_}: the cut at {at} is after the body ({body_len} bytes)"
                )));
            }
            (Some(_), Some(_), _) => {}
        }
        Ok(Exchange {
            number,
            step,
            session,
            request,
            response,
            stop: ending,
        })
    }

    fn response(
        &self,
        value: &Value,
        where_: &str,
        pages: &Path,
    ) -> Result<Response, FixtureError> {
        let r = self.object(value, where_, &["status", "fields", "body"])?;
        let status = r
            .get("status")
            .and_then(Value::as_u64)
            .filter(|s| (100..=599).contains(s))
            .and_then(|s| u16::try_from(s).ok())
            .ok_or_else(|| self.shape(format!("{where_}.status is not a status code")))?;
        let body = match r.get("body") {
            Some(Value::Null) => None,
            Some(b) => Some(self.body(b, &format!("{where_}.body"), pages)?),
            None => return Err(self.shape(format!("{where_}.body is missing"))),
        };
        Ok(Response {
            status,
            fields: self.fields(r.get("fields"), &format!("{where_}.fields"))?,
            body,
        })
    }

    fn stop(&self, value: Option<&Value>, where_: &str) -> Result<Stop, FixtureError> {
        let what = format!("{where_}.stop");
        match value {
            Some(Value::Null) => Ok(Stop::Complete),
            Some(v) => match v.get("kind").and_then(Value::as_str) {
                Some("refused") => {
                    self.object(v, &what, &["kind"])?;
                    Ok(Stop::Refused)
                }
                Some("cut") => {
                    let object = self.object(v, &what, &["kind", "at"])?;
                    let at = self.number(object.get("at"), &format!("{what}.at"), 0)?;
                    let at = usize::try_from(at)
                        .map_err(|_| self.shape(format!("{what}.at is too large")))?;
                    Ok(Stop::Cut { at })
                }
                _ => Err(self.shape(format!("{what}: `kind` is not `cut` or `refused`"))),
            },
            None => Err(self.shape(format!("{what} is missing"))),
        }
    }
}

/// A page id as the generator reads it: `a-z`, `0-9`, `_`, `-`; 1 to 64 bytes.
fn valid_page_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

/// The 6-bit value of one base64 character (RFC 4648 section 4).
fn sextet(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => c.checked_sub(b'A'),
        b'a'..=b'z' => c.checked_sub(b'a').and_then(|v| v.checked_add(26)),
        b'0'..=b'9' => c.checked_sub(b'0').and_then(|v| v.checked_add(52)),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Strict base64 of RFC 4648 section 4: the standard alphabet, padding to a
/// multiple of 4 characters, `=` only at the end, zero bits after the last
/// byte (the canonical form that Node.js writes). At most `max` bytes.
fn decode_base64(text: &str, max: usize) -> Result<Vec<u8>, String> {
    let (groups, rest) = text.as_bytes().as_chunks::<4>();
    if !rest.is_empty() {
        return Err("the length is not a multiple of 4".to_owned());
    }
    if groups.len() > max.div_ceil(3) {
        return Err(format!("more than {max} bytes"));
    }
    let value = |ch: u8| sextet(ch).ok_or_else(|| "a character is not base64".to_owned());
    let mut out = Vec::with_capacity(groups.len().saturating_mul(3));
    for (index, &[first, second, third, fourth]) in groups.iter().enumerate() {
        let pad = match (third, fourth) {
            (b'=', b'=') => 2,
            (_, b'=') => 1,
            (b'=', _) => return Err("`=` before the end".to_owned()),
            _ => 0,
        };
        if pad > 0 && index.checked_add(1) != Some(groups.len()) {
            return Err("`=` before the end".to_owned());
        }
        let third = if pad == 2 { 0 } else { value(third)? };
        let fourth = if pad >= 1 { 0 } else { value(fourth)? };
        let word = u32::from(value(first)?) << 18
            | u32::from(value(second)?) << 12
            | u32::from(third) << 6
            | u32::from(fourth);
        let [_, high, middle, low] = word.to_be_bytes();
        match pad {
            2 if word & 0xFFFF != 0 => {
                return Err("bits after the last byte are not zero".to_owned());
            }
            1 if word & 0xFF != 0 => return Err("bits after the last byte are not zero".to_owned()),
            2 => out.push(high),
            1 => out.extend_from_slice(&[high, middle]),
            _ => out.extend_from_slice(&[high, middle, low]),
        }
    }
    if out.len() > max {
        return Err(format!("more than {max} bytes"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::{decode_base64, valid_page_id};

    #[test]
    fn base64_rfc4648_vectors_decode() {
        for (text, bytes) in [
            ("", ""),
            ("Zg==", "f"),
            ("Zm8=", "fo"),
            ("Zm9v", "foo"),
            ("Zm9vYg==", "foob"),
            ("Zm9vYmE=", "fooba"),
            ("Zm9vYmFy", "foobar"),
        ] {
            assert_eq!(decode_base64(text, 16).unwrap(), bytes.as_bytes(), "{text}");
        }
        assert_eq!(decode_base64("+/8=", 16).unwrap(), [0xFB, 0xFF]);
    }

    #[test]
    fn base64_rejects_every_form_that_is_not_canonical() {
        for text in [
            "Zg=", "Zg", "Z===", "Zh==", "Zm9=", "Zg==Zg==", "Zm9v\n", "Zm-v", "Zm_v", "=Zg=",
        ] {
            assert!(decode_base64(text, 16).is_err(), "{text}");
        }
    }

    #[test]
    fn base64_respects_the_limit() {
        assert_eq!(decode_base64("Zm9vYmFy", 6).unwrap().len(), 6);
        assert!(decode_base64("Zm9vYmFy", 5).is_err());
        assert!(decode_base64("Zm9vYg==", 3).is_err());
        assert!(decode_base64("Zm9vYmFyZm9v", 6).is_err());
    }

    #[test]
    fn page_ids_follow_the_generator_rule() {
        assert!(valid_page_id("a"));
        assert!(valid_page_id("page_2-x"));
        assert!(!valid_page_id(""));
        assert!(!valid_page_id("A"));
        assert!(!valid_page_id("../a"));
        assert!(!valid_page_id("a.html"));
        assert!(!valid_page_id(&"a".repeat(65)));
    }
}
