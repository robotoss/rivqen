// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! The fault model of `FakeStore` (WP-07 S1, T-05; DL-181): what each step
//! makes durable, what a stop loses and what recovery removes.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use rivqen_cache::driver::{RecoveryReport, StorageDriver, StoreErrorKind};
use rivqen_cache::key::Partition;
use rivqen_cache::txn::{
    CacheTransaction, CommitStep, EntryVersion, Epoch, Expect, Preconditions, Refused, TxnOp,
    TxnOutcome, Writer,
};
use rivqen_model::ids::CacheKey;
use rivqen_model::time::UnixTime;
use rivqen_testkit::store::contract::{
    self, Action, KEYS, NOW, PARTITIONS, ScriptOp, sample_entry, sample_policy,
};
use rivqen_testkit::store::{Damage, Defect, FakeStore, FaultInjection, FileCount, StopKind};

const A: Partition = PARTITIONS[0];
const B: Partition = PARTITIONS[1];
const K1: CacheKey = KEYS[0];
const K2: CacheKey = KEYS[1];

fn ready(kind: StopKind) -> FakeStore {
    let mut store = FakeStore::new().with_stop_kind(kind);
    assert_eq!(store.recover(NOW).unwrap(), RecoveryReport::default());
    store
}

fn pre(store: &mut FakeStore, partition: Partition, entry: Expect) -> Preconditions {
    let writer = match partition {
        Partition::Scoped(id) => Writer::request(id, store.partition_epoch(partition).unwrap()),
        Partition::Public => Writer::Maintenance,
    };
    Preconditions {
        entry,
        writer,
        partition_epoch: store.partition_epoch(partition).unwrap(),
        site_epoch: store.site_epoch().unwrap(),
    }
}

fn replace(
    store: &mut FakeStore,
    partition: Partition,
    key: CacheKey,
    tag: &str,
) -> CacheTransaction {
    let (meta, body) = sample_entry(partition, key, "shop.example.com", tag, 7);
    CacheTransaction {
        op: TxnOp::Replace {
            meta: Box::new(meta),
            body: Box::new(body),
        },
        pre: pre(store, partition, Expect::Any),
    }
}

fn put(store: &mut FakeStore, partition: Partition, key: CacheKey, tag: &str) -> EntryVersion {
    let txn = replace(store, partition, key, tag);
    match store.apply(txn).unwrap() {
        TxnOutcome::Applied { version: Some(v) } => v,
        other => panic!("{other:?}"),
    }
}

fn page(store: &mut FakeStore, partition: Partition, key: CacheKey) -> Option<Vec<u8>> {
    store
        .read(partition, &key, NOW)
        .unwrap()
        .map(|entry| entry.body.page.into_vec())
}

fn unavailable<T: std::fmt::Debug>(result: Result<T, rivqen_cache::driver::StoreError>, op: &str) {
    let error = result.expect_err("must fail");
    assert_eq!(error.kind(), StoreErrorKind::Unavailable);
    assert_eq!(error.op(), op);
}

fn files(temp: usize, entry: usize) -> FileCount {
    FileCount { temp, entry }
}

#[test]
fn store_fault_old_or_new_power_loss() {
    if let Err(v) = contract::old_or_new(&mut FakeStore::new) {
        panic!("{v}");
    }
}

#[test]
fn store_fault_old_or_new_process_kill() {
    let mut make = || FakeStore::new().with_stop_kind(StopKind::ProcessKill);
    if let Err(v) = contract::old_or_new(&mut make) {
        panic!("{v}");
    }
}

#[test]
fn store_fault_failed_step_keeps_the_old_state() {
    if let Err(v) = contract::failed_step_keeps_the_old_state(&mut FakeStore::new) {
        panic!("{v}");
    }
}

