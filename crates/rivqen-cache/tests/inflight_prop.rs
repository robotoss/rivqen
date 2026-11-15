// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Property: no request id comes back twice.

#![allow(
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation
)]

use std::collections::HashSet;

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use rivqen_cache::inflight::{InFlight, Join};
use rivqen_cache::key::Partition;
use rivqen_model::ids::{CacheKey, PartitionId, RequestId};

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 256,
        rng_seed: RngSeed::Fixed(13),
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn inflight_prop_no_request_returned_twice(ops in prop::collection::vec((0u8..3, 0u8..3, 0u8..3), 0..60)) {
        let mut t = InFlight::new();
        let mut returned = HashSet::new();
        let mut joined = HashSet::new();
        for (i, (kind, a, b)) in ops.into_iter().enumerate() {
            let part = Partition::Scoped(PartitionId::from_bytes([a; 32]));
            let key = CacheKey::from_bytes([b; 32]);
            let id = RequestId::new(i as u64);
            let out: Vec<RequestId> = match kind {
                0 => {
                    joined.insert(id);
                    if let Join::Follower(l) = t.join(part, key, id) {
                        prop_assert!(joined.contains(&l) && l != id);
                    }
                    vec![]
                }
                1 => t.finish(part, &key),
                _ => t.cancel_partition(part),
            };
            for q in out {
                prop_assert!(joined.contains(&q));
                prop_assert!(returned.insert(q));
            }
        }
    }
}
