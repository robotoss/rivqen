// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! L1 cache behavior (storage section 4; R-CACHE-04).

#![allow(clippy::unwrap_used)]

use rivqen_cache::key::Partition;
use rivqen_cache::memory::{L1Cache, OverBudget, PressureLevel};
use rivqen_model::ids::{CacheKey, PartitionId};

fn k(n: u8) -> CacheKey {
    CacheKey::from_bytes([n; 32])
}
fn p(n: u8) -> Partition {
    Partition::Scoped(PartitionId::from_bytes([n; 32]))
}

#[test]
fn memory_insert_and_get() {
    let mut c = L1Cache::new(100);
    c.insert(p(1), k(1), "a", 40).unwrap();
    assert_eq!(c.get(p(1), &k(1)), Some(&"a"));
    assert_eq!(c.used_bytes(), 40);
    assert_eq!(c.budget_bytes(), 100);
}

#[test]
fn memory_get_is_partition_bound() {
    let mut c = L1Cache::new(100);
    c.insert(p(1), k(1), "a", 10).unwrap();
    assert_eq!(c.get(p(2), &k(1)), None);
    assert_eq!(c.get(Partition::Public, &k(1)), None);
    assert_eq!(c.get(p(1), &k(2)), None);
}

#[test]
fn memory_value_of_exactly_the_budget_fits() {
    let mut c = L1Cache::new(100);
    c.insert(p(1), k(1), "a", 30).unwrap();
    c.insert(p(1), k(2), "b", 100).unwrap();
    assert_eq!(c.used_bytes(), 100);
    assert_eq!(c.get(p(1), &k(1)), None);
    assert_eq!(c.get(p(1), &k(2)), Some(&"b"));
}

#[test]
fn memory_value_over_the_budget_of_a_new_key_changes_nothing() {
    let mut c = L1Cache::new(100);
    c.insert(p(1), k(1), "a", 30).unwrap();
    assert_eq!(c.insert(p(1), k(2), "b", 101), Err(OverBudget));
    assert_eq!(c.used_bytes(), 30);
    assert_eq!(c.get(p(1), &k(1)), Some(&"a"));
    assert_eq!(c.get(p(1), &k(2)), None);
}

#[test]
fn memory_value_over_the_budget_removes_the_old_value_of_its_key() {
    // A newer version that does not fit must not leave the older version
    // readable: L2 already has the newer one.
    let mut c = L1Cache::new(100);
    c.insert(p(1), k(1), "a", 30).unwrap();
    c.insert(p(2), k(1), "other partition", 20).unwrap();
    c.insert(p(1), k(2), "other key", 10).unwrap();
    assert_eq!(c.insert(p(1), k(1), "c", 101), Err(OverBudget));
    assert_eq!(c.get(p(1), &k(1)), None);
    assert_eq!(c.used_bytes(), 30);
    assert_eq!(c.get(p(2), &k(1)), Some(&"other partition"));
    assert_eq!(c.get(p(1), &k(2)), Some(&"other key"));
}

#[test]
fn memory_fills_to_budget_without_eviction() {
    let mut c = L1Cache::new(100);
    c.insert(p(1), k(1), "a", 50).unwrap();
    c.insert(p(1), k(2), "b", 50).unwrap();
    assert_eq!(c.used_bytes(), 100);
    assert!(c.get(p(1), &k(1)).is_some());
    assert!(c.get(p(1), &k(2)).is_some());
}

#[test]
fn memory_evicts_least_recently_used_first() {
    let mut c = L1Cache::new(100);
    c.insert(p(1), k(1), "a", 40).unwrap();
    c.insert(p(1), k(2), "b", 40).unwrap();
    c.insert(p(1), k(3), "c", 21).unwrap();
    assert_eq!(c.get(p(1), &k(1)), None);
    assert!(c.get(p(1), &k(2)).is_some());
    assert!(c.get(p(1), &k(3)).is_some());
    assert_eq!(c.used_bytes(), 61);
}

#[test]
fn memory_get_marks_most_recent() {
    let mut c = L1Cache::new(100);
    c.insert(p(1), k(1), "a", 40).unwrap();
    c.insert(p(1), k(2), "b", 40).unwrap();
    assert!(c.get(p(1), &k(1)).is_some());
    c.insert(p(1), k(3), "c", 40).unwrap();
    assert!(c.get(p(1), &k(1)).is_some());
    assert_eq!(c.get(p(1), &k(2)), None);
}

#[test]
fn memory_evicts_across_partitions() {
    let mut c = L1Cache::new(100);
    c.insert(p(1), k(1), "a", 60).unwrap();
    c.insert(p(2), k(1), "b", 60).unwrap();
    assert_eq!(c.get(p(1), &k(1)), None);
    assert_eq!(c.get(p(2), &k(1)), Some(&"b"));
}