/// Replace over an existing entry, stop after each step: the entry, the
/// recovery report and the files, exact, for both stop kinds.
#[test]
fn store_fault_replace_table() {
    // (step, new entry, orphans after power loss, orphans after a kill)
    let table = [
        (CommitStep::WriteTemp, false, 0, 1),
        (CommitStep::SyncTemp, false, 1, 1),
        (CommitStep::Promote, false, 1, 1),
        (CommitStep::CommitMeta, true, 1, 1),
        (CommitStep::RemoveOld, true, 1, 0),
    ];
    for kind in [StopKind::PowerLoss, StopKind::ProcessKill] {
        for (step, new, orphans_power, orphans_kill) in table {
            let mut store = ready(kind);
            let old = put(&mut store, A, K1, "old");
            put(&mut store, B, K1, "other");
            let txn = replace(&mut store, A, K1, "new");
            store.stop_after(step);
            unavailable(store.apply(txn), "apply");
            store.restart().unwrap();
            let report = store.recover(NOW).unwrap();
            let orphans = if kind == StopKind::PowerLoss {
                orphans_power
            } else {
                orphans_kill
            };
            assert_eq!(
                report,
                RecoveryReport {
                    orphans_removed: orphans,
                    entries_removed: 0
                },
                "{kind:?} {step:?}"
            );
            let entry = store.read(A, &K1, NOW).unwrap().unwrap();
            let want: &[u8] = if new { b"new/page" } else { b"old/page" };
            assert_eq!(entry.body.page.as_slice(), want, "{kind:?} {step:?}");
            if new {
                assert!(entry.version > old, "{kind:?} {step:?}");
            } else {
                assert_eq!(entry.version, old, "{kind:?} {step:?}");
            }
            assert_eq!(page(&mut store, B, K1).unwrap(), b"other/page");
            assert_eq!(store.files(), files(0, 2), "{kind:?} {step:?}");
            assert_eq!(store.durable_files(), files(0, 2), "{kind:?} {step:?}");
            assert_eq!(store.recover(NOW).unwrap(), RecoveryReport::default());
        }
    }
}

/// A transaction without content (Remove): nothing to do before the
/// commit; the old file is an orphan after the commit.
#[test]
fn store_fault_remove_table() {
    // (step, removed, orphans after power loss, orphans after a kill)
    let table = [
        (CommitStep::WriteTemp, false, 0, 0),
        (CommitStep::SyncTemp, false, 0, 0),
        (CommitStep::Promote, false, 0, 0),
        (CommitStep::CommitMeta, true, 1, 1),
        (CommitStep::RemoveOld, true, 1, 0),
    ];
    for kind in [StopKind::PowerLoss, StopKind::ProcessKill] {
        for (step, removed, orphans_power, orphans_kill) in table {
            let mut store = ready(kind);
            put(&mut store, A, K1, "old");
            let txn = CacheTransaction {
                op: TxnOp::Remove {
                    partition: A,
                    key: K1,
                },
                pre: pre(&mut store, A, Expect::Any),
            };
            store.stop_after(step);
            unavailable(store.apply(txn), "apply");
            store.restart().unwrap();
            let orphans = if kind == StopKind::PowerLoss {
                orphans_power
            } else {
                orphans_kill
            };
            assert_eq!(
                store.recover(NOW).unwrap(),
                RecoveryReport {
                    orphans_removed: orphans,
                    entries_removed: 0
                },
                "{kind:?} {step:?}"
            );
            assert_eq!(
                page(&mut store, A, K1).is_none(),
                removed,
                "{kind:?} {step:?}"
            );
            assert_eq!(store.files(), files(0, usize::from(!removed)));
        }
    }
}

#[test]
fn store_fault_layers_after_each_step() {
    // What the running store and the durable layer hold right after the
    // stop, before the restart.
    let table = [
        (CommitStep::WriteTemp, files(1, 1), files(0, 1)),
        (CommitStep::SyncTemp, files(1, 1), files(1, 1)),
        (CommitStep::Promote, files(0, 2), files(0, 2)),
        (CommitStep::CommitMeta, files(0, 2), files(0, 2)),
        (CommitStep::RemoveOld, files(0, 1), files(0, 2)),
    ];
    for (step, live, durable) in table {
        let mut store = ready(StopKind::PowerLoss);
        put(&mut store, A, K1, "old");
        let txn = replace(&mut store, A, K1, "new");
        store.stop_after(step);
        unavailable(store.apply(txn), "apply");
        assert_eq!(store.files(), live, "{step:?}");
        assert_eq!(store.durable_files(), durable, "{step:?}");
    }
}

