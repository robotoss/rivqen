// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! The kill tests of the reference driver (WP-07 S2 T-05; INV-14; ADR-004
//! "Verification": "Kill at every step").
//!
//! For each transaction kind and each `CommitStep` the parent:
//!
//! 1. seeds a store in its own directory under `CARGO_TARGET_TMPDIR` and
//!    closes it;
//! 2. starts this test binary again as a child, with the ignored test
//!    `driver_kill_child_process` and the environment variable
//!    `RIVQEN_STORE_KILL_CHILD`. The child opens the store, recovers, sets
//!    the fault point `FileStore::stop_after(step)` and applies the
//!    transaction. The call returns right after the step, with no cleanup;
//!    the child writes a line on stdout and blocks on stdin with the store
//!    still open;
//! 3. sends `SIGKILL` to the child (`std::process::Child::kill`) when it
//!    reads that line, and checks that the child died of signal 9;
//! 4. opens the store, recovers and compares every entry, every disabled
//!    key and every epoch with the old state (a step before `CommitMeta`)
//!    or the new state (from `CommitMeta` on). The two states come from
//!    `FakeStore` of `rivqen-testkit` with the same seed and transaction;
//!    then a second recovery removes nothing and no file is left that no
//!    row names.
//!
//! So the process dies between two steps of the commit, with the open
//! SQLite connection and the open files of the driver: what the operating
//! system keeps of a killed process is what the parent finds. The library
//! has no kill code; only this test binary has.
//!
//! Keys: `HostKeys` keeps keys in memory, so the child and the parent use
//! [`FixedKeys`]: the key of a partition is a fixed function of the
//! partition, the same in both processes; random bytes (nonces, file ids)
//! come from the system source.
#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use rivqen_cache::driver::{RecoveryReport, StorageDriver, StoreError, StoreErrorKind};
use rivqen_cache::key::Partition;
use rivqen_cache::txn::{
    CacheTransaction, CommitStep, EntryMeta, Epoch, Expect, Preconditions, SnapshotBody, TxnOp,
    Writer,
};
use rivqen_model::http::FieldLine;
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_store::crypto::{DataKey, HostKeys, KEY_BYTES, KeyProvider};
use rivqen_store::driver::{FILES_DIR, FileStore, TMP_DIR, schema_dir};
use rivqen_testkit::store::FakeStore;
use rivqen_testkit::store::contract::{HOSTS, KEYS, NOW, PARTITIONS, sample_entry, sample_policy};

const CHILD_ENV: &str = "RIVQEN_STORE_KILL_CHILD";
const CHILD_TEST: &str = "driver_kill_child_process";
const READY: &str = "RIVQEN-KILL-READY";
/// The longest wait for the child to reach its step.
const CHILD_TIMEOUT: Duration = Duration::from_secs(120);

// ---------------------------------------------------------------- keys

/// A key port whose keys are a fixed function of the partition, so two
/// processes get the same keys. `delete_key` gives the next keys of the
/// partition in this process only. Not a key store: for this test only.
struct FixedKeys {
    generation: HashMap<Partition, u8>,
    random: HostKeys,
}

impl FixedKeys {
    fn new() -> Self {
        Self {
            generation: HashMap::new(),
            random: HostKeys::new().unwrap(),
        }
    }
}

impl KeyProvider for FixedKeys {
    fn data_key(&mut self, partition: Partition) -> Result<DataKey, StoreError> {
        let mut bytes = match partition {
            Partition::Public => [0x50; KEY_BYTES],
            Partition::Scoped(id) => id.as_bytes().map(|byte| byte ^ 0x5A),
        };
        bytes[0] ^= self.generation.get(&partition).copied().unwrap_or(0);
        Ok(DataKey::from_bytes(&mut bytes))
    }

    fn delete_key(&mut self, partition: Partition) -> Result<(), StoreError> {
        let generation = self.generation.entry(partition).or_insert(0);
        *generation = generation.wrapping_add(1);
        Ok(())
    }

    fn random(&mut self, buf: &mut [u8]) -> Result<(), StoreError> {
        self.random.random(buf)
    }
}

// ---------------------------------------------------------------- transactions

/// The transaction kinds of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    ReplaceExisting,
    ReplaceNew,
    UpdateFields,
    Remove,
    DisableKey,
    RevokeScoped,
    RevokePublic,
    ClearSite,
    ClearAll,
    Refused,
}

