// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The `client` member of a client `expected.json` as typed values (DL-110,
//! DL-118; `fixtures/legacy/README.md` "Client `expected.json`").
//!
//! [`Projection::from_json`] reads the member strictly: every object has
//! exactly its members of the README (no unknown member, no missing member).
//! [`Projection::to_json_text`] writes it back with the member order of the
//! README; a parsed projection gives the same JSON value again (test
//! `legacy_client_expected_parses`).
//!
//! `Debug` of the projection writes counts and lengths: no message text, no
//! field value, no hash.

use std::fmt;

use serde_json::{Map, Value};

use super::json;

/// A `[name, value]` pair.
pub type Pair = (String, String);

/// The projection of one run: `sessions`, then `phases`.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Projection {
    /// One entry for each `create_session` step, in step order.
    pub sessions: Vec<SessionView>,
    /// The phases of the schedule (a phase ends with a `settle` step).
    pub phases: Vec<Phase>,
    /// What the runner could not run or map (a fixture that the runner
    /// cannot run is a finding). Not part of `expected.json`: an expected
    /// projection has none, so every problem is a difference.
    pub problems: Vec<String>,
}

impl fmt::Debug for Projection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Projection")
            .field("sessions", &self.sessions.len())
            .field("phases", &self.phases)
            .field("problems", &self.problems.len())
            .finish()
    }
}

/// One `create_session` step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionView {
    /// The step number.
    pub step: u32,
    /// The 1-based index among the opened sessions; `None` when no session
    /// opened (CL-07, CL-08).
    pub session: Option<u32>,
    /// The account at that step; `None` for the account `null`.
    pub partition: Option<String>,
    /// The page URL.
    pub url: String,
}

/// One phase.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Phase {
    /// The first and the last step number; `None` for a phase without steps.
    pub steps: Option<(u32, u32)>,
    /// The requests that reached the server, in exchange order.
    pub requests: Vec<RequestView>,
    /// The cookie calls (a multiset).
    pub cookies: Vec<CookieCall>,
    /// The web view calls in order.
    pub actions: Vec<Action>,
    /// The intercept answers (a multiset).
    pub intercepts: Vec<InterceptView>,
    /// The codes of each session at its `destroy` step (a multiset).
    pub results: Vec<ResultView>,
    /// One entry for each `snapshot_cache` step.
    pub snapshots: Vec<Snapshot>,
}

impl fmt::Debug for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Phase")
            .field("steps", &self.steps)
            .field("requests", &self.requests.len())
            .field("cookies", &self.cookies.len())
            .field("actions", &self.actions.len())
            .field("intercepts", &self.intercepts.len())
            .field("results", &self.results.len())
            .field("snapshots", &self.snapshots.len())
            .finish()
    }
}

/// A request that reached the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestView {
    /// The 1-based exchange number over all sessions.
    pub exchange: u32,
    /// The method.
    pub method: String,
    /// The path and the query.
    pub target: String,
    /// The legacy request fields in sent order, names in lower case.
    pub fields: Vec<Pair>,
    /// The response; `None` while no head arrived.
    pub response: Option<ResponseView>,
}

/// The summary of a response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseView {
    /// The status code.
    pub status: u16,
    /// The legacy response fields in order, names in lower case.
    pub fields: Vec<Pair>,
    /// False when the body ended with an error.
    pub complete: bool,
}

/// A cookie call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CookieCall {
    /// `cookie_get` with the value that the runtime gave.
    Get {
        /// The page URL.
        url: String,
        /// The value, `None` for `null`.
        value: Option<String>,
    },
    /// `cookie_set` with the `Set-Cookie` values in order.
    Set {
        /// The page URL.
        url: String,
        /// The values.
        values: Vec<String>,
    },
}

/// A web view call of a session on the main thread.
#[derive(Clone, PartialEq, Eq)]
pub enum Action {
    /// `load_url`.
    LoadUrl {
        /// The session index.
        session: u32,
        /// The URL.
        url: String,
    },
    /// `load_data`.
    LoadData {
        /// The session index.
        session: u32,
        /// The base URL.
        url: String,
        /// The fields, names in lower case, sorted by name (stable).
        fields: Vec<Pair>,
        /// The body length.
        body_len: u64,
        /// The lowercase hex SHA-256 of the body.
        body_sha256: String,
    },
    /// A page message.
    Message {
        /// The session index.
        session: u32,
        /// The message text without `local_refresh_time`.
        json: String,
    },
}

