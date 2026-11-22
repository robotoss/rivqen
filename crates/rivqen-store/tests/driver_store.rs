// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! Tests of the reference driver on disk (WP-07 S2 T-05; ADR-004 sections
//! 1 to 4; ADR-011 sections 3 and 4; RSK-06, INV-14, R-CACHE-04,
//! R-CACHE-06): the files of a commit, the checks of a read, recovery, the
//! logout and two partitions. Each test works in its own directory under
//! `CARGO_TARGET_TMPDIR` and removes it at the end.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use rivqen_cache::driver::{RecoveryReport, StorageDriver, StoreError, StoreErrorKind};
use rivqen_cache::key::Partition;
use rivqen_cache::txn::{
    CacheTransaction, CommitStep, EntryMeta, EntryVersion, Expect, Preconditions, Refused,
    SCHEMA_VERSION, SnapshotBody, TxnOp, TxnOutcome,
};
use rivqen_model::http::FieldLine;
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_model::safe::PageBytes;
use rivqen_model::time::UnixTime;
use rivqen_store::crypto::{
    Aad, DataKey, DecodeLimits, FileId, HostKeys, KeyProvider, MAGIC, Role, SEAL_OVERHEAD,
    encode_row, open,
};
use rivqen_store::driver::{FILE_EXT, FILES_DIR, FileStore, TMP_DIR, entry_file_path, schema_dir};
use rivqen_store::meta::{DB_FILE, MAX_SEALED_BYTES, MetaDb};
use rivqen_testkit::store::contract::sample_entry;

// ---------------------------------------------------------------- helpers

const NOW: UnixTime = UnixTime(1_800_000_000);
const A: PartitionId = PartitionId::from_bytes([0xA1; 32]);
const B: PartitionId = PartitionId::from_bytes([0xB2; 32]);
const K1: CacheKey = CacheKey::from_bytes([0x11; 32]);
const K2: CacheKey = CacheKey::from_bytes([0x22; 32]);
const K3: CacheKey = CacheKey::from_bytes([0x33; 32]);
const HOST: &str = "shop.example.com";
const NO_FILES: Vec<PathBuf> = Vec::new();

/// The directory of one test, removed on drop.
struct TestDir(PathBuf);

impl TestDir {
    fn new(name: &str) -> Self {
        let path = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("rivqen-store-driver-store")
            .join(name);
        if path.exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    /// The root of the store of the test.
    fn root(&self) -> PathBuf {
        self.0.join("store")
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

/// A recovered store with the host key provider.
fn store(dir: &TestDir) -> FileStore<HostKeys> {
    let mut store = FileStore::open(&dir.root(), HostKeys::new().unwrap()).unwrap();
    assert_eq!(store.recover(NOW).unwrap(), RecoveryReport::default());
    store
}

/// The preconditions of a session that starts now on `partition`, with the
/// request partition `A`.
fn fresh<D: StorageDriver>(store: &mut D, partition: Partition, entry: Expect) -> Preconditions {
    Preconditions {
        entry,
        request: Some((A, store.partition_epoch(Partition::Scoped(A)).unwrap())),
        partition_epoch: store.partition_epoch(partition).unwrap(),
        site_epoch: store.site_epoch().unwrap(),
    }
}

fn replace_txn(meta: EntryMeta, body: SnapshotBody, pre: Preconditions) -> CacheTransaction {
    CacheTransaction {
        op: TxnOp::Replace {
            meta: Box::new(meta),
            body: Box::new(body),
        },
        pre,
    }
}

/// Stores a sample entry and gives its version.
fn put<D: StorageDriver>(
    store: &mut D,
    partition: Partition,
    key: CacheKey,
    tag: &str,
) -> EntryVersion {
    let (meta, body) = sample_entry(partition, key, HOST, tag, 0);
    let pre = fresh(store, partition, Expect::Any);
    match store.apply(replace_txn(meta, body, pre)).unwrap() {
        TxnOutcome::Applied {
            version: Some(version),
        } => version,
        other => panic!("replace gave {other:?}"),
    }
}

fn apply<D: StorageDriver>(store: &mut D, op: TxnOp, partition: Partition) -> TxnOutcome {
    let pre = fresh(store, partition, Expect::Any);
    store.apply(CacheTransaction { op, pre }).unwrap()
}

fn page_of<D: StorageDriver>(
    store: &mut D,
    partition: Partition,
    key: CacheKey,
) -> Option<Vec<u8>> {
    store
        .read(partition, &key, NOW)
        .unwrap()
        .map(|entry| entry.body.page.into_vec())
}

fn kind<T: std::fmt::Debug>(result: Result<T, StoreError>) -> StoreErrorKind {
    result.expect_err("an error").kind()
}

/// Every regular file under `dir`, recursively, sorted.
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries {
        let path = entry.unwrap().path();
        let meta = fs::symlink_metadata(&path).unwrap();
        if meta.is_dir() {
            out.extend(files_under(&path));
        } else {
            out.push(path);
        }
    }
    out.sort();
    out
}

fn entry_files(dir: &TestDir) -> Vec<PathBuf> {
    files_under(&schema_dir(&dir.root()).join(FILES_DIR))
}

fn temp_files(dir: &TestDir) -> Vec<PathBuf> {
    files_under(&schema_dir(&dir.root()).join(TMP_DIR))
}

/// The entry file of the row of `key` in `partition`, through a second
/// connection.
fn file_of(dir: &TestDir, partition: Partition, key: CacheKey) -> PathBuf {
    let mut db = MetaDb::open(&schema_dir(&dir.root())).unwrap();
    let row = db
        .transaction(|txn| txn.row(partition, &key))
        .unwrap()
        .expect("a row");
    entry_file_path(&dir.root(), &FileId::from_bytes(row.file_id))
}

/// A raw connection to the metadata, for damage that the driver API cannot
/// make.
fn raw(dir: &TestDir) -> rusqlite::Connection {
    rusqlite::Connection::open(schema_dir(&dir.root()).join(DB_FILE)).unwrap()
}

/// The partition column of ADR-004 section 3.
fn partition_column(partition: Partition) -> Vec<u8> {
    match partition {
        Partition::Public => vec![0x00],
        Partition::Scoped(id) => [&[0x01][..], id.as_bytes()].concat(),
    }
}

/// A key port that records its calls and can fail `delete_key`.
#[derive(Clone)]
struct Recording {
    inner: Rc<RefCell<HostKeys>>,
    deleted: Rc<RefCell<Vec<Partition>>>,
    fail_delete: Rc<RefCell<bool>>,
}

impl Recording {
    fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(HostKeys::new().unwrap())),
            deleted: Rc::new(RefCell::new(Vec::new())),
            fail_delete: Rc::new(RefCell::new(false)),
        }
    }
}

