// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Fixture loader (DL-146). Reads the committed files of `fixtures/rqp/protocol`,
//! `fixtures/legacy` and `fixtures/malformed` as data.
//!
//! The JSON of `case.json` and `expected.json` stays a [`serde_json::Value`]. Typed
//! accessors give the members that the protocol checks need. Page and body files are
//! raw bytes. Every failure names the path of the file in the error.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rivqen_model::http::FieldLine;
use serde_json::Value;

use crate::fixtures_root;

/// A fixture could not be loaded or has an unexpected shape. Always names a path.
#[derive(Debug)]
#[non_exhaustive]
pub enum FixtureError {
    /// A file or directory cannot be read.
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The cause.
        source: io::Error,
    },
    /// A JSON file is not valid JSON.
    Json {
        /// The path that failed.
        path: PathBuf,
        /// The parser message.
        message: String,
    },
    /// A member is missing or has the wrong type.
    Shape {
        /// The file that holds the member.
        path: PathBuf,
        /// What is wrong.
        what: String,
    },
}

impl fmt::Display for FixtureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Json { path, message } => {
                write!(f, "{}: invalid JSON: {message}", path.display())
            }
            Self::Shape { path, what } => write!(f, "{}: {what}", path.display()),
        }
    }
}

impl std::error::Error for FixtureError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, FixtureError> {
    fs::read(path).map_err(|source| FixtureError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn read_json(path: &Path) -> Result<Value, FixtureError> {
    let bytes = read_bytes(path)?;
    serde_json::from_slice(&bytes).map_err(|e| FixtureError::Json {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}

/// Read one JSON file of the repository (for example a known-answer file of
/// `spec/rqp/examples/kat`) with the same reader as the fixtures.
///
/// # Errors
/// [`FixtureError::Io`] or [`FixtureError::Json`] with the path.
pub fn load_json(path: &Path) -> Result<Value, FixtureError> {
    read_json(path)
}

fn io_err(path: &Path) -> impl Fn(io::Error) -> FixtureError + '_ {
    move |source| FixtureError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Sub-directories of `root` whose name starts with `prefix`, sorted.
fn list_dirs(root: &Path, prefix: &str) -> Result<Vec<PathBuf>, FixtureError> {
    let mut out = Vec::new();
    for entry in fs::read_dir(root).map_err(io_err(root))? {
        let entry = entry.map_err(io_err(root))?;
        let is_dir = entry.file_type().map_err(io_err(root))?.is_dir();
        if is_dir && entry.file_name().to_string_lossy().starts_with(prefix) {
            out.push(entry.path());
        }
    }
    out.sort();
    Ok(out)
}

/// A raw seed file (page or request field value), byte for byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seed {
    /// The file name.
    pub name: String,
    /// The bytes of the file.
    pub bytes: Vec<u8>,
}

fn load_seeds(dir: &Path) -> Result<Vec<Seed>, FixtureError> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(dir).map_err(io_err(dir))? {
        let entry = entry.map_err(io_err(dir))?;
        if entry.file_type().map_err(io_err(dir))?.is_file() {
            paths.push(entry.path());
        }
    }
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            Ok(Seed {
                bytes: read_bytes(&path)?,
                name,
            })
        })
        .collect()
}

/// Which kind of fixture a directory holds. It decides how the page is found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Server, legacy or malformed case: the page id is `case.page`.
    Page,
    /// RQP client case: the page id is the snapshot page of the context.
    Client,
}

/// One fixture directory: `case.json`, `expected.json` and the byte files it names.
#[derive(Debug, Clone)]
pub struct Fixture {
    /// The directory name, for example `FX-RQ-C01-OTHER-BASE`.
    pub id: String,
    /// The directory of the fixture.
    pub dir: PathBuf,
    /// `case.json`.
    pub case: Value,
    /// `expected.json`.
    pub expected: Value,
    /// `response.body` of a client case that has a body, exact bytes.
    pub response_body: Option<Vec<u8>>,
    /// The bytes of the page that the case names (`pages/<id>.html`), if any.
    pub page: Option<Vec<u8>>,
    /// The bytes of `client_page` of the case, if any.
    pub client_page: Option<Vec<u8>>,
}

