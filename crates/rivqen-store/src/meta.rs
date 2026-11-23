// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Owner: WP-07 S2 T-04: the SQLite metadata (ADR-004 section 3).
//!
//! [`MetaDb`] opens `meta.sqlite` in the schema directory of the store and
//! gives the driver one transaction type, [`MetaTxn`], with every read and
//! write that the commit and the recovery need. The driver owns the files;
//! this module touches files only for a reset.
//!
//! # Settings
//!
//! `journal_mode=WAL`, `synchronous=FULL`, `fullfsync=ON`,
//! `checkpoint_fullfsync=ON`, `foreign_keys=ON`, a busy timeout of 0 (one
//! writer; a lock held by another connection fails at once),
//! `trusted_schema=OFF` and `cell_size_check=ON` (more corruption is found
//! as an error). [`MetaDb::open`] sets them, reads them back and refuses a
//! difference with [`StoreErrorKind::Unavailable`]. Every transaction is
//! `BEGIN IMMEDIATE`.
//!
//! `fullfsync` and `checkpoint_fullfsync`: on Apple targets SQLite syncs
//! with `fcntl(F_FULLFSYNC)` only when these are on; without them
//! `synchronous=FULL` is a plain `fsync()`, which does not flush the drive
//! cache, and a revoke could be lost at a power cut. Both are flag pragmas
//! of SQLite on every platform (`PragTyp_FLAG` in `sqlite3.c`): the read-back
//! is the flag bit, 1 after it is set, also on Linux and Android. There the
//! flag changes nothing: the unix VFS ignores it when `F_FULLFSYNC` is not
//! defined (`HAVE_FULLFSYNC` is 0) and keeps `fsync()` or `fdatasync()`.
//!
//! # Tables
//!
//! | Table | Primary key | Columns |
//! |---|---|---|
//! | `entry` | `partition`, `key` | `host`, `version`, `file_id`, `content_bytes`, `stored_at`, `last_used_at`, `sealed` |
//! | `partition_epoch` | `partition` | `epoch` |
//! | `disabled_key` | `partition`, `key` | `until` |
//! | `counter` | `name` | `value`: `version`, `site_epoch`, `partition_epoch_base` |
//!
//! The partition column is `00` for `public` and `01` followed by the 32 id
//! bytes for a scoped partition. Every statement that names an entry, a
//! disabled key or an epoch has the partition in its key (RSK-06).
//! `partition_epoch_base` is the epoch of a partition that has no epoch
//! row. `PRAGMA user_version` is [`META_FORMAT`].
//!
//! At open, `sqlite_schema` must hold exactly the four tables, with the
//! text of their `CREATE TABLE` statements, and the automatic index of
//! each primary key: a trigger, a view, another index or another table is
//! `Corrupted`, so a reset follows. An epoch write and a counter write
//! must change exactly one row, else the transaction gives `Unavailable`
//! and commits nothing.
//!
//! # Files
//!
//! On unix, `meta.sqlite` is created with mode 0600 before SQLite opens it
//! (`create_new`; SQLite gives `-wal` and `-shm` the mode of the database
//! file), and the reset marker is created with `create_new` and mode 0600.
//! The driver creates the directories with mode 0700.
//!
//! # Integers
//!
//! SQLite stores a signed 64-bit integer. The rules:
//!
//! - A version, an epoch, a counter and a size above `i64::MAX` is refused
//!   with [`StoreErrorKind::Unavailable`] and nothing is written. An
//!   increment that would pass `i64::MAX` is refused the same way.
//! - A time above `i64::MAX` is written as `i64::MAX`. The map keeps the
//!   order of all times below that value, so `until < now` in SQL gives
//!   the same answer as for the `u64` values.
//! - A stored value that is negative or not an integer does not decode:
//!   [`StoreErrorKind::Corrupted`].
//!
//! # Errors
//!
//! Every SQLite or file error is [`StoreErrorKind::Unavailable`]. A stored
//! value that does not decode is [`StoreErrorKind::Corrupted`]. At open,
//! these are `Corrupted` too: a file that is not a database, a database that
//! SQLite finds malformed or that fails `PRAGMA quick_check`, an unknown
//! `user_version`, a missing counter, tables of another shape, and a reset
//! that did not finish. A busy lock or an I/O error is never `Corrupted`, so
//! it never causes a reset. No error holds SQL text, a path or a value.
//!
//! # Reset
//!
//! [`MetaDb::open_or_reset`] replaces a `Corrupted` database by an empty one
//! (ADR-004 section 3). The new store starts the version counter, the site
//! epoch and the base of the partition epochs at random 62-bit values, so a
//! writer that holds an epoch of the old store is refused (S1 security
//! review R2). Values of a database that failed its check are not trusted,
//! so no counter is kept. The steps:
//!
//! 1. Take the three random values. A failure of the source changes
//!    nothing.
//! 2. Create the marker file `meta.reset` and sync it and the directory. A
//!    marker that exists is kept when it is a regular file; any other
//!    entry with its name (a link, a directory) is `Unavailable`.
//! 3. Remove every other entry of the schema directory: the database with
//!    its `-wal` and `-shm` files, `files/`, `tmp/`.
//! 4. Create the new database with the random counters in one transaction.
//! 5. Remove the marker and sync the directory.
//!
//! While an entry named `meta.reset` exists, [`MetaDb::open`] gives
//! `Corrupted`, so a reset that stops at any step runs again with new
//! random values. It never leaves a store whose counters start at 0.
//!
//! [`MetaDb::open_or_reset`] also resets when the database has no tables
//! (or does not exist) and the schema directory is not fresh: it holds an
//! entry file, a temporary file or any other entry. Only an empty
//! directory, or one with `meta.sqlite`, an empty `tmp/` and empty
//! directories in `files/`, gets counters at 0.