impl fmt::Debug for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LoadUrl { session, .. } => f
                .debug_struct("LoadUrl")
                .field("session", session)
                .finish_non_exhaustive(),
            Self::LoadData {
                session, body_len, ..
            } => f
                .debug_struct("LoadData")
                .field("session", session)
                .field("body_len", body_len)
                .finish_non_exhaustive(),
            Self::Message { session, json } => f
                .debug_struct("Message")
                .field("session", session)
                .field("len", &json.len())
                .finish(),
        }
    }
}

/// The answer to an intercepted main-document request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterceptView {
    /// `{ "null": true }`: the web view loads from the network.
    Null {
        /// The session index.
        session: u32,
    },
    /// A document.
    Document {
        /// The session index.
        session: u32,
        /// The status.
        status: u16,
        /// The fields, names in lower case, sorted by name (stable).
        fields: Vec<Pair>,
        /// The body length (the bytes before the error when incomplete).
        body_len: u64,
        /// The lowercase hex SHA-256 of the body.
        body_sha256: String,
        /// False when the stream ended with an error.
        complete: bool,
    },
}

/// The codes of a session at its `destroy` step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResultView {
    /// The session index.
    pub session: u32,
    /// The source code.
    pub src_code: i32,
    /// The final code.
    pub final_code: i32,
}

/// The cache at one `snapshot_cache` step.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Snapshot {
    /// The records, sorted by `partition`, `url`, `vary`.
    pub entries: Vec<RecordView>,
}

/// The abstract cache record of CL-38.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordView {
    /// The partition (the account); `None` for `null`.
    pub partition: Option<String>,
    /// The page URL.
    pub url: String,
    /// The Vary values (lowercase names, sorted; `None` for an absent field).
    pub vary: Vec<(String, Option<String>)>,
    /// The stored etag.
    pub etag: Option<String>,
    /// The stored template tag.
    pub template_tag: Option<String>,
    /// SHA-256 of the page.
    pub page_sha256: Option<String>,
    /// SHA-256 of the template.
    pub template_sha256: Option<String>,
    /// SHA-256 of the compact JSON text of the data; compared by presence
    /// only (DL-118).
    pub data_sha256: Option<String>,
    /// The stored page fields, names in lower case, sorted by name (stable).
    pub fields: Vec<Pair>,
}

// --- canonical text and the members of a comparison ---

/// One member of an entry for the comparison: the path and the JSON text of
/// the value.
pub(crate) type Member = (String, String);

fn member(name: &str, value: String) -> Member {
    (name.to_owned(), value)
}

impl SessionView {
    pub(crate) fn members(&self) -> Vec<Member> {
        let mut out = vec![
            member("step", self.step.to_string()),
            member("opened", self.session.is_some().to_string()),
        ];
        if let Some(index) = self.session {
            out.push(member("session", index.to_string()));
        }
        out.push(member(
            "partition",
            json::opt_string(self.partition.as_deref()),
        ));
        out.push(member("url", json::string(&self.url)));
        out
    }

    pub(crate) fn text(&self) -> String {
        text_of(&self.members())
    }
}

impl ResponseView {
    fn text(&self) -> String {
        json::object(&[
            ("status", self.status.to_string()),
            ("fields", json::pairs(&self.fields)),
            ("complete", self.complete.to_string()),
        ])
    }
}

impl RequestView {
    pub(crate) fn members(&self) -> Vec<Member> {
        let mut out = vec![
            member("exchange", self.exchange.to_string()),
            member("method", json::string(&self.method)),
            member("target", json::string(&self.target)),
            member("fields", json::pairs(&self.fields)),
        ];
        match &self.response {
            None => out.push(member("response", "null".to_owned())),
            Some(r) => {
                out.push(member("response.status", r.status.to_string()));
                out.push(member("response.fields", json::pairs(&r.fields)));
                out.push(member("response.complete", r.complete.to_string()));
            }
        }
        out
    }

    pub(crate) fn text(&self) -> String {
        json::object(&[
            ("exchange", self.exchange.to_string()),
            ("method", json::string(&self.method)),
            ("target", json::string(&self.target)),
            ("fields", json::pairs(&self.fields)),
            (
                "response",
                self.response
                    .as_ref()
                    .map_or_else(|| "null".to_owned(), ResponseView::text),
            ),
        ])
    }
}