impl Fixture {
    fn load(dir: &Path, pages: &Path, kind: Kind) -> Result<Self, FixtureError> {
        let id = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let case = read_json(&dir.join("case.json"))?;
        let expected = read_json(&dir.join("expected.json"))?;
        let page_id = match kind {
            Kind::Page => case.get("page").and_then(Value::as_str),
            Kind::Client => case
                .get("context")
                .and_then(|c| c.get("snapshot"))
                .and_then(|s| s.as_str().or_else(|| s.get("page").and_then(Value::as_str))),
        };
        let page_file = |p: &str| read_bytes(&pages.join(format!("{p}.html")));
        let page = page_id.map(page_file).transpose()?;
        let client_page = case
            .get("client_page")
            .and_then(Value::as_str)
            .map(page_file)
            .transpose()?;
        let has_body = expected
            .get("response")
            .and_then(|r| r.get("body"))
            .is_some_and(Value::is_string);
        let response_body = has_body
            .then(|| read_bytes(&dir.join("response.body")))
            .transpose()?;
        Ok(Self {
            id,
            dir: dir.to_path_buf(),
            case,
            expected,
            response_body,
            page,
            client_page,
        })
    }

    fn shape(&self, what: impl Into<String>) -> FixtureError {
        FixtureError::Shape {
            path: self.dir.join("expected.json"),
            what: what.into(),
        }
    }

