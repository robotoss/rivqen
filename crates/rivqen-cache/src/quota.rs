// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The L2 eviction plan (`diskL2Bytes`; freshness section 4, "Quota
//! exceeded"). Owner: WP-07 S1 T-06.
//!
//! Contract (architect): the public items of this file.

use rivqen_model::ids::CacheKey;
use rivqen_model::time::UnixTime;

use crate::key::Partition;

/// The size and the last use of one stored entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryUsage {
    /// The partition.
    pub partition: Partition,
    /// The key.
    pub key: CacheKey,
    /// The stored size in bytes.
    pub size_bytes: u64,
    /// When the entry was last used.
    pub last_used_at: UnixTime,
}

/// The new entry is larger than the whole budget: it is not stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooLarge;

/// The entries to remove before an entry of `incoming_bytes` is stored in
/// `writer`, so that the total stays within `budget_bytes`: least recently
/// used entries of the partition of the writer first, then of the other
/// partitions. The entry that the write replaces (`replaced`) does not count
/// and is never in the result.
///
/// # Errors
///
/// [`TooLarge`] when `incoming_bytes` is over `budget_bytes`.
pub fn plan_eviction(
    entries: &[EntryUsage],
    writer: Partition,
    replaced: Option<&CacheKey>,
    incoming_bytes: u64,
    budget_bytes: u64,
) -> Result<Vec<(Partition, CacheKey)>, TooLarge> {
    // Stub (T-06): the strictest answer.
    let _ = (entries, writer, replaced, incoming_bytes, budget_bytes);
    Err(TooLarge)
}
