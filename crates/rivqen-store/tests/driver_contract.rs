// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! The driver contract suite of `rivqen-testkit` on `FileStore` (WP-07 S2
//! T-05; DL-181; ADR-004 "Verification"). Each check runs as its own test;
//! `run_all` runs them in order. Every driver of a check is a new store in
//! its own directory under `CARGO_TARGET_TMPDIR`; each test removes its
//! directory at the end.
//!
//! `FaultInjection` is a trait of `rivqen-testkit`, a dev-dependency, so
//! the impl is on a wrapper here: `stop_after`, `fail_at` and `restart` call
//! the fault points of `FileStore`; `damage` changes the files on disk
//! directly, with the file id of the row read through a second connection
//! of `rivqen_store::meta`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use rivqen_cache::driver::{
    EntrySummary, RecoveryReport, StorageDriver, StoreError, StoreErrorKind,
};
use rivqen_cache::key::Partition;
use rivqen_cache::txn::{CacheTransaction, CommitStep, Epoch, StoredEntry, TxnOutcome};
use rivqen_model::ids::CacheKey;
use rivqen_model::time::UnixTime;
use rivqen_store::crypto::{FileId, HostKeys, MAGIC, NONCE_BYTES};
use rivqen_store::driver::{FileStore, entry_file_path, schema_dir};
use rivqen_store::meta::MetaDb;
use rivqen_testkit::store::contract::{self, Checked, ScriptOp};
use rivqen_testkit::store::{Damage, FaultInjection};

// ---------------------------------------------------------------- helpers

/// The directory of one test, removed on drop.
struct TestDir(PathBuf);

impl TestDir {
    fn new(name: &str) -> Self {
        let path = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("rivqen-store-driver-contract")
            .join(name);
        if path.exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    /// A factory of new stores, each in its own sub-directory.
    fn factory(&self) -> Maker<'_> {
        let mut next = 0_u32;
        Box::new(move || {
            next += 1;
            ContractStore::open(&self.0.join(format!("store-{next}")))
        })
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        assert!(
            fs::remove_dir_all(&self.0).is_ok() || std::thread::panicking(),
            "cannot remove the test directory"
        );
    }
}

fn unavailable(op: &'static str) -> StoreError {
    StoreError::new(StoreErrorKind::Unavailable, op)
}

/// `FileStore` with `FaultInjection`.
struct ContractStore {
    store: FileStore<HostKeys>,
    root: PathBuf,
}

impl ContractStore {
    fn open(root: &Path) -> Self {
        let keys = HostKeys::new().expect("system random source");
        Self {
            store: FileStore::open(root, keys).expect("open"),
            root: root.to_path_buf(),
        }
    }

    /// The entry file of the row of `key` in `partition`, read through a
    /// second connection while the driver is between two calls.
    fn file_of(&self, partition: Partition, key: &CacheKey) -> Result<Option<PathBuf>, StoreError> {
        let mut db = MetaDb::open(&schema_dir(&self.root))?;
        let row = db.transaction(|txn| txn.row(partition, key))?;
        Ok(row.map(|row| entry_file_path(&self.root, &FileId::from_bytes(row.file_id))))
    }
}

impl StorageDriver for ContractStore {
    fn recover(&mut self, now: UnixTime) -> Result<RecoveryReport, StoreError> {
        self.store.recover(now)
    }

    fn read(
        &mut self,
        partition: Partition,
        key: &CacheKey,
        now: UnixTime,
    ) -> Result<Option<StoredEntry>, StoreError> {
        self.store.read(partition, key, now)
    }

    fn apply(&mut self, txn: CacheTransaction) -> Result<TxnOutcome, StoreError> {
        self.store.apply(txn)
    }

    fn partition_epoch(&mut self, partition: Partition) -> Result<Epoch, StoreError> {
        self.store.partition_epoch(partition)
    }

    fn site_epoch(&mut self) -> Result<Epoch, StoreError> {
        self.store.site_epoch()
    }

