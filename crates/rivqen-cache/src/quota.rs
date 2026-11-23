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
    if incoming_bytes > budget_bytes {
        return Err(TooLarge);
    }
    let counted: Vec<&EntryUsage> = entries
        .iter()
        .filter(|entry| !(entry.partition == writer && replaced == Some(&entry.key)))
        .collect();
    let total = counted
        .iter()
        .fold(u128::from(incoming_bytes), |sum, entry| {
            sum.saturating_add(u128::from(entry.size_bytes))
        });
    let mut missing = total.saturating_sub(u128::from(budget_bytes));
    let mut own: Vec<&EntryUsage> = Vec::new();
    let mut others: Vec<&EntryUsage> = Vec::new();
    for entry in counted {
        if entry.partition == writer {
            own.push(entry);
        } else {
            others.push(entry);
        }
    }
    // Stable sort: equal time and key keep the input order.
    own.sort_by_key(|entry| (entry.last_used_at, entry.key));
    others.sort_by_key(|entry| (entry.last_used_at, entry.key));
    let mut plan = Vec::new();
    for entry in own.into_iter().chain(others) {
        if missing == 0 {
            break;
        }
        missing = missing.saturating_sub(u128::from(entry.size_bytes));
        plan.push((entry.partition, entry.key));
    }
    Ok(plan)
}