impl KeyProvider for Recording {
    fn data_key(&mut self, partition: Partition) -> Result<DataKey, StoreError> {
        self.inner.borrow_mut().data_key(partition)
    }

    fn delete_key(&mut self, partition: Partition) -> Result<(), StoreError> {
        if *self.fail_delete.borrow() {
            return Err(StoreError::new(StoreErrorKind::Unavailable, "test"));
        }
        self.deleted.borrow_mut().push(partition);
        self.inner.borrow_mut().delete_key(partition)
    }

    fn random(&mut self, buf: &mut [u8]) -> Result<(), StoreError> {
        self.inner.borrow_mut().random(buf)
    }
}

// ---------------------------------------------------------------- commit

#[test]
fn driver_commit_replace_writes_one_sealed_file() {
    let dir = TestDir::new("commit_replace_writes_one_sealed_file");
    let mut store = store(&dir);
    put(&mut store, Partition::Scoped(A), K1, "p0/secret-page");

    let files = entry_files(&dir);
    assert_eq!(files.len(), 1);
    assert_eq!(temp_files(&dir), NO_FILES);
    let file = &files[0];
    let stem = file.file_stem().unwrap().to_str().unwrap();
    let id = FileId::from_hex(stem).expect("the name is a file id");
    assert_eq!(file.extension().unwrap(), FILE_EXT);
    assert_eq!(*file, entry_file_path(&dir.root(), &id));
    let bucket = file
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(bucket, &stem[..2]);
    let bytes = fs::read(file).unwrap();
    assert_eq!(bytes[..4], MAGIC);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    let clear = b"secret-page";
    assert!(!bytes.windows(clear.len()).any(|window| window == clear));
    assert_eq!(
        page_of(&mut store, Partition::Scoped(A), K1).unwrap(),
        b"p0/secret-page/page"
    );
}

#[test]
fn driver_commit_second_replace_removes_the_old_file() {
    let dir = TestDir::new("commit_second_replace_removes_the_old_file");
    let mut store = store(&dir);
    let first = put(&mut store, Partition::Scoped(A), K1, "p0/one");
    let before = entry_files(&dir);
    let second = put(&mut store, Partition::Scoped(A), K1, "p0/two");
    let after = entry_files(&dir);
    assert_ne!(first, second);
    assert_eq!(after.len(), 1);
    assert_ne!(before, after);
    assert_eq!(
        page_of(&mut store, Partition::Scoped(A), K1).unwrap(),
        b"p0/two/page"
    );
}

#[test]
fn driver_commit_refused_replace_removes_its_promoted_file() {
    let dir = TestDir::new("commit_refused_replace_removes_its_promoted_file");
    let mut store = store(&dir);
    put(&mut store, Partition::Scoped(A), K1, "p0/old");
    let kept = entry_files(&dir);
    for stale in [
        Refused::PartitionRevoked,
        Refused::SiteCleared,
        Refused::Entry,
    ] {
        let mut pre = fresh(&mut store, Partition::Scoped(A), Expect::Any);
        match stale {
            Refused::PartitionRevoked => pre.partition_epoch.0 += 1,
            Refused::SiteCleared => pre.site_epoch.0 += 1,
            _ => pre.entry = Expect::Absent,
        }
        let (meta, body) = sample_entry(Partition::Scoped(A), K1, HOST, "p0/new", 7);
        let outcome = store.apply(replace_txn(meta, body, pre)).unwrap();
        assert_eq!(outcome, TxnOutcome::Refused(stale));
        assert_eq!(entry_files(&dir), kept);
        assert_eq!(temp_files(&dir), NO_FILES);
    }
    assert_eq!(
        page_of(&mut store, Partition::Scoped(A), K1).unwrap(),
        b"p0/old/page"
    );
}

#[test]
fn driver_commit_invalid_host_writes_no_file() {
    let dir = TestDir::new("commit_invalid_host_writes_no_file");
    let mut store = store(&dir);
    let (meta, body) = sample_entry(Partition::Public, K1, "Shop.example.com", "p2/x", 3);
    let pre = fresh(&mut store, Partition::Public, Expect::Any);
    let outcome = store.apply(replace_txn(meta, body, pre)).unwrap();
    assert_eq!(outcome, TxnOutcome::Refused(Refused::InvalidHost));
    assert_eq!(entry_files(&dir), NO_FILES);
    assert_eq!(temp_files(&dir), NO_FILES);
    assert_eq!(store.list().unwrap().len(), 0);
}

/// Field lines whose row plain text is exactly `plain_len` bytes for the
/// sample entry of `key`.
fn fields_of_row_len(key: CacheKey, plain_len: usize) -> (EntryMeta, SnapshotBody) {
    let (meta, mut body) = sample_entry(Partition::Scoped(A), key, HOST, "p0/big", 0);
    body.fields = vec![FieldLine::new("x-big", String::new())];
    let base = encode_row(&meta, &body.fields).len();
    body.fields[0].value = "a".repeat(plain_len - base);
    assert_eq!(encode_row(&meta, &body.fields).len(), plain_len);
    (meta, body)
}

