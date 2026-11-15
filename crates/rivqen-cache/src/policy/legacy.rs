// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The legacy `cache-offline` rules of the canonical client (legacy wire
//! section 7; legacy client CL-15, CL-19, CL-25, CL-30, CL-39; R-CACHE-08).
//! Owner: WP-07 S1 T-03.
//!
//! Legacy mode has no fresh-entry skip (CL-39): the client sends the request
//! each time. This module decides only what happens to the cache.

use crate::header::CacheOffline;
use crate::policy::StoreDecision;

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
#[must_use]
pub fn legacy_cache_outcome(
    offline: CacheOffline,
    store: StoreDecision,
    unavailable_time_ms: u64,
) -> LegacyCacheOutcome {
    // Stub (T-03): the strictest answer.
    let _ = (offline, store, unavailable_time_ms);
    LegacyCacheOutcome {
        commit: false,
        remove_entry: true,
        disable_key_ms: None,
    }
}
