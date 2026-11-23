// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Owner: WP-07 S2 T-05: the commit procedure, recovery and the driver (ADR-004 sections 2 and 4).
//!
//! [`FileStore`] implements [`StorageDriver`] on real files and the SQLite
//! metadata of [`crate::meta`], and seals every entry with
//! [`crate::crypto`] (ADR-011, H-151).
//!
//! # Layout (ADR-004 section 1)
//!
//! ```text
//! <root>/schema-1/meta.sqlite            metadata (crate::meta)
//! <root>/schema-1/files/<hh>/<id>.rqf    one sealed file for each entry version
//! <root>/schema-1/tmp/<id>.tmp           files that are not promoted yet
//! ```
//!
//! `<id>` is a [`FileId`](crate::crypto::FileId) (16 random bytes from the key port), `<hh>` its
//! first byte. A file name holds no partition, no key and no host.
//!
//! # Invariants
//!
//! - INV-14: a commit is old or new. Only `Replace` writes a file; the file
//!   has a new random name, is synced before it is renamed into `files/`,
//!   and only the metadata transaction of `CommitMeta` (the commit point)
//!   makes a row name it. A file is never written twice.
//! - The checks of a keyed kind run inside that transaction, in the order
//!   of `rivqen_cache::txn` (DL-200, DL-205, DL-208): request partition
//!   epoch, target partition epoch, site epoch, host (`Replace`), entry. A
//!   refused transaction removes its promoted file and commits no row.
//! - RSK-06: a row and a file open only with the key of their partition and
//!   the associated data of their partition, key and file id. A read checks
//!   the address of the row against the request. The host and the store
//!   time are sealed in the row too: every open of a row (read, list,
//!   update) compares them with the clear columns `host` and `stored_at`.
//!   `version` and `last_used_at` are not authenticated; `content_bytes`
//!   is compared with the decoded file at a read.
//! - Every entry that the driver stores decodes with its [`DecodeLimits`]:
//!   a `Replace` whose content or row would not decode, and an
//!   `UpdateFields` whose row would not, writes nothing.
//! - R-CACHE-04: `RevokePartition` deletes the key of the partition before
//!   its metadata transaction; `ClearSite` of all sites deletes the key of
//!   `public` (ADR-011 section 4).
//! - R-CACHE-06: an entry that fails a check is removed with its file; the
//!   other entries stay. A failing key port removes nothing at a read, a
//!   list or an update.
//! - No path, host, key, file id or content is in an error or in a `Debug`
//!   text.
//!
//! # Failure modes
//!
//! | Failure | Result |
//! |---|---|
//! | I/O error, full disk, SQLite error, key port error before the commit point | `Unavailable`; the new file is removed and the store holds the state before the call |
//! | An error of the metadata transaction of `CommitMeta` | `Unavailable`; the promoted file stays, as SQLite may replay the commit at the next open; recovery removes it when no row names it |
//! | A failure at `RemoveOld` | The commit stands; recovery removes the old files (DL-200 h) |
//! | A content or a row that would not decode with the limits, a row that seals to more than `meta::MAX_SEALED_BYTES` | `Unavailable` before the first step (`UpdateFields`: inside its transaction); nothing is written (DL-211) |
//! | The key port does not delete the key of a `RevokePartition` or a `ClearSite` of all sites | The metadata transaction and `RemoveOld` still run (rows gone, epoch incremented); then `Unavailable`, so the caller runs it again: a second run is a normal one and deletes the key |
//! | The key is deleted, then the metadata transaction fails | `Unavailable`; the caller must run the revoke again. The old rows stay until then, but they cannot be opened: a read or a list gives `Corrupted` and removes them |
//! | A row or a file that does not open or decode, a host or a store time under the seal that differs from its clear column, a missing file, a file length that no file of `content_bytes` has | `Corrupted` at a read; the entry is removed. `list` removes it and does not list it; `UpdateFields` removes it and gives `Refused(Entry)` |
//! | A file that cannot be unlinked after its row is removed | Not an error: the call gives its own result; the file is an orphan for the next recovery |
//! | A database that is not a database, fails its check, or holds an object that is not ours | Reset at open (`MetaDb::open_or_reset`); every file goes with it |
//! | A missing database in a schema directory that holds files | Reset at open: the counters start at random values, not at 0 |
//! | A process kill or a power loss at any point | Recovery gives the old or the new entry |
//!
//! # Resource limits
//!
//! - A read allocates at most the length of one file, and only when that
//!   length is one that a file of the `content_bytes` of its row can have
//!   under the [`DecodeLimits`] of the store.
//! - A `sealed` column is at most `meta::MAX_SEALED_BYTES`.
//! - `list` and recovery read every row once; the number of rows is bounded
//!   by the quota of the session (WP-08).
//! - Every loop runs over a directory listing or the rows of one
//!   transaction.
//!
//! # Syncs (ADR-004 section 2)
//!
//! | Where | Sync |
//! |---|---|
//! | `SyncTemp` | `sync_all` on the temporary file |
//! | `Promote` | `sync_all` on `files/` when `files/<hh>` is new; after the rename, on `files/<hh>` and on `tmp` |
//! | `CommitMeta` | SQLite (`synchronous=FULL`, `fullfsync`; [`crate::meta`]) |
//! | Open and recovery | `sync_all` on the parent of each directory that the driver creates (mode 0700 on unix) |
//!
//! `RemoveOld` and the removals of recovery are not synced: a file that
//! comes back is an orphan that the next recovery removes.
//!
//! # Fault points
//!
//! `FileStore::stop_after` and `FileStore::fail_at` stop or fail the
//! next `apply` at one step, for the driver contract suite and the kill
//! tests (DL-181). They and `FileStore::restart` exist only with the
//! feature `test-support` (H-154): a build without it cannot stop a commit. A stop returns at once, with no cleanup, as if the
//! process died there; the kill test kills the process right after it. No
//! code of this crate kills a process.