#[test]
fn driver_commit_row_over_the_limit_writes_nothing() {
    let dir = TestDir::new("commit_row_over_the_limit_writes_nothing");
    // Decode limits that let one field line fill the row, so a row at the
    // limit is read back (the default limits refuse a field over 16 KiB).
    let limits = DecodeLimits {
        max_header_bytes: 2 << 20,
        max_headers_total_bytes: 2 << 20,
        ..DecodeLimits::default()
    };
    let mut store =
        FileStore::open_with_limits(&dir.root(), HostKeys::new().unwrap(), limits).unwrap();
    store.recover(NOW).unwrap();
    let limit = MAX_SEALED_BYTES - SEAL_OVERHEAD;

    // One byte over the limit: nothing is written.
    let (meta, body) = fields_of_row_len(K1, limit + 1);
    let pre = fresh(&mut store, Partition::Scoped(A), Expect::Any);
    assert_eq!(
        kind(store.apply(replace_txn(meta, body, pre))),
        StoreErrorKind::Unavailable
    );
    assert_eq!(entry_files(&dir), NO_FILES);
    assert_eq!(temp_files(&dir), NO_FILES);
    assert_eq!(store.list().unwrap().len(), 0);

    // Exactly at the limit: stored and read back.
    let (meta, body) = fields_of_row_len(K2, limit);
    let pre = fresh(&mut store, Partition::Scoped(A), Expect::Any);
    let stored = store.apply(replace_txn(meta, body.clone(), pre)).unwrap();
    assert!(matches!(stored, TxnOutcome::Applied { version: Some(_) }));
    let read = store.read(Partition::Scoped(A), &K2, NOW).unwrap().unwrap();
    assert_eq!(read.body, body);

    // An update whose row would be over the limit writes nothing.
    let version = put(&mut store, Partition::Scoped(A), K3, "p0/small");
    let (_, big) = fields_of_row_len(K3, limit + 1);
    let pre = fresh(&mut store, Partition::Scoped(A), Expect::Version(version));
    let update = CacheTransaction {
        op: TxnOp::UpdateFields {
            partition: Partition::Scoped(A),
            key: K3,
            fields: big.fields,
            policy: rivqen_testkit::store::contract::sample_policy(5),
        },
        pre,
    };
    assert_eq!(kind(store.apply(update)), StoreErrorKind::Unavailable);
    let kept = store.read(Partition::Scoped(A), &K3, NOW).unwrap().unwrap();
    assert_eq!(kept.version, version);
    assert_eq!(kept.body.page.as_slice(), b"p0/small/page");
}

#[test]
fn driver_commit_failed_step_leaves_no_new_file() {
    let dir = TestDir::new("commit_failed_step_leaves_no_new_file");
    let mut store = store(&dir);
    put(&mut store, Partition::Scoped(A), K1, "p0/old");
    let kept = entry_files(&dir);
    for step in [
        CommitStep::WriteTemp,
        CommitStep::SyncTemp,
        CommitStep::Promote,
        CommitStep::CommitMeta,
    ] {
        store.fail_at(step);
        let (meta, body) = sample_entry(Partition::Scoped(A), K1, HOST, "p0/new", 5);
        let pre = fresh(&mut store, Partition::Scoped(A), Expect::Any);
        assert_eq!(
            kind(store.apply(replace_txn(meta, body, pre))),
            StoreErrorKind::Unavailable,
            "{step:?}"
        );
        assert_eq!(entry_files(&dir), kept, "{step:?}");
        assert_eq!(temp_files(&dir), NO_FILES, "{step:?}");
        assert_eq!(
            page_of(&mut store, Partition::Scoped(A), K1).unwrap(),
            b"p0/old/page"
        );
    }
    // At `RemoveOld` the commit stands and the old file stays for recovery.
    store.fail_at(CommitStep::RemoveOld);
    put(&mut store, Partition::Scoped(A), K1, "p0/new");
    assert_eq!(entry_files(&dir).len(), 2);
    store.restart().unwrap();
    let report = store.recover(NOW).unwrap();
    assert_eq!(report.orphans_removed, 1);
    assert_eq!(entry_files(&dir).len(), 1);
}

#[test]
fn driver_commit_key_port_failure_writes_nothing() {
    let dir = TestDir::new("commit_key_port_failure_writes_nothing");
    let mut store = store(&dir);
    put(&mut store, Partition::Scoped(A), K1, "p0/old");
    let kept = entry_files(&dir);
    let pre = fresh(&mut store, Partition::Scoped(A), Expect::Any);
    store.keys_mut().set_locked(true);
    let (meta, body) = sample_entry(Partition::Scoped(A), K1, HOST, "p0/new", 1);
    assert_eq!(
        kind(store.apply(replace_txn(meta, body, pre))),
        StoreErrorKind::Unavailable
    );
    store.keys_mut().set_locked(false);
    assert_eq!(entry_files(&dir), kept);
    assert_eq!(temp_files(&dir), NO_FILES);
    assert_eq!(
        page_of(&mut store, Partition::Scoped(A), K1).unwrap(),
        b"p0/old/page"
    );
}

#[test]
fn driver_commit_revoke_deletes_the_key_first() {
    let dir = TestDir::new("commit_revoke_deletes_the_key_first");
    let keys = Recording::new();
    let mut store = FileStore::open(&dir.root(), keys.clone()).unwrap();
    store.recover(NOW).unwrap();
    put(&mut store, Partition::Scoped(A), K1, "p0/a");
    put(&mut store, Partition::Scoped(B), K1, "p1/b");

    // A key port that cannot delete the key stops the revoke before the
    // commit: nothing changes.
    *keys.fail_delete.borrow_mut() = true;
    let revoke = TxnOp::RevokePartition {
        partition: Partition::Scoped(A),
    };
    let pre = fresh(&mut store, Partition::Scoped(A), Expect::Any);
    let epoch = pre.partition_epoch;
    assert_eq!(
        kind(store.apply(CacheTransaction {
            op: revoke.clone(),
            pre,
        })),
        StoreErrorKind::Unavailable
    );
    assert_eq!(store.partition_epoch(Partition::Scoped(A)).unwrap(), epoch);
    assert!(page_of(&mut store, Partition::Scoped(A), K1).is_some());

    *keys.fail_delete.borrow_mut() = false;
    let outcome = apply(&mut store, revoke, Partition::Scoped(A));
    assert_eq!(outcome, TxnOutcome::Applied { version: None });
    assert_eq!(*keys.deleted.borrow(), vec![Partition::Scoped(A)]);
    assert!(page_of(&mut store, Partition::Scoped(A), K1).is_none());
    assert!(page_of(&mut store, Partition::Scoped(B), K1).is_some());
    assert_eq!(entry_files(&dir).len(), 1);
}

#[test]
fn driver_commit_clear_of_all_sites_deletes_the_public_key() {
    let dir = TestDir::new("commit_clear_of_all_sites_deletes_the_public_key");
    let keys = Recording::new();
    let mut store = FileStore::open(&dir.root(), keys.clone()).unwrap();
    store.recover(NOW).unwrap();
    put(&mut store, Partition::Public, K1, "p2/a");
    apply(
        &mut store,
        TxnOp::ClearSite {
            domain: Some("example.com".to_owned()),
        },
        Partition::Public,
    );
    assert_eq!(keys.deleted.borrow().len(), 0);
    apply(
        &mut store,
        TxnOp::ClearSite { domain: None },
        Partition::Public,
    );
    assert_eq!(*keys.deleted.borrow(), vec![Partition::Public]);
    assert_eq!(entry_files(&dir), NO_FILES);
}

