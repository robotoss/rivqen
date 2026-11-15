// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Properties of the eviction plan.

#![allow(
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation
)]

use std::collections::HashSet;

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use rivqen_cache::key::Partition;
use rivqen_cache::quota::{EntryUsage, plan_eviction};
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_model::time::UnixTime;

fn p(n: u8) -> Partition {
    Partition::Scoped(PartitionId::from_bytes([n; 32]))
}
fn k(n: u8) -> CacheKey {
    CacheKey::from_bytes([n; 32])
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 512,
        rng_seed: RngSeed::Fixed(11),
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn quota_prop_plan_frees_the_missing_bytes(
        raw in prop::collection::vec((0u8..3, 0u8..8, 0u64..100, 0u64..10), 0..12),
        writer in 0u8..3,
        replaced in prop::option::of(0u8..8),
        incoming in 0u64..200,
        budget in 0u64..300,
    ) {
        let mut seen = HashSet::new();
        let entries: Vec<EntryUsage> = raw
            .into_iter()
            .filter(|(a, b, _, _)| seen.insert((*a, *b)))
            .map(|(a, b, s, t)| EntryUsage { partition: p(a), key: k(b), size_bytes: s, last_used_at: UnixTime(t) })
            .collect();
        let rk = replaced.map(k);
        let result = plan_eviction(&entries, p(writer), rk.as_ref(), incoming, budget);
        if incoming > budget {
            prop_assert!(result.is_err());
            return Ok(());
        }
        let plan = result.unwrap();
        for (i, a) in plan.iter().enumerate() {
            prop_assert!(plan.iter().skip(i + 1).all(|b| b != a));
            prop_assert!(!(a.0 == p(writer) && Some(a.1) == rk));
        }
        let counted = |e: &&EntryUsage| !(e.partition == p(writer) && Some(e.key) == rk);
        let total: u64 = entries.iter().filter(counted).map(|e| e.size_bytes).sum::<u64>() + incoming;
        let freed: u64 = entries
            .iter()
            .filter(|e| plan.contains(&(e.partition, e.key)))
            .map(|e| e.size_bytes)
            .sum();
        prop_assert!(total.saturating_sub(freed) <= budget);
        if total <= budget {
            prop_assert!(plan.is_empty());
        }
    }
}
