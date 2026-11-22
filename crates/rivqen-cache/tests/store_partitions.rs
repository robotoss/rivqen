// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! Multi-partition acceptance tests of the cache store (WP-07 S1, T-05;
//! R-CACHE-01, R-CACHE-04, R-CACHE-07, R-CACHE-09; legacy client section
//! 9.12; RSK-06). They run on the fake store of the test kit.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use rivqen_cache::driver::{RecoveryReport, StorageDriver};
use rivqen_cache::key::{KeyParts, Partition, Protocol, cache_key};
use rivqen_cache::txn::{
    CacheTransaction, CommitStep, Expect, Preconditions, Refused, StoredEntry, TxnOp, TxnOutcome,
};
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_testkit::store::contract::{NOW, sample_entry};
use rivqen_testkit::store::{FakeStore, FaultInjection, FileCount, StopKind};

const ORIGIN: &str = "https://shop.example.com";
const URL: &str = "https://shop.example.com/cart?item=7";
const HOST: &str = "shop.example.com";

fn profile(byte: u8) -> Partition {
    Partition::Scoped(PartitionId::from_bytes([byte; 32]))
}

fn key_for(partition: Partition) -> CacheKey {
    cache_key(&KeyParts {
        partition,
        origin: ORIGIN,
        canonical_url: URL,
        vary: &[],
        protocol: Protocol::Rqp,
        locale: None,
    })
}

fn ready() -> FakeStore {
    let mut store = FakeStore::new();
    store.recover(NOW).unwrap();
    store
}

/// The preconditions that a session reads when it starts. A scoped
/// partition is also the partition of the request; `Public` has none here.
fn session_start(store: &mut FakeStore, partition: Partition, entry: Expect) -> Preconditions {
    match partition {
        Partition::Scoped(id) => session_of(store, id, partition, entry),
        Partition::Public => Preconditions {
            entry,
            request: None,
            partition_epoch: store.partition_epoch(partition).unwrap(),
            site_epoch: store.site_epoch().unwrap(),
        },
    }
}

/// The preconditions of a session of the request partition `request` that
/// writes into `target`.
fn session_of(
    store: &mut FakeStore,
    request: PartitionId,
    target: Partition,
    entry: Expect,
) -> Preconditions {
    Preconditions {
        entry,
        request: Some((
            request,
            store.partition_epoch(Partition::Scoped(request)).unwrap(),
        )),
        partition_epoch: store.partition_epoch(target).unwrap(),
        site_epoch: store.site_epoch().unwrap(),
    }
}

fn id_of(partition: Partition) -> PartitionId {
    match partition {
        Partition::Scoped(id) => id,
        Partition::Public => panic!("not scoped"),
    }
}

fn replace(partition: Partition, key: CacheKey, tag: &str, pre: Preconditions) -> CacheTransaction {
    let (meta, body) = sample_entry(partition, key, HOST, tag, 7);
    CacheTransaction {
        op: TxnOp::Replace {
            meta: Box::new(meta),
            body: Box::new(body),
        },
        pre,
    }
}

fn revoke(partition: Partition, store: &mut FakeStore) -> CacheTransaction {
    CacheTransaction {
        op: TxnOp::RevokePartition { partition },
        pre: session_start(store, partition, Expect::Any),
    }
}

fn read(store: &mut FakeStore, partition: Partition, key: CacheKey) -> Option<StoredEntry> {
    store.read(partition, &key, NOW).unwrap()
}

fn content(entry: &StoredEntry) -> Vec<u8> {
    let mut bytes = entry.body.page.as_slice().to_vec();
    if let Some(template) = &entry.body.template {
        bytes.extend_from_slice(template.as_slice());
    }
    for (name, value) in &entry.body.data {
        bytes.extend_from_slice(name.as_bytes());
        bytes.extend_from_slice(value.as_slice());
    }
    bytes
}