    fn str_member(&self, v: &Value, key: &str, owner: &str) -> Result<String, FixtureError> {
        v.get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| self.shape(format!("{owner}.{key} is missing or not a string")))
    }

    fn lines(&self, v: Option<&Value>, owner: &str) -> Result<Vec<FieldLine>, FixtureError> {
        let items = v.and_then(Value::as_array).ok_or_else(|| {
            self.shape(format!("{owner} is missing or not a list of field lines"))
        })?;
        items
            .iter()
            .map(|item| match item.as_array().map(Vec::as_slice) {
                Some([Value::String(n), Value::String(val)]) => Ok(FieldLine::new(n, val)),
                _ => Err(self.shape(format!("{owner} has an entry that is not [name, value]"))),
            })
            .collect()
    }

    /// The `request` of `expected.json`: method, optional target and the field lines.
    ///
    /// # Errors
    /// [`FixtureError::Shape`] when `request` or its members are missing or mistyped.
    pub fn expected_request(&self) -> Result<ExpectedRequest, FixtureError> {
        let req = self
            .expected
            .get("request")
            .ok_or_else(|| self.shape("request is missing"))?;
        Ok(ExpectedRequest {
            method: self.str_member(req, "method", "request")?,
            target: req.get("target").and_then(Value::as_str).map(str::to_owned),
            headers: self.lines(req.get("headers"), "request.headers")?,
        })
    }

    /// The `context` of a client fixture, as the JSON value of `expected.json`.
    ///
    /// # Errors
    /// [`FixtureError::Shape`] when `context` is missing.
    pub fn client_context(&self) -> Result<&Value, FixtureError> {
        self.expected
            .get("context")
            .ok_or_else(|| self.shape("context is missing"))
    }

    /// The `context` of a client fixture with the members that the protocol
    /// checks read, typed.
    ///
    /// # Errors
    /// [`FixtureError::Shape`] when `context` or one of these members is
    /// missing or mistyped.
    pub fn client_context_data(&self) -> Result<ClientContext, FixtureError> {
        let ctx = self.client_context()?;
        let snapshot = match ctx.get("snapshot") {
            None | Some(Value::Null) => None,
            Some(s) => Some(
                SnapshotData::from_json(s)
                    .map_err(|what| self.shape(format!("context.snapshot: {what}")))?,
            ),
        };
        let nonce = match ctx.get("nonce") {
            None | Some(Value::Null) => None,
            Some(Value::String(n)) => Some(n.clone()),
            Some(_) => return Err(self.shape("context.nonce is not a string")),
        };
        let public_enabled = ctx
            .get("public_enabled")
            .and_then(Value::as_bool)
            .ok_or_else(|| self.shape("context.public_enabled is missing or not a boolean"))?;
        Ok(ClientContext {
            url: self.str_member(ctx, "url", "context")?,
            final_url: self.str_member(ctx, "final_url", "context")?,
            navigation: self.str_member(ctx, "navigation", "context")?,
            nonce,
            public_enabled,
            snapshot,
        })
    }

    /// The `response` of a client fixture: status, field lines and body sizes.
    ///
    /// # Errors
    /// [`FixtureError::Shape`] when `response`, `status` or `headers` is missing or mistyped.
    pub fn client_response(&self) -> Result<ClientResponse, FixtureError> {
        let resp = self
            .expected
            .get("response")
            .ok_or_else(|| self.shape("response is missing"))?;
        let status = resp
            .get("status")
            .and_then(Value::as_u64)
            .and_then(|s| u16::try_from(s).ok())
            .ok_or_else(|| self.shape("response.status is missing or not a status code"))?;
        Ok(ClientResponse {
            status,
            headers: self.lines(resp.get("headers"), "response.headers")?,
            body_bytes: resp.get("body_bytes").and_then(Value::as_u64),
            body_sha256: resp
                .get("body_sha256")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })
    }

    /// The `result` of a client fixture.
    ///
    /// # Errors
    /// [`FixtureError::Shape`] when `result` or its `outcome` is missing.
    pub fn client_result(&self) -> Result<ClientResult, FixtureError> {
        let res = self
            .expected
            .get("result")
            .ok_or_else(|| self.shape("result is missing"))?;
        let opt = |key: &str| res.get(key).and_then(Value::as_str).map(str::to_owned);
        Ok(ClientResult {
            outcome: self.str_member(res, "outcome", "result")?,
            error: opt("error"),
            next: opt("next"),
            metric: res.get("metric").cloned().unwrap_or(Value::Null),
            snapshot: res.get("snapshot").filter(|s| !s.is_null()).cloned(),
        })
    }

    /// The expected `body` of a server fixture.
    ///
    /// # Errors
    /// [`FixtureError::Shape`] when `body` is missing, has an unknown `kind` or a delta
    /// misses `binding`, `binding_inputs` or `blocks`.
    pub fn server_body(&self) -> Result<ExpectedBody, FixtureError> {
        let body = self
            .expected
            .get("body")
            .ok_or_else(|| self.shape("body is missing"))?;
        match self.str_member(body, "kind", "body")?.as_str() {
            "app" => Ok(ExpectedBody::App),
            "none" => Ok(ExpectedBody::None),
            "full" => Ok(ExpectedBody::Full {
                insert_at: body.get("insert_at").cloned().unwrap_or(Value::Null),
                manifest: body.get("manifest").cloned().unwrap_or(Value::Null),
            }),
            "delta" => self.delta_body(body).map(ExpectedBody::Delta),
            other => Err(self.shape(format!("body.kind {other:?} is unknown"))),
        }
    }

    fn delta_body(&self, body: &Value) -> Result<DeltaBody, FixtureError> {
        let inputs = body
            .get("binding_inputs")
            .ok_or_else(|| self.shape("body.binding_inputs is missing"))?;
        let blocks = body
            .get("blocks")
            .and_then(Value::as_array)
            .ok_or_else(|| self.shape("body.blocks is missing or not a list"))?
            .iter()
            .map(|b| {
                Ok(BlockHash {
                    id: self.str_member(b, "id", "body.blocks[]")?,
                    sha256: self.str_member(b, "sha256", "body.blocks[]")?,
                })
            })
            .collect::<Result<Vec<_>, FixtureError>>()?;
        let input = |key: &str| self.str_member(inputs, key, "body.binding_inputs");
        Ok(DeltaBody {
            binding: self.str_member(body, "binding", "body")?,
            inputs: BindingInputs {
                url: input("url")?,
                base_revision: input("base_revision")?,
                nonce: input("nonce")?,
                next_revision: input("next_revision")?,
            },
            blocks,
        })
    }
}