mod codec;
mod txn;

use std::fmt;
use std::fs;
use std::io;
use std::path::Path;
use std::time::Duration;

use rivqen_cache::driver::{StoreError, StoreErrorKind};
use rivqen_model::time::UnixTime;
use rusqlite::types::ValueRef;
use rusqlite::{Connection, ErrorCode, OpenFlags, TransactionBehavior, params};

use self::codec::{corrupted, decode_u64, sql_u64, unavailable};
pub use self::txn::{FILE_ID_LEN, MetaTxn, Row};
use crate::driver::{FILES_DIR, TMP_DIR};

/// The name of the database file in the schema directory.
pub const DB_FILE: &str = "meta.sqlite";

/// The name of the marker of a reset that did not finish.
pub const RESET_MARKER: &str = "meta.reset";

/// The format of the tables: the value of `PRAGMA user_version`.
pub const META_FORMAT: i64 = 1;

/// The longest host that a row holds: the longest DNS name.
pub const MAX_HOST_BYTES: usize = 253;

/// The longest `sealed` value that a row holds (1 MiB). A longer value is
/// refused on write and does not decode on read; SQLite never copies it.
pub const MAX_SEALED_BYTES: usize = 1 << 20;

/// The counter of the last entry version.
const COUNTER_VERSION: &str = "version";
/// The counter of the site epoch.
const COUNTER_SITE_EPOCH: &str = "site_epoch";
/// The epoch of a partition with no epoch row.
const COUNTER_PARTITION_EPOCH_BASE: &str = "partition_epoch_base";
/// Every counter, in the order of [`Counters`].
const COUNTERS: [&str; 3] = [
    COUNTER_VERSION,
    COUNTER_SITE_EPOCH,
    COUNTER_PARTITION_EPOCH_BASE,
];

/// The low 62 bits: the range of a random start value. It leaves room for
/// 2^62 increments below `i64::MAX`.
const RANDOM_MASK: u64 = 0x3FFF_FFFF_FFFF_FFFF;

/// The tables of ADR-004 section 3. SQLite keeps the text of each
/// statement in `sqlite_schema`; the schema check compares it.
const TABLE_ENTRY: &str = r#"CREATE TABLE entry (
    "partition" BLOB NOT NULL,
    "key" BLOB NOT NULL,
    host TEXT NOT NULL,
    version INTEGER NOT NULL,
    file_id BLOB NOT NULL,
    content_bytes INTEGER NOT NULL,
    stored_at INTEGER NOT NULL,
    last_used_at INTEGER NOT NULL,
    sealed BLOB NOT NULL,
    PRIMARY KEY ("partition", "key")
)"#;
const TABLE_PARTITION_EPOCH: &str = r#"CREATE TABLE partition_epoch (
    "partition" BLOB NOT NULL PRIMARY KEY,
    epoch INTEGER NOT NULL
)"#;
const TABLE_DISABLED_KEY: &str = r#"CREATE TABLE disabled_key (
    "partition" BLOB NOT NULL,
    "key" BLOB NOT NULL,
    until INTEGER NOT NULL,
    PRIMARY KEY ("partition", "key")
)"#;
const TABLE_COUNTER: &str = "CREATE TABLE counter (
    name TEXT NOT NULL PRIMARY KEY,
    value INTEGER NOT NULL
)";
const TABLES: [&str; 4] = [
    TABLE_ENTRY,
    TABLE_PARTITION_EPOCH,
    TABLE_DISABLED_KEY,
    TABLE_COUNTER,
];