/// Two profiles open the same URL: two entries, each read in its own
/// partition, and a change of one never reaches the other.
#[test]
fn store_partitions_same_url_two_profiles() {
    let (alice, bob) = (profile(0xA1), profile(0xB2));
    let mut store = ready();
    let (key_a, key_b) = (key_for(alice), key_for(bob));
    let pre = session_start(&mut store, alice, Expect::Absent);
    store
        .apply(replace(alice, key_a, "alice-secret", pre))
        .unwrap();
    let pre = session_start(&mut store, bob, Expect::Absent);
    store.apply(replace(bob, key_b, "bob-secret", pre)).unwrap();

    let a = read(&mut store, alice, key_a).unwrap();
    let b = read(&mut store, bob, key_b).unwrap();
    assert_eq!(a.meta.partition, alice);
    assert_eq!(b.meta.partition, bob);
    // No shared bytes: no content of one entry holds the tag of the other,
    // and each has its own files.
    assert!(!content(&a).windows(3).any(|w| w == b"bob"));
    assert!(!content(&b).windows(5).any(|w| w == b"alice"));
    assert_eq!(store.files(), FileCount { temp: 0, entry: 2 });
    // The key of one profile reads nothing in the other.
    if key_a != key_b {
        assert_eq!(read(&mut store, bob, key_a), None);
        assert_eq!(read(&mut store, alice, key_b), None);
    }
    assert_eq!(read(&mut store, Partition::Public, key_a), None);

    // A remove in one profile leaves the other.
    let pre = session_start(&mut store, alice, Expect::Any);
    store
        .apply(CacheTransaction {
            op: TxnOp::Remove {
                partition: alice,
                key: key_a,
            },
            pre,
        })
        .unwrap();
    assert_eq!(read(&mut store, alice, key_a), None);
    assert_eq!(read(&mut store, bob, key_b).unwrap(), b);
}

/// The worst case for the store: both profiles use the same key bytes.
/// The store still keeps two entries.
#[test]
fn store_partitions_same_key_bytes_stay_apart() {
    let (alice, bob) = (profile(0xA1), profile(0xB2));
    let key = CacheKey::from_bytes([0x5A; 32]);
    let mut store = ready();
    for (partition, tag) in [
        (alice, "alice"),
        (bob, "bob"),
        (Partition::Public, "public"),
    ] {
        let pre = session_start(&mut store, partition, Expect::Absent);
        store.apply(replace(partition, key, tag, pre)).unwrap();
    }
    for (partition, tag) in [
        (alice, "alice"),
        (bob, "bob"),
        (Partition::Public, "public"),
    ] {
        let entry = read(&mut store, partition, key).unwrap();
        assert_eq!(entry.meta.partition, partition);
        assert_eq!(entry.body.page.as_slice(), format!("{tag}/page").as_bytes());
    }
    let txn = revoke(alice, &mut store);
    store.apply(txn).unwrap();
    assert_eq!(read(&mut store, alice, key), None);
    assert_eq!(
        read(&mut store, bob, key).unwrap().body.page.as_slice(),
        b"bob/page"
    );
    assert_eq!(
        read(&mut store, Partition::Public, key)
            .unwrap()
            .body
            .page
            .as_slice(),
        b"public/page"
    );
}

/// With the key layout of T-01, the same URL gives another key in each
/// partition (R-CACHE-01).
#[test]
fn store_partitions_keys_differ() {
    assert_ne!(key_for(profile(0xA1)), key_for(profile(0xB2)));
    assert_ne!(key_for(profile(0xA1)), key_for(Partition::Public));
}

/// A session reads the epochs at its start; a logout revokes the
/// partition; the session's commit comes after the revoke and is refused.
#[test]
fn store_partitions_commit_planned_before_revoke_is_refused() {
    let alice = profile(0xA1);
    let mut store = ready();
    let key = key_for(alice);
    let pre_old = session_start(&mut store, alice, Expect::Absent);
    let txn = revoke(alice, &mut store);
    assert_eq!(
        store.apply(txn).unwrap(),
        TxnOutcome::Applied { version: None }
    );
    for op in [
        replace(alice, key, "late", pre_old),
        CacheTransaction {
            op: TxnOp::UpdateFields {
                partition: alice,
                key,
                fields: Vec::new(),
                policy: rivqen_testkit::store::contract::sample_policy(60),
            },
            pre: pre_old,
        },
        CacheTransaction {
            op: TxnOp::DisableKey {
                partition: alice,
                key,
                until: NOW.plus(60),
            },
            pre: pre_old,
        },
    ] {
        assert_eq!(
            store.apply(op).unwrap(),
            TxnOutcome::Refused(Refused::PartitionRevoked)
        );
    }
    assert_eq!(read(&mut store, alice, key), None);
    assert!(!store.key_disabled(alice, &key, NOW).unwrap());
    assert_eq!(store.list().unwrap(), Vec::new());
    assert_eq!(store.files(), FileCount::default());
    // A new session of the same profile reads the new epoch and commits.
    let pre_new = session_start(&mut store, alice, Expect::Absent);
    assert!(matches!(
        store.apply(replace(alice, key, "fresh", pre_new)).unwrap(),
        TxnOutcome::Applied { version: Some(_) }
    ));
}