/// The members of a client `context` that the protocol checks read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientContext {
    /// The request URL.
    pub url: String,
    /// The final URL after redirects, as the fixture writes it (not always
    /// canonical).
    pub final_url: String,
    /// `first-visit`, `delta` or `fallback`.
    pub navigation: String,
    /// The text N of the request nonce (base64url, no padding), if any.
    pub nonce: Option<String>,
    /// The app setting for the `public` partition.
    pub public_enabled: bool,
    /// The snapshot that the client holds, if any.
    pub snapshot: Option<SnapshotData>,
}

/// A snapshot as a fixture writes it: the computed values of manifest-patch
/// section 1.2 as text. The conversion to the core types belongs to the
/// crate under test (the test kit does not depend on it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotData {
    /// The page id of the snapshot document, if the object names one.
    pub page: Option<String>,
    /// Template revision text `t1.`.
    pub template_revision: String,
    /// Page revision text `r1.`.
    pub page_revision: String,
    /// The blocks in document order.
    pub blocks: Vec<BlockData>,
    /// `request` or `public`, if the object names one.
    pub partition: Option<String>,
    /// The `policy` and `limits` members of the snapshot manifest.
    pub members: Members,
}

/// One block of a [`SnapshotData`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockData {
    /// Block id.
    pub id: String,
    /// `html` or `json`.
    pub format: String,
    /// Base64url SHA-256 of the content.
    pub sha256: String,
}

/// The `policy` and `limits` members of a manifest, a case or a snapshot. A
/// member or a field that is absent stays `None`: presence changes the page
/// revision (manifest-patch section 1.3, policy line).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Members {
    /// `policy`, if present.
    pub policy: Option<PolicyMembers>,
    /// `limits`, if present.
    pub limits: Option<LimitsMembers>,
}

/// The fields of a `policy` member.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PolicyMembers {
    /// `mode` text, if present.
    pub mode: Option<String>,
    /// `ttl_seconds`, if present.
    pub ttl_seconds: Option<u64>,
    /// `offline`, if present.
    pub offline: Option<bool>,
}

/// The fields of a `limits` member.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LimitsMembers {
    /// `max_patch_bytes`, if present.
    pub max_patch_bytes: Option<u64>,
}

/// The members of `object` when every name is in `names`; another member
/// name is an error, so a new field never drops out of a computed value
/// silently.
fn known_members<'a>(
    object: &'a Value,
    owner: &str,
    names: &[&str],
) -> Result<&'a serde_json::Map<String, Value>, String> {
    let map = object
        .as_object()
        .ok_or_else(|| format!("{owner} is not an object"))?;
    match map.keys().find(|k| !names.contains(&k.as_str())) {
        Some(other) => Err(format!("{owner} has the unknown member {other:?}")),
        None => Ok(map),
    }
}

fn opt_u64(map: &serde_json::Map<String, Value>, key: &str) -> Result<Option<u64>, String> {
    map.get(key)
        .map(|v| v.as_u64().ok_or_else(|| format!("{key} is not an integer")))
        .transpose()
}

fn req_str(object: &Value, key: &str) -> Result<String, String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("{key} is missing or not a string"))
}

