// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The members of a canonical client `case.json` that the runner needs
//! (`fixtures/legacy/README.md` "Canonical client `case.json`"; the step
//! words of `tools/upstream-lab/client/CONTRACT.md` section 3).
//!
//! The generator checks a case in full (`gen.mjs --check`, check 6); this
//! reader checks what the runner reads and the limits of the README, so that
//! a damaged file gives an error, never a panic or an unbounded run. The
//! `responses` member is not read: the runner takes every response from
//! `exchanges.json` (DL-216). A `case` entry of the schedule is not a step
//! and is dropped; a `case` step stays a step (it is numbered).

use serde_json::{Map, Value};

use crate::exchange::MAX_BODY_BYTES;

/// At most this many steps (README limits).
pub const MAX_STEPS: usize = 256;
/// At most this many `create_session` steps (README limits).
pub const MAX_SESSIONS: usize = 16;
/// The largest `settle.ms` and `timeout_ms` (README limits).
pub const MAX_SETTLE_MS: u64 = 3_600_000;
/// The default page URL (README `url`).
pub const DEFAULT_URL: &str = "https://shop.example.com/catalog";
/// The default `http` period of the case `config` (6 hours).
pub const DEFAULT_UNAVAILABLE_TIME_MS: u64 = 21_600_000;
/// The largest `unavailable_time_ms` (README `config`).
pub const MAX_UNAVAILABLE_TIME_MS: u64 = 86_400_000;

/// The presentation of the case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `quick`.
    Quick,
    /// `standard`.
    Standard,
}

/// The `config` member (README table of keys). Only the keys with a
/// canonical effect (CL-03) are kept; the others are checked for their type.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "the boolean keys of the case config (README table)"
)]
pub struct CaseConfig {
    /// The account at the start; `None` for `null`.
    pub account: Option<String>,
    /// The cookie of the runtime at the start.
    pub cookie: Option<String>,
    /// The network is valid at the start.
    pub network_valid: bool,
    /// The `accept-diff` value (CL-09).
    pub accept_diff: bool,
    /// The `http` period of CL-19.
    pub unavailable_time_ms: u64,
    /// CL-37 (off by default). The session contract has no switch for it:
    /// a case that turns it on is a problem of the run.
    pub support_local_server: bool,
    /// CL-25: a credentialed exchange can commit.
    pub authenticated_cache: bool,
}

impl Default for CaseConfig {
    fn default() -> Self {
        Self {
            account: Some("rq-test-account".to_owned()),
            cookie: None,
            network_valid: true,
            accept_diff: true,
            unavailable_time_ms: DEFAULT_UNAVAILABLE_TIME_MS,
            support_local_server: false,
            authenticated_cache: false,
        }
    }
}

/// Where `hold` holds the next exchange (CONTRACT section 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldPoint {
    /// Before the request goes on.
    Request,
    /// After the response head.
    Head,
    /// After `bytes` body bytes.
    Body,
}

/// One step of the schedule. `session` is the optional 1-based index of the
/// opened session that a harness step acts on (default: the last opened).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Step {
    /// `create_session`.
    CreateSession,
    /// `bind_client`.
    BindClient(Option<u32>),
    /// `client_ready`.
    ClientReady(Option<u32>),
    /// `request_resource`.
    RequestResource(Option<u32>),
    /// `page_finished`.
    PageFinished(Option<u32>),
    /// `get_diff`.
    GetDiff(Option<u32>),
    /// `refresh`.
    Refresh(Option<u32>),
    /// `destroy`.
    Destroy(Option<u32>),
    /// `set_account` with the new account.
    SetAccount(Option<String>),
    /// `set_cookie` with the new cookie.
    SetCookie(Option<String>),
    /// `set_network` with the new validity.
    SetNetwork(bool),
    /// `hold` the next exchange.
    Hold {
        /// The point.
        point: HoldPoint,
        /// The body bytes before a `body` hold.
        bytes: usize,
    },
    /// `release` the current hold.
    Release,
    /// `cut` the next exchange after `bytes` body bytes.
    Cut(usize),
    /// `drop_field`: already applied in `exchanges.json`.
    DropField,
    /// `set_field`: already applied in `exchanges.json`.
    SetField,
    /// `set_body_member`: already applied in `exchanges.json`.
    SetBodyMember,
    /// A `case` step: the responses of `exchanges.json` already follow it.
    Case,
    /// `server_down`.
    ServerDown,
    /// `server_up`.
    ServerUp,
    /// `wait` for an event name.
    Wait(String),
    /// `settle` for `ms` milliseconds.
    Settle(u64),
    /// `snapshot_cache`.
    SnapshotCache,
}