/// The same for a site clear (CL-45): a commit planned before the clear is
/// refused, in every partition.
#[test]
fn store_partitions_commit_planned_before_site_clear_is_refused() {
    let (alice, bob) = (profile(0xA1), profile(0xB2));
    let mut store = ready();
    let pre_a = session_start(&mut store, alice, Expect::Absent);
    let pre_b = session_start(&mut store, bob, Expect::Absent);
    let clear = CacheTransaction {
        op: TxnOp::ClearSite {
            domain: Some("example.com".to_owned()),
        },
        pre: pre_a,
    };
    store.apply(clear).unwrap();
    for (partition, pre) in [(alice, pre_a), (bob, pre_b)] {
        let txn = replace(partition, key_for(partition), "late", pre);
        assert_eq!(
            store.apply(txn).unwrap(),
            TxnOutcome::Refused(Refused::SiteCleared)
        );
    }
    assert_eq!(store.list().unwrap(), Vec::new());
}

/// A revoke runs between two steps of a pending commit of the same
/// partition: the commit is refused at its commit point and leaves no
/// entry and no file of the old partition, for every step.
#[test]
fn store_partitions_revoke_during_pending_commit() {
    let (alice, bob) = (profile(0xA1), profile(0xB2));
    for step in [
        CommitStep::WriteTemp,
        CommitStep::SyncTemp,
        CommitStep::Promote,
    ] {
        let mut store = ready();
        let key = key_for(alice);
        let pre = session_start(&mut store, alice, Expect::Absent);
        store.apply(replace(alice, key, "old", pre)).unwrap();
        let pre = session_start(&mut store, bob, Expect::Absent);
        store.apply(replace(bob, key_for(bob), "bob", pre)).unwrap();

        let pending = session_start(&mut store, alice, Expect::Any);
        let txn = revoke(alice, &mut store);
        store.run_between(step, txn);
        let outcome = store.apply(replace(alice, key, "new", pending)).unwrap();
        assert_eq!(
            outcome,
            TxnOutcome::Refused(Refused::PartitionRevoked),
            "{step:?}"
        );
        assert_eq!(
            store.between_outcome(),
            Some(Ok(TxnOutcome::Applied { version: None }))
        );
        assert_eq!(read(&mut store, alice, key), None, "{step:?}");
        assert!(
            store
                .list()
                .unwrap()
                .iter()
                .all(|s| s.meta.partition != alice)
        );
        // Only the file of bob is left.
        assert_eq!(store.files(), FileCount { temp: 0, entry: 1 }, "{step:?}");
        store.restart().unwrap();
        store.recover(NOW).unwrap();
        assert_eq!(store.files(), FileCount { temp: 0, entry: 1 }, "{step:?}");
        assert_eq!(read(&mut store, alice, key), None, "{step:?}");
        assert!(read(&mut store, bob, key_for(bob)).is_some());
    }
}

/// A commit after its commit point, then a revoke before the old files are
/// deleted: the revoke removes the new entry too.
#[test]
fn store_partitions_revoke_after_commit_point() {
    let alice = profile(0xA1);
    let mut store = ready();
    let key = key_for(alice);
    let pre = session_start(&mut store, alice, Expect::Absent);
    store.apply(replace(alice, key, "old", pre)).unwrap();
    let pending = session_start(&mut store, alice, Expect::Any);
    let txn = revoke(alice, &mut store);
    store.run_between(CommitStep::CommitMeta, txn);
    let outcome = store.apply(replace(alice, key, "new", pending)).unwrap();
    assert!(matches!(outcome, TxnOutcome::Applied { version: Some(_) }));
    assert_eq!(read(&mut store, alice, key), None);
    assert_eq!(store.files(), FileCount::default());
}

