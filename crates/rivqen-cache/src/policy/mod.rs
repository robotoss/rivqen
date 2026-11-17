// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Freshness and the store decision (freshness sections 2 and 3; RFC 9111
//! sections 3, 4.2 and 4.3; RFC 5861; DL-182, DL-183). Owner: WP-07 S1 T-03.
//!
//! Contract (architect): the names and the meaning of the public items. T-03
//! may add fields and variants; it lists them in its report. Every function
//! is pure and takes the time as an input. There is no heuristic freshness:
//! a response without an explicit lifetime is stale at once (DL-182).

use rivqen_model::time::UnixTime;

use crate::header::{CacheControl, Vary};
use crate::partition::Credentials;

#[cfg(feature = "legacy")]
pub mod legacy;

/// The `Expires` field of a response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expires {
    /// No `Expires` field.
    Absent,
    /// A value that is not a valid date: already expired (RFC 9111 section
    /// 5.3).
    Invalid,
    /// A valid date.
    At(UnixTime),
}

/// What an entry keeps to compute its freshness (data model section 3,
/// `EntryMeta.policy`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryPolicy {
    /// The `Cache-Control` directives of the stored response.
    pub cache_control: CacheControl,
    /// The `Expires` field.
    pub expires: Expires,
    /// The `Date` field, when it is a valid date.
    pub date: Option<UnixTime>,
    /// The `Age` field, when it is valid.
    pub age: Option<u64>,
    /// The time of the request (RFC 9111 section 4.2.3, `request_time`).
    pub request_time: UnixTime,
    /// The time of the response (`response_time`). A `304` that meets its
    /// conditions moves it (freshness section 2, last paragraph).
    pub response_time: UnixTime,
    /// `ttl_seconds` of the RQP manifest: an upper bound of the lifetime
    /// (manifest and patch section 1.4). `None` in legacy mode.
    pub ttl_seconds: Option<u32>,
    /// `offline` of the RQP manifest: allows rule 6 of freshness section 2.
    pub offline: bool,
}

/// How a stored entry may be used for a navigation (freshness section 2,
/// rules 4, 5 and 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Use {
    /// Rule 4: show it; no request is needed.
    Fresh,
    /// Rule 5: show it and revalidate in the background.
    StaleWhileRevalidate,
    /// Rule 8: revalidate before it is shown.
    Revalidate,
}

/// Why a request failed (freshness section 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// A network error or a server error that RFC 5861 section 4 names.
    Network,
    /// A TLS error: never a stale sensitive page (freshness section 3,
    /// rule 2).
    Tls,
}

/// Why a response is not stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SkipReason {
    /// `no-store` in the request or the response (R-CACHE-02).
    NoStore,
    /// `Vary: *`.
    VaryAny,
    /// A credentialed exchange and no permission of the app (R-CACHE-03).
    Credentialed,
    /// The status is not one that the cache stores: a `3xx`, or a status
    /// other than `200` that is not a [`SkipReason::TransientStatus`]
    /// (legacy client CL-44, CL-18 item 2).
    Status,
    /// A server error `5xx`, `408` or `429` (legacy client CL-18 item 1):
    /// the server said nothing about the entry, so the entry of the key
    /// stays. Added by WP-07 S1 T-03: the legacy rules keep the entry for
    /// these codes and remove it for the other codes of
    /// [`SkipReason::Status`].
    TransientStatus,
}

/// The store decision for a full document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreDecision {
    /// Store the snapshot.
    Store,
    /// Do not store it; an entry of the key is removed, except for
    /// [`SkipReason::TransientStatus`].
    Skip(SkipReason),
}

/// The inputs of the store decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreInput<'a> {
    /// Status code of the response.
    pub status: u16,
    /// `Cache-Control` of the request.
    pub request: &'a CacheControl,
    /// `Cache-Control` of the response.
    pub response: &'a CacheControl,
    /// `Vary` of the response.
    pub vary: &'a Vary,
    /// The credentials of the request (H-62).
    pub credentials: Credentials,
    /// The response has a `Set-Cookie` or `Set-Cookie2` field.
    pub set_cookie: bool,
    /// The app allows the cache of authenticated pages (R-CACHE-03; off by
    /// default).
    pub authenticated_cache: bool,
}

/// The status code that the cache stores for a full document.
const STORED_STATUS: u16 = 200;

