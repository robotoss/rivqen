// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! Tests of the SQLite metadata of `rivqen-store` (WP-07 S2 T-04; ADR-004
//! section 3; DL-200; RSK-06). Each test works in its own directory under
//! `CARGO_TARGET_TMPDIR` and removes it at the end.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    // `MetaTxn::next_version` in place of the closure does not compile: the
    // closure of `MetaDb::transaction` is generic over the lifetime of the
    // transaction.
    clippy::redundant_closure_for_method_calls
)]

use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rivqen_cache::driver::{StoreError, StoreErrorKind};
use rivqen_cache::key::Partition;
use rivqen_cache::txn::{EntryVersion, Epoch};
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_model::time::UnixTime;
use rivqen_store::meta::{
    DB_FILE, MAX_HOST_BYTES, MAX_SEALED_BYTES, MetaDb, MetaTxn, Opened, RESET_MARKER, Row, Settings,
};

// ---------------------------------------------------------------- helpers

/// A directory of one test, removed on drop.
struct TestDir(PathBuf);

impl TestDir {
    fn new(name: &str) -> Self {
        let path = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("rivqen-store-meta-db")
            .join(name);
        if path.exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn db_path(&self) -> PathBuf {
        self.0.join(DB_FILE)
    }

    /// A raw connection for setup and inspection.
    fn raw(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(self.db_path()).unwrap()
    }

    /// The names in the directory, sorted.
    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(&self.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
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

fn scoped(byte: u8) -> Partition {
    Partition::Scoped(PartitionId::from_bytes([byte; 32]))
}

fn key(byte: u8) -> CacheKey {
    CacheKey::from_bytes([byte; 32])
}

fn row(partition: Partition, key_byte: u8, version: u64) -> Row {
    Row {
        partition,
        key: key(key_byte),
        host: "shop.example.com".to_owned(),
        version: EntryVersion(version),
        file_id: [key_byte; 16],
        content_bytes: 1000 + version,
        stored_at: UnixTime(1_700_000_000),
        last_used_at: UnixTime(1_700_000_100),
        sealed: vec![key_byte, 0xAA, 0x55],
    }
}

fn open(dir: &TestDir) -> MetaDb {
    MetaDb::open(dir.path()).unwrap()
}

fn put(db: &mut MetaDb, row: &Row) {
    db.transaction(|txn| txn.put_row(row)).unwrap();
}

fn read(db: &mut MetaDb, partition: Partition, key_byte: u8) -> Result<Option<Row>, StoreError> {
    db.transaction(|txn| txn.row(partition, &key(key_byte)))
}

fn kind<T: std::fmt::Debug>(result: Result<T, StoreError>) -> StoreErrorKind {
    result.unwrap_err().kind()
}

/// A random source that gives 1, 2, 3, ... and counts its calls.
struct Counting {
    next: u8,
    calls: usize,
}

impl Counting {
    fn new() -> Self {
        Self { next: 0, calls: 0 }
    }

    fn fill(&mut self, buf: &mut [u8]) {
        self.calls += 1;
        for byte in buf.iter_mut() {
            self.next = self.next.wrapping_add(1);
            *byte = self.next;
        }
    }
}

/// The value of the n-th 8-byte draw of [`Counting`], reduced to 62 bits.
fn counting_value(draw: u8) -> u64 {
    let start = draw * 8;
    let bytes: [u8; 8] = std::array::from_fn(|i| start + 1 + u8::try_from(i).unwrap());
    u64::from_be_bytes(bytes) & 0x3FFF_FFFF_FFFF_FFFF
}

fn open_or_reset(dir: &TestDir, random: &mut Counting) -> Result<(MetaDb, Opened), StoreError> {
    MetaDb::open_or_reset(dir.path(), &mut |buf: &mut [u8]| {
        random.fill(buf);
        Ok(())
    })
}

/// A database with `n` rows whose pages are checkpointed into the main
/// file (the last connection closes).
fn filled(dir: &TestDir, n: u8) {
    let mut db = open(dir);
    db.transaction(|txn| {
        for byte in 0..n {
            let mut entry = row(scoped(1), byte, u64::from(byte) + 1);
            entry.sealed = vec![byte; 300];
            txn.put_row(&entry)?;
        }
        Ok(())
    })
    .unwrap();
    drop(db);
    assert!(!dir.path().join(format!("{DB_FILE}-wal")).exists());
}

/// Overwrites 64 bytes at the start of page `page` (1-based, 4096 bytes).
fn damage_page(dir: &TestDir, page: usize) {
    let mut bytes = fs::read(dir.db_path()).unwrap();
    let start = (page - 1) * 4096;
    assert!(bytes.len() >= start + 64, "the database has no page {page}");
    for byte in &mut bytes[start..start + 64] {
        *byte = 0xAB;
    }
    fs::write(dir.db_path(), bytes).unwrap();
}

/// Holds the write lock of the database through another connection.
fn hold_write_lock(dir: &TestDir) -> rusqlite::Connection {
    let raw = dir.raw();
    raw.execute_batch("BEGIN IMMEDIATE").unwrap();
    raw
}

// ---------------------------------------------------------------- open

#[test]
fn meta_open_creates_the_four_tables() {
    let dir = TestDir::new("open_creates");
    drop(open(&dir));
    let raw = dir.raw();
    let mut stmt = raw
        .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' ORDER BY name")
        .unwrap();
    let tables: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        tables,
        ["counter", "disabled_key", "entry", "partition_epoch"]
    );
    let format: i64 = raw
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(format, rivqen_store::meta::META_FORMAT);
}

#[test]
fn meta_open_applies_the_settings() {
    let dir = TestDir::new("open_settings");
    let db = open(&dir);
    assert_eq!(db.settings().unwrap(), Settings::REQUIRED);
    // WAL is a property of the file: another connection sees it.
    let mode: String = dir
        .raw()
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
}

#[test]
fn meta_open_new_store_starts_at_zero() {
    let dir = TestDir::new("open_zero");
    let mut db = open(&dir);
    db.transaction(|txn| {
        assert_eq!(txn.site_epoch()?, Epoch(0));
        assert_eq!(txn.partition_epoch(Partition::Public)?, Epoch(0));
        assert_eq!(txn.partition_epoch(scoped(9))?, Epoch(0));
        assert_eq!(txn.next_version()?, EntryVersion(1));
        Ok(())
    })
    .unwrap();
}

#[test]
fn meta_open_second_open_keeps_the_data() {
    let dir = TestDir::new("open_keeps");
    let mut db = open(&dir);
    let stored = row(scoped(1), 7, 3);
    db.transaction(|txn| {
        txn.put_row(&stored)?;
        txn.bump_partition_epoch(scoped(1))?;
        txn.bump_site_epoch()?;
        txn.next_version()?;
        txn.disable(Partition::Public, &key(2), UnixTime(50))
    })
    .unwrap();
    drop(db);

    let mut db = open(&dir);
    db.transaction(|txn| {
        assert_eq!(txn.row(scoped(1), &key(7))?, Some(stored.clone()));
        assert_eq!(txn.partition_epoch(scoped(1))?, Epoch(1));
        assert_eq!(txn.site_epoch()?, Epoch(1));
        assert_eq!(txn.next_version()?, EntryVersion(2));
        assert_eq!(
            txn.disabled_until(Partition::Public, &key(2))?,
            Some(UnixTime(50))
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn meta_open_missing_directory_is_unavailable() {
    let dir = TestDir::new("open_missing_dir");
    let missing = dir.path().join("absent");
    assert_eq!(kind(MetaDb::open(&missing)), StoreErrorKind::Unavailable);
    assert!(!missing.exists());
}

#[test]
fn meta_open_not_a_database_is_corrupted() {
    let dir = TestDir::new("open_not_db");
    fs::write(dir.db_path(), vec![0x5A; 8192]).unwrap();
    assert_eq!(kind(MetaDb::open(dir.path())), StoreErrorKind::Corrupted);
}

#[test]
fn meta_open_malformed_database_is_corrupted() {
    let dir = TestDir::new("open_malformed");
    filled(&dir, 60);
    damage_page(&dir, 2);
    assert_eq!(kind(MetaDb::open(dir.path())), StoreErrorKind::Corrupted);
}

#[test]
fn meta_open_unknown_format_is_corrupted() {
    let dir = TestDir::new("open_format");
    drop(open(&dir));
    dir.raw().execute_batch("PRAGMA user_version = 7").unwrap();
    assert_eq!(kind(MetaDb::open(dir.path())), StoreErrorKind::Corrupted);
}

#[test]
fn meta_open_missing_or_broken_counter_is_corrupted() {
    for (name, damage) in [
        (
            "open_counter_missing",
            "DELETE FROM counter WHERE name = 'site_epoch'",
        ),
        (
            "open_counter_negative",
            "UPDATE counter SET value = -1 WHERE name = 'version'",
        ),
        (
            "open_counter_text",
            "UPDATE counter SET value = 'x' WHERE name = 'partition_epoch_base'",
        ),
    ] {
        let dir = TestDir::new(name);
        drop(open(&dir));
        dir.raw().execute_batch(damage).unwrap();
        assert_eq!(
            kind(MetaDb::open(dir.path())),
            StoreErrorKind::Corrupted,
            "{name}"
        );
    }
}

#[test]
fn meta_open_foreign_tables_are_corrupted() {
    let dir = TestDir::new("open_foreign");
    dir.raw()
        .execute_batch("CREATE TABLE other (x INTEGER)")
        .unwrap();
    assert_eq!(kind(MetaDb::open(dir.path())), StoreErrorKind::Corrupted);
}

#[test]
fn meta_open_tables_of_another_shape_are_corrupted() {
    let dir = TestDir::new("open_shape");
    dir.raw()
        .execute_batch(
            r#"CREATE TABLE entry ("partition" BLOB, "key" BLOB, host TEXT,
                   PRIMARY KEY ("partition", "key"));
               CREATE TABLE partition_epoch ("partition" BLOB PRIMARY KEY, epoch INTEGER);
               CREATE TABLE disabled_key ("partition" BLOB, "key" BLOB, until INTEGER,
                   PRIMARY KEY ("partition", "key"));
               CREATE TABLE counter (name TEXT PRIMARY KEY, value INTEGER);
               INSERT INTO counter VALUES ('version', 0), ('site_epoch', 0),
                   ('partition_epoch_base', 0);
               PRAGMA user_version = 1;"#,
        )
        .unwrap();
    assert_eq!(kind(MetaDb::open(dir.path())), StoreErrorKind::Corrupted);
}

#[test]
fn meta_open_busy_lock_is_unavailable_at_once() {
    let dir = TestDir::new("open_busy");
    drop(open(&dir));
    let lock = hold_write_lock(&dir);
    let started = Instant::now();
    assert_eq!(kind(MetaDb::open(dir.path())), StoreErrorKind::Unavailable);
    assert!(started.elapsed() < Duration::from_secs(1));
    drop(lock);
}

#[test]
fn meta_open_pending_reset_marker_is_corrupted() {
    let dir = TestDir::new("open_marker");
    drop(open(&dir));
    fs::write(dir.path().join(RESET_MARKER), b"").unwrap();
    assert_eq!(kind(MetaDb::open(dir.path())), StoreErrorKind::Corrupted);
}

// ---------------------------------------------------------------- transactions

#[test]
fn meta_txn_commit_on_ok_is_durable() {
    let dir = TestDir::new("txn_commit");
    let mut db = open(&dir);
    let value = db
        .transaction(|txn| {
            txn.put_row(&row(Partition::Public, 1, 1))?;
            Ok(42)
        })
        .unwrap();
    assert_eq!(value, 42);
    drop(db);
    let mut db = open(&dir);
    assert!(read(&mut db, Partition::Public, 1).unwrap().is_some());
}

/// Every kind of write, then a failure of the closure.
fn write_everything(txn: &mut MetaTxn<'_>) -> Result<(), StoreError> {
    txn.put_row(&row(scoped(1), 1, 9))?;
    txn.delete_row(scoped(1), &key(2))?;
    txn.touch(scoped(1), &key(3), UnixTime(999))?;
    txn.bump_partition_epoch(scoped(1))?;
    txn.bump_site_epoch()?;
    txn.next_version()?;
    txn.disable(scoped(1), &key(4), UnixTime(70))?;
    txn.purge_disabled(UnixTime(1_000))?;
    Ok(())
}

/// The state that [`write_everything`] would change.
fn snapshot(db: &mut MetaDb) -> (Vec<Row>, Epoch, Epoch, Option<UnixTime>, Option<UnixTime>) {
    db.transaction(|txn| {
        Ok((
            txn.rows()?,
            txn.partition_epoch(scoped(1))?,
            txn.site_epoch()?,
            txn.disabled_until(scoped(1), &key(4))?,
            txn.disabled_until(scoped(1), &key(5))?,
        ))
    })
    .unwrap()
}

fn seeded(dir: &TestDir) -> MetaDb {
    let mut db = open(dir);
    db.transaction(|txn| {
        txn.put_row(&row(scoped(1), 2, 2))?;
        txn.put_row(&row(scoped(1), 3, 3))?;
        txn.disable(scoped(1), &key(5), UnixTime(10))?;
        txn.next_version()?;
        Ok(())
    })
    .unwrap();
    db
}

#[test]
fn meta_txn_rollback_on_err() {
    let dir = TestDir::new("txn_rollback_err");
    let mut db = seeded(&dir);
    let before = snapshot(&mut db);
    let result: Result<(), StoreError> = db.transaction(|txn| {
        write_everything(txn)?;
        Err(StoreError::new(StoreErrorKind::Corrupted, "test"))
    });
    assert_eq!(
        result,
        Err(StoreError::new(StoreErrorKind::Corrupted, "test"))
    );
    assert_eq!(snapshot(&mut db), before);
    // The version counter was rolled back too.
    assert_eq!(
        db.transaction(|txn| txn.next_version()).unwrap(),
        EntryVersion(2)
    );
}

#[test]
fn meta_txn_rollback_on_panic() {
    let dir = TestDir::new("txn_rollback_panic");
    let mut db = seeded(&dir);
    let before = snapshot(&mut db);
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        db.transaction(|txn| -> Result<(), StoreError> {
            write_everything(txn)?;
            panic!("stop inside the transaction");
        })
    }));
    assert!(outcome.is_err());
    // The connection is usable and holds the state before the call.
    assert_eq!(snapshot(&mut db), before);
    drop(db);
    let mut db = open(&dir);
    assert_eq!(snapshot(&mut db), before);
}

#[test]
fn meta_txn_rollback_keeps_the_store_usable() {
    let dir = TestDir::new("txn_rollback_usable");
    let mut db = open(&dir);
    let failed: Result<(), StoreError> =
        db.transaction(|_| Err(StoreError::new(StoreErrorKind::Unavailable, "test")));
    assert!(failed.is_err());
    put(&mut db, &row(Partition::Public, 1, 1));
    assert!(read(&mut db, Partition::Public, 1).unwrap().is_some());
}

#[test]
fn meta_txn_immediate_fails_at_once_when_another_writer_holds_the_lock() {
    let dir = TestDir::new("txn_immediate");
    let mut db = open(&dir);
    let lock = hold_write_lock(&dir);
    let started = Instant::now();
    // A read-only closure: a deferred transaction would read in WAL mode;
    // `BEGIN IMMEDIATE` asks for the write lock first.
    let mut ran = false;
    let result = db.transaction(|txn| {
        ran = true;
        txn.site_epoch()
    });
    assert_eq!(kind(result), StoreErrorKind::Unavailable);
    assert!(!ran);
    assert!(started.elapsed() < Duration::from_secs(1));
    drop(lock);
    assert_eq!(db.transaction(|txn| txn.site_epoch()).unwrap(), Epoch(0));
}

// ---------------------------------------------------------------- rows

#[test]
fn meta_rows_put_and_read_back_every_field() {
    let dir = TestDir::new("rows_round_trip");
    let mut db = open(&dir);
    let stored = Row {
        partition: scoped(4),
        key: key(8),
        host: "[2001:db8::1]".to_owned(),
        version: EntryVersion(u64::try_from(i64::MAX).unwrap()),
        file_id: [0x10; 16],
        content_bytes: u64::try_from(i64::MAX).unwrap(),
        stored_at: UnixTime(0),
        last_used_at: UnixTime(u64::try_from(i64::MAX).unwrap()),
        sealed: Vec::new(),
    };
    put(&mut db, &stored);
    assert_eq!(read(&mut db, scoped(4), 8).unwrap(), Some(stored));
    assert_eq!(read(&mut db, scoped(4), 9).unwrap(), None);
}

#[test]
fn meta_rows_put_replaces_the_row_of_the_address() {
    let dir = TestDir::new("rows_replace");
    let mut db = open(&dir);
    put(&mut db, &row(Partition::Public, 1, 1));
    let mut newer = row(Partition::Public, 1, 2);
    newer.host = "other.example".to_owned();
    newer.file_id = [0xEE; 16];
    newer.content_bytes = 5;
    newer.stored_at = UnixTime(7);
    newer.last_used_at = UnixTime(8);
    newer.sealed = vec![1, 2, 3, 4];
    put(&mut db, &newer);
    let rows = db.transaction(|txn| txn.rows()).unwrap();
    assert_eq!(rows, vec![newer]);
}

#[test]
fn meta_rows_delete_row_reports_a_removal() {
    let dir = TestDir::new("rows_delete");
    let mut db = open(&dir);
    put(&mut db, &row(Partition::Public, 1, 1));
    put(&mut db, &row(Partition::Public, 2, 2));
    assert!(
        db.transaction(|txn| txn.delete_row(Partition::Public, &key(1)))
            .unwrap()
    );
    assert!(
        !db.transaction(|txn| txn.delete_row(Partition::Public, &key(1)))
            .unwrap()
    );
    assert_eq!(read(&mut db, Partition::Public, 1).unwrap(), None);
    assert!(read(&mut db, Partition::Public, 2).unwrap().is_some());
}

#[test]
fn meta_rows_list_in_partition_and_key_order() {
    let dir = TestDir::new("rows_order");
    let mut db = open(&dir);
    for (partition, key_byte) in [
        (scoped(2), 1),
        (Partition::Public, 9),
        (scoped(1), 5),
        (scoped(1), 3),
    ] {
        put(&mut db, &row(partition, key_byte, 1));
    }
    let order: Vec<(Partition, CacheKey)> = db
        .transaction(|txn| txn.rows())
        .unwrap()
        .into_iter()
        .map(|found| (found.partition, found.key))
        .collect();
    assert_eq!(
        order,
        [
            (Partition::Public, key(9)),
            (scoped(1), key(3)),
            (scoped(1), key(5)),
            (scoped(2), key(1)),
        ]
    );
}

#[test]
fn meta_rows_of_partition_and_delete_of_partition() {
    let dir = TestDir::new("rows_partition");
    let mut db = open(&dir);
    for (partition, key_byte) in [
        (scoped(1), 2),
        (scoped(1), 1),
        (scoped(2), 1),
        (Partition::Public, 1),
    ] {
        put(&mut db, &row(partition, key_byte, 1));
    }
    let keys: Vec<CacheKey> = db
        .transaction(|txn| txn.rows_of_partition(scoped(1)))
        .unwrap()
        .into_iter()
        .map(|found| found.key)
        .collect();
    assert_eq!(keys, [key(1), key(2)]);
    assert_eq!(
        db.transaction(|txn| txn.delete_rows_of_partition(scoped(1)))
            .unwrap(),
        2
    );
    let left: Vec<Partition> = db
        .transaction(|txn| txn.rows())
        .unwrap()
        .into_iter()
        .map(|found| found.partition)
        .collect();
    assert_eq!(left, [Partition::Public, scoped(2)]);
    assert_eq!(
        db.transaction(|txn| txn.delete_rows_of_partition(scoped(1)))
            .unwrap(),
        0
    );
}

#[test]
fn meta_rows_touch_sets_last_used_only() {
    let dir = TestDir::new("rows_touch");
    let mut db = open(&dir);
    let stored = row(scoped(1), 1, 1);
    put(&mut db, &stored);
    assert!(
        db.transaction(|txn| txn.touch(scoped(1), &key(1), UnixTime(5_000_000_000)))
            .unwrap()
    );
    let mut expected = stored;
    expected.last_used_at = UnixTime(5_000_000_000);
    assert_eq!(read(&mut db, scoped(1), 1).unwrap(), Some(expected));
    assert!(
        !db.transaction(|txn| txn.touch(scoped(1), &key(2), UnixTime(1)))
            .unwrap()
    );
    assert_eq!(read(&mut db, scoped(1), 2).unwrap(), None);
}

#[test]
fn meta_rows_refuse_integers_above_i64_max() {
    let dir = TestDir::new("rows_range");
    let mut db = open(&dir);
    let above = u64::try_from(i64::MAX).unwrap() + 1;
    let mut big_version = row(Partition::Public, 1, 1);
    big_version.version = EntryVersion(above);
    let mut big_size = row(Partition::Public, 2, 1);
    big_size.content_bytes = u64::MAX;
    db.transaction(|txn| {
        assert_eq!(kind(txn.put_row(&big_version)), StoreErrorKind::Unavailable);
        assert_eq!(kind(txn.put_row(&big_size)), StoreErrorKind::Unavailable);
        // Nothing was written, also inside the transaction.
        assert_eq!(txn.rows()?, Vec::new());
        Ok(())
    })
    .unwrap();
}

#[test]
fn meta_rows_times_above_i64_max_are_stored_as_i64_max() {
    let dir = TestDir::new("rows_time_saturate");
    let mut db = open(&dir);
    let mut stored = row(Partition::Public, 1, 1);
    stored.stored_at = UnixTime(u64::MAX);
    stored.last_used_at = UnixTime(u64::try_from(i64::MAX).unwrap() + 1);
    put(&mut db, &stored);
    let found = read(&mut db, Partition::Public, 1).unwrap().unwrap();
    let max = UnixTime(u64::try_from(i64::MAX).unwrap());
    assert_eq!(found.stored_at, max);
    assert_eq!(found.last_used_at, max);
    db.transaction(|txn| txn.touch(Partition::Public, &key(1), UnixTime(u64::MAX)))
        .unwrap();
    let found = read(&mut db, Partition::Public, 1).unwrap().unwrap();
    assert_eq!(found.last_used_at, max);
}

#[test]
fn meta_rows_limits_of_host_and_sealed() {
    let dir = TestDir::new("rows_limits");
    let mut db = open(&dir);
    let mut longest = row(Partition::Public, 1, 1);
    longest.host = "a".repeat(MAX_HOST_BYTES);
    longest.sealed = vec![7; MAX_SEALED_BYTES];
    put(&mut db, &longest);
    assert_eq!(read(&mut db, Partition::Public, 1).unwrap(), Some(longest));

    let mut long_host = row(Partition::Public, 2, 1);
    long_host.host = "a".repeat(MAX_HOST_BYTES + 1);
    let mut long_sealed = row(Partition::Public, 3, 1);
    long_sealed.sealed = vec![7; MAX_SEALED_BYTES + 1];
    db.transaction(|txn| {
        assert_eq!(kind(txn.put_row(&long_host)), StoreErrorKind::Unavailable);
        assert_eq!(kind(txn.put_row(&long_sealed)), StoreErrorKind::Unavailable);
        Ok(())
    })
    .unwrap();
    assert_eq!(read(&mut db, Partition::Public, 2).unwrap(), None);
    assert_eq!(read(&mut db, Partition::Public, 3).unwrap(), None);
}

/// Damages that make one stored row undecodable; the row is `scoped(1)`,
/// key 1, the other rows stay good.
const ROW_DAMAGES: [(&str, &str); 16] = [
    ("version negative", "version = -1"),
    ("version real", "version = 1.5"),
    ("size negative", "content_bytes = -2"),
    ("stored_at text", "stored_at = 'x'"),
    ("last_used_at negative", "last_used_at = -3"),
    ("key short", "\"key\" = zeroblob(31)"),
    ("key text", "\"key\" = 'k'"),
    ("file_id short", "file_id = zeroblob(15)"),
    ("file_id long", "file_id = zeroblob(17)"),
    ("host blob", "host = x'61'"),
    ("host not utf-8", "host = CAST(x'FF' AS TEXT)"),
    ("sealed text", "sealed = 's'"),
    ("sealed over the limit", "sealed = zeroblob(1048577)"),
    ("partition tag", "\"partition\" = x'02'"),
    ("partition public with a tail", "\"partition\" = x'0000'"),
    (
        "partition scoped short",
        "\"partition\" = x'0100000000000000000000000000000000000000000000000000000000000000'",
    ),
];

fn damage_row(dir: &TestDir, damage: &str) {
    let partition_column: Vec<u8> = [&[0x01_u8][..], &[1_u8; 32][..]].concat();
    let sql = format!("UPDATE entry SET {damage} WHERE \"partition\" = ?1 AND \"key\" = ?2");
    let changed = dir
        .raw()
        .execute(&sql, rusqlite::params![partition_column, [1_u8; 32]])
        .unwrap();
    assert_eq!(changed, 1, "{damage}");
}

#[test]
fn meta_rows_undecodable_row_is_corrupted_and_removable() {
    for (index, (name, damage)) in ROW_DAMAGES.iter().enumerate() {
        let dir = TestDir::new(&format!("rows_damage_{index}"));
        let mut db = open(&dir);
        put(&mut db, &row(scoped(1), 1, 1));
        put(&mut db, &row(scoped(1), 2, 2));
        put(&mut db, &row(Partition::Public, 1, 3));
        damage_row(&dir, damage);

        assert_eq!(
            kind(db.transaction(|txn| txn.rows())),
            StoreErrorKind::Corrupted,
            "{name}"
        );
        // A broken partition column moves the row out of every partition:
        // no address names it, only the scan of all rows finds it.
        let moved = damage.starts_with("\"partition\"");
        let in_partition = db.transaction(|txn| txn.rows_of_partition(scoped(1)));
        if moved {
            assert_eq!(in_partition, Ok(vec![row(scoped(1), 2, 2)]), "{name}");
        } else {
            assert_eq!(kind(in_partition), StoreErrorKind::Corrupted, "{name}");
        }
        if !damage.starts_with("\"key\"") && !moved {
            assert_eq!(
                kind(read(&mut db, scoped(1), 1)),
                StoreErrorKind::Corrupted,
                "{name}"
            );
        }
        assert_eq!(
            db.transaction(|txn| txn.remove_undecodable_rows()).unwrap(),
            1,
            "{name}"
        );
        let left = db.transaction(|txn| txn.rows()).unwrap();
        assert_eq!(
            left,
            vec![row(Partition::Public, 1, 3), row(scoped(1), 2, 2)],
            "{name}"
        );
        assert_eq!(
            db.transaction(|txn| txn.remove_undecodable_rows()).unwrap(),
            0,
            "{name}"
        );
    }
}

#[test]
fn meta_rows_delete_row_removes_an_undecodable_row() {
    let dir = TestDir::new("rows_delete_broken");
    let mut db = open(&dir);
    put(&mut db, &row(scoped(1), 1, 1));
    damage_row(&dir, "version = -1");
    assert_eq!(kind(read(&mut db, scoped(1), 1)), StoreErrorKind::Corrupted);
    assert!(
        db.transaction(|txn| txn.delete_row(scoped(1), &key(1)))
            .unwrap()
    );
    assert_eq!(read(&mut db, scoped(1), 1).unwrap(), None);
}

// ---------------------------------------------------------------- epochs

#[test]
fn meta_epochs_partition_grows_by_one_and_alone() {
    let dir = TestDir::new("epochs_partition");
    let mut db = open(&dir);
    db.transaction(|txn| {
        assert_eq!(txn.bump_partition_epoch(scoped(1))?, Epoch(1));
        assert_eq!(txn.bump_partition_epoch(scoped(1))?, Epoch(2));
        assert_eq!(txn.partition_epoch(scoped(1))?, Epoch(2));
        assert_eq!(txn.partition_epoch(scoped(2))?, Epoch(0));
        assert_eq!(txn.partition_epoch(Partition::Public)?, Epoch(0));
        assert_eq!(txn.bump_partition_epoch(Partition::Public)?, Epoch(1));
        assert_eq!(txn.partition_epoch(scoped(1))?, Epoch(2));
        assert_eq!(txn.site_epoch()?, Epoch(0));
        Ok(())
    })
    .unwrap();
}

#[test]
fn meta_epochs_site_grows_by_one() {
    let dir = TestDir::new("epochs_site");
    let mut db = open(&dir);
    db.transaction(|txn| {
        assert_eq!(txn.bump_site_epoch()?, Epoch(1));
        assert_eq!(txn.bump_site_epoch()?, Epoch(2));
        assert_eq!(txn.site_epoch()?, Epoch(2));
        assert_eq!(txn.partition_epoch(scoped(1))?, Epoch(0));
        assert_eq!(txn.next_version()?, EntryVersion(1));
        Ok(())
    })
    .unwrap();
}

#[test]
fn meta_epochs_row_is_never_deleted() {
    let dir = TestDir::new("epochs_kept");
    let mut db = open(&dir);
    db.transaction(|txn| {
        txn.put_row(&row(scoped(1), 1, 1))?;
        txn.disable(scoped(1), &key(1), UnixTime(5))?;
        txn.bump_partition_epoch(scoped(1))?;
        txn.delete_rows_of_partition(scoped(1))?;
        txn.delete_disabled_of_partition(scoped(1))?;
        txn.delete_all_disabled()?;
        txn.purge_disabled(UnixTime(u64::MAX))?;
        txn.remove_undecodable_rows()?;
        Ok(())
    })
    .unwrap();
    drop(db);
    let mut db = open(&dir);
    assert_eq!(
        db.transaction(|txn| txn.partition_epoch(scoped(1)))
            .unwrap(),
        Epoch(1)
    );
}

#[test]
fn meta_epochs_increment_past_i64_max_is_refused() {
    let dir = TestDir::new("epochs_overflow");
    let mut db = open(&dir);
    db.transaction(|txn| txn.bump_partition_epoch(scoped(1)).map(drop))
        .unwrap();
    let raw = dir.raw();
    raw.execute_batch(
        "UPDATE partition_epoch SET epoch = 9223372036854775807;
         UPDATE counter SET value = 9223372036854775807 WHERE name = 'site_epoch';",
    )
    .unwrap();
    drop(raw);
    let max = Epoch(u64::try_from(i64::MAX).unwrap());
    db.transaction(|txn| {
        assert_eq!(
            kind(txn.bump_partition_epoch(scoped(1))),
            StoreErrorKind::Unavailable
        );
        assert_eq!(kind(txn.bump_site_epoch()), StoreErrorKind::Unavailable);
        assert_eq!(txn.partition_epoch(scoped(1))?, max);
        assert_eq!(txn.site_epoch()?, max);
        Ok(())
    })
    .unwrap();
}

#[test]
fn meta_epochs_undecodable_value_is_corrupted() {
    let dir = TestDir::new("epochs_corrupt");
    let mut db = open(&dir);
    db.transaction(|txn| txn.bump_partition_epoch(scoped(1)).map(drop))
        .unwrap();
    dir.raw()
        .execute_batch(
            "UPDATE partition_epoch SET epoch = -1;
             UPDATE counter SET value = 2.5 WHERE name = 'site_epoch';",
        )
        .unwrap();
    db.transaction(|txn| {
        assert_eq!(
            kind(txn.partition_epoch(scoped(1))),
            StoreErrorKind::Corrupted
        );
        assert_eq!(
            kind(txn.bump_partition_epoch(scoped(1))),
            StoreErrorKind::Corrupted
        );
        assert_eq!(kind(txn.site_epoch()), StoreErrorKind::Corrupted);
        assert_eq!(kind(txn.bump_site_epoch()), StoreErrorKind::Corrupted);
        // Another partition reads the base and stays usable.
        assert_eq!(txn.partition_epoch(scoped(2))?, Epoch(0));
        Ok(())
    })
    .unwrap();
}

// ---------------------------------------------------------------- versions

#[test]
fn meta_versions_grow_and_are_never_given_twice() {
    let dir = TestDir::new("versions_grow");
    let mut db = open(&dir);
    let first = db
        .transaction(|txn| Ok((txn.next_version()?, txn.next_version()?)))
        .unwrap();
    assert_eq!(first, (EntryVersion(1), EntryVersion(2)));
    // Removing rows does not reset the counter.
    db.transaction(|txn| {
        txn.put_row(&row(Partition::Public, 1, 2))?;
        txn.delete_rows_of_partition(Partition::Public)?;
        Ok(())
    })
    .unwrap();
    drop(db);
    let mut db = open(&dir);
    assert_eq!(
        db.transaction(|txn| txn.next_version()).unwrap(),
        EntryVersion(3)
    );
}

#[test]
fn meta_versions_increment_past_i64_max_is_refused() {
    let dir = TestDir::new("versions_overflow");
    let mut db = open(&dir);
    dir.raw()
        .execute_batch("UPDATE counter SET value = 9223372036854775806 WHERE name = 'version'")
        .unwrap();
    let last = db.transaction(|txn| txn.next_version()).unwrap();
    assert_eq!(last, EntryVersion(u64::try_from(i64::MAX).unwrap()));
    assert_eq!(
        kind(db.transaction(|txn| txn.next_version())),
        StoreErrorKind::Unavailable
    );
    let stored: i64 = dir
        .raw()
        .query_row(
            "SELECT value FROM counter WHERE name = 'version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored, i64::MAX);
}

#[test]
fn meta_versions_missing_counter_is_corrupted() {
    let dir = TestDir::new("versions_missing");
    let mut db = open(&dir);
    dir.raw()
        .execute_batch("DELETE FROM counter WHERE name = 'version'")
        .unwrap();
    assert_eq!(
        kind(db.transaction(|txn| txn.next_version())),
        StoreErrorKind::Corrupted
    );
}

// ---------------------------------------------------------------- disabled keys

#[test]
fn meta_disabled_record_is_set_and_replaced() {
    let dir = TestDir::new("disabled_set");
    let mut db = open(&dir);
    db.transaction(|txn| {
        assert_eq!(txn.disabled_until(scoped(1), &key(1))?, None);
        txn.disable(scoped(1), &key(1), UnixTime(100))?;
        assert_eq!(txn.disabled_until(scoped(1), &key(1))?, Some(UnixTime(100)));
        txn.disable(scoped(1), &key(1), UnixTime(40))?;
        assert_eq!(txn.disabled_until(scoped(1), &key(1))?, Some(UnixTime(40)));
        assert_eq!(txn.disabled_until(scoped(1), &key(2))?, None);
        txn.disable(scoped(1), &key(3), UnixTime(u64::MAX))?;
        assert_eq!(
            txn.disabled_until(scoped(1), &key(3))?,
            Some(UnixTime(u64::try_from(i64::MAX).unwrap()))
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn meta_disabled_purge_removes_only_records_before_now() {
    let dir = TestDir::new("disabled_purge");
    let mut db = open(&dir);
    db.transaction(|txn| {
        txn.disable(scoped(1), &key(1), UnixTime(99))?;
        txn.disable(scoped(1), &key(2), UnixTime(100))?;
        txn.disable(Partition::Public, &key(3), UnixTime(101))?;
        txn.disable(Partition::Public, &key(4), UnixTime(0))?;
        assert_eq!(txn.purge_disabled(UnixTime(100))?, 2);
        assert_eq!(txn.disabled_until(scoped(1), &key(1))?, None);
        assert_eq!(txn.disabled_until(Partition::Public, &key(4))?, None);
        assert_eq!(txn.disabled_until(scoped(1), &key(2))?, Some(UnixTime(100)));
        assert_eq!(
            txn.disabled_until(Partition::Public, &key(3))?,
            Some(UnixTime(101))
        );
        assert_eq!(txn.purge_disabled(UnixTime(100))?, 0);
        Ok(())
    })
    .unwrap();
}

#[test]
fn meta_disabled_delete_of_partition_and_all() {
    let dir = TestDir::new("disabled_delete");
    let mut db = open(&dir);
    db.transaction(|txn| {
        txn.disable(scoped(1), &key(1), UnixTime(9))?;
        txn.disable(scoped(1), &key(2), UnixTime(9))?;
        txn.disable(scoped(2), &key(1), UnixTime(9))?;
        txn.disable(Partition::Public, &key(1), UnixTime(9))?;
        assert_eq!(txn.delete_disabled_of_partition(scoped(1))?, 2);
        assert_eq!(txn.disabled_until(scoped(1), &key(1))?, None);
        assert_eq!(txn.disabled_until(scoped(2), &key(1))?, Some(UnixTime(9)));
        assert_eq!(
            txn.disabled_until(Partition::Public, &key(1))?,
            Some(UnixTime(9))
        );
        assert_eq!(txn.delete_all_disabled()?, 2);
        assert_eq!(txn.disabled_until(scoped(2), &key(1))?, None);
        assert_eq!(txn.disabled_until(Partition::Public, &key(1))?, None);
        assert_eq!(txn.delete_all_disabled()?, 0);
        Ok(())
    })
    .unwrap();
}

#[test]
fn meta_disabled_undecodable_time_is_corrupted() {
    let dir = TestDir::new("disabled_corrupt");
    let mut db = open(&dir);
    db.transaction(|txn| txn.disable(scoped(1), &key(1), UnixTime(9)))
        .unwrap();
    dir.raw()
        .execute_batch("UPDATE disabled_key SET until = -5")
        .unwrap();
    assert_eq!(
        kind(db.transaction(|txn| txn.disabled_until(scoped(1), &key(1)))),
        StoreErrorKind::Corrupted
    );
}

// ---------------------------------------------------------------- partitions

#[test]
fn meta_partitions_same_key_stay_apart() {
    let dir = TestDir::new("partitions_apart");
    let mut db = open(&dir);
    let partitions = [Partition::Public, scoped(1), scoped(2)];
    let mut rows = Vec::new();
    for (index, partition) in partitions.iter().enumerate() {
        let mut stored = row(*partition, 7, u64::try_from(index).unwrap() + 1);
        stored.file_id = [u8::try_from(index).unwrap(); 16];
        rows.push(stored);
    }
    db.transaction(|txn| {
        for stored in &rows {
            txn.put_row(stored)?;
        }
        Ok(())
    })
    .unwrap();

    db.transaction(|txn| {
        for (partition, stored) in partitions.iter().zip(&rows) {
            assert_eq!(txn.row(*partition, &key(7))?.as_ref(), Some(stored));
            assert_eq!(txn.rows_of_partition(*partition)?, vec![stored.clone()]);
        }
        // A scoped id whose bytes start like the public column is another
        // partition.
        assert_eq!(
            txn.row(Partition::Scoped(PartitionId::from_bytes([0; 32])), &key(7))?,
            None
        );

        assert!(txn.touch(scoped(1), &key(7), UnixTime(1))?);
        txn.disable(scoped(1), &key(7), UnixTime(500))?;
        txn.bump_partition_epoch(scoped(1))?;
        assert!(txn.delete_row(scoped(1), &key(7))?);

        assert_eq!(txn.row(scoped(1), &key(7))?, None);
        assert_eq!(
            txn.row(Partition::Public, &key(7))?.as_ref(),
            Some(&rows[0])
        );
        assert_eq!(txn.row(scoped(2), &key(7))?.as_ref(), Some(&rows[2]));
        assert_eq!(txn.disabled_until(Partition::Public, &key(7))?, None);
        assert_eq!(txn.disabled_until(scoped(2), &key(7))?, None);
        assert_eq!(txn.partition_epoch(Partition::Public)?, Epoch(0));
        assert_eq!(txn.partition_epoch(scoped(2))?, Epoch(0));

        assert_eq!(txn.delete_rows_of_partition(scoped(2))?, 1);
        assert_eq!(txn.delete_disabled_of_partition(scoped(2))?, 0);
        assert_eq!(txn.disabled_until(scoped(1), &key(7))?, Some(UnixTime(500)));
        assert_eq!(txn.rows()?, vec![rows[0].clone()]);
        Ok(())
    })
    .unwrap();
}

#[test]
fn meta_partitions_column_layout() {
    let dir = TestDir::new("partitions_layout");
    let mut db = open(&dir);
    put(&mut db, &row(Partition::Public, 1, 1));
    put(&mut db, &row(scoped(0xC3), 1, 2));
    let raw = dir.raw();
    let mut stmt = raw
        .prepare("SELECT \"partition\" FROM entry ORDER BY version")
        .unwrap();
    let columns: Vec<Vec<u8>> = stmt
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let mut scoped_column = vec![0x01];
    scoped_column.extend_from_slice(&[0xC3; 32]);
    assert_eq!(columns, vec![vec![0x00], scoped_column]);
}

// ---------------------------------------------------------------- reset

/// Puts files where a store keeps them.
fn store_files(dir: &TestDir) {
    fs::create_dir_all(dir.path().join("files/ab")).unwrap();
    fs::write(dir.path().join("files/ab/abcd.rqf"), b"sealed").unwrap();
    fs::create_dir_all(dir.path().join("tmp")).unwrap();
    fs::write(dir.path().join("tmp/ef.tmp"), b"partial").unwrap();
}

fn assert_random_start(db: &mut MetaDb) {
    db.transaction(|txn| {
        assert_eq!(txn.rows()?, Vec::new());
        assert_eq!(txn.site_epoch()?, Epoch(counting_value(1)));
        assert_eq!(txn.partition_epoch(scoped(1))?, Epoch(counting_value(2)));
        assert_eq!(
            txn.partition_epoch(Partition::Public)?,
            Epoch(counting_value(2))
        );
        assert_eq!(txn.disabled_until(scoped(1), &key(1))?, None);
        Ok(())
    })
    .unwrap();
}

#[test]
fn meta_reset_not_a_database_starts_empty_with_random_counters() {
    let dir = TestDir::new("reset_not_db");
    fs::write(dir.db_path(), vec![0x5A; 8192]).unwrap();
    fs::write(dir.path().join(format!("{DB_FILE}-wal")), vec![0x11; 100]).unwrap();
    store_files(&dir);
    fs::write(dir.path().join("unknown"), b"x").unwrap();

    let mut random = Counting::new();
    let (mut db, opened) = open_or_reset(&dir, &mut random).unwrap();
    assert_eq!(opened, Opened::Reset);
    assert_eq!(random.calls, 3);
    assert_random_start(&mut db);
    let version = db.transaction(|txn| txn.next_version()).unwrap();
    assert_eq!(version, EntryVersion(counting_value(0) + 1));
    for name in dir.names() {
        assert!(name.starts_with(DB_FILE), "left after the reset: {name}");
    }
    drop(db);

    // The new store is a normal store: a plain open keeps its counters.
    let mut db = open(&dir);
    assert_random_start(&mut db);
    assert_eq!(
        db.transaction(|txn| txn.next_version()).unwrap(),
        EntryVersion(counting_value(0) + 2)
    );
}

#[test]
fn meta_reset_malformed_database() {
    let dir = TestDir::new("reset_malformed");
    filled(&dir, 60);
    damage_page(&dir, 2);
    store_files(&dir);
    let mut random = Counting::new();
    let (mut db, opened) = open_or_reset(&dir, &mut random).unwrap();
    assert_eq!(opened, Opened::Reset);
    assert_random_start(&mut db);
    assert!(!dir.path().join("files").exists());
    assert!(!dir.path().join("tmp").exists());
}

#[test]
fn meta_reset_values_use_62_bits() {
    let dir = TestDir::new("reset_62_bits");
    fs::write(dir.db_path(), vec![0x5A; 8192]).unwrap();
    let mut fill = |buf: &mut [u8]| {
        buf.fill(0xFF);
        Ok(())
    };
    let (mut db, _) = MetaDb::open_or_reset(dir.path(), &mut fill).unwrap();
    let top = (1_u64 << 62) - 1;
    db.transaction(|txn| {
        assert_eq!(txn.site_epoch()?, Epoch(top));
        assert_eq!(txn.partition_epoch(scoped(1))?, Epoch(top));
        assert_eq!(txn.bump_site_epoch()?, Epoch(top + 1));
        assert_eq!(txn.bump_partition_epoch(scoped(1))?, Epoch(top + 1));
        assert_eq!(txn.next_version()?, EntryVersion(top + 1));
        Ok(())
    })
    .unwrap();
}

#[test]
fn meta_reset_keeps_a_good_database() {
    let dir = TestDir::new("reset_keeps");
    let mut db = open(&dir);
    put(&mut db, &row(scoped(1), 1, 1));
    db.transaction(|txn| txn.bump_site_epoch().map(drop))
        .unwrap();
    drop(db);
    store_files(&dir);
    let mut random = Counting::new();
    let (mut db, opened) = open_or_reset(&dir, &mut random).unwrap();
    assert_eq!(opened, Opened::Kept);
    assert_eq!(random.calls, 0);
    assert!(read(&mut db, scoped(1), 1).unwrap().is_some());
    assert_eq!(db.transaction(|txn| txn.site_epoch()).unwrap(), Epoch(1));
    assert!(dir.path().join("files/ab/abcd.rqf").exists());
}

#[test]
fn meta_reset_random_failure_changes_nothing() {
    let dir = TestDir::new("reset_random_fails");
    let garbage = vec![0x5A; 8192];
    fs::write(dir.db_path(), &garbage).unwrap();
    store_files(&dir);
    let before = dir.names();
    let refused = StoreError::new(StoreErrorKind::Unavailable, "key port");
    let mut calls = 0;
    let mut failing = |_: &mut [u8]| {
        calls += 1;
        Err(refused)
    };
    let result = MetaDb::open_or_reset(dir.path(), &mut failing);
    assert_eq!(result.map(drop), Err(refused));
    assert_eq!(calls, 1);
    assert_eq!(dir.names(), before);
    assert_eq!(fs::read(dir.db_path()).unwrap(), garbage);
    assert!(dir.path().join("files/ab/abcd.rqf").exists());
}

#[test]
fn meta_reset_never_on_a_busy_database() {
    let dir = TestDir::new("reset_busy");
    let mut db = open(&dir);
    put(&mut db, &row(scoped(1), 1, 1));
    drop(db);
    let lock = hold_write_lock(&dir);
    let mut random = Counting::new();
    assert_eq!(
        kind(open_or_reset(&dir, &mut random)),
        StoreErrorKind::Unavailable
    );
    assert_eq!(random.calls, 0);
    drop(lock);
    let mut db = open(&dir);
    assert!(read(&mut db, scoped(1), 1).unwrap().is_some());
}

#[test]
fn meta_reset_that_did_not_finish_runs_again() {
    let dir = TestDir::new("reset_again");
    let mut db = open(&dir);
    put(&mut db, &row(scoped(1), 1, 1));
    drop(db);
    // A reset that stopped after its marker: the old data must not come
    // back with counters that a writer could know.
    fs::write(dir.path().join(RESET_MARKER), b"").unwrap();
    let mut random = Counting::new();
    let (mut db, opened) = open_or_reset(&dir, &mut random).unwrap();
    assert_eq!(opened, Opened::Reset);
    assert_random_start(&mut db);
    assert!(!dir.path().join(RESET_MARKER).exists());
}

#[cfg(unix)]
#[test]
fn meta_reset_removes_a_link_and_not_its_target() {
    let dir = TestDir::new("reset_link");
    let outside = TestDir::new("reset_link_target");
    fs::write(outside.path().join("keep"), b"keep").unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("files")).unwrap();
    fs::write(dir.db_path(), vec![0x5A; 8192]).unwrap();
    let mut random = Counting::new();
    let (_db, opened) = open_or_reset(&dir, &mut random).unwrap();
    assert_eq!(opened, Opened::Reset);
    assert!(fs::symlink_metadata(dir.path().join("files")).is_err());
    assert!(outside.path().join("keep").exists());
}