impl CookieCall {
    pub(crate) fn members(&self) -> Vec<Member> {
        match self {
            Self::Get { url, value } => vec![
                member("call", json::string("get")),
                member("url", json::string(url)),
                member("value", json::opt_string(value.as_deref())),
            ],
            Self::Set { url, values } => vec![
                member("call", json::string("set")),
                member("url", json::string(url)),
                member("values", json::strings(values)),
            ],
        }
    }

    pub(crate) fn text(&self) -> String {
        text_of(&self.members())
    }
}

impl Action {
    pub(crate) fn members(&self) -> Vec<Member> {
        match self {
            Self::LoadUrl { session, url } => vec![
                member("do", json::string("load_url")),
                member("session", session.to_string()),
                member("url", json::string(url)),
            ],
            Self::LoadData {
                session,
                url,
                fields,
                body_len,
                body_sha256,
            } => vec![
                member("do", json::string("load_data")),
                member("session", session.to_string()),
                member("url", json::string(url)),
                member("fields", json::pairs(fields)),
                member("body_len", body_len.to_string()),
                member("body_sha256", json::string(body_sha256)),
            ],
            Self::Message { session, json } => vec![
                member("do", json::string("message")),
                member("session", session.to_string()),
                member("json", json::string(json)),
            ],
        }
    }

    pub(crate) fn text(&self) -> String {
        text_of(&self.members())
    }
}

impl InterceptView {
    pub(crate) fn members(&self) -> Vec<Member> {
        match self {
            Self::Null { session } => vec![
                member("session", session.to_string()),
                member("null", "true".to_owned()),
            ],
            Self::Document {
                session,
                status,
                fields,
                body_len,
                body_sha256,
                complete,
            } => vec![
                member("session", session.to_string()),
                member("status", status.to_string()),
                member("fields", json::pairs(fields)),
                member("body_len", body_len.to_string()),
                member("body_sha256", json::string(body_sha256)),
                member("complete", complete.to_string()),
            ],
        }
    }

    pub(crate) fn text(&self) -> String {
        text_of(&self.members())
    }
}

impl ResultView {
    pub(crate) fn members(&self) -> Vec<Member> {
        vec![
            member("session", self.session.to_string()),
            member("src_code", self.src_code.to_string()),
            member("final_code", self.final_code.to_string()),
        ]
    }

    pub(crate) fn text(&self) -> String {
        text_of(&self.members())
    }
}

/// The name of the member that DL-118 compares by presence only.
pub(crate) const DATA_PRESENCE: &str = "data_sha256 (presence, DL-118)";

impl RecordView {
    /// The members of the comparison: `data_sha256` by presence only
    /// (DL-118).
    pub(crate) fn members(&self) -> Vec<Member> {
        vec![
            member("partition", json::opt_string(self.partition.as_deref())),
            member("url", json::string(&self.url)),
            member("vary", json::opt_pairs(&self.vary)),
            member("etag", json::opt_string(self.etag.as_deref())),
            member(
                "template_tag",
                json::opt_string(self.template_tag.as_deref()),
            ),
            member("page_sha256", json::opt_string(self.page_sha256.as_deref())),
            member(
                "template_sha256",
                json::opt_string(self.template_sha256.as_deref()),
            ),
            member(
                DATA_PRESENCE,
                if self.data_sha256.is_some() {
                    json::string("present")
                } else {
                    "null".to_owned()
                },
            ),
            member("fields", json::pairs(&self.fields)),
        ]
    }

    pub(crate) fn text(&self) -> String {
        json::object(&[
            ("partition", json::opt_string(self.partition.as_deref())),
            ("url", json::string(&self.url)),
            ("vary", json::opt_pairs(&self.vary)),
            ("etag", json::opt_string(self.etag.as_deref())),
            (
                "template_tag",
                json::opt_string(self.template_tag.as_deref()),
            ),
            ("page_sha256", json::opt_string(self.page_sha256.as_deref())),
            (
                "template_sha256",
                json::opt_string(self.template_sha256.as_deref()),
            ),
            ("data_sha256", json::opt_string(self.data_sha256.as_deref())),
            ("fields", json::pairs(&self.fields)),
        ])
    }