#[test]
fn memory_second_insert_replaces() {
    let mut c = L1Cache::new(100);
    c.insert(p(1), k(1), "a", 60).unwrap();
    c.insert(p(1), k(2), "x", 30).unwrap();
    c.insert(p(1), k(1), "b", 70).unwrap();
    assert_eq!(c.get(p(1), &k(1)), Some(&"b"));
    assert_eq!(c.used_bytes(), 100);
    assert!(c.get(p(1), &k(2)).is_some());
}

#[test]
fn memory_replace_marks_most_recent() {
    let mut c = L1Cache::new(100);
    c.insert(p(1), k(1), "a", 40).unwrap();
    c.insert(p(1), k(2), "b", 40).unwrap();
    c.insert(p(1), k(1), "c", 40).unwrap();
    c.insert(p(1), k(3), "d", 40).unwrap();
    assert_eq!(c.get(p(1), &k(2)), None);
    assert_eq!(c.get(p(1), &k(1)), Some(&"c"));
}

#[test]
fn memory_remove_one() {
    let mut c = L1Cache::new(100);
    c.insert(p(1), k(1), "a", 10).unwrap();
    c.insert(p(2), k(1), "b", 20).unwrap();
    c.remove(p(1), &k(1));
    c.remove(p(1), &k(9));
    assert_eq!(c.get(p(1), &k(1)), None);
    assert_eq!(c.get(p(2), &k(1)), Some(&"b"));
    assert_eq!(c.used_bytes(), 20);
}

#[test]
fn memory_remove_partition_only_that_one() {
    let mut c = L1Cache::new(100);
    c.insert(p(1), k(1), "a", 10).unwrap();
    c.insert(p(1), k(2), "b", 10).unwrap();
    c.insert(p(2), k(1), "c", 20).unwrap();
    c.insert(Partition::Public, k(1), "d", 5).unwrap();
    c.remove_partition(p(1));
    assert_eq!(c.used_bytes(), 25);
    assert_eq!(c.get(p(1), &k(1)), None);
    assert_eq!(c.get(p(1), &k(2)), None);
    assert_eq!(c.get(p(2), &k(1)), Some(&"c"));
    assert_eq!(c.get(Partition::Public, &k(1)), Some(&"d"));
}

#[test]
fn memory_clear_empties() {
    let mut c = L1Cache::new(100);
    c.insert(p(1), k(1), "a", 10).unwrap();
    c.clear();
    assert_eq!(c.used_bytes(), 0);
    assert_eq!(c.get(p(1), &k(1)), None);
}

#[test]
fn memory_zero_budget_rejects_non_empty_values() {
    let mut c = L1Cache::new(0);
    assert_eq!(c.insert(p(1), k(1), "a", 1), Err(OverBudget));
    assert_eq!(c.used_bytes(), 0);
}

#[test]
fn memory_pressure_moderate_halves() {
    let mut c = L1Cache::new(100);
    for n in 1..=4 {
        c.insert(p(1), k(n), "v", 25).unwrap();
    }
    c.on_pressure(PressureLevel::Moderate);
    assert_eq!(c.used_bytes(), 50);
    assert_eq!(c.get(p(1), &k(1)), None);
    assert_eq!(c.get(p(1), &k(2)), None);
    assert!(c.get(p(1), &k(3)).is_some());
    assert!(c.get(p(1), &k(4)).is_some());
}

#[test]
fn memory_pressure_moderate_at_half_keeps_all() {
    let mut c = L1Cache::new(100);
    c.insert(p(1), k(1), "a", 50).unwrap();
    c.on_pressure(PressureLevel::Moderate);
    assert_eq!(c.used_bytes(), 50);
    assert!(c.get(p(1), &k(1)).is_some());
}

#[test]
fn memory_pressure_moderate_just_over_half_evicts() {
    let mut c = L1Cache::new(100);
    c.insert(p(1), k(1), "a", 51).unwrap();
    c.on_pressure(PressureLevel::Moderate);
    assert_eq!(c.used_bytes(), 0);
}

#[test]
fn memory_pressure_moderate_odd_budget_rounds_down() {
    let mut c = L1Cache::new(101);
    c.insert(p(1), k(1), "a", 50).unwrap();
    c.insert(p(1), k(2), "b", 1).unwrap();
    c.on_pressure(PressureLevel::Moderate);
    assert_eq!(c.used_bytes(), 1);
    assert_eq!(c.get(p(1), &k(1)), None);
}

#[test]
fn memory_pressure_critical_clears() {
    let mut c = L1Cache::new(100);
    c.insert(p(1), k(1), "a", 10).unwrap();
    c.on_pressure(PressureLevel::Critical);
    assert_eq!(c.used_bytes(), 0);
    assert_eq!(c.get(p(1), &k(1)), None);
}