impl Members {
    /// The `policy` and `limits` members of a JSON object; other members of
    /// the object are not read.
    ///
    /// # Errors
    /// A text that names the member when `policy` or `limits` has an unknown
    /// field or a field of the wrong type.
    pub fn from_json(object: &Value) -> Result<Self, String> {
        let policy = object
            .get("policy")
            .map(|p| {
                let map = known_members(p, "policy", &["mode", "ttl_seconds", "offline"])?;
                let mode = map
                    .get("mode")
                    .map(|m| {
                        m.as_str()
                            .map(str::to_owned)
                            .ok_or_else(|| "policy.mode is not a string".to_owned())
                    })
                    .transpose()?;
                let offline = map
                    .get("offline")
                    .map(|o| {
                        o.as_bool()
                            .ok_or_else(|| "policy.offline is not a boolean".to_owned())
                    })
                    .transpose()?;
                Ok::<_, String>(PolicyMembers {
                    mode,
                    ttl_seconds: opt_u64(map, "ttl_seconds")?,
                    offline,
                })
            })
            .transpose()?;
        let limits = object
            .get("limits")
            .map(|l| {
                let map = known_members(l, "limits", &["max_patch_bytes"])?;
                Ok::<_, String>(LimitsMembers {
                    max_patch_bytes: opt_u64(map, "max_patch_bytes")?,
                })
            })
            .transpose()?;
        Ok(Self { policy, limits })
    }
}

impl SnapshotData {
    /// A snapshot object: `template_revision`, `page_revision`, `blocks`
    /// (`id`, `format`, `sha256`), and the optional `page`, `partition`,
    /// `policy` and `limits`.
    ///
    /// # Errors
    /// A text that names the member when a member is missing, mistyped or
    /// unknown.
    pub fn from_json(object: &Value) -> Result<Self, String> {
        known_members(
            object,
            "snapshot",
            &[
                "page",
                "template_revision",
                "page_revision",
                "blocks",
                "partition",
                "policy",
                "limits",
            ],
        )?;
        let blocks = object
            .get("blocks")
            .and_then(Value::as_array)
            .ok_or_else(|| "blocks is missing or not a list".to_owned())?
            .iter()
            .map(|b| {
                known_members(b, "block", &["id", "format", "sha256"])?;
                Ok(BlockData {
                    id: req_str(b, "id")?,
                    format: req_str(b, "format")?,
                    sha256: req_str(b, "sha256")?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let opt = |key: &str| object.get(key).and_then(Value::as_str).map(str::to_owned);
        Ok(Self {
            page: opt("page"),
            template_revision: req_str(object, "template_revision")?,
            page_revision: req_str(object, "page_revision")?,
            blocks,
            partition: opt("partition"),
            members: Members::from_json(object)?,
        })
    }
}

/// The request of `expected.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedRequest {
    /// HTTP method.
    pub method: String,
    /// Target URL, if the fixture gives one.
    pub target: Option<String>,
    /// Field lines in order; a name can repeat.
    pub headers: Vec<FieldLine>,
}

/// The response of a client fixture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientResponse {
    /// HTTP status.
    pub status: u16,
    /// Field lines in order.
    pub headers: Vec<FieldLine>,
    /// Size of `response.body`, if the fixture has a body.
    pub body_bytes: Option<u64>,
    /// Base64url SHA-256 of `response.body`, if the fixture has a body.
    pub body_sha256: Option<String>,
}

/// The result that a client under test must return.
#[derive(Debug, Clone, PartialEq)]
pub struct ClientResult {
    /// `normal`, `not-modified`, `patched`, `rejected` or `stored`.
    pub outcome: String,
    /// The stable error code, if any.
    pub error: Option<String>,
    /// The next step, for example `fallback-load`.
    pub next: Option<String>,
    /// The metric flag as the fixture gives it.
    pub metric: Value,
    /// The snapshot the client holds after the step, if any.
    pub snapshot: Option<Value>,
}

/// The expected body of a server fixture.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ExpectedBody {
    /// The page with the manifest inserted.
    Full {
        /// Where the manifest goes.
        insert_at: Value,
        /// The expected manifest as a JSON value.
        manifest: Value,
    },
    /// A delta response.
    Delta(DeltaBody),
    /// The application body unchanged.
    App,
    /// No body.
    None,
}

/// Expected delta of a server fixture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeltaBody {
    /// The expected binding, compared exactly.
    pub binding: String,
    /// U, B, N and R' of the binding.
    pub inputs: BindingInputs,
    /// The changed blocks in order.
    pub blocks: Vec<BlockHash>,
}