#[test]
fn driver_commit_never_replaces_an_existing_file() {
    let dir = TestDir::new("commit_never_replaces_an_existing_file");
    // A random source that gives the same bytes every time: the second
    // entry gets the file id of the first.
    let keys = HostKeys::with_random_source(Box::new(std::io::repeat(0x3c)));
    let mut store = FileStore::open(&dir.root(), keys).unwrap();
    store.recover(NOW).unwrap();
    put(&mut store, Partition::Public, K1, "p2/a");
    let first = entry_files(&dir);
    let (meta, body) = sample_entry(Partition::Public, K2, HOST, "p2/b", 0);
    let pre = fresh(&mut store, Partition::Public, Expect::Any);
    assert_eq!(
        kind(store.apply(replace_txn(meta, body, pre))),
        StoreErrorKind::Unavailable
    );
    assert_eq!(entry_files(&dir), first);
    assert_eq!(temp_files(&dir), NO_FILES);
    assert_eq!(
        page_of(&mut store, Partition::Public, K1).unwrap(),
        b"p2/a/page"
    );
}

#[test]
fn driver_commit_versions_differ_after_a_restart() {
    let dir = TestDir::new("commit_versions_differ_after_a_restart");
    let mut store = store(&dir);
    let first = put(&mut store, Partition::Scoped(A), K1, "p0/a");
    store.restart().unwrap();
    store.recover(NOW).unwrap();
    let second = put(&mut store, Partition::Scoped(A), K1, "p0/b");
    assert_ne!(first, second);
}

/// Makes the row of `key` in `partition` undecodable (a negative version).
fn break_row(dir: &TestDir, partition: Partition, key: CacheKey) {
    let changed = raw(dir)
        .execute(
            r#"UPDATE entry SET version = -1 WHERE "partition" = ?1 AND "key" = ?2"#,
            rusqlite::params![partition_column(partition), key.as_bytes().as_slice()],
        )
        .unwrap();
    assert_eq!(changed, 1);
}

#[test]
fn driver_commit_over_a_row_that_does_not_decode() {
    let dir = TestDir::new("commit_over_a_row_that_does_not_decode");
    let mut store = store(&dir);
    let scoped = Partition::Scoped(A);
    put(&mut store, scoped, K1, "p0/a");
    break_row(&dir, scoped, K1);
    // An entry with no known version: `Absent`, a version and an update
    // are refused.
    for (entry, update) in [
        (Expect::Absent, false),
        (Expect::Version(EntryVersion(1)), false),
        (Expect::Any, true),
    ] {
        let pre = fresh(&mut store, scoped, entry);
        let op = if update {
            TxnOp::UpdateFields {
                partition: scoped,
                key: K1,
                fields: Vec::new(),
                policy: rivqen_testkit::store::contract::sample_policy(1),
            }
        } else {
            TxnOp::Remove {
                partition: scoped,
                key: K1,
            }
        };
        assert_eq!(
            store.apply(CacheTransaction { op, pre }).unwrap(),
            TxnOutcome::Refused(Refused::Entry)
        );
    }
    // `Any` replaces it.
    put(&mut store, scoped, K1, "p0/b");
    assert_eq!(page_of(&mut store, scoped, K1).unwrap(), b"p0/b/page");
    // `Any` removes and disables it.
    break_row(&dir, scoped, K1);
    let removed = apply(
        &mut store,
        TxnOp::Remove {
            partition: scoped,
            key: K1,
        },
        scoped,
    );
    assert_eq!(removed, TxnOutcome::Applied { version: None });
    assert!(page_of(&mut store, scoped, K1).is_none());
    put(&mut store, scoped, K1, "p0/c");
    break_row(&dir, scoped, K1);
    let disabled = apply(
        &mut store,
        TxnOp::DisableKey {
            partition: scoped,
            key: K1,
            until: NOW.plus(5),
        },
        scoped,
    );
    assert_eq!(disabled, TxnOutcome::Applied { version: None });
    assert!(store.key_disabled(scoped, &K1, NOW).unwrap());
    assert!(store.read(scoped, &K1, NOW.plus(5)).unwrap().is_none());
}

#[test]
fn driver_commit_removals_remove_rows_that_do_not_decode() {
    let dir = TestDir::new("commit_removals_remove_rows_that_do_not_decode");
    let mut store = store(&dir);
    put(&mut store, Partition::Scoped(A), K1, "p0/a");
    put(&mut store, Partition::Scoped(A), K2, "p0/b");
    put(&mut store, Partition::Public, K1, "p2/a");
    put(&mut store, Partition::Public, K2, "p2/b");
    break_row(&dir, Partition::Scoped(A), K1);
    let revoked = apply(
        &mut store,
        TxnOp::RevokePartition {
            partition: Partition::Scoped(A),
        },
        Partition::Scoped(A),
    );
    assert_eq!(revoked, TxnOutcome::Applied { version: None });
    assert!(page_of(&mut store, Partition::Scoped(A), K2).is_none());
    // A site clear removes a row whose host cannot be read.
    break_row(&dir, Partition::Public, K1);
    let cleared = apply(
        &mut store,
        TxnOp::ClearSite {
            domain: Some("other.org".to_owned()),
        },
        Partition::Public,
    );
    assert_eq!(cleared, TxnOutcome::Applied { version: None });
    assert_eq!(
        store
            .list()
            .unwrap()
            .into_iter()
            .map(|summary| (summary.meta.partition, summary.meta.key))
            .collect::<Vec<_>>(),
        vec![(Partition::Public, K2)]
    );
    store.restart().unwrap();
    // The files of the removed rows were orphans; nothing else is left.
    let report = store.recover(NOW).unwrap();
    assert_eq!(report.entries_removed, 0);
    assert_eq!(entry_files(&dir).len(), 1);
}

// ---------------------------------------------------------------- read

#[test]
fn driver_read_list_removes_a_row_that_does_not_open() {
    let dir = TestDir::new("read_list_removes_a_row_that_does_not_open");
    let mut store = store(&dir);
    put(&mut store, Partition::Scoped(A), K1, "p0/a");
    put(&mut store, Partition::Scoped(B), K1, "p1/a");
    let file = file_of(&dir, Partition::Scoped(A), K1);
    // The key of A is gone: its row does not open any more.
    store.keys_mut().delete_key(Partition::Scoped(A)).unwrap();
    let listed = store.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].meta.partition, Partition::Scoped(B));
    assert!(!file.exists());
    assert!(page_of(&mut store, Partition::Scoped(A), K1).is_none());
}