const KINDS: [Kind; 10] = [
    Kind::ReplaceExisting,
    Kind::ReplaceNew,
    Kind::UpdateFields,
    Kind::Remove,
    Kind::DisableKey,
    Kind::RevokeScoped,
    Kind::RevokePublic,
    Kind::ClearSite,
    Kind::ClearAll,
    Kind::Refused,
];

fn kind_of(name: &str) -> Kind {
    KINDS
        .into_iter()
        .find(|kind| format!("{kind:?}") == name)
        .expect("a kind")
}

fn step_of(name: &str) -> CommitStep {
    CommitStep::ALL
        .into_iter()
        .find(|step| format!("{step:?}") == name)
        .expect("a step")
}

fn entry(partition: usize, key: usize, tag: &str, variant: u8) -> (EntryMeta, SnapshotBody) {
    sample_entry(PARTITIONS[partition], KEYS[key], HOSTS[key], tag, variant)
}

/// The preconditions of a session that starts now on `partition`; the
/// request partition is the first scoped partition.
fn fresh<D: StorageDriver>(store: &mut D, partition: Partition) -> Preconditions {
    let Partition::Scoped(request) = PARTITIONS[0] else {
        panic!("the first partition is scoped");
    };
    Preconditions {
        entry: Expect::Any,
        writer: Writer::request(
            request,
            store.partition_epoch(Partition::Scoped(request)).unwrap(),
        ),
        partition_epoch: store.partition_epoch(partition).unwrap(),
        site_epoch: store.site_epoch().unwrap(),
    }
}

fn replace(meta: EntryMeta, body: SnapshotBody) -> TxnOp {
    TxnOp::Replace {
        meta: Box::new(meta),
        body: Box::new(body),
    }
}

/// Entries in every partition, the same key in two of them, and a disabled
/// key (as `seed_script` of the suite).
fn seed<D: StorageDriver>(store: &mut D) {
    let puts = [
        (0, 0, 7),
        (1, 0, 1),
        (2, 1, 4),
        (0, 1, 0),
        (1, 2, 0),
        (0, 3, 2),
    ];
    for (partition, key, variant) in puts {
        let (meta, body) = entry(
            partition,
            key,
            &format!("p{partition}/k{key}/seed"),
            variant,
        );
        let pre = fresh(store, PARTITIONS[partition]);
        store
            .apply(CacheTransaction {
                op: replace(meta, body),
                pre,
            })
            .unwrap();
    }
    let pre = fresh(store, PARTITIONS[1]);
    store
        .apply(CacheTransaction {
            op: TxnOp::DisableKey {
                partition: PARTITIONS[1],
                key: KEYS[3],
                until: NOW.plus(41),
            },
            pre,
        })
        .unwrap();
}

/// The transaction of `kind` on the seeded store.
fn transaction<D: StorageDriver>(kind: Kind, store: &mut D) -> CacheTransaction {
    let (target, op) = match kind {
        Kind::ReplaceExisting => {
            let (meta, body) = entry(0, 0, "p0/k0/new", 6);
            (0, replace(meta, body))
        }
        Kind::ReplaceNew => {
            let (meta, body) = entry(0, 2, "p0/k2/new", 5);
            (0, replace(meta, body))
        }
        Kind::UpdateFields => (
            0,
            TxnOp::UpdateFields {
                partition: PARTITIONS[0],
                key: KEYS[0],
                fields: vec![FieldLine::new("x-step", "kill")],
                policy: sample_policy(1_234),
            },
        ),
        Kind::Remove => (
            0,
            TxnOp::Remove {
                partition: PARTITIONS[0],
                key: KEYS[0],
            },
        ),
        Kind::DisableKey => (
            0,
            TxnOp::DisableKey {
                partition: PARTITIONS[0],
                key: KEYS[1],
                until: NOW.plus(30),
            },
        ),
        Kind::RevokeScoped => (
            0,
            TxnOp::RevokePartition {
                partition: PARTITIONS[0],
            },
        ),
        Kind::RevokePublic => (
            2,
            TxnOp::RevokePartition {
                partition: PARTITIONS[2],
            },
        ),
        Kind::ClearSite => (
            0,
            TxnOp::ClearSite {
                domain: Some("example.com".to_owned()),
            },
        ),
        Kind::ClearAll => (0, TxnOp::ClearSite { domain: None }),
        Kind::Refused => {
            let (meta, body) = entry(0, 0, "p0/k0/late", 3);
            (0, replace(meta, body))
        }
    };
    let mut pre = fresh(store, PARTITIONS[target]);
    if kind == Kind::Refused {
        pre.partition_epoch = Epoch(pre.partition_epoch.0 + 1);
    }
    CacheTransaction { op, pre }
}