/// The process stops during a commit; the logout runs at the next start
/// (after recovery): nothing of the partition is readable, also not the
/// files of the stopped commit.
#[test]
fn store_partitions_revoke_after_a_stopped_commit() {
    let (alice, bob) = (profile(0xA1), profile(0xB2));
    for kind in [StopKind::PowerLoss, StopKind::ProcessKill] {
        for step in CommitStep::ALL {
            let mut store = FakeStore::new().with_stop_kind(kind);
            store.recover(NOW).unwrap();
            let key = key_for(alice);
            let pre = session_start(&mut store, alice, Expect::Absent);
            store.apply(replace(alice, key, "old", pre)).unwrap();
            let pre = session_start(&mut store, bob, Expect::Absent);
            store.apply(replace(bob, key_for(bob), "bob", pre)).unwrap();
            let pre = session_start(&mut store, alice, Expect::Any);
            store.stop_after(step);
            assert!(store.apply(replace(alice, key, "new", pre)).is_err());
            store.restart().unwrap();
            store.recover(NOW).unwrap();
            let txn = revoke(alice, &mut store);
            store.apply(txn).unwrap();
            assert_eq!(read(&mut store, alice, key), None, "{kind:?} {step:?}");
            assert_eq!(store.list().unwrap().len(), 1, "{kind:?} {step:?}");
            // The deleted files of alice are orphans after a power loss;
            // the next recovery removes them.
            store.restart().unwrap();
            let report = store.recover(NOW).unwrap();
            assert_eq!(report.entries_removed, 0);
            assert_eq!(
                store.files(),
                FileCount { temp: 0, entry: 1 },
                "{kind:?} {step:?}"
            );
            assert_eq!(store.recover(NOW).unwrap(), RecoveryReport::default());
        }
    }
}

/// A revoke of a scoped partition keeps the `Public` entry of the same URL;
/// a revoke of `Public` keeps the scoped one.
#[test]
fn store_partitions_public_and_scoped_revoke_apart() {
    let alice = profile(0xA1);
    let mut store = ready();
    let key = key_for(alice);
    let public_key = key_for(Partition::Public);
    let pre = session_start(&mut store, alice, Expect::Absent);
    store.apply(replace(alice, key, "alice", pre)).unwrap();
    let pre = session_start(&mut store, Partition::Public, Expect::Absent);
    store
        .apply(replace(Partition::Public, public_key, "public", pre))
        .unwrap();
    let txn = revoke(alice, &mut store);
    store.apply(txn).unwrap();
    assert!(read(&mut store, Partition::Public, public_key).is_some());
    let pre = session_start(&mut store, alice, Expect::Absent);
    store.apply(replace(alice, key, "alice2", pre)).unwrap();
    let txn = revoke(Partition::Public, &mut store);
    store.apply(txn).unwrap();
    assert_eq!(read(&mut store, Partition::Public, public_key), None);
    assert_eq!(
        read(&mut store, alice, key).unwrap().body.page.as_slice(),
        b"alice2/page"
    );
}