#[test]
fn driver_read_entry_at_the_block_limit() {
    let dir = TestDir::new("read_entry_at_the_block_limit");
    let limits = DecodeLimits {
        max_blocks: 3,
        ..DecodeLimits::default()
    };
    let mut store =
        FileStore::open_with_limits(&dir.root(), HostKeys::new().unwrap(), limits).unwrap();
    store.recover(NOW).unwrap();
    // Variant 7: a template and 3 data blocks, the most that the limits
    // allow.
    let (meta, body) = sample_entry(Partition::Public, K1, HOST, "p2/max", 7);
    assert_eq!(body.data.len(), 3);
    let pre = fresh(&mut store, Partition::Public, Expect::Any);
    store.apply(replace_txn(meta, body.clone(), pre)).unwrap();
    assert_eq!(
        store
            .read(Partition::Public, &K1, NOW)
            .unwrap()
            .unwrap()
            .body,
        body
    );
    store.restart().unwrap();
    assert_eq!(store.recover(NOW).unwrap(), RecoveryReport::default());
    // One block more: stored, but no file of that length fits the limits.
    let (meta, mut body) = sample_entry(Partition::Public, K2, HOST, "p2/over", 7);
    body.data
        .push(("{b3}".to_owned(), PageBytes::new(b"x".to_vec())));
    let pre = fresh(&mut store, Partition::Public, Expect::Any);
    store.apply(replace_txn(meta, body, pre)).unwrap();
    assert_eq!(
        kind(store.read(Partition::Public, &K2, NOW)),
        StoreErrorKind::Corrupted
    );
}