/// The exact objects of `sqlite_schema`: `type`, `name`, `tbl_name` and
/// `sql`. Each table has the automatic index of its primary key (a key
/// that is not `INTEGER PRIMARY KEY`), whose `sql` is `NULL`. Any other
/// object (a trigger, a view, another index or table) is not ours.
const SCHEMA_OBJECTS: [(&str, &str, &str, Option<&str>); 8] = [
    ("table", "entry", "entry", Some(TABLE_ENTRY)),
    ("index", "sqlite_autoindex_entry_1", "entry", None),
    (
        "table",
        "partition_epoch",
        "partition_epoch",
        Some(TABLE_PARTITION_EPOCH),
    ),
    (
        "index",
        "sqlite_autoindex_partition_epoch_1",
        "partition_epoch",
        None,
    ),
    (
        "table",
        "disabled_key",
        "disabled_key",
        Some(TABLE_DISABLED_KEY),
    ),
    (
        "index",
        "sqlite_autoindex_disabled_key_1",
        "disabled_key",
        None,
    ),
    ("table", "counter", "counter", Some(TABLE_COUNTER)),
    ("index", "sqlite_autoindex_counter_1", "counter", None),
];

const INSERT_COUNTER: &str = "INSERT INTO counter (name, value) VALUES (?1, ?2)";
const SELECT_COUNTER: &str = "SELECT value FROM counter WHERE name = ?1";
const COUNT_SCHEMA_OBJECTS: &str = "SELECT count(*) FROM sqlite_schema";
const SELECT_SCHEMA_OBJECTS: &str = "SELECT type, name, tbl_name, sql FROM sqlite_schema";

const OP_OPEN: &str = "meta.open";
const OP_RESET: &str = "meta.reset";

/// The start values of the counters of a new database, in the order of
/// [`COUNTERS`].
type Counters = [u64; 3];

/// The effective settings of the connection (ADR-004 section 3).
#[allow(clippy::struct_excessive_bools)] // one flag per on/off pragma
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// `journal_mode` is `wal`.
    pub journal_mode_wal: bool,
    /// `synchronous` is `FULL`.
    pub synchronous_full: bool,
    /// `foreign_keys` is on.
    pub foreign_keys: bool,
    /// The busy timeout in milliseconds.
    pub busy_timeout_ms: u64,
    /// `fullfsync` is on: a commit syncs with `F_FULLFSYNC` on Apple
    /// targets.
    pub fullfsync: bool,
    /// `checkpoint_fullfsync` is on: a checkpoint syncs with `F_FULLFSYNC`
    /// on Apple targets.
    pub checkpoint_fullfsync: bool,
}

impl Settings {
    /// The settings that [`MetaDb::open`] requires.
    pub const REQUIRED: Self = Self {
        journal_mode_wal: true,
        synchronous_full: true,
        foreign_keys: true,
        busy_timeout_ms: 0,
        fullfsync: true,
        checkpoint_fullfsync: true,
    };
}

/// How [`MetaDb::open_or_reset`] opened the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opened {
    /// The database was opened or created; its data is kept.
    Kept,
    /// The database was `Corrupted`; the schema directory was emptied and
    /// the counters start at random values.
    Reset,
}

/// The metadata database of one store.
pub struct MetaDb {
    conn: Connection,
}

impl fmt::Debug for MetaDb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MetaDb { .. }")
    }
}

impl MetaDb {
    /// Opens `dir/meta.sqlite`, or creates it with every counter at 0. The
    /// directory `dir` must exist; the driver creates it and syncs its
    /// parent.
    ///
    /// # Errors
    ///
    /// `Corrupted` for the cases of the module documentation (the caller
    /// may use [`Self::open_or_reset`]); `Unavailable` for every other
    /// failure, also a lock that another connection holds.
    pub fn open(dir: &Path) -> Result<Self, StoreError> {
        Self::open_checked(dir, true)
    }