/// The freshness lifetime in seconds (RFC 9111 section 4.2.1), bounded by
/// `ttl_seconds`.
///
/// Rule order (freshness section 2, "Arithmetic of rules 4 to 6"; DL-182,
/// DL-198 items b, c, d):
///
/// 1. `invalid_freshness`: 0.
/// 2. `max-age`: its value (`Expires` is ignored, also when it is not
///    valid).
/// 3. `Expires` minus `Date`, when both are valid dates; 0 when `Expires`
///    is not after `Date`. `Expires` without `Date`, or an `Expires` that is
///    not valid: 0.
/// 4. Else 0: no heuristic freshness.
///
/// Then the smaller of that value and `ttl_seconds`.
#[must_use]
pub fn freshness_lifetime(policy: &EntryPolicy) -> u64 {
    let server = server_lifetime(policy);
    match policy.ttl_seconds {
        Some(ttl) => server.min(u64::from(ttl)),
        None => server,
    }
}

/// Steps 1 to 4 of [`freshness_lifetime`], before the `ttl_seconds` bound.
fn server_lifetime(policy: &EntryPolicy) -> u64 {
    let cc = &policy.cache_control;
    if cc.invalid_freshness {
        return 0;
    }
    if let Some(max_age) = cc.max_age {
        return max_age;
    }
    match (policy.expires, policy.date) {
        // `since` is `None` when `Expires` is before `Date`: 0, never a
        // negative lifetime (DL-198 item c).
        (Expires::At(expires), Some(date)) => expires.since(date).unwrap_or(0),
        // No `Date` (DL-198 item b), an `Expires` that is not valid (RFC
        // 9111 section 5.3) or no `Expires` (DL-182).
        _ => 0,
    }
}

/// True when the entry has valid explicit freshness information: `max-age`,
/// or a valid `Expires` with a valid `Date` (DL-198 item f). `max-age=0`
/// and an `Expires` that is not after `Date` are explicit.
fn explicit_freshness(policy: &EntryPolicy) -> bool {
    let cc = &policy.cache_control;
    if cc.invalid_freshness {
        return false;
    }
    cc.max_age.is_some() || (matches!(policy.expires, Expires::At(_)) && policy.date.is_some())
}

/// True when a directive forbids every use without a request: `no-store`
/// (rule 1), `no-cache` (RFC 9111 section 5.2.2.4).
fn never_reuse(cc: &CacheControl) -> bool {
    cc.no_store || cc.no_cache
}

/// True when a stale use is allowed at all: valid explicit freshness, and
/// no `no-store`, `no-cache` or `must-revalidate` (DL-198 items e and f).
fn stale_use_allowed(policy: &EntryPolicy) -> bool {
    let cc = &policy.cache_control;
    !never_reuse(cc) && !cc.must_revalidate && explicit_freshness(policy)
}

/// True when `age` is inside a stale window of `window` seconds that starts
/// at the end of `lifetime`; the window is inclusive (DL-198 items d, g).
/// An `age` that is still inside the lifetime is also inside.
fn within_window(age: u64, lifetime: u64, window: u64) -> bool {
    // When the sum does not fit, the true sum is larger than every `u64`
    // age, so the saturated sum gives the same answer.
    age <= lifetime.saturating_add(window)
}

/// The current age in seconds (RFC 9111 section 4.2.3). `None` when `now`
/// is before the response time: the entry is then stale (DL-183).
///
/// - `apparent_age = max(0, response_time - Date)`; 0 without a `Date`.
/// - `corrected_age_value = Age + (response_time - request_time)`; an
///   absent or not valid `Age` is 0; a `request_time` after the
///   `response_time` (the clock went back) gives the delay 0.
/// - `corrected_initial_age = max(apparent_age, corrected_age_value)`
///   (DL-198 item a).
/// - `current_age = corrected_initial_age + (now - response_time)`.
///
/// Every sum saturates at `u64::MAX`: a larger age is the stricter result.
#[must_use]
pub fn current_age(policy: &EntryPolicy, now: UnixTime) -> Option<u64> {
    let resident_time = now.since(policy.response_time)?;
    let apparent_age = policy
        .date
        .and_then(|date| policy.response_time.since(date))
        .unwrap_or(0);
    let response_delay = policy.response_time.since(policy.request_time).unwrap_or(0);
    let corrected_age_value = policy.age.unwrap_or(0).saturating_add(response_delay);
    let corrected_initial_age = apparent_age.max(corrected_age_value);
    Some(corrected_initial_age.saturating_add(resident_time))
}