#[test]
fn store_fault_stopped_store_answers_unavailable() {
    let mut store = ready(StopKind::PowerLoss);
    put(&mut store, A, K1, "old");
    let txn = replace(&mut store, A, K1, "new");
    store.stop_after(CommitStep::CommitMeta);
    unavailable(store.apply(txn.clone()), "apply");
    unavailable(store.read(A, &K1, NOW), "read");
    unavailable(store.apply(txn.clone()), "apply");
    unavailable(store.partition_epoch(A), "partition_epoch");
    unavailable(store.site_epoch(), "site_epoch");
    unavailable(store.key_disabled(A, &K1, NOW), "key_disabled");
    unavailable(store.list(), "list");
    unavailable(store.touch(A, &K1, NOW), "touch");
    unavailable(store.recover(NOW), "recover");
    store.restart().unwrap();
    // After a restart: recover first.
    unavailable(store.read(A, &K1, NOW), "read");
    unavailable(store.list(), "list");
    store.recover(NOW).unwrap();
    assert_eq!(page(&mut store, A, K1).unwrap(), b"new/page");
}

#[test]
fn store_fault_new_store_needs_recover() {
    let mut store = FakeStore::default();
    unavailable(store.read(A, &K1, NOW), "read");
    unavailable(store.site_epoch(), "site_epoch");
    assert_eq!(store.recover(NOW).unwrap(), RecoveryReport::default());
    assert_eq!(store.site_epoch().unwrap(), Epoch(0));
    assert_eq!(store.partition_epoch(A).unwrap(), Epoch(0));
    assert_eq!(store.partition_epoch(Partition::Public).unwrap(), Epoch(0));
    assert_eq!(store.list().unwrap(), Vec::new());
}

#[test]
fn store_fault_stop_is_for_the_next_apply_only() {
    let mut store = ready(StopKind::PowerLoss);
    let txn = replace(&mut store, A, K1, "x");
    store.stop_after(CommitStep::WriteTemp);
    unavailable(store.apply(txn), "apply");
    store.restart().unwrap();
    store.recover(NOW).unwrap();
    // A stop is used once.
    put(&mut store, A, K1, "a");
    // A restart clears an armed stop and an armed failure.
    store.stop_after(CommitStep::Promote);
    store.fail_at(CommitStep::SyncTemp);
    store.restart().unwrap();
    store.recover(NOW).unwrap();
    put(&mut store, A, K1, "b");
    assert_eq!(page(&mut store, A, K1).unwrap(), b"b/page");
    // A failure is used once too.
    store.fail_at(CommitStep::WriteTemp);
    let txn = replace(&mut store, A, K1, "c");
    unavailable(store.apply(txn), "apply");
    put(&mut store, A, K1, "d");
    assert_eq!(page(&mut store, A, K1).unwrap(), b"d/page");
}

#[test]
fn store_fault_fail_at_each_step() {
    for step in CommitStep::ALL {
        let mut store = ready(StopKind::PowerLoss);
        let old = put(&mut store, A, K1, "old");
        let txn = replace(&mut store, A, K1, "new");
        store.fail_at(step);
        let result = store.apply(txn);
        if step == CommitStep::RemoveOld {
            let TxnOutcome::Applied { version: Some(new) } = result.unwrap() else {
                panic!("RemoveOld: the commit stands");
            };
            assert!(new > old);
            assert_eq!(page(&mut store, A, K1).unwrap(), b"new/page");
            // The old file stays until recovery.
            assert_eq!(store.files(), files(0, 2));
            assert_eq!(
                store.recover(NOW).unwrap(),
                RecoveryReport {
                    orphans_removed: 1,
                    entries_removed: 0
                }
            );
        } else {
            unavailable(result, "apply");
            // The store still works and holds the old state.
            assert_eq!(
                store.read(A, &K1, NOW).unwrap().unwrap().version,
                old,
                "{step:?}"
            );
            assert_eq!(store.files(), files(0, 1), "{step:?}");
            put(&mut store, A, K2, "after");
        }
    }
}