mod commit;
mod entry;
mod layout;
mod recover;

use std::fmt;
use std::path::Path;

use rivqen_cache::driver::{
    EntrySummary, RecoveryReport, StorageDriver, StoreError, StoreErrorKind,
};
use rivqen_cache::key::Partition;
use rivqen_cache::txn::{CacheTransaction, CommitStep, Epoch, StoredEntry, TxnOutcome};
use rivqen_model::ids::CacheKey;
use rivqen_model::time::UnixTime;

pub use self::layout::{
    FILE_EXT, FILES_DIR, SCHEMA_PREFIX, TEMP_EXT, TMP_DIR, entry_file_path, schema_dir,
};

use self::entry::FileShape;
use self::layout::{Layout, ensure_dir};
use crate::crypto::{DecodeLimits, KeyProvider};
use crate::meta::MetaDb;

const OP_OPEN: &str = "driver.open";

/// The state of a driver (the trait: `recover` runs before any other call).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Life {
    /// Opened; only `recover` is answered.
    NeedsRecover,
    /// Recovered.
    Running,
    /// Stopped at a fault point; nothing is answered until
    /// `FileStore::restart`.
    Stopped,
}

/// The reference storage driver: sealed entry files and SQLite metadata in
/// one directory (ADR-004, ADR-011).
///
/// One driver owns its directory; a second driver on the same directory is
/// not supported (ADR-004 section 2). [`fmt::Debug`] writes the state only.
pub struct FileStore<P: KeyProvider> {
    layout: Layout,
    keys: P,
    db: Option<MetaDb>,
    life: Life,
    limits: DecodeLimits,
    shape: FileShape,
    stop: Option<CommitStep>,
    fail: Option<CommitStep>,
}

impl<P: KeyProvider> fmt::Debug for FileStore<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileStore")
            .field("life", &self.life)
            .field("open", &self.db.is_some())
            .finish_non_exhaustive()
    }
}