/// The inputs of the binding of a delta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingInputs {
    /// U: the canonical URL.
    pub url: String,
    /// B: the base page revision.
    pub base_revision: String,
    /// N: the base64url text of the nonce.
    pub nonce: String,
    /// R': the next page revision.
    pub next_revision: String,
}

/// A changed block of an expected delta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockHash {
    /// The block id.
    pub id: String,
    /// Base64url SHA-256 of the current content.
    pub sha256: String,
}

/// All fixtures of `fixtures/rqp/protocol`.
#[derive(Debug, Clone)]
pub struct RqpProtocol {
    /// Server cases `FX-RQ-P*`, sorted by id.
    pub server: Vec<Fixture>,
    /// Client cases `FX-RQ-C*`, sorted by id.
    pub client: Vec<Fixture>,
}

/// Load `fixtures/rqp/protocol`.
///
/// # Errors
/// [`FixtureError`] with the path of the file that failed.
pub fn load_rqp_protocol() -> Result<RqpProtocol, FixtureError> {
    load_rqp_protocol_at(&fixtures_root().join("rqp").join("protocol"))
}

/// Load an RQP protocol tree from `root` (same layout as `fixtures/rqp/protocol`).
///
/// # Errors
/// [`FixtureError`] with the path of the file that failed.
pub fn load_rqp_protocol_at(root: &Path) -> Result<RqpProtocol, FixtureError> {
    let pages = root.join("pages");
    let load = |prefix: &str, kind: Kind| -> Result<Vec<Fixture>, FixtureError> {
        list_dirs(root, prefix)?
            .iter()
            .map(|d| Fixture::load(d, &pages, kind))
            .collect()
    };
    Ok(RqpProtocol {
        server: load("FX-RQ-P", Kind::Page)?,
        client: load("FX-RQ-C", Kind::Client)?,
    })
}

/// Load every `FX-LEG-*` case of `fixtures/legacy`.
///
/// # Errors
/// [`FixtureError`] with the path of the file that failed.
pub fn load_legacy() -> Result<Vec<Fixture>, FixtureError> {
    load_legacy_at(&fixtures_root().join("legacy"))
}

/// Load every `FX-LEG-*` case below `root` (pages in `root/pages`).
///
/// # Errors
/// [`FixtureError`] with the path of the file that failed.
pub fn load_legacy_at(root: &Path) -> Result<Vec<Fixture>, FixtureError> {
    let pages = root.join("pages");
    list_dirs(root, "FX-LEG-")?
        .iter()
        .map(|d| Fixture::load(d, &pages, Kind::Page))
        .collect()
}

/// The negative cases and fuzz seeds of `fixtures/malformed`.
#[derive(Debug, Clone)]
pub struct Malformed {
    /// The `FX-LEG-MAL-*` cases of `malformed/legacy`.
    pub cases: Vec<Fixture>,
    /// Raw pages of `malformed/seeds/pages`, sorted by name.
    pub seed_pages: Vec<Seed>,
    /// Raw request field values of `malformed/seeds/fields`, sorted by name.
    pub seed_fields: Vec<Seed>,
}

/// Load `fixtures/malformed`.
///
/// # Errors
/// [`FixtureError`] with the path of the file that failed.
pub fn load_malformed() -> Result<Malformed, FixtureError> {
    load_malformed_at(&fixtures_root().join("malformed"))
}

/// Load a malformed tree from `root` (layout of `fixtures/malformed`).
///
/// # Errors
/// [`FixtureError`] with the path of the file that failed.
pub fn load_malformed_at(root: &Path) -> Result<Malformed, FixtureError> {
    Ok(Malformed {
        cases: load_legacy_at(&root.join("legacy"))?,
        seed_pages: load_seeds(&root.join("seeds").join("pages"))?,
        seed_fields: load_seeds(&root.join("seeds").join("fields"))?,
    })
}