#[test]
fn store_fault_versions_are_never_used_twice() {
    let mut store = ready(StopKind::PowerLoss);
    assert_eq!(put(&mut store, A, K1, "a"), EntryVersion(1));
    assert_eq!(put(&mut store, B, K1, "b"), EntryVersion(2));
    let txn = replace(&mut store, A, K1, "c");
    store.stop_after(CommitStep::CommitMeta);
    unavailable(store.apply(txn), "apply");
    store.restart().unwrap();
    store.recover(NOW).unwrap();
    assert_eq!(
        store.read(A, &K1, NOW).unwrap().unwrap().version,
        EntryVersion(3)
    );
    let txn = replace(&mut store, A, K1, "d");
    store.stop_after(CommitStep::Promote);
    unavailable(store.apply(txn), "apply");
    store.restart().unwrap();
    store.recover(NOW).unwrap();
    // A version that never was committed is not given again either.
    assert_eq!(put(&mut store, A, K1, "e"), EntryVersion(4));
    let update = CacheTransaction {
        op: TxnOp::UpdateFields {
            partition: A,
            key: K1,
            fields: Vec::new(),
            policy: sample_policy(1),
        },
        pre: pre(&mut store, A, Expect::Version(EntryVersion(4))),
    };
    assert_eq!(
        store.apply(update).unwrap(),
        TxnOutcome::Applied {
            version: Some(EntryVersion(5))
        }
    );
}

#[test]
fn store_fault_refusal_order_and_epochs() {
    let mut store = ready(StopKind::PowerLoss);
    put(&mut store, A, K1, "a");
    let fresh = pre(&mut store, A, Expect::Version(EntryVersion(99)));
    let stale_both = Preconditions {
        partition_epoch: Epoch(1),
        site_epoch: Epoch(1),
        ..fresh
    };
    let remove = |pre| CacheTransaction {
        op: TxnOp::Remove {
            partition: A,
            key: K1,
        },
        pre,
    };
    assert_eq!(
        store.apply(remove(stale_both)).unwrap(),
        TxnOutcome::Refused(Refused::PartitionRevoked)
    );
    let stale_site = Preconditions {
        site_epoch: Epoch(1),
        ..fresh
    };
    assert_eq!(
        store.apply(remove(stale_site)).unwrap(),
        TxnOutcome::Refused(Refused::SiteCleared)
    );
    assert_eq!(
        store.apply(remove(fresh)).unwrap(),
        TxnOutcome::Refused(Refused::Entry)
    );
    // Epochs grow by one; a revoke of B does not change A.
    let revoke = |partition| CacheTransaction {
        op: TxnOp::RevokePartition { partition },
        pre: fresh,
    };
    assert_eq!(
        store.apply(revoke(B)).unwrap(),
        TxnOutcome::Applied { version: None }
    );
    assert_eq!(
        store.apply(revoke(B)).unwrap(),
        TxnOutcome::Applied { version: None }
    );
    assert_eq!(store.partition_epoch(B).unwrap(), Epoch(2));
    assert_eq!(store.partition_epoch(A).unwrap(), Epoch(0));
    let clear = CacheTransaction {
        op: TxnOp::ClearSite { domain: None },
        pre: stale_both,
    };
    assert_eq!(
        store.apply(clear).unwrap(),
        TxnOutcome::Applied { version: None }
    );
    assert_eq!(store.site_epoch().unwrap(), Epoch(1));
    // The epochs survive a restart.
    store.restart().unwrap();
    store.recover(NOW).unwrap();
    assert_eq!(store.partition_epoch(B).unwrap(), Epoch(2));
    assert_eq!(store.site_epoch().unwrap(), Epoch(1));
}

#[test]
fn store_fault_refused_replace_leaves_no_file() {
    let mut store = ready(StopKind::PowerLoss);
    put(&mut store, A, K1, "a");
    let mut txn = replace(&mut store, A, K1, "b");
    txn.pre.entry = Expect::Absent;
    assert_eq!(
        store.apply(txn).unwrap(),
        TxnOutcome::Refused(Refused::Entry)
    );
    assert_eq!(store.files(), files(0, 1));
    // The synced file of the refused replace is an orphan after a power
    // loss.
    store.restart().unwrap();
    assert_eq!(
        store.recover(NOW).unwrap(),
        RecoveryReport {
            orphans_removed: 1,
            entries_removed: 0
        }
    );
    assert_eq!(page(&mut store, A, K1).unwrap(), b"a/page");
}