    /// [`Self::open`] with one more rule; when that gives `Corrupted`,
    /// removes the content of `dir` and starts an empty store whose
    /// counters start at random 62-bit values from `random` (see "Reset"
    /// in the module documentation). `dir` must be the schema directory of
    /// the store.
    ///
    /// The rule: a database with no tables (also a `meta.sqlite` that does
    /// not exist) gets counters at 0 only in a fresh schema directory, one
    /// that holds nothing else than `meta.sqlite`, an empty `tmp/` and a
    /// `files/` with only empty directories. In any other directory a lost
    /// database is `Corrupted` and takes the reset: its files could be of a
    /// store whose epochs a writer holds (S2 security review L1).
    ///
    /// # Errors
    ///
    /// `Unavailable` when the database cannot be opened, when `random`
    /// fails (the error of `random` is given back) or when a step of the
    /// reset fails; a later call runs the reset again.
    pub fn open_or_reset(
        dir: &Path,
        random: &mut dyn FnMut(&mut [u8]) -> Result<(), StoreError>,
    ) -> Result<(Self, Opened), StoreError> {
        let opened = fresh_dir(dir).and_then(|fresh| Self::open_checked(dir, fresh));
        match opened {
            Ok(db) => Ok((db, Opened::Kept)),
            Err(error) if error.kind() == StoreErrorKind::Corrupted => {
                Self::reset(dir, random).map(|db| (db, Opened::Reset))
            }
            Err(error) => Err(error),
        }
    }

    /// Runs `f` in one `BEGIN IMMEDIATE` transaction. The transaction
    /// commits when `f` returns `Ok` and rolls back when `f` returns `Err`
    /// or panics.
    ///
    /// # Errors
    ///
    /// The error of `f`; `Unavailable` when the transaction cannot begin
    /// (also when another connection holds the write lock: the busy
    /// timeout is 0), commit or roll back. After a failed commit nothing of
    /// the transaction is in the database.
    pub fn transaction<T, F>(&mut self, f: F) -> Result<T, StoreError>
    where
        F: FnOnce(&mut MetaTxn<'_>) -> Result<T, StoreError>,
    {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| unavailable("meta.begin"))?;
        let mut txn = MetaTxn::new(tx);
        match f(&mut txn) {
            Ok(value) => {
                txn.into_inner()
                    .commit()
                    .map_err(|_| unavailable("meta.commit"))?;
                Ok(value)
            }
            Err(error) => {
                txn.into_inner()
                    .rollback()
                    .map_err(|_| unavailable("meta.rollback"))?;
                Err(error)
            }
        }
    }

    /// The effective settings of the connection.
    ///
    /// # Errors
    ///
    /// `Unavailable` when SQLite fails.
    pub fn settings(&self) -> Result<Settings, StoreError> {
        const OP: &str = "meta.settings";
        let mode: String = self
            .conn
            .query_row("PRAGMA journal_mode", params![], |row| row.get(0))
            .map_err(|_| unavailable(OP))?;
        let synchronous = pragma_int(&self.conn, "PRAGMA synchronous", OP)?;
        let foreign_keys = pragma_int(&self.conn, "PRAGMA foreign_keys", OP)?;
        let busy_timeout = pragma_int(&self.conn, "PRAGMA busy_timeout", OP)?;
        let fullfsync = pragma_int(&self.conn, "PRAGMA fullfsync", OP)?;
        let checkpoint_fullfsync = pragma_int(&self.conn, "PRAGMA checkpoint_fullfsync", OP)?;
        Ok(Settings {
            journal_mode_wal: mode.eq_ignore_ascii_case("wal"),
            // `FULL` is 2 (SQLite pragma documentation).
            synchronous_full: synchronous == 2,
            foreign_keys: foreign_keys == 1,
            busy_timeout_ms: u64::try_from(busy_timeout).map_err(|_| unavailable(OP))?,
            fullfsync: fullfsync == 1,
            checkpoint_fullfsync: checkpoint_fullfsync == 1,
        })
    }

    /// [`Self::open`]: `may_create` allows the tables of a new database.
    fn open_checked(dir: &Path, may_create: bool) -> Result<Self, StoreError> {
        if marker_exists(dir)? {
            return Err(corrupted(OP_OPEN));
        }
        Self::open_with(dir, [0; 3], may_create).map(|(db, _)| db)
    }