    /// The sort key of a snapshot: `[partition, url, vary]` as JSON text.
    pub(crate) fn sort_key(&self) -> String {
        json::array(&[
            json::opt_string(self.partition.as_deref()),
            json::string(&self.url),
            json::opt_pairs(&self.vary),
        ])
    }
}

impl Snapshot {
    fn text(&self) -> String {
        json::object(&[(
            "entries",
            json::array(
                &self
                    .entries
                    .iter()
                    .map(RecordView::text)
                    .collect::<Vec<_>>(),
            ),
        )])
    }
}

fn text_of(members: &[Member]) -> String {
    let borrowed: Vec<(&str, String)> = members
        .iter()
        .map(|(n, v)| (n.as_str(), v.clone()))
        .collect();
    json::object(&borrowed)
}

fn list_text<T>(items: &[T], text: impl Fn(&T) -> String) -> String {
    json::array(&items.iter().map(text).collect::<Vec<_>>())
}

impl Phase {
    fn text(&self) -> String {
        let steps = self
            .steps
            .map_or_else(|| "[]".to_owned(), |(a, b)| format!("[{a},{b}]"));
        json::object(&[
            ("steps", steps),
            ("requests", list_text(&self.requests, RequestView::text)),
            ("cookies", list_text(&self.cookies, CookieCall::text)),
            ("actions", list_text(&self.actions, Action::text)),
            (
                "intercepts",
                list_text(&self.intercepts, InterceptView::text),
            ),
            ("results", list_text(&self.results, ResultView::text)),
            ("snapshots", list_text(&self.snapshots, Snapshot::text)),
        ])
    }
}

impl Projection {
    /// The `client` member as JSON text with the member order of the README
    /// (the problems are not part of it).
    #[must_use]
    pub fn to_json_text(&self) -> String {
        json::object(&[
            ("sessions", list_text(&self.sessions, SessionView::text)),
            ("phases", list_text(&self.phases, Phase::text)),
        ])
    }

    /// Read the `client` member of an `expected.json`.
    ///
    /// # Errors
    /// The path of the first member that breaks the README form.
    pub fn from_json(client: &Value) -> Result<Self, String> {
        let top = object(client, "client", &["sessions", "phases"], &[])?;
        let sessions = list(top, "sessions", "client")?
            .iter()
            .enumerate()
            .map(|(i, v)| parse_session(v, &format!("client.sessions[{i}]")))
            .collect::<Result<Vec<_>, _>>()?;
        let phases = list(top, "phases", "client")?
            .iter()
            .enumerate()
            .map(|(i, v)| parse_phase(v, &format!("client.phases[{i}]")))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            sessions,
            phases,
            problems: Vec::new(),
        })
    }
}

// --- the strict reader ---

/// The object `value` with the members `required` and at most `optional`.
fn object<'v>(
    value: &'v Value,
    at: &str,
    required: &[&str],
    optional: &[&str],
) -> Result<&'v Map<String, Value>, String> {
    let map = value
        .as_object()
        .ok_or_else(|| format!("{at} is not an object"))?;
    if let Some(extra) = map
        .keys()
        .find(|k| !required.contains(&k.as_str()) && !optional.contains(&k.as_str()))
    {
        return Err(format!("{at}: unknown member `{extra}`"));
    }
    if let Some(missing) = required.iter().find(|k| !map.contains_key(**k)) {
        return Err(format!("{at}: member `{missing}` is missing"));
    }
    Ok(map)
}

fn list<'v>(map: &'v Map<String, Value>, key: &str, at: &str) -> Result<&'v Vec<Value>, String> {
    map.get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{at}.{key} is not a list"))
}

fn text(map: &Map<String, Value>, key: &str, at: &str) -> Result<String, String> {
    map.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("{at}.{key} is not a string"))
}

fn opt_text(map: &Map<String, Value>, key: &str, at: &str) -> Result<Option<String>, String> {
    match map.get(key) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        _ => Err(format!("{at}.{key} is not a string or null")),
    }
}

fn boolean(map: &Map<String, Value>, key: &str, at: &str) -> Result<bool, String> {
    map.get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("{at}.{key} is not a boolean"))
}

fn uint<T: TryFrom<u64>>(map: &Map<String, Value>, key: &str, at: &str) -> Result<T, String> {
    map.get(key)
        .and_then(Value::as_u64)
        .and_then(|n| T::try_from(n).ok())
        .ok_or_else(|| format!("{at}.{key} is not an integer in range"))
}