// ---------------------------------------------------------------- states

/// What a store answers: every cell (entry without its version, disabled),
/// every partition epoch and the site epoch.
#[derive(Debug, PartialEq, Eq)]
struct State {
    cells: Vec<(Option<(EntryMeta, SnapshotBody)>, bool)>,
    epochs: Vec<Epoch>,
    site: Epoch,
}

fn state<D: StorageDriver>(store: &mut D) -> State {
    let mut cells = Vec::new();
    for partition in PARTITIONS {
        for key in &KEYS {
            let read = store
                .read(partition, key, NOW)
                .unwrap()
                .map(|entry| (entry.meta, entry.body));
            cells.push((read, store.key_disabled(partition, key, NOW).unwrap()));
        }
    }
    State {
        cells,
        epochs: PARTITIONS
            .iter()
            .map(|partition| store.partition_epoch(*partition).unwrap())
            .collect(),
        site: store.site_epoch().unwrap(),
    }
}

fn fake_state(kind: Kind, apply: bool) -> State {
    let mut fake = FakeStore::new();
    fake.recover(NOW).unwrap();
    seed(&mut fake);
    if apply {
        let txn = transaction(kind, &mut fake);
        fake.apply(txn).unwrap();
    }
    state(&mut fake)
}

// ---------------------------------------------------------------- the child

/// The child process of the kill tests: runs only under `CHILD_ENV`.
#[test]
#[ignore = "the child process of the kill tests; the parent starts it"]
fn driver_kill_child_process() {
    let Ok(spec) = std::env::var(CHILD_ENV) else {
        return;
    };
    let mut parts = spec.split('\t');
    let root = PathBuf::from(parts.next().unwrap());
    let kind = kind_of(parts.next().unwrap());
    let step = step_of(parts.next().unwrap());
    let mut store = FileStore::open(&root, FixedKeys::new()).unwrap();
    store.recover(NOW).unwrap();
    let txn = transaction(kind, &mut store);
    store.stop_after(step);
    let stopped = store.apply(txn);
    assert_eq!(
        stopped.map_err(|error| error.kind()),
        Err(StoreErrorKind::Unavailable)
    );
    let mut out = std::io::stdout().lock();
    writeln!(out, "{READY}").unwrap();
    out.flush().unwrap();
    drop(out);
    // Wait for SIGKILL with the store open. If the parent closes stdin
    // instead, leave without running a destructor.
    let mut line = String::new();
    let _read = std::io::stdin().read_line(&mut line);
    std::process::exit(3);
}

/// Starts the child for `kind` at `step` on `root` and kills it with
/// `SIGKILL` once it stopped after the step.
fn run_and_kill_child(root: &Path, kind: Kind, step: CommitStep) {
    let exe = std::env::current_exe().unwrap();
    let spec = format!("{}\t{kind:?}\t{step:?}", root.display());
    let mut child = Command::new(exe)
        .args([
            CHILD_TEST,
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_ENV, spec)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    // `Child::wait` closes the stdin of the child before it waits. The
    // child reads its stdin to block, so an early close lets it leave on
    // its own before the signal ends it. Keep the handle until the child
    // is gone.
    let stdin = child.stdin.take();
    let (ready, wait) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if line.contains(READY) && ready.send(()).is_err() {
                break;
            }
        }
    });
    let reached = wait.recv_timeout(CHILD_TIMEOUT);
    child.kill().unwrap();
    let status = child.wait().unwrap();
    drop(stdin);
    reader.join().unwrap();
    assert!(
        reached.is_ok(),
        "{kind:?} at {step:?}: the child did not reach its step ({status:?})"
    );
    assert_eq!(
        status.signal(),
        Some(9),
        "{kind:?} at {step:?}: the child was not killed by SIGKILL"
    );
}

// ---------------------------------------------------------------- the parent