#[test]
fn store_fault_damage_and_recovery() {
    // Content: same size, found by read only.
    let mut store = ready(StopKind::PowerLoss);
    put(&mut store, A, K1, "a");
    put(&mut store, B, K1, "b");
    assert!(store.damage(A, &K1, Damage::Content).unwrap());
    store.restart().unwrap();
    assert_eq!(store.recover(NOW).unwrap(), RecoveryReport::default());
    let error = store.read(A, &K1, NOW).unwrap_err();
    assert_eq!(error.kind(), StoreErrorKind::Corrupted);
    assert_eq!(error.op(), "read");
    assert_eq!(store.read(A, &K1, NOW).unwrap(), None);
    assert_eq!(page(&mut store, B, K1).unwrap(), b"b/page");
    assert_eq!(store.list().unwrap().len(), 1);

    // Missing: found by recovery.
    let mut store = ready(StopKind::PowerLoss);
    put(&mut store, A, K1, "a");
    put(&mut store, A, K2, "c");
    assert!(store.damage(A, &K1, Damage::Missing).unwrap());
    store.restart().unwrap();
    assert_eq!(
        store.recover(NOW).unwrap(),
        RecoveryReport {
            orphans_removed: 0,
            entries_removed: 1
        }
    );
    assert_eq!(store.read(A, &K1, NOW).unwrap(), None);
    assert_eq!(page(&mut store, A, K2).unwrap(), b"c/page");

    // Moved with another size: found by recovery; with the same size: by
    // read (the seal names the place).
    let mut store = ready(StopKind::PowerLoss);
    put(&mut store, A, K1, "aa");
    put(&mut store, B, K1, "b");
    put(&mut store, B, K2, "c");
    assert!(
        store
            .damage(
                A,
                &K1,
                Damage::MovedFrom {
                    partition: B,
                    key: K1
                }
            )
            .unwrap()
    );
    assert!(
        store
            .damage(
                B,
                &K2,
                Damage::MovedFrom {
                    partition: B,
                    key: K1
                }
            )
            .unwrap()
    );
    store.restart().unwrap();
    assert_eq!(
        store.recover(NOW).unwrap(),
        RecoveryReport {
            orphans_removed: 0,
            entries_removed: 1
        }
    );
    assert_eq!(
        store.read(B, &K2, NOW).unwrap_err().kind(),
        StoreErrorKind::Corrupted
    );
    assert_eq!(page(&mut store, B, K1).unwrap(), b"b/page");

    // No damage without an entry, a source, or with itself as the source.
    assert!(!store.damage(A, &K1, Damage::Content).unwrap());
    assert!(!store.damage(A, &K1, Damage::Missing).unwrap());
    assert!(
        !store
            .damage(
                B,
                &K1,
                Damage::MovedFrom {
                    partition: A,
                    key: K1
                }
            )
            .unwrap()
    );
    assert!(
        !store
            .damage(
                B,
                &K1,
                Damage::MovedFrom {
                    partition: B,
                    key: K1
                }
            )
            .unwrap()
    );
    assert_eq!(page(&mut store, B, K1).unwrap(), b"b/page");
}

#[test]
fn store_fault_damage_of_an_empty_page() {
    let mut store = ready(StopKind::PowerLoss);
    let (meta, mut body) = sample_entry(A, K1, "shop.example.com", "e", 0);
    body.page = rivqen_model::safe::PageBytes::new(Vec::new());
    let txn = CacheTransaction {
        op: TxnOp::Replace {
            meta: Box::new(meta),
            body: Box::new(body),
        },
        pre: pre(&mut store, A, Expect::Absent),
    };
    store.apply(txn).unwrap();
    assert!(store.damage(A, &K1, Damage::Content).unwrap());
    store.restart().unwrap();
    // The page got a byte: the size differs.
    assert_eq!(
        store.recover(NOW).unwrap(),
        RecoveryReport {
            orphans_removed: 0,
            entries_removed: 1
        }
    );
}

