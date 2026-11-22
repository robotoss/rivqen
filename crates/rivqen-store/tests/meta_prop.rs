// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! Property: a random sequence of row writes and deletes, in transactions
//! that commit or roll back, leaves the `entry` table equal to a map model
//! (WP-07 S2 T-04; ADR-004 section 3; RSK-06: three partitions share the
//! same key bytes). Each case works in its own directory under
//! `CARGO_TARGET_TMPDIR` and removes it at the end.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use proptest::prelude::*;
use rivqen_cache::driver::{StoreError, StoreErrorKind};
use rivqen_cache::key::Partition;
use rivqen_cache::txn::EntryVersion;
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_model::time::UnixTime;
use rivqen_store::meta::{MetaDb, Row};

/// Public, and two scoped partitions; the same key bytes in each.
const PARTITIONS: usize = 3;
const KEYS: u8 = 3;
/// The largest value a counter or a size can hold.
const I64_MAX: u64 = 0x7FFF_FFFF_FFFF_FFFF;

fn partition(index: usize) -> Partition {
    match index {
        0 => Partition::Public,
        n => Partition::Scoped(PartitionId::from_bytes([u8::try_from(n).unwrap(); 32])),
    }
}

fn key(byte: u8) -> CacheKey {
    CacheKey::from_bytes([byte; 32])
}

#[derive(Debug, Clone)]
enum Op {
    Put {
        partition: usize,
        key: u8,
        version: u64,
        content_bytes: u64,
        stored_at: u64,
        last_used_at: u64,
        host: usize,
        file_byte: u8,
        sealed: Vec<u8>,
    },
    Delete {
        partition: usize,
        key: u8,
    },
    DeletePartition {
        partition: usize,
    },
    Touch {
        partition: usize,
        key: u8,
        now: u64,
    },
}

const HOSTS: [&str; 3] = ["a.example", "shop.example.com", "127.0.0.1"];

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (
            0..PARTITIONS,
            0..KEYS,
            0..=I64_MAX,
            0..=I64_MAX,
            0..=I64_MAX,
            0..=I64_MAX,
            0..HOSTS.len(),
            any::<u8>(),
            proptest::collection::vec(any::<u8>(), 0..48),
        )
            .prop_map(
                |(partition, key, version, content_bytes, stored_at, last_used_at, host, file_byte, sealed)| {
                    Op::Put {
                        partition,
                        key,
                        version,
                        content_bytes,
                        stored_at,
                        last_used_at,
                        host,
                        file_byte,
                        sealed,
                    }
                }
            ),
        2 => (0..PARTITIONS, 0..KEYS).prop_map(|(partition, key)| Op::Delete { partition, key }),
        1 => (0..PARTITIONS).prop_map(|partition| Op::DeletePartition { partition }),
        1 => (0..PARTITIONS, 0..KEYS, 0..=I64_MAX)
            .prop_map(|(partition, key, now)| Op::Touch { partition, key, now }),
    ]
}

/// A group of operations in one transaction, and whether it commits.
fn group() -> impl Strategy<Value = (Vec<Op>, bool)> {
    (
        proptest::collection::vec(op(), 1..10),
        prop::bool::weighted(0.75),
    )
}

type Model = BTreeMap<(usize, u8), Row>;

/// Applies `op` to the store and to the model; checks the answers agree.
fn apply(
    txn: &mut rivqen_store::meta::MetaTxn<'_>,
    model: &mut Model,
    op: &Op,
) -> Result<(), StoreError> {
    match op {
        Op::Put {
            partition: p,
            key: k,
            version,
            content_bytes,
            stored_at,
            last_used_at,
            host,
            file_byte,
            sealed,
        } => {
            let row = Row {
                partition: partition(*p),
                key: key(*k),
                host: HOSTS[*host].to_owned(),
                version: EntryVersion(*version),
                file_id: [*file_byte; 16],
                content_bytes: *content_bytes,
                stored_at: UnixTime(*stored_at),
                last_used_at: UnixTime(*last_used_at),
                sealed: sealed.clone(),
            };
            txn.put_row(&row)?;
            model.insert((*p, *k), row);
        }
        Op::Delete {
            partition: p,
            key: k,
        } => {
            let removed = txn.delete_row(partition(*p), &key(*k))?;
            assert_eq!(removed, model.remove(&(*p, *k)).is_some());
        }
        Op::DeletePartition { partition: p } => {
            let removed = txn.delete_rows_of_partition(partition(*p))?;
            let before = model.len();
            model.retain(|(q, _), _| q != p);
            assert_eq!(removed, u64::try_from(before - model.len()).unwrap());
        }
        Op::Touch {
            partition: p,
            key: k,
            now,
        } => {
            let touched = txn.touch(partition(*p), &key(*k), UnixTime(*now))?;
            let found = model.get_mut(&(*p, *k));
            assert_eq!(touched, found.is_some());
            if let Some(row) = found {
                row.last_used_at = UnixTime(*now);
            }
        }
    }
    Ok(())
}

/// The store equals the model: every address, every partition, all rows.
fn check(txn: &rivqen_store::meta::MetaTxn<'_>, model: &Model) -> Result<(), StoreError> {
    for p in 0..PARTITIONS {
        for k in 0..KEYS {
            assert_eq!(txn.row(partition(p), &key(k))?.as_ref(), model.get(&(p, k)));
        }
        let expected: Vec<&Row> = model
            .iter()
            .filter(|((q, _), _)| *q == p)
            .map(|(_, row)| row)
            .collect();
        let found = txn.rows_of_partition(partition(p))?;
        assert_eq!(found.iter().collect::<Vec<_>>(), expected);
    }
    let mut all = txn.rows()?;
    let mut expected: Vec<Row> = model.values().cloned().collect();
    let order = |row: &Row| {
        (
            row.partition != Partition::Public,
            format!("{:?}", row.partition),
            *row.key.as_bytes(),
        )
    };
    all.sort_by_key(order);
    expected.sort_by_key(order);
    assert_eq!(all, expected);
    Ok(())
}

static CASE: AtomicUsize = AtomicUsize::new(0);

struct CaseDir(PathBuf);

impl CaseDir {
    fn new() -> Self {
        let case = CASE.fetch_add(1, Ordering::Relaxed);
        let path = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("rivqen-store-meta-prop")
            .join(case.to_string());
        if path.exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for CaseDir {
    fn drop(&mut self) {
        assert!(
            fs::remove_dir_all(&self.0).is_ok() || std::thread::panicking(),
            "cannot remove the case directory"
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 24,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn meta_prop_rows_equal_a_map_model(groups in proptest::collection::vec(group(), 1..6)) {
        let dir = CaseDir::new();
        let mut db = MetaDb::open(&dir.0).unwrap();
        let mut model = Model::new();
        for (ops, commit) in &groups {
            let mut working = model.clone();
            let result = db.transaction(|txn| {
                for op in ops {
                    apply(txn, &mut working, op)?;
                }
                check(txn, &working)?;
                if *commit {
                    Ok(())
                } else {
                    Err(StoreError::new(StoreErrorKind::Unavailable, "test"))
                }
            });
            prop_assert_eq!(result.is_ok(), *commit);
            if *commit {
                model = working;
            }
            db.transaction(|txn| check(txn, &model)).unwrap();
        }
        // The committed state survives a new open.
        drop(db);
        let mut db = MetaDb::open(&dir.0).unwrap();
        db.transaction(|txn| check(txn, &model)).unwrap();
    }
}
