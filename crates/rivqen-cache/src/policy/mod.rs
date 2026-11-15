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
    /// The status is not one that the cache stores.
    Status,
}

/// The store decision for a full document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreDecision {
    /// Store the snapshot.
    Store,
    /// Do not store it; an entry of the key is removed.
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

/// The freshness lifetime in seconds (RFC 9111 section 4.2.1), bounded by
/// `ttl_seconds`.
#[must_use]
pub fn freshness_lifetime(policy: &EntryPolicy) -> u64 {
    // Stub (T-03): the strictest answer.
    let _ = policy;
    0
}

/// The current age in seconds (RFC 9111 section 4.2.3). `None` when `now`
/// is before the response time: the entry is then stale (DL-183).
#[must_use]
pub fn current_age(policy: &EntryPolicy, now: UnixTime) -> Option<u64> {
    // Stub (T-03): the strictest answer.
    let _ = (policy, now);
    None
}

/// Rules 4, 5 and 8 of freshness section 2 for a readable entry that passed
/// its integrity check.
#[must_use]
pub fn lookup_use(policy: &EntryPolicy, now: UnixTime) -> Use {
    // Stub (T-03): the strictest answer.
    let _ = (policy, now);
    Use::Revalidate
}

/// Rule 6 of freshness section 2 and the rules of section 3: true when the
/// stale entry may be shown, with the stale state, after `failure`.
#[must_use]
pub fn serve_on_failure(
    policy: &EntryPolicy,
    now: UnixTime,
    failure: Failure,
    sensitive: bool,
) -> bool {
    // Stub (T-03): the strictest answer.
    let _ = (policy, now, failure, sensitive);
    false
}

/// Rule 1 of freshness section 2 and R-CACHE-03 for a full document.
#[must_use]
pub fn store_decision(input: &StoreInput<'_>) -> StoreDecision {
    // Stub (T-03): the strictest answer.
    let _ = input;
    StoreDecision::Skip(SkipReason::NoStore)
}