#[test]
fn store_fault_touch_is_lost_at_a_stop_until_a_commit() {
    let mut store = ready(StopKind::ProcessKill);
    put(&mut store, A, K1, "a");
    store.touch(A, &K1, NOW.plus(5)).unwrap();
    assert_eq!(store.list().unwrap()[0].meta.last_used_at, NOW.plus(5));
    store.restart().unwrap();
    store.recover(NOW).unwrap();
    assert_eq!(store.list().unwrap()[0].meta.last_used_at, NOW);
    store.touch(A, &K1, NOW.plus(7)).unwrap();
    put(&mut store, B, K2, "b");
    store.restart().unwrap();
    store.recover(NOW).unwrap();
    let entry = store.read(A, &K1, NOW).unwrap().unwrap();
    assert_eq!(entry.meta.last_used_at, NOW.plus(7));
}

#[test]
fn store_fault_list_sizes_count_the_fields() {
    let mut store = ready(StopKind::PowerLoss);
    let (meta, body) = sample_entry(A, K1, "shop.example.com", "a", 7);
    let size = body.size_bytes();
    let txn = CacheTransaction {
        op: TxnOp::Replace {
            meta: Box::new(meta),
            body: Box::new(body),
        },
        pre: pre(&mut store, A, Expect::Absent),
    };
    store.apply(txn).unwrap();
    assert_eq!(store.list().unwrap()[0].size_bytes, size);
    let fields = vec![rivqen_model::http::FieldLine::new(
        "x-long-name",
        "0123456789",
    )];
    let update = CacheTransaction {
        op: TxnOp::UpdateFields {
            partition: A,
            key: K1,
            fields: fields.clone(),
            policy: sample_policy(3),
        },
        pre: pre(&mut store, A, Expect::Any),
    };
    store.apply(update).unwrap();
    let entry = store.read(A, &K1, NOW).unwrap().unwrap();
    assert_eq!(entry.body.fields, fields);
    assert_eq!(store.list().unwrap()[0].size_bytes, entry.body.size_bytes());
}

#[test]
fn store_fault_disabled_key_boundary() {
    let mut store = ready(StopKind::PowerLoss);
    put(&mut store, A, K1, "a");
    let until = UnixTime(NOW.0 + 100);
    let txn = CacheTransaction {
        op: TxnOp::DisableKey {
            partition: A,
            key: K1,
            until,
        },
        pre: pre(&mut store, A, Expect::Any),
    };
    assert_eq!(
        store.apply(txn).unwrap(),
        TxnOutcome::Applied { version: None }
    );
    assert!(store.key_disabled(A, &K1, UnixTime(until.0 - 1)).unwrap());
    assert!(!store.key_disabled(A, &K1, until).unwrap());
    assert!(!store.key_disabled(B, &K1, NOW).unwrap());
    assert!(!store.key_disabled(A, &K2, NOW).unwrap());
    // A replace during the period is stored and hidden.
    put(&mut store, A, K1, "b");
    assert_eq!(store.read(A, &K1, UnixTime(until.0 - 1)).unwrap(), None);
    assert_eq!(
        store
            .read(A, &K1, until)
            .unwrap()
            .unwrap()
            .body
            .page
            .as_slice(),
        b"b/page"
    );
    assert_eq!(store.list().unwrap().len(), 1);
    // A site clear with a domain keeps the disabled key; one without clears it.
    let clear = |domain: Option<&str>, store: &mut FakeStore| CacheTransaction {
        op: TxnOp::ClearSite {
            domain: domain.map(str::to_owned),
        },
        pre: pre(store, A, Expect::Any),
    };
    let txn = clear(Some("example.com"), &mut store);
    store.apply(txn).unwrap();
    assert!(store.key_disabled(A, &K1, NOW).unwrap());
    let txn = clear(None, &mut store);
    store.apply(txn).unwrap();
    assert!(!store.key_disabled(A, &K1, NOW).unwrap());
}