impl<P: KeyProvider> FileStore<P> {
    /// Opens the store at `root` with the decode limits of the default
    /// resource limits. See [`FileStore::open_with_limits`].
    ///
    /// # Errors
    ///
    /// As [`FileStore::open_with_limits`].
    pub fn open(root: &Path, keys: P) -> Result<Self, StoreError> {
        Self::open_with_limits(root, keys, DecodeLimits::default())
    }

    /// Opens the store at `root` with the key port `keys`. Creates `root`
    /// (its parent must exist), the schema directory, `files/` and `tmp/`
    /// when they do not exist (mode 0700 on unix), and syncs the parent of
    /// each. A metadata database that is not a database or fails its check,
    /// and a missing database in a schema directory that holds files, is
    /// replaced by an empty store whose counters start at random values
    /// from the key port (ADR-004 section 3); every file goes with it.
    ///
    /// `limits` bound every decode of a row and a file; use the limits of
    /// the core configuration. Call [`StorageDriver::recover`] next.
    ///
    /// # Errors
    ///
    /// `Unavailable` when a directory or the database cannot be opened or
    /// created, or when the key port gives no random bytes for a reset.
    pub fn open_with_limits(
        root: &Path,
        keys: P,
        limits: DecodeLimits,
    ) -> Result<Self, StoreError> {
        let mut store = Self {
            layout: Layout::new(root),
            keys,
            db: None,
            life: Life::NeedsRecover,
            shape: FileShape::new(&limits),
            limits,
            stop: None,
            fail: None,
        };
        store.db = Some(store.open_db()?);
        Ok(store)
    }

    /// The key port.
    pub fn keys_mut(&mut self) -> &mut P {
        &mut self.keys
    }

    /// Closes the store and gives the key port back.
    pub fn into_keys(self) -> P {
        self.keys
    }

    /// Closes the database and opens the directory again with the same key
    /// port, as a new process would. The next call must be
    /// [`StorageDriver::recover`]. Clears the fault points.
    ///
    /// # Errors
    ///
    /// As [`FileStore::open_with_limits`]; the store then answers nothing
    /// until a `restart` succeeds.
    #[cfg(feature = "test-support")]
    pub fn restart(&mut self) -> Result<(), StoreError> {
        self.db = None;
        self.life = Life::NeedsRecover;
        self.stop = None;
        self.fail = None;
        self.db = Some(self.open_db()?);
        Ok(())
    }

    /// Fault point: the next [`StorageDriver::apply`] stops right after
    /// `step`, with no cleanup, and returns `Unavailable`; every later call
    /// gives `Unavailable` until [`FileStore::restart`]. For fault tests
    /// only (DL-181).
    #[cfg(feature = "test-support")]
    pub fn stop_after(&mut self, step: CommitStep) {
        self.stop = Some(step);
    }

    /// Fault point: the next [`StorageDriver::apply`] fails at `step` as an
    /// I/O error before the step would. Up to `CommitMeta` it removes its
    /// new file and returns `Unavailable` (at `CommitMeta` no metadata
    /// transaction and no key deletion ran); at `RemoveOld` the commit
    /// stands and the call succeeds. For fault tests only (DL-181).
    #[cfg(feature = "test-support")]
    pub fn fail_at(&mut self, step: CommitStep) {
        self.fail = Some(step);
    }

    /// The directories and the metadata database (see
    /// [`FileStore::open_with_limits`]).
    fn open_db(&mut self) -> Result<MetaDb, StoreError> {
        ensure_dir(&self.layout.root, OP_OPEN)?;
        ensure_dir(&self.layout.schema, OP_OPEN)?;
        let keys = &mut self.keys;
        let (db, _) = MetaDb::open_or_reset(&self.layout.schema, &mut |buf| keys.random(buf))
            .map_err(as_unavailable)?;
        // After the database: a reset removes the content of the schema
        // directory, `files/` and `tmp/` too.
        ensure_dir(&self.layout.files, OP_OPEN)?;
        ensure_dir(&self.layout.tmp, OP_OPEN)?;
        Ok(db)
    }