    /// Opens the database; creates the tables with `start` when it is new
    /// and `may_create` allows it (else a new database is `Corrupted`).
    /// True when it was created.
    fn open_with(
        dir: &Path,
        start: Counters,
        may_create: bool,
    ) -> Result<(Self, bool), StoreError> {
        let path = dir.join(DB_FILE);
        create_db_file(dir, &path)?;
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW;
        let conn = Connection::open_with_flags(&path, flags).map_err(|error| open_error(&error))?;
        conn.busy_timeout(Duration::ZERO)
            .map_err(|error| open_error(&error))?;
        // The first statement reads the file: a file that is not a
        // database fails here.
        let mode: String = conn
            .query_row("PRAGMA journal_mode = WAL", params![], |row| row.get(0))
            .map_err(|error| open_error(&error))?;
        if !mode.eq_ignore_ascii_case("wal") {
            return Err(unavailable(OP_OPEN));
        }
        conn.execute_batch(
            "PRAGMA synchronous = FULL; PRAGMA fullfsync = ON; \
             PRAGMA checkpoint_fullfsync = ON; PRAGMA foreign_keys = ON; \
             PRAGMA trusted_schema = OFF; PRAGMA cell_size_check = ON;",
        )
        .map_err(|error| open_error(&error))?;
        let mut db = Self { conn };
        if db.settings()? != Settings::REQUIRED {
            return Err(unavailable(OP_OPEN));
        }
        let check: String = db
            .conn
            .query_row("PRAGMA quick_check(1)", params![], |row| row.get(0))
            .map_err(|error| open_error(&error))?;
        if check != "ok" {
            return Err(corrupted(OP_OPEN));
        }
        let created = db.prepare_schema(start, may_create)?;
        Ok((db, created))
    }

    /// Creates the tables of a new database when `may_create` allows it,
    /// or checks those of an existing one. True when it created them.
    fn prepare_schema(&mut self, start: Counters, may_create: bool) -> Result<bool, StoreError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| open_error(&error))?;
        let format = pragma_int(&tx, "PRAGMA user_version", OP_OPEN)?;
        let created = match format {
            0 => {
                // A database with no format holds nothing of ours. Any
                // object in it is not ours either.
                let objects = query_int(&tx, COUNT_SCHEMA_OBJECTS)?;
                if objects != 0 || !may_create {
                    return Err(corrupted(OP_OPEN));
                }
                for table in TABLES {
                    tx.execute_batch(table)
                        .map_err(|error| open_error(&error))?;
                }
                for (name, value) in COUNTERS.iter().zip(start) {
                    let value = sql_u64(value, OP_OPEN)?;
                    tx.execute(INSERT_COUNTER, params![name, value])
                        .map_err(|error| open_error(&error))?;
                }
                tx.execute_batch("PRAGMA user_version = 1")
                    .map_err(|error| open_error(&error))?;
                true
            }
            META_FORMAT => false,
            _ => return Err(corrupted(OP_OPEN)),
        };
        check_schema_objects(&tx)?;
        // Tables of another shape fail to prepare a statement.
        for sql in txn::ALL_STATEMENTS {
            tx.prepare(sql).map_err(|error| schema_error(&error))?;
        }
        for name in COUNTERS {
            let stored = tx
                .query_row(SELECT_COUNTER, params![name], |row| {
                    row.get_ref(0).map(|value| decode_u64(value, OP_OPEN))
                })
                .map_err(|error| match error {
                    rusqlite::Error::QueryReturnedNoRows => corrupted(OP_OPEN),
                    other => open_error(&other),
                })?;
            stored?;
        }
        tx.commit().map_err(|error| open_error(&error))?;
        Ok(created)
    }

    /// The reset of the module documentation.
    fn reset(
        dir: &Path,
        random: &mut dyn FnMut(&mut [u8]) -> Result<(), StoreError>,
    ) -> Result<Self, StoreError> {
        let mut start: Counters = [0; 3];
        for value in &mut start {
            let mut bytes = [0_u8; 8];
            random(&mut bytes)?;
            *value = u64::from_be_bytes(bytes) & RANDOM_MASK;
        }
        let marker = dir.join(RESET_MARKER);
        create_marker(&marker)?;
        sync_dir(dir, OP_RESET)?;
        clear_dir(dir)?;
        // The new database is made by this call: any failure here is a
        // failure of the reset, never a reason for another one.
        let (db, created) = Self::open_with(dir, start, true).map_err(|_| unavailable(OP_RESET))?;
        if !created {
            return Err(unavailable(OP_RESET));
        }
        fs::remove_file(&marker).map_err(|_| unavailable(OP_RESET))?;
        sync_dir(dir, OP_RESET)?;
        Ok(db)
    }
}

