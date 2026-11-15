// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The L2 eviction plan (freshness section 4, "Quota exceeded").

#![allow(clippy::unwrap_used)]

use rivqen_cache::key::Partition;
use rivqen_cache::quota::{EntryUsage, TooLarge, plan_eviction};
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_model::time::UnixTime;

fn k(n: u8) -> CacheKey {
    CacheKey::from_bytes([n; 32])
}
fn p(n: u8) -> Partition {
    Partition::Scoped(PartitionId::from_bytes([n; 32]))
}
fn e(part: Partition, key: u8, size: u64, t: u64) -> EntryUsage {
    EntryUsage {
        partition: part,
        key: k(key),
        size_bytes: size,
        last_used_at: UnixTime(t),
    }
}

#[test]
fn quota_nothing_when_it_fits() {
    let entries = [e(p(1), 1, 60, 1), e(p(1), 2, 30, 2)];
    assert_eq!(plan_eviction(&entries, p(1), None, 10, 100), Ok(vec![]));
}

#[test]
fn quota_one_byte_over_evicts_oldest() {
    let entries = [e(p(1), 1, 60, 1), e(p(1), 2, 30, 2)];
    assert_eq!(
        plan_eviction(&entries, p(1), None, 11, 100),
        Ok(vec![(p(1), k(1))])
    );
}

#[test]
fn quota_stops_when_enough_is_freed() {
    let entries = [e(p(1), 1, 40, 1), e(p(1), 2, 40, 2), e(p(1), 3, 20, 3)];
    assert_eq!(
        plan_eviction(&entries, p(1), None, 40, 100),
        Ok(vec![(p(1), k(1))])
    );
    assert_eq!(
        plan_eviction(&entries, p(1), None, 41, 100),
        Ok(vec![(p(1), k(1)), (p(1), k(2))])
    );
}

#[test]
fn quota_writer_partition_first_then_others_oldest_first() {
    let entries = [
        e(p(2), 1, 10, 1),
        e(p(1), 2, 10, 50),
        e(p(3), 3, 10, 2),
        e(p(1), 4, 10, 40),
    ];
    assert_eq!(
        plan_eviction(&entries, p(1), None, 40, 40),
        Ok(vec![(p(1), k(4)), (p(1), k(2)), (p(2), k(1)), (p(3), k(3))])
    );
}

#[test]
fn quota_replaced_entry_does_not_count_and_is_not_planned() {
    let entries = [e(p(1), 1, 90, 1), e(p(1), 2, 10, 2)];
    assert_eq!(
        plan_eviction(&entries, p(1), Some(&k(1)), 90, 100),
        Ok(vec![])
    );
    assert_eq!(
        plan_eviction(&entries, p(1), Some(&k(1)), 95, 100),
        Ok(vec![(p(1), k(2))])
    );
}

#[test]
fn quota_replaced_key_in_another_partition_still_counts() {
    let entries = [e(p(2), 1, 90, 1)];
    assert_eq!(
        plan_eviction(&entries, p(1), Some(&k(1)), 20, 100),
        Ok(vec![(p(2), k(1))])
    );
}

#[test]
fn quota_equal_times_break_by_key_order() {
    let entries = [e(p(1), 3, 10, 5), e(p(1), 1, 10, 5), e(p(1), 2, 10, 5)];
    assert_eq!(
        plan_eviction(&entries, p(1), None, 10, 30),
        Ok(vec![(p(1), k(1))])
    );
    assert_eq!(
        plan_eviction(&entries, p(1), None, 20, 30),
        Ok(vec![(p(1), k(1)), (p(1), k(2))])
    );
}

#[test]
fn quota_too_large() {
    assert_eq!(plan_eviction(&[], p(1), None, 101, 100), Err(TooLarge));
    assert_eq!(plan_eviction(&[], p(1), None, 100, 100), Ok(vec![]));
}

#[test]
fn quota_huge_sizes_do_not_overflow() {
    let entries = [e(p(1), 1, u64::MAX, 1), e(p(1), 2, u64::MAX, 2)];
    assert_eq!(
        plan_eviction(&entries, p(1), None, u64::MAX, u64::MAX),
        Ok(vec![(p(1), k(1)), (p(1), k(2))])
    );
}