    /// The database, when the store is open.
    fn db_mut(&mut self, op: &'static str) -> Result<&mut MetaDb, StoreError> {
        self.db.as_mut().ok_or_else(|| unavailable(op))
    }

    /// Fails unless the store was recovered and is not stopped.
    fn ready(&self, op: &'static str) -> Result<(), StoreError> {
        match self.life {
            Life::Running if self.db.is_some() => Ok(()),
            Life::Running | Life::NeedsRecover | Life::Stopped => Err(unavailable(op)),
        }
    }

    /// One metadata transaction of a call that only needs the database.
    fn meta<T>(
        &mut self,
        op: &'static str,
        f: impl FnOnce(&mut crate::meta::MetaTxn<'_>) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        self.ready(op)?;
        self.db_mut(op)?.transaction(f).map_err(as_unavailable)
    }
}

impl<P: KeyProvider> StorageDriver for FileStore<P> {
    fn recover(&mut self, now: UnixTime) -> Result<RecoveryReport, StoreError> {
        if self.life == Life::Stopped || self.db.is_none() {
            return Err(unavailable("driver.recover"));
        }
        self.recover_store(now)
    }

    fn read(
        &mut self,
        partition: Partition,
        key: &CacheKey,
        now: UnixTime,
    ) -> Result<Option<StoredEntry>, StoreError> {
        self.ready("driver.read")?;
        self.read_entry(partition, key, now)
    }

    fn apply(&mut self, txn: CacheTransaction) -> Result<TxnOutcome, StoreError> {
        self.ready("driver.apply")?;
        let stop = self.stop.take();
        let fail = self.fail.take();
        self.commit(txn, stop, fail)
    }

    fn partition_epoch(&mut self, partition: Partition) -> Result<Epoch, StoreError> {
        self.meta("driver.partition_epoch", |txn| {
            txn.partition_epoch(partition)
        })
    }

    fn site_epoch(&mut self) -> Result<Epoch, StoreError> {
        self.meta("driver.site_epoch", |txn| txn.site_epoch())
    }

    fn key_disabled(
        &mut self,
        partition: Partition,
        key: &CacheKey,
        now: UnixTime,
    ) -> Result<bool, StoreError> {
        self.meta("driver.key_disabled", |txn| {
            match txn.disabled_until(partition, key) {
                Ok(until) => Ok(until.is_some_and(|until| now < until)),
                // A record that does not decode disables the key (DL-209).
                Err(error) if error.kind() == StoreErrorKind::Corrupted => Ok(true),
                Err(error) => Err(error),
            }
        })
    }

    fn list(&mut self) -> Result<Vec<EntrySummary>, StoreError> {
        self.ready("driver.list")?;
        self.list_entries()
    }

    fn touch(
        &mut self,
        partition: Partition,
        key: &CacheKey,
        now: UnixTime,
    ) -> Result<(), StoreError> {
        // A touch without an entry changes nothing; the answer is not
        // needed.
        self.meta("driver.touch", |txn| {
            txn.touch(partition, key, now).map(|_| ())
        })
    }
}

/// An `Unavailable` error of the operation `op`.
const fn unavailable(op: &'static str) -> StoreError {
    StoreError::new(StoreErrorKind::Unavailable, op)
}

/// A `Corrupted` error of the operation `op`.
const fn corrupted(op: &'static str) -> StoreError {
    StoreError::new(StoreErrorKind::Corrupted, op)
}

/// `error` as `Unavailable`, with its operation: a call whose contract
/// knows no `Corrupted` (a write, an epoch) gives this.
const fn as_unavailable(error: StoreError) -> StoreError {
    unavailable(error.op())
}