#[test]
fn driver_read_tampered_row_is_corrupted_and_removed() {
    let dir = TestDir::new("read_tampered_row_is_corrupted_and_removed");
    let mut store = store(&dir);
    put(&mut store, Partition::Scoped(A), K1, "p0/a");
    put(&mut store, Partition::Scoped(A), K2, "p0/b");
    let file = file_of(&dir, Partition::Scoped(A), K1);
    let conn = raw(&dir);
    let mut sealed: Vec<u8> = conn
        .query_row(
            r#"SELECT sealed FROM entry WHERE "key" = ?1"#,
            [K1.as_bytes().as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    let last = sealed.len() - 1;
    sealed[last] ^= 0x80;
    conn.execute(
        r#"UPDATE entry SET sealed = ?1 WHERE "key" = ?2"#,
        rusqlite::params![sealed, K1.as_bytes().as_slice()],
    )
    .unwrap();
    drop(conn);

    assert_eq!(
        kind(store.read(Partition::Scoped(A), &K1, NOW)),
        StoreErrorKind::Corrupted
    );
    assert!(
        store
            .read(Partition::Scoped(A), &K1, NOW)
            .unwrap()
            .is_none()
    );
    assert!(!file.exists());
    assert!(page_of(&mut store, Partition::Scoped(A), K2).is_some());
    assert_eq!(store.list().unwrap().len(), 1);
}

#[test]
fn driver_read_row_of_another_key_is_corrupted() {
    let dir = TestDir::new("read_row_of_another_key_is_corrupted");
    let mut store = store(&dir);
    put(&mut store, Partition::Scoped(A), K1, "p0/a");
    put(&mut store, Partition::Scoped(A), K2, "p0/b");
    let conn = raw(&dir);
    conn.execute(
        r#"UPDATE entry SET sealed = (SELECT sealed FROM entry WHERE "key" = ?1) WHERE "key" = ?2"#,
        [K1.as_bytes().as_slice(), K2.as_bytes().as_slice()],
    )
    .unwrap();
    drop(conn);
    assert_eq!(
        kind(store.read(Partition::Scoped(A), &K2, NOW)),
        StoreErrorKind::Corrupted
    );
    assert!(page_of(&mut store, Partition::Scoped(A), K1).is_some());
}

#[test]
fn driver_read_row_that_does_not_decode_is_corrupted_and_removed() {
    let dir = TestDir::new("read_row_that_does_not_decode");
    let mut store = store(&dir);
    put(&mut store, Partition::Scoped(A), K1, "p0/a");
    put(&mut store, Partition::Scoped(A), K2, "p0/b");
    raw(&dir)
        .execute(
            r#"UPDATE entry SET version = -1 WHERE "key" = ?1"#,
            [K1.as_bytes().as_slice()],
        )
        .unwrap();
    assert_eq!(
        kind(store.read(Partition::Scoped(A), &K1, NOW)),
        StoreErrorKind::Corrupted
    );
    assert!(
        store
            .read(Partition::Scoped(A), &K1, NOW)
            .unwrap()
            .is_none()
    );
    assert!(page_of(&mut store, Partition::Scoped(A), K2).is_some());
}

#[test]
fn driver_read_file_of_another_length_is_corrupted_and_removed() {
    let dir = TestDir::new("read_file_of_another_length");
    let mut store = store(&dir);
    put(&mut store, Partition::Scoped(A), K1, "p0/a");
    put(&mut store, Partition::Scoped(A), K2, "p0/b");
    let file = file_of(&dir, Partition::Scoped(A), K1);
    let mut bytes = fs::read(&file).unwrap();
    bytes.push(0);
    fs::write(&file, &bytes).unwrap();
    assert_eq!(
        kind(store.read(Partition::Scoped(A), &K1, NOW)),
        StoreErrorKind::Corrupted
    );
    assert!(!file.exists());
    assert!(
        store
            .read(Partition::Scoped(A), &K1, NOW)
            .unwrap()
            .is_none()
    );
    assert!(page_of(&mut store, Partition::Scoped(A), K2).is_some());
}

/// Stores an entry with a page of 100 bytes and no blocks.
fn put_page_100(store: &mut FileStore<HostKeys>, key: CacheKey) {
    let (meta, mut body) = sample_entry(Partition::Scoped(A), key, HOST, "p0/x", 0);
    body.page = PageBytes::new(vec![b'x'; 100]);
    let pre = fresh(store, Partition::Scoped(A), Expect::Any);
    store.apply(replace_txn(meta, body, pre)).unwrap();
}

#[test]
fn driver_read_content_size_of_another_value_is_corrupted() {
    let dir = TestDir::new("read_content_size_of_another_value");
    let mut store = store(&dir);
    put_page_100(&mut store, K1);
    // 16 bytes less content: the file length fits one more data block, so
    // only the decoded content size shows the change.
    raw(&dir)
        .execute(
            r#"UPDATE entry SET content_bytes = content_bytes - 16 WHERE "key" = ?1"#,
            [K1.as_bytes().as_slice()],
        )
        .unwrap();
    assert_eq!(
        kind(store.read(Partition::Scoped(A), &K1, NOW)),
        StoreErrorKind::Corrupted
    );
    assert_eq!(store.list().unwrap().len(), 0);
}

#[test]
fn driver_read_content_size_over_the_limits_is_corrupted() {
    let dir = TestDir::new("read_content_size_over_the_limits");
    let mut store = store(&dir);
    put_page_100(&mut store, K1);
    raw(&dir)
        .execute(
            r#"UPDATE entry SET content_bytes = ?1 WHERE "key" = ?2"#,
            rusqlite::params![i64::MAX, K1.as_bytes().as_slice()],
        )
        .unwrap();
    assert_eq!(
        kind(store.read(Partition::Scoped(A), &K1, NOW)),
        StoreErrorKind::Corrupted
    );
}

#[test]
fn driver_read_locked_key_port_is_unavailable_and_removes_nothing() {
    let dir = TestDir::new("read_locked_key_port");
    let mut store = store(&dir);
    put(&mut store, Partition::Scoped(A), K1, "p0/a");
    store.keys_mut().set_locked(true);
    assert_eq!(
        kind(store.read(Partition::Scoped(A), &K1, NOW)),
        StoreErrorKind::Unavailable
    );
    assert_eq!(kind(store.list()), StoreErrorKind::Unavailable);
    store.keys_mut().set_locked(false);
    assert_eq!(
        page_of(&mut store, Partition::Scoped(A), K1).unwrap(),
        b"p0/a/page"
    );
    assert_eq!(entry_files(&dir).len(), 1);
}

#[test]
fn driver_read_disabled_record_that_does_not_decode_hides_the_key() {
    let dir = TestDir::new("read_disabled_record_that_does_not_decode");
    let mut store = store(&dir);
    apply(
        &mut store,
        TxnOp::DisableKey {
            partition: Partition::Scoped(A),
            key: K1,
            until: NOW.plus(1),
        },
        Partition::Scoped(A),
    );
    put(&mut store, Partition::Scoped(A), K1, "p0/a");
    assert!(page_of(&mut store, Partition::Scoped(A), K1).is_none());
    assert!(
        store
            .read(Partition::Scoped(A), &K1, NOW.plus(1))
            .unwrap()
            .is_some()
    );
    raw(&dir)
        .execute("UPDATE disabled_key SET until = -5", [])
        .unwrap();
    assert!(
        store
            .key_disabled(Partition::Scoped(A), &K1, NOW.plus(1))
            .unwrap()
    );
    assert!(
        store
            .read(Partition::Scoped(A), &K1, NOW.plus(1))
            .unwrap()
            .is_none()
    );
}

#[test]
fn driver_read_before_recover_and_after_a_stop_is_unavailable() {
    let dir = TestDir::new("read_before_recover_and_after_a_stop");
    let mut store = FileStore::open(&dir.root(), HostKeys::new().unwrap()).unwrap();
    assert_eq!(
        kind(store.read(Partition::Public, &K1, NOW)),
        StoreErrorKind::Unavailable
    );
    store.recover(NOW).unwrap();
    put(&mut store, Partition::Public, K1, "p2/a");
    store.stop_after(CommitStep::SyncTemp);
    let (meta, body) = sample_entry(Partition::Public, K1, HOST, "p2/b", 0);
    let pre = fresh(&mut store, Partition::Public, Expect::Any);
    assert_eq!(
        kind(store.apply(replace_txn(meta, body, pre))),
        StoreErrorKind::Unavailable
    );
    assert_eq!(
        kind(store.read(Partition::Public, &K1, NOW)),
        StoreErrorKind::Unavailable
    );
    assert_eq!(kind(store.recover(NOW)), StoreErrorKind::Unavailable);
    assert_eq!(kind(store.site_epoch()), StoreErrorKind::Unavailable);
    store.restart().unwrap();
    let report = store.recover(NOW).unwrap();
    assert_eq!(report.orphans_removed, 1);
    assert_eq!(
        page_of(&mut store, Partition::Public, K1).unwrap(),
        b"p2/a/page"
    );
}

// ---------------------------------------------------------------- recover

#[test]
fn driver_recover_removes_temp_files_and_orphans() {
    let dir = TestDir::new("recover_removes_temp_files_and_orphans");
    let mut store = store(&dir);
    put(&mut store, Partition::Scoped(A), K1, "p0/a");
    let kept = file_of(&dir, Partition::Scoped(A), K1);
    let schema = schema_dir(&dir.root());
    let tmp = schema.join(TMP_DIR);
    let files = schema.join(FILES_DIR);
    let bucket = kept.parent().unwrap().to_path_buf();
    let bucket_name = bucket.file_name().unwrap().to_str().unwrap().to_owned();
    fs::write(tmp.join("0123.tmp"), b"x").unwrap();
    fs::create_dir(tmp.join("sub")).unwrap();
    fs::write(tmp.join("sub").join("y"), b"y").unwrap();
    fs::create_dir(files.join("zz")).unwrap();
    // An id that no row names, in its own bucket.
    let unnamed = FileId::from_bytes([0x5c; 16]);
    let unnamed_path = entry_file_path(&dir.root(), &unnamed);
    fs::create_dir_all(unnamed_path.parent().unwrap()).unwrap();
    fs::write(&unnamed_path, b"z").unwrap();
    // A name that is not a file id, and an id in another bucket.
    fs::write(bucket.join("garbage"), b"g").unwrap();
    let foreign = if bucket_name == "00" { "01" } else { "00" };
    fs::write(
        bucket.join(format!("{foreign}{}.{FILE_EXT}", "0".repeat(30))),
        b"w",
    )
    .unwrap();

    store.restart().unwrap();
    let report = store.recover(NOW).unwrap();
    assert_eq!(
        report,
        RecoveryReport {
            orphans_removed: 6,
            entries_removed: 0
        }
    );
    assert_eq!(entry_files(&dir), vec![kept]);
    assert_eq!(temp_files(&dir), NO_FILES);
    assert_eq!(store.recover(NOW).unwrap(), RecoveryReport::default());
    assert!(page_of(&mut store, Partition::Scoped(A), K1).is_some());
}

#[test]
fn driver_recover_removes_rows_whose_file_is_missing_or_of_another_length() {
    let dir = TestDir::new("recover_removes_rows_whose_file_is_missing");
    let mut store = store(&dir);
    put(&mut store, Partition::Scoped(A), K1, "p0/a");
    put(&mut store, Partition::Scoped(A), K2, "p0/b");
    put(&mut store, Partition::Scoped(A), K3, "p0/c");
    let missing = file_of(&dir, Partition::Scoped(A), K1);
    let longer = file_of(&dir, Partition::Scoped(A), K2);
    fs::remove_file(&missing).unwrap();
    let mut bytes = fs::read(&longer).unwrap();
    bytes.extend_from_slice(&[0; 3]);
    fs::write(&longer, &bytes).unwrap();

    store.restart().unwrap();
    let report = store.recover(NOW).unwrap();
    assert_eq!(
        report,
        RecoveryReport {
            orphans_removed: 0,
            entries_removed: 2
        }
    );
    assert!(!longer.exists());
    assert_eq!(store.list().unwrap().len(), 1);
    assert!(page_of(&mut store, Partition::Scoped(A), K3).is_some());
    assert_eq!(store.recover(NOW).unwrap(), RecoveryReport::default());
}

#[test]
fn driver_recover_removes_rows_that_do_not_decode() {
    let dir = TestDir::new("recover_removes_rows_that_do_not_decode");
    let mut store = store(&dir);
    put(&mut store, Partition::Scoped(A), K1, "p0/a");
    put(&mut store, Partition::Scoped(B), K1, "p1/a");
    raw(&dir)
        .execute(
            r#"UPDATE entry SET version = -1 WHERE "partition" = ?1"#,
            [partition_column(Partition::Scoped(A))],
        )
        .unwrap();
    store.restart().unwrap();
    let report = store.recover(NOW).unwrap();
    assert_eq!(
        report,
        RecoveryReport {
            orphans_removed: 1,
            entries_removed: 1
        }
    );
    assert_eq!(entry_files(&dir).len(), 1);
    assert!(page_of(&mut store, Partition::Scoped(B), K1).is_some());
}

#[test]
fn driver_recover_removes_other_schema_directories_only() {
    let dir = TestDir::new("recover_removes_other_schema_directories");
    let root = dir.root();
    let mut store = store(&dir);
    put(&mut store, Partition::Public, K1, "p2/a");
    fs::create_dir_all(root.join("schema-0").join("files")).unwrap();
    fs::write(root.join("schema-0").join("meta.sqlite"), b"old").unwrap();
    fs::write(root.join("schema-2"), b"newer").unwrap();
    fs::create_dir(root.join("schema-x")).unwrap();
    fs::write(root.join("keep.txt"), b"k").unwrap();
    store.restart().unwrap();
    assert_eq!(store.recover(NOW).unwrap(), RecoveryReport::default());
    assert!(!root.join("schema-0").exists());
    assert!(!root.join("schema-2").exists());
    assert!(root.join("schema-x").exists());
    assert!(root.join("keep.txt").exists());
    assert!(schema_dir(&root).exists());
    assert!(page_of(&mut store, Partition::Public, K1).is_some());
}

#[test]
fn driver_recover_resets_a_database_that_is_not_a_database() {
    let dir = TestDir::new("recover_resets_a_database_that_is_not_a_database");
    let mut store = store(&dir);
    put(&mut store, Partition::Scoped(A), K1, "p0/a");
    put(&mut store, Partition::Public, K2, "p2/a");
    drop(store);
    let schema = schema_dir(&dir.root());
    for name in ["meta.sqlite-wal", "meta.sqlite-shm"] {
        let path = schema.join(name);
        if path.exists() {
            fs::remove_file(path).unwrap();
        }
    }
    fs::write(schema.join(DB_FILE), vec![0x42; 4096]).unwrap();

    let mut store = FileStore::open(&dir.root(), HostKeys::new().unwrap()).unwrap();
    assert_eq!(entry_files(&dir), NO_FILES);
    assert_eq!(store.recover(NOW).unwrap(), RecoveryReport::default());
    assert_eq!(store.list().unwrap().len(), 0);
    // A writer of the old store holds epoch 0: it is refused.
    assert_ne!(store.site_epoch().unwrap().0, 0);
    assert_ne!(store.partition_epoch(Partition::Scoped(A)).unwrap().0, 0);
    put(&mut store, Partition::Scoped(A), K1, "p0/b");
    assert_eq!(entry_files(&dir).len(), 1);
}

#[cfg(unix)]
#[test]
fn driver_recover_removes_a_link_and_never_follows_it() {
    let dir = TestDir::new("recover_removes_a_link");
    let mut store = store(&dir);
    put(&mut store, Partition::Scoped(A), K1, "p0/a");
    put(&mut store, Partition::Scoped(A), K2, "p0/b");
    let outside = dir.0.join("outside");
    fs::write(&outside, b"not of the store").unwrap();
    // A link with the name of a file that no row names.
    let unnamed = entry_file_path(&dir.root(), &FileId::from_bytes([0x77; 16]));
    fs::create_dir_all(unnamed.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&outside, &unnamed).unwrap();
    // A link in place of the file of a row.
    let named = file_of(&dir, Partition::Scoped(A), K1);
    fs::remove_file(&named).unwrap();
    std::os::unix::fs::symlink(&outside, &named).unwrap();

    store.restart().unwrap();
    let report = store.recover(NOW).unwrap();
    assert_eq!(
        report,
        RecoveryReport {
            orphans_removed: 2,
            entries_removed: 1
        }
    );
    assert_eq!(fs::read(&outside).unwrap(), b"not of the store");
    assert!(fs::symlink_metadata(&unnamed).is_err());
    assert!(fs::symlink_metadata(&named).is_err());
    assert!(page_of(&mut store, Partition::Scoped(A), K2).is_some());
}

#[test]
fn driver_recover_after_a_stop_removes_the_files_of_the_commit() {
    let dir = TestDir::new("recover_after_a_stop");
    let mut store = store(&dir);
    put(&mut store, Partition::Scoped(A), K1, "p0/old");
    // (step, orphans after the stop, page after recovery)
    let cases: [(CommitStep, u64, &[u8]); 5] = [
        (CommitStep::WriteTemp, 1, b"p0/old/page"),
        (CommitStep::SyncTemp, 1, b"p0/old/page"),
        (CommitStep::Promote, 1, b"p0/old/page"),
        (CommitStep::CommitMeta, 1, b"p0/new/page"),
        (CommitStep::RemoveOld, 0, b"p0/new/page"),
    ];
    for (step, orphans, page) in cases {
        put(&mut store, Partition::Scoped(A), K1, "p0/old");
        store.stop_after(step);
        let (meta, body) = sample_entry(Partition::Scoped(A), K1, HOST, "p0/new", 0);
        let pre = fresh(&mut store, Partition::Scoped(A), Expect::Any);
        assert_eq!(
            kind(store.apply(replace_txn(meta, body, pre))),
            StoreErrorKind::Unavailable
        );
        store.restart().unwrap();
        let report = store.recover(NOW).unwrap();
        assert_eq!(report.orphans_removed, orphans, "{step:?}");
        assert_eq!(report.entries_removed, 0, "{step:?}");
        assert_eq!(entry_files(&dir).len(), 1, "{step:?}");
        assert_eq!(temp_files(&dir), NO_FILES, "{step:?}");
        assert_eq!(
            page_of(&mut store, Partition::Scoped(A), K1).unwrap(),
            page,
            "{step:?}"
        );
    }
}

// ---------------------------------------------------------------- logout

#[test]
fn driver_revoke_shreds() {
    let dir = TestDir::new("revoke_shreds");
    let mut store = store(&dir);
    put(&mut store, Partition::Scoped(A), K1, "p0/a");
    put(&mut store, Partition::Scoped(A), K2, "p0/b");
    put(&mut store, Partition::Scoped(B), K1, "p1/a");
    let mut saved = Vec::new();
    for key in [K1, K2] {
        let path = file_of(&dir, Partition::Scoped(A), key);
        let id = FileId::from_hex(path.file_stem().unwrap().to_str().unwrap()).unwrap();
        saved.push((key, id, path.clone(), fs::read(&path).unwrap()));
    }
    // The old key opens the files: the check below is not vacuous.
    let old_key = store.keys_mut().data_key(Partition::Scoped(A)).unwrap();
    for (key, id, _, bytes) in &saved {
        let aad = Aad::new(Role::File, SCHEMA_VERSION, Partition::Scoped(A), key, id);
        assert!(open(&old_key, &aad, bytes).is_ok());
    }
    drop(old_key);

    // A revoke whose `RemoveOld` fails leaves the files on disk, as a power
    // loss can bring them back.
    store.fail_at(CommitStep::RemoveOld);
    let outcome = apply(
        &mut store,
        TxnOp::RevokePartition {
            partition: Partition::Scoped(A),
        },
        Partition::Scoped(A),
    );
    assert_eq!(outcome, TxnOutcome::Applied { version: None });
    let new_key = store.keys_mut().data_key(Partition::Scoped(A)).unwrap();
    for (key, id, path, bytes) in &saved {
        assert_eq!(fs::read(path).unwrap(), *bytes);
        let aad = Aad::new(Role::File, SCHEMA_VERSION, Partition::Scoped(A), key, id);
        assert_eq!(
            open(&new_key, &aad, bytes).unwrap_err().kind(),
            StoreErrorKind::Corrupted
        );
    }
    assert!(page_of(&mut store, Partition::Scoped(A), K1).is_none());
    assert!(page_of(&mut store, Partition::Scoped(B), K1).is_some());
    store.restart().unwrap();
    assert_eq!(store.recover(NOW).unwrap().orphans_removed, 2);
}

// ---------------------------------------------------------------- partitions

#[test]
fn driver_partitions_same_key_two_entries_no_shared_file() {
    let dir = TestDir::new("partitions_same_key_two_entries");
    let mut store = store(&dir);
    put(&mut store, Partition::Scoped(A), K1, "p0/a");
    put(&mut store, Partition::Public, K1, "p2/a");
    let scoped = file_of(&dir, Partition::Scoped(A), K1);
    let public = file_of(&dir, Partition::Public, K1);
    assert_ne!(scoped, public);
    assert_eq!(entry_files(&dir).len(), 2);
    assert_eq!(
        page_of(&mut store, Partition::Scoped(A), K1).unwrap(),
        b"p0/a/page"
    );
    assert_eq!(
        page_of(&mut store, Partition::Public, K1).unwrap(),
        b"p2/a/page"
    );
    assert!(page_of(&mut store, Partition::Scoped(B), K1).is_none());
}

#[test]
fn driver_partitions_moved_file_is_corrupted() {
    let dir = TestDir::new("partitions_moved_file_is_corrupted");
    let mut store = store(&dir);
    put(&mut store, Partition::Scoped(A), K1, "p0/a");
    put(&mut store, Partition::Public, K1, "p2/a");
    let scoped = file_of(&dir, Partition::Scoped(A), K1);
    let public = file_of(&dir, Partition::Public, K1);
    fs::copy(&scoped, &public).unwrap();
    assert_eq!(
        kind(store.read(Partition::Public, &K1, NOW)),
        StoreErrorKind::Corrupted
    );
    assert_eq!(
        page_of(&mut store, Partition::Scoped(A), K1).unwrap(),
        b"p0/a/page"
    );
}

#[test]
fn driver_partitions_moved_row_is_corrupted() {
    let dir = TestDir::new("partitions_moved_row_is_corrupted");
    let mut store = store(&dir);
    put(&mut store, Partition::Scoped(A), K1, "p0/a");
    // Both partitions have a key, so the read of B does open the row.
    put(&mut store, Partition::Scoped(B), K2, "p1/b");
    raw(&dir)
        .execute(
            r#"UPDATE entry SET "partition" = ?1 WHERE "partition" = ?2 AND "key" = ?3"#,
            rusqlite::params![
                partition_column(Partition::Scoped(B)),
                partition_column(Partition::Scoped(A)),
                K1.as_bytes().as_slice()
            ],
        )
        .unwrap();
    assert_eq!(
        kind(store.read(Partition::Scoped(B), &K1, NOW)),
        StoreErrorKind::Corrupted
    );
    assert!(page_of(&mut store, Partition::Scoped(A), K1).is_none());
    assert!(page_of(&mut store, Partition::Scoped(B), K2).is_some());
}