/// The directory of one case, removed on drop.
struct CaseDir(PathBuf);

impl CaseDir {
    fn new(kind: Kind, step: CommitStep) -> Self {
        let path = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("rivqen-store-driver-kill")
            .join(format!("{kind:?}-{step:?}"));
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
            "cannot remove the test directory"
        );
    }
}

fn count_files(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .map(|entry| {
            let path = entry.unwrap().path();
            if path.is_dir() { count_files(&path) } else { 1 }
        })
        .sum()
}

/// The table row of `kind`: a kill after each step.
fn kill_at_every_step(kind: Kind) {
    let old = fake_state(kind, false);
    let new = fake_state(kind, true);
    let refused = kind == Kind::Refused;
    // A refused transaction changes nothing; every other one changes the
    // state, so old and new differ.
    assert_eq!(old == new, refused, "{kind:?}");
    for step in CommitStep::ALL {
        let dir = CaseDir::new(kind, step);
        let root = dir.0.join("store");
        {
            let mut store = FileStore::open(&root, FixedKeys::new()).unwrap();
            store.recover(NOW).unwrap();
            seed(&mut store);
            assert_eq!(state(&mut store), old, "{kind:?}: the seed");
        }
        run_and_kill_child(&root, kind, step);

        let mut store = FileStore::open(&root, FixedKeys::new()).unwrap();
        store.recover(NOW).unwrap();
        let after_commit_point = matches!(step, CommitStep::CommitMeta | CommitStep::RemoveOld);
        let expected = if after_commit_point && !refused {
            &new
        } else {
            &old
        };
        assert_eq!(&state(&mut store), expected, "{kind:?} at {step:?}");
        assert_eq!(
            store.recover(NOW).unwrap(),
            RecoveryReport::default(),
            "{kind:?} at {step:?}: a second recovery"
        );
        let schema = schema_dir(&root);
        assert_eq!(
            count_files(&schema.join(TMP_DIR)),
            0,
            "{kind:?} at {step:?}"
        );
        assert_eq!(
            count_files(&schema.join(FILES_DIR)),
            store.list().unwrap().len(),
            "{kind:?} at {step:?}: a file that no row names"
        );
    }
}

/// The key of a partition is the same in two processes (the premise of
/// the parent's reads).
#[test]
fn driver_kill_fixed_keys_are_the_same_in_two_providers() {
    let id = PartitionId::from_bytes([7; 32]);
    for partition in [Partition::Public, Partition::Scoped(id)] {
        let one = FixedKeys::new().data_key(partition).unwrap();
        let two = FixedKeys::new().data_key(partition).unwrap();
        let aad = rivqen_store::crypto::Aad::new(
            rivqen_store::crypto::Role::File,
            1,
            partition,
            &CacheKey::from_bytes([1; 32]),
            &rivqen_store::crypto::FileId::from_bytes([2; 16]),
        );
        let mut random = HostKeys::new().unwrap();
        let sealed = rivqen_store::crypto::seal(&one, &mut random, &aad, b"x").unwrap();
        assert_eq!(
            rivqen_store::crypto::open(&two, &aad, &sealed).unwrap(),
            b"x"
        );
    }
}

#[test]
fn driver_kill_replace_existing() {
    kill_at_every_step(Kind::ReplaceExisting);
}

#[test]
fn driver_kill_replace_new() {
    kill_at_every_step(Kind::ReplaceNew);
}

#[test]
fn driver_kill_update_fields() {
    kill_at_every_step(Kind::UpdateFields);
}

#[test]
fn driver_kill_remove() {
    kill_at_every_step(Kind::Remove);
}

#[test]
fn driver_kill_disable_key() {
    kill_at_every_step(Kind::DisableKey);
}

#[test]
fn driver_kill_revoke_scoped_partition() {
    kill_at_every_step(Kind::RevokeScoped);
}

#[test]
fn driver_kill_revoke_public() {
    kill_at_every_step(Kind::RevokePublic);
}

#[test]
fn driver_kill_clear_site() {
    kill_at_every_step(Kind::ClearSite);
}

#[test]
fn driver_kill_clear_all_sites() {
    kill_at_every_step(Kind::ClearAll);
}

#[test]
fn driver_kill_refused_replace() {
    kill_at_every_step(Kind::Refused);
}