fn int(map: &Map<String, Value>, key: &str, at: &str) -> Result<i32, String> {
    map.get(key)
        .and_then(Value::as_i64)
        .and_then(|n| i32::try_from(n).ok())
        .ok_or_else(|| format!("{at}.{key} is not an integer in range"))
}

fn pairs(map: &Map<String, Value>, key: &str, at: &str) -> Result<Vec<Pair>, String> {
    list(map, key, at)?
        .iter()
        .map(|pair| match pair.as_array().map(Vec::as_slice) {
            Some([Value::String(n), Value::String(v)]) => Ok((n.clone(), v.clone())),
            _ => Err(format!("{at}.{key}: an entry is not a pair of strings")),
        })
        .collect()
}

fn parse_session(value: &Value, at: &str) -> Result<SessionView, String> {
    let map = object(
        value,
        at,
        &["step", "opened", "partition", "url"],
        &["session"],
    )?;
    let opened = boolean(map, "opened", at)?;
    let session = if opened {
        Some(uint(map, "session", at)?)
    } else if map.contains_key("session") {
        return Err(format!("{at}: `session` without `opened: true`"));
    } else {
        None
    };
    Ok(SessionView {
        step: uint(map, "step", at)?,
        session,
        partition: opt_text(map, "partition", at)?,
        url: text(map, "url", at)?,
    })
}

fn parse_phase(value: &Value, at: &str) -> Result<Phase, String> {
    let map = object(
        value,
        at,
        &[
            "steps",
            "requests",
            "cookies",
            "actions",
            "intercepts",
            "results",
            "snapshots",
        ],
        &[],
    )?;
    let steps = match list(map, "steps", at)?.as_slice() {
        [] => None,
        [a, b] => Some((
            a.as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(|| format!("{at}.steps[0] is not a step number"))?,
            b.as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(|| format!("{at}.steps[1] is not a step number"))?,
        )),
        _ => return Err(format!("{at}.steps is not [] or [first, last]")),
    };
    let each = |key: &str| -> Result<Vec<(String, &Value)>, String> {
        Ok(list(map, key, at)?
            .iter()
            .enumerate()
            .map(|(i, v)| (format!("{at}.{key}[{i}]"), v))
            .collect())
    };
    Ok(Phase {
        steps,
        requests: each("requests")?
            .into_iter()
            .map(|(p, v)| parse_request(v, &p))
            .collect::<Result<_, _>>()?,
        cookies: each("cookies")?
            .into_iter()
            .map(|(p, v)| parse_cookie(v, &p))
            .collect::<Result<_, _>>()?,
        actions: each("actions")?
            .into_iter()
            .map(|(p, v)| parse_action(v, &p))
            .collect::<Result<_, _>>()?,
        intercepts: each("intercepts")?
            .into_iter()
            .map(|(p, v)| parse_intercept(v, &p))
            .collect::<Result<_, _>>()?,
        results: each("results")?
            .into_iter()
            .map(|(p, v)| {
                let m = object(v, &p, &["session", "src_code", "final_code"], &[])?;
                Ok(ResultView {
                    session: uint(m, "session", &p)?,
                    src_code: int(m, "src_code", &p)?,
                    final_code: int(m, "final_code", &p)?,
                })
            })
            .collect::<Result<_, String>>()?,
        snapshots: each("snapshots")?
            .into_iter()
            .map(|(p, v)| {
                let m = object(v, &p, &["entries"], &[])?;
                let entries = list(m, "entries", &p)?
                    .iter()
                    .enumerate()
                    .map(|(i, r)| parse_record(r, &format!("{p}.entries[{i}]")))
                    .collect::<Result<_, _>>()?;
                Ok(Snapshot { entries })
            })
            .collect::<Result<_, String>>()?,
    })
}

fn parse_request(value: &Value, at: &str) -> Result<RequestView, String> {
    let map = object(
        value,
        at,
        &["exchange", "method", "target", "fields", "response"],
        &[],
    )?;
    let response = match map.get("response") {
        Some(Value::Null) => None,
        Some(r) => {
            let p = format!("{at}.response");
            let m = object(r, &p, &["status", "fields", "complete"], &[])?;
            Some(ResponseView {
                status: uint(m, "status", &p)?,
                fields: pairs(m, "fields", &p)?,
                complete: boolean(m, "complete", &p)?,
            })
        }
        None => return Err(format!("{at}.response is missing")),
    };
    Ok(RequestView {
        exchange: uint(map, "exchange", at)?,
        method: text(map, "method", at)?,
        target: text(map, "target", at)?,
        fields: pairs(map, "fields", at)?,
        response,
    })
}