/// A session of a profile plans a write into `Public`; the profile logs
/// out (its partition is revoked) before the commit: the commit is refused
/// for every keyed kind, also when the revoke runs between two steps, and
/// the `Public` entry of another profile stays (DL-205; cache identity
/// rule 4, RSK-06). A session of another profile and a new session of the
/// same profile commit.
#[test]
fn store_partitions_revoked_request_cannot_commit_to_public() {
    let (alice, bob) = (profile(0xA1), profile(0xB2));
    let mut store = ready();
    let key = key_for(Partition::Public);
    let other = CacheKey::from_bytes([0x77; 32]);
    let pre = session_of(&mut store, id_of(bob), Partition::Public, Expect::Absent);
    store
        .apply(replace(Partition::Public, key, "bob-public", pre))
        .unwrap();
    let bob_entry = read(&mut store, Partition::Public, key).unwrap();

    let late = session_of(&mut store, id_of(alice), Partition::Public, Expect::Any);
    let pre_bob = session_of(&mut store, id_of(bob), Partition::Public, Expect::Absent);
    let txn = revoke(alice, &mut store);
    store.apply(txn).unwrap();
    // The epoch of `Public` did not change: only the request check refuses.
    assert_eq!(
        late.partition_epoch,
        store.partition_epoch(Partition::Public).unwrap()
    );
    for op in [
        replace(Partition::Public, other, "alice-late", late),
        replace(Partition::Public, key, "alice-late", late),
        CacheTransaction {
            op: TxnOp::UpdateFields {
                partition: Partition::Public,
                key,
                fields: Vec::new(),
                policy: rivqen_testkit::store::contract::sample_policy(60),
            },
            pre: late,
        },
        CacheTransaction {
            op: TxnOp::Remove {
                partition: Partition::Public,
                key,
            },
            pre: late,
        },
        CacheTransaction {
            op: TxnOp::DisableKey {
                partition: Partition::Public,
                key,
                until: NOW.plus(60),
            },
            pre: late,
        },
    ] {
        assert_eq!(
            store.apply(op).unwrap(),
            TxnOutcome::Refused(Refused::PartitionRevoked)
        );
    }
    assert_eq!(read(&mut store, Partition::Public, key).unwrap(), bob_entry);
    assert_eq!(read(&mut store, Partition::Public, other), None);
    assert!(!store.key_disabled(Partition::Public, &key, NOW).unwrap());
    assert_eq!(store.files(), FileCount { temp: 0, entry: 1 });

    // The session of bob is not touched by the logout of alice.
    assert!(matches!(
        store
            .apply(replace(Partition::Public, other, "bob-other", pre_bob))
            .unwrap(),
        TxnOutcome::Applied { version: Some(_) }
    ));

    // A revoke between two steps of the commit: refused at the commit
    // point, and the promoted file is gone.
    for step in [
        CommitStep::WriteTemp,
        CommitStep::SyncTemp,
        CommitStep::Promote,
    ] {
        let pending = session_of(&mut store, id_of(alice), Partition::Public, Expect::Any);
        let txn = revoke(alice, &mut store);
        store.run_between(step, txn);
        let outcome = store
            .apply(replace(Partition::Public, key, "alice-pending", pending))
            .unwrap();
        assert_eq!(
            outcome,
            TxnOutcome::Refused(Refused::PartitionRevoked),
            "{step:?}"
        );
        assert_eq!(
            store.between_outcome(),
            Some(Ok(TxnOutcome::Applied { version: None }))
        );
        assert_eq!(
            read(&mut store, Partition::Public, key).unwrap(),
            bob_entry,
            "{step:?}"
        );
        assert_eq!(store.files(), FileCount { temp: 0, entry: 2 }, "{step:?}");
    }

    // A new session of alice reads the new epoch and commits.
    let fresh = session_of(&mut store, id_of(alice), Partition::Public, Expect::Any);
    assert!(matches!(
        store
            .apply(replace(Partition::Public, key, "alice-new", fresh))
            .unwrap(),
        TxnOutcome::Applied { version: Some(_) }
    ));
}

/// A site clear removes the site in every partition, also `Public`.
#[test]
fn store_partitions_site_clear_reaches_every_partition() {
    let (alice, bob) = (profile(0xA1), profile(0xB2));
    let mut store = ready();
    for partition in [alice, bob, Partition::Public] {
        let pre = session_start(&mut store, partition, Expect::Absent);
        store
            .apply(replace(partition, key_for(partition), "x", pre))
            .unwrap();
    }
    let pre = session_start(&mut store, alice, Expect::Any);
    let clear = CacheTransaction {
        op: TxnOp::ClearSite {
            domain: Some("example.com".to_owned()),
        },
        pre,
    };
    store.apply(clear).unwrap();
    assert_eq!(store.list().unwrap(), Vec::new());
    assert_eq!(store.files(), FileCount::default());
}