/// What the runner reads of a canonical client `case.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseSpec {
    /// The fixture id.
    pub id: String,
    /// Quick or Standard.
    pub mode: Mode,
    /// The page URL.
    pub url: String,
    /// The configuration.
    pub config: CaseConfig,
    /// The number of `cache_before` entries.
    pub cache_before: usize,
    /// The steps in order (the `case` entries are dropped).
    pub schedule: Vec<Step>,
}

impl CaseSpec {
    /// Read a canonical client `case.json`.
    ///
    /// # Errors
    /// The member that breaks the README form or a limit.
    pub fn from_json(value: &Value) -> Result<Self, String> {
        let top = value.as_object().ok_or("case.json is not an object")?;
        let id = string(top, "id")?;
        if top.get("kind").and_then(Value::as_str) != Some("client") {
            return Err("`kind` is not `client`".to_owned());
        }
        if top.get("profile").and_then(Value::as_str) != Some("legacy-canonical") {
            return Err("`profile` is not `legacy-canonical`".to_owned());
        }
        let mode = match top.get("mode").and_then(Value::as_str) {
            Some("quick") if id.ends_with("-Q") => Mode::Quick,
            Some("standard") if id.ends_with("-S") => Mode::Standard,
            _ => return Err("`mode` is not `quick` (id `-Q`) or `standard` (id `-S`)".to_owned()),
        };
        let url = match top.get("url") {
            None => DEFAULT_URL.to_owned(),
            Some(Value::String(u)) => u.clone(),
            Some(_) => return Err("`url` is not a string".to_owned()),
        };
        let config = match top.get("config") {
            None => CaseConfig::default(),
            Some(c) => config(c)?,
        };
        let cache_before = top
            .get("cache_before")
            .and_then(Value::as_array)
            .ok_or("`cache_before` is not a list")?
            .len();
        if cache_before > crate::exchange::MAX_RECORDS {
            return Err("`cache_before` has too many entries".to_owned());
        }
        let items = top
            .get("schedule")
            .and_then(Value::as_array)
            .ok_or("`schedule` is not a list")?;
        let mut schedule = Vec::new();
        for (i, item) in items.iter().enumerate() {
            let at = format!("schedule[{i}]");
            let map = item.as_object().ok_or(format!("{at} is not an object"))?;
            if map.contains_key("case") && !map.contains_key("do") {
                continue;
            }
            if schedule.len() >= MAX_STEPS {
                return Err(format!("`schedule` has more than {MAX_STEPS} steps"));
            }
            schedule.push(step(map, &at)?);
        }
        let sessions = schedule
            .iter()
            .filter(|s| matches!(s, Step::CreateSession))
            .count();
        if sessions > MAX_SESSIONS {
            return Err(format!("more than {MAX_SESSIONS} create_session steps"));
        }
        Ok(Self {
            id,
            mode,
            url,
            config,
            cache_before,
            schedule,
        })
    }
}

fn string(map: &Map<String, Value>, key: &str) -> Result<String, String> {
    map.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("`{key}` is not a string"))
}

fn opt_string(map: &Map<String, Value>, key: &str, at: &str) -> Result<Option<String>, String> {
    match map.get(key) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        _ => Err(format!("{at}.{key} is not a string or null")),
    }
}

fn boolean(map: &Map<String, Value>, key: &str, at: &str, default: bool) -> Result<bool, String> {
    match map.get(key) {
        None => Ok(default),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(format!("{at}.{key} is not a boolean")),
    }
}

fn number(map: &Map<String, Value>, key: &str, at: &str, max: u64) -> Result<u64, String> {
    map.get(key)
        .and_then(Value::as_u64)
        .filter(|n| *n <= max)
        .ok_or_else(|| format!("{at}.{key} is not an integer from 0 to {max}"))
}

fn bytes(map: &Map<String, Value>, at: &str) -> Result<usize, String> {
    let max = u64::try_from(MAX_BODY_BYTES).unwrap_or(u64::MAX);
    let n = number(map, "bytes", at, max)?;
    usize::try_from(n).map_err(|_| format!("{at}.bytes is too large"))
}

const CONFIG_KEYS: [&str; 15] = [
    "account",
    "cookie",
    "network_valid",
    "accept_diff",
    "account_related",
    "reload_in_bad_network",
    "support_cache_control",
    "support_local_server",
    "auto_start",
    "get_cookie_when_session_create",
    "cache_verify_with_sha1",
    "unavailable_time_ms",
    "connect_timeout_ms",
    "read_timeout_ms",
    "authenticated_cache",
];

