// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The classification of a response head (legacy client CL-13) and the
//! cache conditions of a response (CL-15, CL-25, CL-12), as pure functions.
//!
//! The body tests of CL-14 (CL-16: valid UTF-8; CL-32: not empty) need the
//! body, so the session applies them while the body arrives
//! (`exchange::Exchange::push`) and at its end.

use rivqen_cache::header::{
    CacheOffline, Vary, parse_cache_control, parse_cache_offline, parse_vary,
};
use rivqen_model::http::FieldLine;
use rivqen_proto_legacy::request::COOKIE;
use rivqen_proto_legacy::response::{
    CACHE_OFFLINE, LegacyHead, bare_etag, etag_value, is_safe_head, is_utf8_text,
};

/// Field name `set-cookie`, lower case.
const SET_COOKIE: &str = "set-cookie";
/// Field name `set-cookie2`, lower case.
const SET_COOKIE2: &str = "set-cookie2";
/// Field name `vary`, lower case.
const VARY: &str = "vary";

/// The outcome class of a response head of the current generation (CL-13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// CL-13 item 2, CL-17: a `304` or a `200` with the stored etag, with a
    /// visible entry and a head that passes `is_safe_head` (DL-226).
    NotModified,
    /// CL-18 item 1: a `5xx`, `408` or `429`.
    KeepCached,
    /// A normal load and the visible entry removed: CL-13 item 2 with a head
    /// that fails `is_safe_head` (DL-226), CL-44, CL-18 item 2, CL-14 and
    /// the charset of CL-16.
    Normal,
    /// CL-19: `cache-offline: http`.
    Http,
    /// CL-25: no visible entry, a legacy `200`.
    FirstLoad,
    /// CL-31: a visible entry, a template change.
    TemplateChange,
    /// CL-27: a visible entry, a data update.
    DataUpdate,
}

impl Kind {
    /// The outcome needs the body (CL-25, CL-27, CL-31), so the session
    /// buffers it, tests it (CL-16, CL-32) and a body failure is CL-33.
    pub(crate) const fn needs_body(self) -> bool {
        matches!(
            self,
            Self::FirstLoad | Self::TemplateChange | Self::DataUpdate
        )
    }
}

/// CL-18 item 1 (DL-124): the statuses that keep the cached page.
const fn keeps_cached_page(status: u16) -> bool {
    status >= 500 || status == 408 || status == 429
}

/// CL-13: the class of a response head. `entry_etag` is the etag of the
/// visible entry, or `None` without one.
pub(crate) fn classify(status: u16, fields: &[FieldLine], entry_etag: Option<&str>) -> Kind {
    if let Some(stored) = entry_etag {
        let same_etag =
            status == 200 && etag_value(fields).is_some_and(|value| bare_etag(value) == stored);
        if status == 304 || same_etag {
            // DL-226: the head test runs also before CL-17.
            return if is_safe_head(fields) {
                Kind::NotModified
            } else {
                Kind::Normal
            };
        }
    }
    if (300..=399).contains(&status) {
        return Kind::Normal;
    }
    if status != 200 {
        return if keeps_cached_page(status) {
            Kind::KeepCached
        } else {
            Kind::Normal
        };
    }
    let Some(head) = LegacyHead::read(fields) else {
        return Kind::Normal;
    };
    // CL-16, the charset part: an empty body is valid UTF-8, so this tests
    // only the charset of `Content-Type`.
    if !is_utf8_text(fields, &[]) {
        return Kind::Normal;
    }
    if offline(fields) == CacheOffline::Http {
        return Kind::Http;
    }
    if entry_etag.is_none() {
        Kind::FirstLoad
    } else if head.is_template_change() {
        Kind::TemplateChange
    } else {
        Kind::DataUpdate
    }
}

/// CL-15: the `cache-offline` value of a response (the first line), with
/// `no-store` counted as `false`; `http` wins over `no-store`.
pub(crate) fn offline(fields: &[FieldLine]) -> CacheOffline {
    let raw = fields
        .iter()
        .find(|line| line.is(CACHE_OFFLINE))
        .map(|line| line.value.trim_matches([' ', '\t']));
    match parse_cache_offline(raw) {
        CacheOffline::Http => CacheOffline::Http,
        _ if parse_cache_control(fields).no_store => CacheOffline::False,
        value => value,
    }
}

/// CL-25, CL-30: the value allows a commit (`true` or `store`).
pub(crate) fn offline_allows_commit(value: CacheOffline) -> bool {
    matches!(value, CacheOffline::True | CacheOffline::Store)
}

/// CL-11 item 2: the `Set-Cookie` and `Set-Cookie2` values, in order.
pub(crate) fn set_cookie_values(fields: &[FieldLine]) -> Vec<String> {
    fields
        .iter()
        .filter(|line| line.is(SET_COOKIE) || line.is(SET_COOKIE2))
        .map(|line| line.value.clone())
        .collect()
}

/// CL-25 (DL-123, H-62): a non-empty request `cookie`, or a `Set-Cookie`
/// or `Set-Cookie2` line in the response.
fn is_credentialed(request: &[FieldLine], response: &[FieldLine]) -> bool {
    request
        .iter()
        .any(|line| line.is(COOKIE) && !line.value.is_empty())
        || response
            .iter()
            .any(|line| line.is(SET_COOKIE) || line.is(SET_COOKIE2))
}

/// CL-25 (DL-123, DL-132): the credentials allow a commit: the exchange is
/// not credentialed, or the app allows authenticated caching and the
/// partition is not the one of the account `null`.
pub(crate) fn credentials_allow_commit(
    request: &[FieldLine],
    response: &[FieldLine],
    authenticated_cache: bool,
    anonymous: bool,
) -> bool {
    !is_credentialed(request, response) || (authenticated_cache && !anonymous)
}

/// CL-12: `Vary: *` (or a member that is not a field name) forbids a
/// commit.
pub(crate) fn vary_forbids_commit(fields: &[FieldLine]) -> bool {
    matches!(parse_vary(fields), Vary::Any)
}

/// CL-17 (DL-126): the response has a `Vary` line.
pub(crate) fn has_vary(fields: &[FieldLine]) -> bool {
    fields.iter().any(|line| line.is(VARY))
}

/// The value of the first `template-tag` line, or the empty text.
pub(crate) fn template_tag(fields: &[FieldLine]) -> &str {
    fields
        .iter()
        .find(|line| line.is(rivqen_proto_legacy::response::TEMPLATE_TAG))
        .map_or("", |line| line.value.trim_matches([' ', '\t']))
}

/// The bare etag of a response (CL-15), or the empty text without one.
pub(crate) fn response_etag(fields: &[FieldLine]) -> &str {
    etag_value(fields).map_or("", bare_etag)
}
