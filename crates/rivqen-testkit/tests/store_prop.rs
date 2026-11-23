// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! Property tests of the fake store against the model of the contract
//! suite (WP-07 S1, T-05; INV-14, RSK-06).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use rivqen_cache::txn::CommitStep;
use rivqen_testkit::store::contract::{self, Action, Fault, ScriptExpect, ScriptOp, Stale};
use rivqen_testkit::store::{Defect, FakeStore, StopKind};

fn config(cases: u32, seed: u64) -> ProptestConfig {
    ProptestConfig {
        cases: std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(cases),
        rng_seed: RngSeed::Fixed(seed),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

fn store(process_kill: bool) -> FakeStore {
    let kind = if process_kill {
        StopKind::ProcessKill
    } else {
        StopKind::PowerLoss
    };
    FakeStore::new().with_stop_kind(kind)
}

proptest! {
    #![proptest_config(config(2_000, 0x7_05))]

    /// A random sequence of transactions on two scoped partitions and
    /// `Public`, with random stops, failures, stale epochs and damage:
    /// after each step (and each recovery) the store equals the model.
    #[test]
    fn store_prop_model(script in prop::collection::vec(any::<u32>(), 0..40), kill in any::<bool>()) {
        let ops: Vec<ScriptOp> = script.into_iter().map(ScriptOp::decode).collect();
        if let Err(violation) = contract::run_script(&mut store(kill), &ops) {
            panic!("{violation}");
        }
    }

    /// The same sequences without the model: a read in a partition never
    /// returns an entry that was stored in another partition, and an
    /// applied revoke leaves nothing of its partition.
    #[test]
    fn store_prop_partitions(script in prop::collection::vec(any::<u32>(), 0..40), kill in any::<bool>()) {
        let ops: Vec<ScriptOp> = script.into_iter().map(ScriptOp::decode).collect();
        if let Err(violation) = contract::script_keeps_partitions_apart(&mut store(kill), &ops) {
            panic!("{violation}");
        }
    }
}

/// `ScriptOp::decode` reaches every action, fault, expectation and stale
/// epoch, so the properties above cover them.
#[test]
fn store_prop_decode_reaches_every_case() {
    let ops: Vec<ScriptOp> = (0..1_u32 << 20).step_by(7).map(ScriptOp::decode).collect();
    let actions = [
        Action::Replace,
        Action::UpdateFields,
        Action::Remove,
        Action::DisableKey,
        Action::RevokePartition,
        Action::ClearSite(None),
        Action::ClearSite(Some(0)),
        Action::ClearSite(Some(1)),
        Action::ClearSite(Some(2)),
        Action::Damage,
    ];
    for action in actions {
        assert!(ops.iter().any(|op| op.action == action), "{action:?}");
    }
    for step in CommitStep::ALL {
        assert!(
            ops.iter()
                .any(|op| op.fault == Some(Fault::StopAfter(step))),
            "{step:?}"
        );
        assert!(
            ops.iter().any(|op| op.fault == Some(Fault::FailAt(step))),
            "{step:?}"
        );
    }
    assert!(ops.iter().any(|op| op.fault.is_none()));
    for expect in [
        ScriptExpect::Any,
        ScriptExpect::Absent,
        ScriptExpect::Current,
        ScriptExpect::Wrong,
    ] {
        assert!(ops.iter().any(|op| op.expect == expect), "{expect:?}");
    }
    for stale in [
        Stale::Neither,
        Stale::Partition,
        Stale::Site,
        Stale::Request,
    ] {
        assert!(ops.iter().any(|op| op.stale == stale), "{stale:?}");
    }
    for partition in 0..3 {
        assert!(ops.iter().any(|op| op.partition == partition));
    }
    for key in 0..4 {
        assert!(ops.iter().any(|op| op.key == key));
    }
    assert!(ops.iter().all(|op| op.partition < 3 && op.key < 4));
}

#[test]
fn store_prop_decode_known_words() {
    assert_eq!(ScriptOp::decode(0).action, Action::Replace);
    let op = ScriptOp::decode(0xFFFF_FFFF);
    assert_eq!(op.action, Action::Damage);
    assert_eq!(op.partition, 0);
    assert_eq!(op.key, 3);
    assert_eq!(op.expect, ScriptExpect::Wrong);
    assert_eq!(op.stale, Stale::Neither);
    assert_eq!(op.fault, None);
    assert_eq!(op.variant, 0xFF);
    // kind 2, partition 1, key 2, expect Current, stale Partition, stop
    // after Promote, variant 9.
    let word = 2 | (1 << 3) | (2 << 5) | (2 << 7) | (2 << 12) | (9 << 19);
    let op = ScriptOp::decode(word);
    assert_eq!(op.action, Action::UpdateFields);
    assert_eq!((op.partition, op.key), (1, 2));
    assert_eq!(op.expect, ScriptExpect::Current);
    assert_eq!(op.stale, Stale::Partition);
    assert_eq!(op.fault, Some(Fault::StopAfter(CommitStep::Promote)));
    assert_eq!(op.variant, 9);
    let op = ScriptOp::decode(6 | (3 << 17) | (7 << 12));
    assert_eq!(op.action, Action::ClearSite(None));
    assert_eq!(op.fault, Some(Fault::FailAt(CommitStep::Promote)));
    assert_eq!(op.stale, Stale::Partition);
}

/// A long fixed script over every action with stops at every step and both
/// stop kinds (a regression case for the properties).
#[test]
fn store_prop_fixed_long_script() {
    let mut ops = Vec::new();
    for (n, step) in CommitStep::ALL.into_iter().enumerate() {
        let n = u8::try_from(n).unwrap();
        ops.push(ScriptOp::new(Action::Replace, n % 3, n % 4).with_variant(n));
        ops.push(
            ScriptOp::new(Action::Replace, (n + 1) % 3, n % 4).with_fault(Fault::StopAfter(step)),
        );
        ops.push(ScriptOp::new(Action::UpdateFields, n % 3, n % 4).with_fault(Fault::FailAt(step)));
        ops.push(
            ScriptOp::new(Action::DisableKey, (n + 2) % 3, 3).with_fault(Fault::StopAfter(step)),
        );
        ops.push(ScriptOp::new(Action::Damage, (n + 1) % 3, n % 4).with_variant(n));
        ops.push(
            ScriptOp::new(Action::RevokePartition, n % 3, 0).with_fault(Fault::StopAfter(step)),
        );
        ops.push(
            ScriptOp::new(Action::ClearSite(Some(n % 3)), 0, 0).with_fault(Fault::StopAfter(step)),
        );
        ops.push(ScriptOp::new(Action::Replace, 2, 1).with_stale(Stale::Site));
    }
    ops.push(ScriptOp::new(Action::ClearSite(None), 0, 0));
    for kill in [false, true] {
        if let Err(violation) = contract::run_script(&mut store(kill), &ops) {
            panic!("{violation}");
        }
        if let Err(violation) = contract::script_keeps_partitions_apart(&mut store(kill), &ops) {
            panic!("{violation}");
        }
    }
}

/// Scripts from a fixed word sequence (a linear congruential generator).
fn scripts(count: usize, len: usize) -> Vec<Vec<ScriptOp>> {
    let mut state: u32 = 0x2545_F491;
    let mut next = move || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        state
    };
    (0..count)
        .map(|_| (0..len).map(|_| ScriptOp::decode(next())).collect())
        .collect()
}

/// The generated scripts reach each defect of the fake: the properties
/// above would find a driver with it.
#[test]
fn store_prop_scripts_find_each_defect() {
    let all = scripts(64, 30);
    for defect in [
        Defect::RemoveOldBeforeCommit,
        Defect::UnsyncedPromote,
        Defect::NoPreconditions,
        Defect::ReadIgnoresPartition,
        Defect::NoRequestCheck,
    ] {
        let found = all
            .iter()
            .filter(|ops| {
                contract::run_script(&mut FakeStore::new().with_defect(defect), ops).is_err()
            })
            .count();
        assert!(found > 0, "{defect:?}");
    }
    let leaks = all
        .iter()
        .filter(|ops| {
            let mut leaky = FakeStore::new().with_defect(Defect::ReadIgnoresPartition);
            contract::script_keeps_partitions_apart(&mut leaky, ops).is_err()
        })
        .count();
    assert!(leaks > 0);
    // And the store without a defect passes all of them.
    for ops in &all {
        assert_eq!(contract::run_script(&mut FakeStore::new(), ops), Ok(()));
    }
}
