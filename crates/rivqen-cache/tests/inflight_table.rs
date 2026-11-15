// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The in-flight table (R-CACHE-07; freshness section 5).

#![allow(clippy::unwrap_used)]

use rivqen_cache::inflight::{InFlight, Join};
use rivqen_cache::key::Partition;
use rivqen_model::ids::{CacheKey, PartitionId, RequestId};

fn k(n: u8) -> CacheKey {
    CacheKey::from_bytes([n; 32])
}
fn p(n: u8) -> Partition {
    Partition::Scoped(PartitionId::from_bytes([n; 32]))
}
fn r(n: u64) -> RequestId {
    RequestId::new(n)
}

#[test]
fn inflight_first_is_leader_later_follow() {
    let mut t = InFlight::new();
    assert!(t.is_empty());
    assert_eq!(t.join(p(1), k(1), r(1)), Join::Leader);
    assert_eq!(t.join(p(1), k(1), r(2)), Join::Follower(r(1)));
    assert_eq!(t.join(p(1), k(1), r(3)), Join::Follower(r(1)));
    assert_eq!(t.len(), 1);
}

#[test]
fn inflight_same_key_other_partition_is_leader() {
    let mut t = InFlight::new();
    assert_eq!(t.join(p(1), k(1), r(1)), Join::Leader);
    assert_eq!(t.join(p(2), k(1), r(2)), Join::Leader);
    assert_eq!(t.join(Partition::Public, k(1), r(3)), Join::Leader);
    assert_eq!(t.join(p(1), k(2), r(4)), Join::Leader);
    assert_eq!(t.len(), 4);
}

#[test]
fn inflight_finish_returns_followers_in_order_and_removes() {
    let mut t = InFlight::new();
    t.join(p(1), k(1), r(1));
    t.join(p(1), k(1), r(5));
    t.join(p(1), k(1), r(3));
    t.join(p(2), k(1), r(9));
    assert_eq!(t.finish(p(1), &k(1)), vec![r(5), r(3)]);
    assert_eq!(t.len(), 1);
    assert_eq!(t.join(p(1), k(1), r(10)), Join::Leader);
}

#[test]
fn inflight_finish_unknown_is_empty() {
    let mut t = InFlight::new();
    t.join(p(1), k(1), r(1));
    assert_eq!(t.finish(p(2), &k(1)), vec![]);
    assert_eq!(t.finish(p(1), &k(2)), vec![]);
    assert_eq!(t.len(), 1);
}

#[test]
fn inflight_cancel_partition_only_that_partition() {
    let mut t = InFlight::new();
    t.join(p(1), k(1), r(1));
    t.join(p(1), k(1), r(2));
    t.join(p(1), k(2), r(3));
    t.join(p(2), k(1), r(4));
    t.join(p(2), k(1), r(5));
    assert_eq!(t.cancel_partition(p(1)), vec![r(1), r(2), r(3)]);
    assert_eq!(t.len(), 1);
    assert_eq!(t.finish(p(2), &k(1)), vec![r(5)]);
    assert!(t.is_empty());
    assert_eq!(t.cancel_partition(p(1)), vec![]);
}