/// A time as the metadata stores it (see "Integers"): a time above
/// `i64::MAX` is `i64::MAX`. The driver seals this value, so the sealed
/// copy of `stored_at` equals the clear column.
pub(crate) const fn stored_time(time: UnixTime) -> UnixTime {
    let max = i64::MAX.unsigned_abs();
    if time.0 > max { UnixTime(max) } else { time }
}

/// True when an entry named like the marker of an unfinished reset is in
/// `dir`, of any type: a link is not followed, so a link is a marker too.
fn marker_exists(dir: &Path) -> Result<bool, StoreError> {
    match fs::symlink_metadata(dir.join(RESET_MARKER)) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(unavailable(OP_OPEN)),
    }
}

/// Creates the reset marker and syncs it. A marker that exists already (a
/// reset that did not finish) is kept as it is, but only when it is a
/// regular file: the marker is never followed through a link and never
/// truncated.
fn create_marker(path: &Path) -> Result<(), StoreError> {
    match new_file(path) {
        Ok(file) => file.sync_all().map_err(|_| unavailable(OP_RESET)),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            match fs::symlink_metadata(path) {
                Ok(meta) if meta.is_file() => Ok(()),
                Ok(_) | Err(_) => Err(unavailable(OP_RESET)),
            }
        }
        Err(_) => Err(unavailable(OP_RESET)),
    }
}