#[test]
fn store_fault_second_writer_between_steps() {
    // A revoke between Promote and CommitMeta of a replace: the replace is
    // refused at the commit point and its promoted file is deleted.
    let mut store = ready(StopKind::PowerLoss);
    put(&mut store, A, K1, "old");
    put(&mut store, B, K1, "b");
    let txn = replace(&mut store, A, K2, "late");
    let revoke = CacheTransaction {
        op: TxnOp::RevokePartition { partition: A },
        pre: pre(&mut store, A, Expect::Any),
    };
    store.run_between(CommitStep::Promote, revoke);
    assert_eq!(
        store.apply(txn).unwrap(),
        TxnOutcome::Refused(Refused::PartitionRevoked)
    );
    assert_eq!(
        store.between_outcome(),
        Some(Ok(TxnOutcome::Applied { version: None }))
    );
    assert_eq!(store.between_outcome(), None);
    assert_eq!(store.read(A, &K1, NOW).unwrap(), None);
    assert_eq!(store.read(A, &K2, NOW).unwrap(), None);
    assert_eq!(store.files(), files(0, 1));
    store.restart().unwrap();
    // The old file of A and the promoted file come back after a power loss
    // and recovery removes both.
    assert_eq!(
        store.recover(NOW).unwrap(),
        RecoveryReport {
            orphans_removed: 2,
            entries_removed: 0
        }
    );
    assert_eq!(store.files(), files(0, 1));
    assert_eq!(page(&mut store, B, K1).unwrap(), b"b/page");
}

#[test]
fn store_fault_debug_has_no_content() {
    let mut store = ready(StopKind::PowerLoss);
    let (meta, body) = sample_entry(A, K1, "secret-host.example", "s3cr3t-marker", 7);
    let txn = CacheTransaction {
        op: TxnOp::Replace {
            meta: Box::new(meta),
            body: Box::new(body),
        },
        pre: pre(&mut store, A, Expect::Absent),
    };
    store.apply(txn).unwrap();
    let text = format!("{store:?}");
    assert!(!text.contains("s3cr3t"), "{text}");
    assert!(!text.contains("secret-host"), "{text}");
    assert!(text.contains("entries: 1"), "{text}");
}

/// The suite finds a driver with a defect: each defect fails the check
/// that covers it.
#[test]
fn store_fault_defects_fail_the_suite() {
    let mut early_delete = || FakeStore::new().with_defect(Defect::RemoveOldBeforeCommit);
    let violation = contract::old_or_new(&mut early_delete).unwrap_err();
    assert_eq!(violation.check, "old_or_new");

    let mut unsynced = || FakeStore::new().with_defect(Defect::UnsyncedPromote);
    assert!(contract::old_or_new(&mut unsynced).is_err());
    // A process kill does not show it: only the power loss model does.
    let mut unsynced_kill = || {
        FakeStore::new()
            .with_defect(Defect::UnsyncedPromote)
            .with_stop_kind(StopKind::ProcessKill)
    };
    assert!(contract::old_or_new(&mut unsynced_kill).is_ok());

    let mut no_pre = || FakeStore::new().with_defect(Defect::NoPreconditions);
    assert!(contract::refused_partition_revoked(&mut no_pre).is_err());
    assert!(contract::refused_site_cleared(&mut no_pre).is_err());
    assert!(contract::refused_entry(&mut no_pre).is_err());

    let mut leaky = || FakeStore::new().with_defect(Defect::ReadIgnoresPartition);
    assert!(contract::partitions_never_mix(&mut leaky).is_err());
    let script = [
        ScriptOp::new(Action::Replace, 0, 0),
        ScriptOp::new(Action::Replace, 2, 1),
    ];
    let violation = contract::script_keeps_partitions_apart(&mut leaky(), &script).unwrap_err();
    assert_eq!(violation.check, "script_keeps_partitions_apart");
    assert!(contract::run_script(&mut leaky(), &script).is_err());
    assert!(contract::script_keeps_partitions_apart(&mut FakeStore::new(), &script).is_ok());
}