fn config(value: &Value) -> Result<CaseConfig, String> {
    let at = "config";
    let map = value.as_object().ok_or("`config` is not an object")?;
    if let Some(extra) = map.keys().find(|k| !CONFIG_KEYS.contains(&k.as_str())) {
        return Err(format!("config: unknown key `{extra}`"));
    }
    let defaults = CaseConfig::default();
    for key in [
        "account_related",
        "reload_in_bad_network",
        "support_cache_control",
        "auto_start",
        "get_cookie_when_session_create",
        "cache_verify_with_sha1",
    ] {
        boolean(map, key, at, true)?;
    }
    for key in ["connect_timeout_ms", "read_timeout_ms"] {
        if map.contains_key(key) {
            number(map, key, at, u64::MAX)?;
        }
    }
    Ok(CaseConfig {
        account: if map.contains_key("account") {
            opt_string(map, "account", at)?
        } else {
            defaults.account
        },
        cookie: if map.contains_key("cookie") {
            opt_string(map, "cookie", at)?
        } else {
            defaults.cookie
        },
        network_valid: boolean(map, "network_valid", at, defaults.network_valid)?,
        accept_diff: boolean(map, "accept_diff", at, defaults.accept_diff)?,
        unavailable_time_ms: if map.contains_key("unavailable_time_ms") {
            number(map, "unavailable_time_ms", at, MAX_UNAVAILABLE_TIME_MS)?
        } else {
            defaults.unavailable_time_ms
        },
        support_local_server: boolean(
            map,
            "support_local_server",
            at,
            defaults.support_local_server,
        )?,
        authenticated_cache: boolean(map, "authenticated_cache", at, defaults.authenticated_cache)?,
    })
}

fn session(map: &Map<String, Value>, at: &str) -> Result<Option<u32>, String> {
    match map.get("session") {
        None => Ok(None),
        Some(v) => v
            .as_u64()
            .filter(|n| *n >= 1)
            .and_then(|n| u32::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| format!("{at}.session is not a session index")),
    }
}

fn step(map: &Map<String, Value>, at: &str) -> Result<Step, String> {
    let word = map
        .get("do")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{at}.do is not a string"))?;
    let s = session(map, at)?;
    Ok(match word {
        "create_session" => Step::CreateSession,
        "bind_client" => Step::BindClient(s),
        "client_ready" => Step::ClientReady(s),
        "request_resource" => Step::RequestResource(s),
        "page_finished" => Step::PageFinished(s),
        "get_diff" => Step::GetDiff(s),
        "refresh" => Step::Refresh(s),
        "destroy" => Step::Destroy(s),
        "set_account" => Step::SetAccount(opt_string(map, "value", at)?),
        "set_cookie" => Step::SetCookie(opt_string(map, "value", at)?),
        "set_network" => Step::SetNetwork(
            map.get("valid")
                .and_then(Value::as_bool)
                .ok_or_else(|| format!("{at}.valid is not a boolean"))?,
        ),
        "hold" => {
            let point = match map.get("point").and_then(Value::as_str) {
                Some("request") => HoldPoint::Request,
                Some("head") => HoldPoint::Head,
                Some("body") => HoldPoint::Body,
                _ => return Err(format!("{at}.point is not `request`, `head` or `body`")),
            };
            let bytes = if point == HoldPoint::Body || map.contains_key("bytes") {
                bytes(map, at)?
            } else {
                0
            };
            Step::Hold { point, bytes }
        }
        "release" => Step::Release,
        "cut" => Step::Cut(bytes(map, at)?),
        "drop_field" => Step::DropField,
        "set_field" => Step::SetField,
        "set_body_member" => Step::SetBodyMember,
        "case" => Step::Case,
        "server_down" => Step::ServerDown,
        "server_up" => Step::ServerUp,
        "wait" => {
            if map.contains_key("timeout_ms") {
                number(map, "timeout_ms", at, MAX_SETTLE_MS)?;
            }
            Step::Wait(string(map, "for").map_err(|e| format!("{at}: {e}"))?)
        }
        "settle" => Step::Settle(number(map, "ms", at, MAX_SETTLE_MS)?),
        "snapshot_cache" => Step::SnapshotCache,
        other => return Err(format!("{at}: unknown step word `{other}`")),
    })
}