/// Creates `meta.sqlite` with mode 0600 (unix) before SQLite opens it, when
/// it does not exist, and syncs `dir`. SQLite gives its `-wal` and `-shm`
/// files the mode of the database file. An entry that exists is left to
/// SQLite (`SQLITE_OPEN_NOFOLLOW` refuses a link).
fn create_db_file(dir: &Path, path: &Path) -> Result<(), StoreError> {
    match new_file(path) {
        Ok(file) => {
            drop(file);
            sync_dir(dir, OP_OPEN)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(_) => Err(unavailable(OP_OPEN)),
    }
}

/// A new empty file at `path` (`create_new`: never an existing entry,
/// never through a link), mode 0600 on unix.
fn new_file(path: &Path) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// True when `dir` holds nothing of a store (see
/// [`MetaDb::open_or_reset`]): no entry other than `meta.sqlite`, an empty
/// `tmp/` and a `files/` that holds only empty directories. A link is not
/// followed and is not fresh. The walk stops at the first entry that is
/// not fresh, and reads at most `files/` and each of its directories.
fn fresh_dir(dir: &Path) -> Result<bool, StoreError> {
    for entry in fs::read_dir(dir).map_err(|_| unavailable(OP_OPEN))? {
        let entry = entry.map_err(|_| unavailable(OP_OPEN))?;
        let name = entry.file_name();
        let is_dir = entry
            .file_type()
            .map_err(|_| unavailable(OP_OPEN))?
            .is_dir();
        let fresh = if name == DB_FILE {
            true
        } else if is_dir && name == TMP_DIR {
            dir_is_empty(&entry.path())?
        } else if is_dir && name == FILES_DIR {
            holds_only_empty_dirs(&entry.path())?
        } else {
            false
        };
        if !fresh {
            return Ok(false);
        }
    }
    Ok(true)
}

/// True when the directory `dir` has no entry.
fn dir_is_empty(dir: &Path) -> Result<bool, StoreError> {
    let mut entries = fs::read_dir(dir).map_err(|_| unavailable(OP_OPEN))?;
    Ok(entries.next().is_none())
}

/// True when every entry of `dir` is an empty directory (not a link).
fn holds_only_empty_dirs(dir: &Path) -> Result<bool, StoreError> {
    for entry in fs::read_dir(dir).map_err(|_| unavailable(OP_OPEN))? {
        let entry = entry.map_err(|_| unavailable(OP_OPEN))?;
        let is_dir = entry
            .file_type()
            .map_err(|_| unavailable(OP_OPEN))?
            .is_dir();
        if !is_dir || !dir_is_empty(&entry.path())? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Checks that `sqlite_schema` holds exactly [`SCHEMA_OBJECTS`]. Any other
/// object, a missing one or another `sql` text is `Corrupted`. The loop
/// ends at the first object that is unknown or seen twice, so it reads at
/// most one row more than the expected set.
fn check_schema_objects(conn: &Connection) -> Result<(), StoreError> {
    let mut stmt = conn
        .prepare(SELECT_SCHEMA_OBJECTS)
        .map_err(|error| open_error(&error))?;
    let mut rows = stmt.query(params![]).map_err(|error| open_error(&error))?;
    let mut seen = [false; SCHEMA_OBJECTS.len()];
    while let Some(row) = rows.next().map_err(|error| open_error(&error))? {
        let column = |index: usize| -> Result<Option<&[u8]>, StoreError> {
            match row.get_ref(index).map_err(|error| open_error(&error))? {
                ValueRef::Text(text) => Ok(Some(text)),
                ValueRef::Null => Ok(None),
                _ => Err(corrupted(OP_OPEN)),
            }
        };
        let found = (column(0)?, column(1)?, column(2)?, column(3)?);
        let known = SCHEMA_OBJECTS.iter().position(|&(kind, name, table, sql)| {
            found
                == (
                    Some(kind.as_bytes()),
                    Some(name.as_bytes()),
                    Some(table.as_bytes()),
                    sql.map(str::as_bytes),
                )
        });
        match known.and_then(|index| seen.get_mut(index)) {
            Some(flag) if !*flag => *flag = true,
            _ => return Err(corrupted(OP_OPEN)),
        }
    }
    if seen.iter().all(|flag| *flag) {
        Ok(())
    } else {
        Err(corrupted(OP_OPEN))
    }
}

/// Removes every entry of `dir` except the reset marker. A symbolic link is
/// removed, never followed.
fn clear_dir(dir: &Path) -> Result<(), StoreError> {
    let entries = fs::read_dir(dir).map_err(|_| unavailable(OP_RESET))?;
    for entry in entries {
        let entry = entry.map_err(|_| unavailable(OP_RESET))?;
        if entry.file_name() == RESET_MARKER {
            continue;
        }
        let kind = entry.file_type().map_err(|_| unavailable(OP_RESET))?;
        let path = entry.path();
        let removed = if kind.is_dir() {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        match removed {
            Ok(()) => {}
            // Gone already: the goal of the step holds.
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return Err(unavailable(OP_RESET)),
        }
    }
    Ok(())
}

/// Makes the entries of `dir` durable.
fn sync_dir(dir: &Path, op: &'static str) -> Result<(), StoreError> {
    fs::File::open(dir)
        .and_then(|handle| handle.sync_all())
        .map_err(|_| unavailable(op))
}

/// The integer answer of a constant pragma.
fn pragma_int(conn: &Connection, sql: &str, op: &'static str) -> Result<i64, StoreError> {
    conn.query_row(sql, params![], |row| row.get::<_, i64>(0))
        .map_err(|_| unavailable(op))
}

/// The integer answer of a constant query at open.
fn query_int(conn: &Connection, sql: &str) -> Result<i64, StoreError> {
    conn.query_row(sql, params![], |row| row.get::<_, i64>(0))
        .map_err(|error| open_error(&error))
}

/// An error at open: `Corrupted` when SQLite found the file is not a
/// database or is malformed, else `Unavailable`.
fn open_error(error: &rusqlite::Error) -> StoreError {
    match sqlite_code(error) {
        Some(ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt) => corrupted(OP_OPEN),
        _ => unavailable(OP_OPEN),
    }
}

/// An error when a statement is prepared at open: a statement that does
/// not compile (`SQLITE_ERROR`: a missing table or column) means tables of
/// another shape.
fn schema_error(error: &rusqlite::Error) -> StoreError {
    match sqlite_code(error) {
        Some(ErrorCode::Unknown) => corrupted(OP_OPEN),
        _ => open_error(error),
    }
}

/// The SQLite result code of `error`. `rusqlite` gives an error of the
/// statement text (with the offset of the bad token) as its own variant.
fn sqlite_code(error: &rusqlite::Error) -> Option<ErrorCode> {
    match error {
        rusqlite::Error::SqlInputError { error, .. } => Some(error.code),
        other => other.sqlite_error_code(),
    }
}