fn parse_cookie(value: &Value, at: &str) -> Result<CookieCall, String> {
    match value.get("call").and_then(Value::as_str) {
        Some("get") => {
            let m = object(value, at, &["call", "url", "value"], &[])?;
            Ok(CookieCall::Get {
                url: text(m, "url", at)?,
                value: opt_text(m, "value", at)?,
            })
        }
        Some("set") => {
            let m = object(value, at, &["call", "url", "values"], &[])?;
            let values = list(m, "values", at)?
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| format!("{at}.values: an entry is not a string"))
                })
                .collect::<Result<_, _>>()?;
            Ok(CookieCall::Set {
                url: text(m, "url", at)?,
                values,
            })
        }
        _ => Err(format!("{at}.call is not `get` or `set`")),
    }
}

fn parse_action(value: &Value, at: &str) -> Result<Action, String> {
    match value.get("do").and_then(Value::as_str) {
        Some("load_url") => {
            let m = object(value, at, &["do", "session", "url"], &[])?;
            Ok(Action::LoadUrl {
                session: uint(m, "session", at)?,
                url: text(m, "url", at)?,
            })
        }
        Some("load_data") => {
            let m = object(
                value,
                at,
                &["do", "session", "url", "fields", "body_len", "body_sha256"],
                &[],
            )?;
            Ok(Action::LoadData {
                session: uint(m, "session", at)?,
                url: text(m, "url", at)?,
                fields: pairs(m, "fields", at)?,
                body_len: uint(m, "body_len", at)?,
                body_sha256: text(m, "body_sha256", at)?,
            })
        }
        Some("message") => {
            let m = object(value, at, &["do", "session", "json"], &[])?;
            Ok(Action::Message {
                session: uint(m, "session", at)?,
                json: text(m, "json", at)?,
            })
        }
        _ => Err(format!(
            "{at}.do is not `load_url`, `load_data` or `message`"
        )),
    }
}

fn parse_intercept(value: &Value, at: &str) -> Result<InterceptView, String> {
    if value.get("null").is_some() {
        let m = object(value, at, &["session", "null"], &[])?;
        if m.get("null") != Some(&Value::Bool(true)) {
            return Err(format!("{at}.null is not `true`"));
        }
        return Ok(InterceptView::Null {
            session: uint(m, "session", at)?,
        });
    }
    let m = object(
        value,
        at,
        &[
            "session",
            "status",
            "fields",
            "body_len",
            "body_sha256",
            "complete",
        ],
        &[],
    )?;
    Ok(InterceptView::Document {
        session: uint(m, "session", at)?,
        status: uint(m, "status", at)?,
        fields: pairs(m, "fields", at)?,
        body_len: uint(m, "body_len", at)?,
        body_sha256: text(m, "body_sha256", at)?,
        complete: boolean(m, "complete", at)?,
    })
}

fn parse_record(value: &Value, at: &str) -> Result<RecordView, String> {
    let m = object(
        value,
        at,
        &[
            "partition",
            "url",
            "vary",
            "etag",
            "template_tag",
            "page_sha256",
            "template_sha256",
            "data_sha256",
            "fields",
        ],
        &[],
    )?;
    let vary = list(m, "vary", at)?
        .iter()
        .map(|pair| match pair.as_array().map(Vec::as_slice) {
            Some([Value::String(n), Value::String(v)]) => Ok((n.clone(), Some(v.clone()))),
            Some([Value::String(n), Value::Null]) => Ok((n.clone(), None)),
            _ => Err(format!("{at}.vary: an entry is not [name, value or null]")),
        })
        .collect::<Result<_, _>>()?;
    Ok(RecordView {
        partition: opt_text(m, "partition", at)?,
        url: text(m, "url", at)?,
        vary,
        etag: opt_text(m, "etag", at)?,
        template_tag: opt_text(m, "template_tag", at)?,
        page_sha256: opt_text(m, "page_sha256", at)?,
        template_sha256: opt_text(m, "template_sha256", at)?,
        data_sha256: opt_text(m, "data_sha256", at)?,
        fields: pairs(m, "fields", at)?,
    })
}