/// The suite finds each defect of the DL-205 rules, and only the check of
/// the rule fails for it among the refusal checks.
#[test]
fn store_fault_dl205_defects_fail_the_suite() {
    let mut no_pre = || FakeStore::new().with_defect(Defect::NoPreconditions);
    assert!(contract::refused_request_partition_revoked(&mut no_pre).is_err());
    assert!(contract::refused_invalid_host(&mut no_pre).is_err());
    assert!(contract::refusal_order(&mut no_pre).is_err());

    let mut no_request = || FakeStore::new().with_defect(Defect::NoRequestCheck);
    let violation = contract::refused_request_partition_revoked(&mut no_request).unwrap_err();
    assert_eq!(violation.check, "refused_request_partition_revoked");
    assert!(contract::refusal_order(&mut no_request).is_err());
    assert!(contract::refused_partition_revoked(&mut no_request).is_ok());
    assert!(contract::refused_invalid_host(&mut no_request).is_ok());
    assert!(contract::run_all(&mut no_request).is_err());

    let mut any_host = || FakeStore::new().with_defect(Defect::AnyHost);
    let violation = contract::refused_invalid_host(&mut any_host).unwrap_err();
    assert_eq!(violation.check, "refused_invalid_host");
    assert!(contract::refusal_order(&mut any_host).is_err());
    assert!(contract::refused_request_partition_revoked(&mut any_host).is_ok());
    assert!(contract::run_all(&mut any_host).is_err());

    let mut keeps = || FakeStore::new().with_defect(Defect::KeepsExpiredDisabledKeys);
    let violation = contract::recover_removes_expired_disabled_keys(&mut keeps).unwrap_err();
    assert_eq!(violation.check, "recover_removes_expired_disabled_keys");
    assert!(contract::disable_key(&mut keeps).is_ok());
    assert!(contract::run_all(&mut keeps).is_err());
}

/// `recover(now)` removes a disabled-key record whose time is before `now`
/// and keeps one whose time is `now` or later, on both sides of the
/// boundary; the removal survives a power loss; the entry of the expired
/// key becomes readable; a site clear and a revoke are not needed for it.
#[test]
fn store_fault_recover_removes_expired_disabled_keys() {
    let mut store = ready(StopKind::PowerLoss);
    let now = UnixTime(NOW.0 + 100);
    let disable = |store: &mut FakeStore, partition, key, until| {
        let txn = CacheTransaction {
            op: TxnOp::DisableKey {
                partition,
                key,
                until,
            },
            pre: pre(store, partition, Expect::Any),
        };
        assert_eq!(
            store.apply(txn).unwrap(),
            TxnOutcome::Applied { version: None }
        );
    };
    // One second before now, exactly now, one second after now.
    disable(&mut store, A, K1, UnixTime(now.0 - 1));
    disable(&mut store, A, K2, now);
    disable(&mut store, B, K1, UnixTime(now.0 + 1));
    put(&mut store, A, K1, "a");
    assert_eq!(store.read(A, &K1, NOW).unwrap(), None);
    // A recover at an earlier time removes nothing.
    assert_eq!(store.recover(NOW).unwrap(), RecoveryReport::default());
    assert!(store.key_disabled(A, &K1, NOW).unwrap());

    store.restart().unwrap();
    assert_eq!(store.recover(now).unwrap(), RecoveryReport::default());
    // Asked at NOW, before every time: only the removed record is gone.
    assert!(!store.key_disabled(A, &K1, NOW).unwrap());
    assert!(store.key_disabled(A, &K2, NOW).unwrap());
    assert!(store.key_disabled(B, &K1, NOW).unwrap());
    assert_eq!(page(&mut store, A, K1).unwrap(), b"a/page");

    // Recovery synced the removal: a power loss does not bring it back.
    store.restart().unwrap();
    store.recover(NOW).unwrap();
    assert!(!store.key_disabled(A, &K1, NOW).unwrap());
    assert!(store.key_disabled(A, &K2, NOW).unwrap());

    // One second later the record of `now` goes too.
    store.restart().unwrap();
    store.recover(UnixTime(now.0 + 1)).unwrap();
    assert!(!store.key_disabled(A, &K2, NOW).unwrap());
    assert!(store.key_disabled(B, &K1, NOW).unwrap());
}
