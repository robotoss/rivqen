// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Property: the L1 cache never holds more than its budget.

#![allow(
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation
)]

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use rivqen_cache::key::Partition;
use rivqen_cache::memory::{L1Cache, PressureLevel};
use rivqen_model::ids::{CacheKey, PartitionId};

#[derive(Debug, Clone)]
enum Op {
    Insert(u8, u8, u64),
    Get(u8, u8),
    Remove(u8, u8),
    RemovePartition(u8),
    Clear,
    Moderate,
    Critical,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0u8..3, 0u8..5, 0u64..150).prop_map(|(a, b, c)| Op::Insert(a, b, c)),
        (0u8..3, 0u8..5).prop_map(|(a, b)| Op::Get(a, b)),
        (0u8..3, 0u8..5).prop_map(|(a, b)| Op::Remove(a, b)),
        (0u8..3).prop_map(Op::RemovePartition),
        Just(Op::Clear),
        Just(Op::Moderate),
        Just(Op::Critical),
    ]
}

fn p(n: u8) -> Partition {
    Partition::Scoped(PartitionId::from_bytes([n; 32]))
}
fn k(n: u8) -> CacheKey {
    CacheKey::from_bytes([n; 32])
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 256,
        rng_seed: RngSeed::Fixed(7),
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn memory_prop_used_bytes_within_budget(budget in 0u64..120, ops in prop::collection::vec(op(), 0..60)) {
        let mut c: L1Cache<u8> = L1Cache::new(budget);
        for o in ops {
            match o {
                Op::Insert(a, b, s) => {
                    let ok = c.insert(p(a), k(b), b, s).is_ok();
                    prop_assert_eq!(ok, s <= budget);
                    if ok {
                        prop_assert_eq!(c.get(p(a), &k(b)), Some(&b));
                    }
                }
                Op::Get(a, b) => { let _ = c.get(p(a), &k(b)); }
                Op::Remove(a, b) => c.remove(p(a), &k(b)),
                Op::RemovePartition(a) => c.remove_partition(p(a)),
                Op::Clear => c.clear(),
                Op::Moderate => {
                    c.on_pressure(PressureLevel::Moderate);
                    prop_assert!(c.used_bytes() <= budget / 2);
                }
                Op::Critical => {
                    c.on_pressure(PressureLevel::Critical);
                    prop_assert_eq!(c.used_bytes(), 0);
                }
            }
            prop_assert!(c.used_bytes() <= budget);
        }
    }
}
