// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The legacy `cache-offline` rules of the canonical client (legacy wire
//! section 7; legacy client CL-15, CL-19, CL-25, CL-30, CL-39; R-CACHE-08).
//! Owner: WP-07 S1 T-03.
//!
//! Legacy mode has no fresh-entry skip (CL-39): the client sends the request
//! each time. This module decides only what happens to the cache.

use crate::header::CacheOffline;
use crate::policy::{SkipReason, StoreDecision};

/// What the cache does after a legacy response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegacyCacheOutcome {
    /// Commit the new record (CL-25, CL-26).
    pub commit: bool,
    /// Remove the entry of the key.
    pub remove_entry: bool,
    /// Disable the key for this time in milliseconds (CL-19).
    pub disable_key_ms: Option<u64>,
}

/// The cache part of the legacy rules: `no-store` wins over `true` and
/// `store`; `http` removes the entry and disables the key, also with
/// `no-store` (CL-15).
///
/// `store` is the [`crate::policy::store_decision`] of the response. Rule
/// order; the first rule that matches decides (CL-13 steps 3 to 7):
///
/// 1. [`SkipReason::TransientStatus`] (a `5xx`, `408` or `429`): the entry
///    stays, nothing else (CL-18 item 1).
/// 2. [`SkipReason::Status`] (a `3xx` or another status than `200`): the
///    entry is removed, the key is not disabled (CL-44, CL-18 item 2).
/// 3. `cache-offline: http`: the entry is removed and the key is disabled
///    for `unavailable_time_ms` (CL-19), also with `no-store`, `Vary: *` or
///    a credentialed exchange.
/// 4. Another skip (`no-store`, `Vary: *`, a credentialed exchange that the
///    app does not allow): no commit, the entry is removed (CL-15, CL-25,
///    CL-27, CL-39).
/// 5. `true` or `store`: commit (CL-25, CL-30).
/// 6. `false`, another value or no field: no commit, the entry is removed
///    (CL-14, CL-15, CL-27).
///
/// The caller handles a `304` and an etag match (CL-13 step 2, CL-17)
/// before; it never passes them here.
#[must_use]
pub fn legacy_cache_outcome(
    offline: CacheOffline,
    store: StoreDecision,
    unavailable_time_ms: u64,
) -> LegacyCacheOutcome {
    match (store, offline) {
        (StoreDecision::Skip(SkipReason::TransientStatus), _) => KEEP,
        (StoreDecision::Skip(SkipReason::Status), _) => REMOVE,
        (_, CacheOffline::Http) => LegacyCacheOutcome {
            commit: false,
            remove_entry: true,
            disable_key_ms: Some(unavailable_time_ms),
        },
        (StoreDecision::Skip(_), _) | (StoreDecision::Store, CacheOffline::False) => REMOVE,
        (StoreDecision::Store, CacheOffline::True | CacheOffline::Store) => COMMIT,
    }
}

/// The entry stays; no commit.
const KEEP: LegacyCacheOutcome = LegacyCacheOutcome {
    commit: false,
    remove_entry: false,
    disable_key_ms: None,
};

/// No commit; the entry of the key is removed.
const REMOVE: LegacyCacheOutcome = LegacyCacheOutcome {
    commit: false,
    remove_entry: true,
    disable_key_ms: None,
};

/// Commit the new record; it replaces the entry of the key.
const COMMIT: LegacyCacheOutcome = LegacyCacheOutcome {
    commit: true,
    remove_entry: false,
    disable_key_ms: None,
};