/// Rules 4, 5 and 8 of freshness section 2 for a readable entry that passed
/// its integrity check. `sensitive` is true when the app marked the route
/// sensitive (freshness section 3, rules 1 and 2).
///
/// Rule order:
///
/// 1. `no-store` or `no-cache`: [`Use::Revalidate`].
/// 2. `now` before the response time (no age): [`Use::Revalidate`].
/// 3. age less than [`freshness_lifetime`]: [`Use::Fresh`].
/// 4. A stale use is allowed (valid explicit freshness, no
///    `must-revalidate`), `stale-while-revalidate` is present, the age is
///    at most the lifetime plus its value, `ttl_seconds` is not `0` and the
///    route is not sensitive: [`Use::StaleWhileRevalidate`].
/// 5. Else [`Use::Revalidate`].
///
/// `ttl_seconds` `0` means that the client revalidates before it shows the
/// snapshot (manifest and patch section 1.4), so it has no stale window.
/// A stale sensitive page is not shown before the request: a TLS failure of
/// the background request would come after the page is visible (freshness
/// section 3 rule 2).
#[must_use]
pub fn lookup_use(policy: &EntryPolicy, now: UnixTime, sensitive: bool) -> Use {
    let cc = &policy.cache_control;
    if never_reuse(cc) {
        return Use::Revalidate;
    }
    let Some(age) = current_age(policy, now) else {
        return Use::Revalidate;
    };
    let lifetime = freshness_lifetime(policy);
    if age < lifetime {
        return Use::Fresh;
    }
    let window_allowed = !sensitive && policy.ttl_seconds != Some(0) && stale_use_allowed(policy);
    match cc.stale_while_revalidate {
        Some(window) if window_allowed && within_window(age, lifetime, window) => {
            Use::StaleWhileRevalidate
        }
        _ => Use::Revalidate,
    }
}

/// Rule 6 of freshness section 2 and the rules of section 3: true when the
/// stale entry may be shown, with the stale state, after `failure`.
///
/// True only when every condition holds: `offline` of the manifest is true;
/// not a TLS failure on a sensitive route (section 3 rule 2); a stale use
/// is allowed (valid explicit freshness, no `no-store`, `no-cache` or
/// `must-revalidate`); `stale-if-error` is present; `now` is not before the
/// response time (DL-198 item h); the age is at most the lifetime plus the
/// `stale-if-error` value (inclusive).
#[must_use]
pub fn serve_on_failure(
    policy: &EntryPolicy,
    now: UnixTime,
    failure: Failure,
    sensitive: bool,
) -> bool {
    if !policy.offline {
        return false;
    }
    if failure == Failure::Tls && sensitive {
        return false;
    }
    if !stale_use_allowed(policy) {
        return false;
    }
    let Some(window) = policy.cache_control.stale_if_error else {
        return false;
    };
    let Some(age) = current_age(policy, now) else {
        return false;
    };
    within_window(age, freshness_lifetime(policy), window)
}

/// Rule 1 of freshness section 2 and R-CACHE-03 for a full document.
///
/// Rule order; the first rule that matches decides:
///
/// 1. `no-store` in the request or the response: [`SkipReason::NoStore`]
///    (R-CACHE-02).
/// 2. `Vary: *`: [`SkipReason::VaryAny`].
/// 3. A credentialed exchange (credentials present or unknown, or a
///    `Set-Cookie` or `Set-Cookie2` field; H-62) without
///    `authenticated_cache`: [`SkipReason::Credentialed`] (R-CACHE-03).
/// 4. A status other than `200`: [`SkipReason::TransientStatus`] for a
///    `5xx`, `408` or `429`, else [`SkipReason::Status`].
/// 5. Else [`StoreDecision::Store`]. `private` and `no-cache` allow the
///    store (a private cache; RFC 9111 sections 5.2.2.4 and 5.2.2.7).
///
/// The caller never passes the fields of an RQP delta (DL-199).
#[must_use]
pub fn store_decision(input: &StoreInput<'_>) -> StoreDecision {
    if input.request.no_store || input.response.no_store {
        return StoreDecision::Skip(SkipReason::NoStore);
    }
    if matches!(input.vary, Vary::Any) {
        return StoreDecision::Skip(SkipReason::VaryAny);
    }
    let credentialed = input.credentials != Credentials::Absent || input.set_cookie;
    if credentialed && !input.authenticated_cache {
        return StoreDecision::Skip(SkipReason::Credentialed);
    }
    if input.status != STORED_STATUS {
        return StoreDecision::Skip(status_reason(input.status));
    }
    StoreDecision::Store
}

/// The skip reason of a status other than `200` (legacy client CL-18).
fn status_reason(status: u16) -> SkipReason {
    if (500..=599).contains(&status) || status == 408 || status == 429 {
        SkipReason::TransientStatus
    } else {
        SkipReason::Status
    }
}