    fn key_disabled(
        &mut self,
        partition: Partition,
        key: &CacheKey,
        now: UnixTime,
    ) -> Result<bool, StoreError> {
        self.store.key_disabled(partition, key, now)
    }

    fn list(&mut self) -> Result<Vec<EntrySummary>, StoreError> {
        self.store.list()
    }

    fn touch(
        &mut self,
        partition: Partition,
        key: &CacheKey,
        now: UnixTime,
    ) -> Result<(), StoreError> {
        self.store.touch(partition, key, now)
    }
}

/// The offset of the first page byte in a sealed file: the head, then the
/// format byte and the page length of the plain text. `XChaCha20` is a
/// stream cipher, so this ciphertext byte is the first page byte.
const PAGE_OFFSET: u64 = (MAGIC.len() + NONCE_BYTES + 1 + 8) as u64;

impl FaultInjection for ContractStore {
    fn stop_after(&mut self, step: CommitStep) {
        self.store.stop_after(step);
    }

    fn fail_at(&mut self, step: CommitStep) {
        self.store.fail_at(step);
    }

    fn restart(&mut self) -> Result<(), StoreError> {
        self.store.restart()
    }

    fn damage(
        &mut self,
        partition: Partition,
        key: &CacheKey,
        damage: Damage,
    ) -> Result<bool, StoreError> {
        let Some(target) = self.file_of(partition, key)? else {
            return Ok(false);
        };
        match damage {
            Damage::Content => {
                let mut file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&target)
                    .map_err(|_| unavailable("damage"))?;
                let len = file.metadata().map_err(|_| unavailable("damage"))?.len();
                let at = PAGE_OFFSET.min(len.saturating_sub(1));
                let mut byte = [0_u8; 1];
                file.seek(SeekFrom::Start(at))
                    .and_then(|_| file.read_exact(&mut byte))
                    .map_err(|_| unavailable("damage"))?;
                byte[0] ^= 0x01;
                file.seek(SeekFrom::Start(at))
                    .and_then(|_| file.write_all(&byte))
                    .and_then(|()| file.sync_all())
                    .map_err(|_| unavailable("damage"))?;
            }
            Damage::Missing => fs::remove_file(&target).map_err(|_| unavailable("damage"))?,
            Damage::MovedFrom {
                partition: source_partition,
                key: source_key,
            } => {
                if (source_partition, source_key) == (partition, *key) {
                    return Ok(false);
                }
                let Some(source) = self.file_of(source_partition, &source_key)? else {
                    return Ok(false);
                };
                fs::copy(&source, &target).map_err(|_| unavailable("damage"))?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}

fn passes(result: Checked) {
    if let Err(violation) = result {
        panic!("{violation}");
    }
}

/// A factory of new, empty stores.
type Maker<'a> = Box<dyn FnMut() -> ContractStore + 'a>;

/// Runs one check of the suite on new stores in the directory `name`.
fn check(name: &str, run: fn(&mut Maker<'_>) -> Checked) {
    let dir = TestDir::new(name);
    let mut make = dir.factory();
    passes(run(&mut make));
}

// ---------------------------------------------------------------- the suite

#[test]
fn driver_contract_replace_then_read() {
    check("replace_then_read", |make| {
        contract::replace_then_read(make)
    });
}

#[test]
fn driver_contract_update_fields() {
    check("update_fields", |make| contract::update_fields(make));
}

#[test]
fn driver_contract_remove() {
    check("remove", |make| contract::remove(make));
}

#[test]
fn driver_contract_disable_key() {
    check("disable_key", |make| contract::disable_key(make));
}

#[test]
fn driver_contract_revoke_partition() {
    check("revoke_partition", |make| contract::revoke_partition(make));
}

#[test]
fn driver_contract_clear_site_every_partition() {
    check("clear_site_every_partition", |make| {
        contract::clear_site_every_partition(make)
    });
}

#[test]
fn driver_contract_clear_site_keeps_other_sites() {
    check("clear_site_keeps_other_sites", |make| {
        contract::clear_site_keeps_other_sites(make)
    });
}

#[test]
fn driver_contract_clear_site_all() {
    check("clear_site_all", |make| contract::clear_site_all(make));
}

#[test]
fn driver_contract_refused_entry() {
    check("refused_entry", |make| contract::refused_entry(make));
}

#[test]
fn driver_contract_refused_partition_revoked() {
    check("refused_partition_revoked", |make| {
        contract::refused_partition_revoked(make)
    });
}

#[test]
fn driver_contract_refused_site_cleared() {
    check("refused_site_cleared", |make| {
        contract::refused_site_cleared(make)
    });
}

#[test]
fn driver_contract_refused_request_partition_revoked() {
    check("refused_request_partition_revoked", |make| {
        contract::refused_request_partition_revoked(make)
    });
}

#[test]
fn driver_contract_refused_request_partition_none_is_not_checked() {
    check("refused_request_partition_none", |make| {
        contract::refused_request_partition_none_is_not_checked(make)
    });
}

#[test]
fn driver_contract_refusal_order() {
    check("refusal_order", |make| contract::refusal_order(make));
}

#[test]
fn driver_contract_refused_invalid_host() {
    check("refused_invalid_host", |make| {
        contract::refused_invalid_host(make)
    });
}

#[test]
fn driver_contract_recover_removes_expired_disabled_keys() {
    check("recover_removes_expired_disabled_keys", |make| {
        contract::recover_removes_expired_disabled_keys(make)
    });
}

#[test]
fn driver_contract_removals_are_never_refused() {
    check("removals_are_never_refused", |make| {
        contract::removals_are_never_refused(make)
    });
}

#[test]
fn driver_contract_partitions_never_mix() {
    check("partitions_never_mix", |make| {
        contract::partitions_never_mix(make)
    });
}

#[test]
fn driver_contract_corrupted() {
    check("corrupted", |make| contract::corrupted(make));
}

#[test]
fn driver_contract_touch() {
    check("touch", |make| contract::touch(make));
}

#[test]
fn driver_contract_old_or_new() {
    check("old_or_new", |make| contract::old_or_new(make));
}

#[test]
fn driver_contract_failed_step_keeps_the_old_state() {
    check("failed_step_keeps_the_old_state", |make| {
        contract::failed_step_keeps_the_old_state(make)
    });
}

#[test]
fn driver_contract_run_all() {
    check("run_all", |make| contract::run_all(make));
}

// ---------------------------------------------------------------- properties

fn config(cases: u32, seed: u64) -> ProptestConfig {
    ProptestConfig {
        cases: std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|v| v.parse().ok())
            .map_or(cases, |wanted: u32| wanted.min(cases)),
        rng_seed: RngSeed::Fixed(seed),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

proptest! {
    // Real files and syncs: a small case count (the brief), a fixed seed.
    #![proptest_config(config(12, 0x7_05_05))]

    /// A random script with stops, failures, stale epochs and damage: after
    /// each step and each recovery the store equals the model of the suite.
    #[test]
    fn driver_contract_prop_model(script in prop::collection::vec(any::<u32>(), 0..24)) {
        let dir = TestDir::new("prop-model");
        let ops: Vec<ScriptOp> = script.into_iter().map(ScriptOp::decode).collect();
        let mut store = ContractStore::open(&dir.0.join("store"));
        if let Err(violation) = contract::run_script(&mut store, &ops) {
            panic!("{violation}");
        }
    }

    /// The same scripts without the model: a read never gives an entry of
    /// another partition, and an applied revoke leaves nothing of its
    /// partition.
    #[test]
    fn driver_contract_prop_partitions(script in prop::collection::vec(any::<u32>(), 0..24)) {
        let dir = TestDir::new("prop-partitions");
        let ops: Vec<ScriptOp> = script.into_iter().map(ScriptOp::decode).collect();
        let mut store = ContractStore::open(&dir.0.join("store"));
        if let Err(violation) = contract::script_keeps_partitions_apart(&mut store, &ops) {
            panic!("{violation}");
        }
    }
}
