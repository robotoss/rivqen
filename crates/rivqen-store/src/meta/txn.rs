// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The metadata transaction: every read and write of the four tables.
//!
//! Every statement is a constant with parameters. Every statement that
//! names one entry, one disabled key or one partition epoch has the
//! partition column in its `WHERE` clause or in its primary key (RSK-06).
//! Only [`MetaTxn::rows`] and [`MetaTxn::remove_undecodable_rows`] read all
//! partitions: they serve the list and the recovery.

use std::fmt;

use rivqen_cache::driver::StoreError;
use rivqen_cache::key::Partition;
use rivqen_cache::txn::{EntryVersion, Epoch};
use rivqen_model::ids::CacheKey;
use rivqen_model::time::UnixTime;
use rusqlite::{OptionalExtension, Transaction, params};

use super::codec::{
    corrupted, decode_array, decode_blob, decode_key, decode_partition, decode_text, decode_time,
    decode_u64, partition_bytes, sql_time, sql_u64, unavailable,
};
use super::{
    COUNTER_PARTITION_EPOCH_BASE, COUNTER_SITE_EPOCH, COUNTER_VERSION, MAX_HOST_BYTES,
    MAX_SEALED_BYTES,
};

/// The size of a file id (ADR-004 section 1).
pub const FILE_ID_LEN: usize = 16;

/// One row of the `entry` table.
///
/// `sealed` is opaque here: the driver seals and opens it (ADR-011).
/// [`fmt::Debug`] writes no host, no file id and no sealed byte.
#[derive(Clone, PartialEq, Eq)]
pub struct Row {
    /// The partition.
    pub partition: Partition,
    /// The cache key.
    pub key: CacheKey,
    /// The host of the origin, for the site clear (DL-184). At most
    /// [`MAX_HOST_BYTES`] bytes.
    pub host: String,
    /// The version of the entry.
    pub version: EntryVersion,
    /// The file that holds the content of this version.
    pub file_id: [u8; FILE_ID_LEN],
    /// The size of the content in bytes.
    pub content_bytes: u64,
    /// When the entry was stored.
    pub stored_at: UnixTime,
    /// When the entry was last used.
    pub last_used_at: UnixTime,
    /// The sealed row metadata. At most [`MAX_SEALED_BYTES`] bytes.
    pub sealed: Vec<u8>,
}

impl fmt::Debug for Row {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Row")
            .field("partition", &self.partition)
            .field("key", &self.key)
            .field("version", &self.version)
            .field("content_bytes", &self.content_bytes)
            .field("stored_at", &self.stored_at)
            .field("last_used_at", &self.last_used_at)
            .field("sealed_bytes", &self.sealed.len())
            .finish_non_exhaustive()
    }
}

/// The columns of a row, in the order of [`decode_row`]. `?{max}` is the
/// parameter that holds [`MAX_SEALED_BYTES`]: a longer `sealed` value is
/// read as `NULL` and never copied.
macro_rules! row_columns {
    ($max:literal) => {
        concat!(
            r#""partition", "key", host, version, file_id, content_bytes, stored_at, last_used_at, "#,
            "CASE WHEN length(sealed) <= ?",
            $max,
            " THEN sealed END"
        )
    };
}

const SELECT_ROW: &str = concat!(
    "SELECT ",
    row_columns!("3"),
    r#" FROM entry WHERE "partition" = ?1 AND "key" = ?2"#
);
const SELECT_ROWS: &str = concat!(
    "SELECT ",
    row_columns!("1"),
    r#" FROM entry ORDER BY "partition", "key""#
);
const SELECT_ROWS_OF_PARTITION: &str = concat!(
    "SELECT ",
    row_columns!("2"),
    r#" FROM entry WHERE "partition" = ?1 ORDER BY "key""#
);
const SELECT_ROWS_WITH_ROWID: &str = concat!("SELECT ", row_columns!("1"), ", rowid FROM entry");
const UPSERT_ROW: &str = r#"INSERT INTO entry
    ("partition", "key", host, version, file_id, content_bytes, stored_at, last_used_at, sealed)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
    ON CONFLICT ("partition", "key") DO UPDATE SET
    host = excluded.host, version = excluded.version, file_id = excluded.file_id,
    content_bytes = excluded.content_bytes, stored_at = excluded.stored_at,
    last_used_at = excluded.last_used_at, sealed = excluded.sealed"#;
const DELETE_ROW: &str = r#"DELETE FROM entry WHERE "partition" = ?1 AND "key" = ?2"#;
const DELETE_ROWS_OF_PARTITION: &str = r#"DELETE FROM entry WHERE "partition" = ?1"#;
const DELETE_ROW_BY_ROWID: &str = "DELETE FROM entry WHERE rowid = ?1";
const TOUCH_ROW: &str =
    r#"UPDATE entry SET last_used_at = ?3 WHERE "partition" = ?1 AND "key" = ?2"#;
const SELECT_EPOCH: &str = r#"SELECT epoch FROM partition_epoch WHERE "partition" = ?1"#;
const UPSERT_EPOCH: &str = r#"INSERT INTO partition_epoch ("partition", epoch) VALUES (?1, ?2)
    ON CONFLICT ("partition") DO UPDATE SET epoch = excluded.epoch"#;
const SELECT_COUNTER: &str = "SELECT value FROM counter WHERE name = ?1";
const UPDATE_COUNTER: &str = "UPDATE counter SET value = ?2 WHERE name = ?1";
const UPSERT_DISABLED: &str = r#"INSERT INTO disabled_key ("partition", "key", until)
    VALUES (?1, ?2, ?3) ON CONFLICT ("partition", "key") DO UPDATE SET until = excluded.until"#;
const SELECT_DISABLED: &str =
    r#"SELECT until FROM disabled_key WHERE "partition" = ?1 AND "key" = ?2"#;
const DELETE_DISABLED_OF_PARTITION: &str = r#"DELETE FROM disabled_key WHERE "partition" = ?1"#;
const DELETE_ALL_DISABLED: &str = "DELETE FROM disabled_key";
const PURGE_DISABLED: &str = "DELETE FROM disabled_key WHERE until < ?1";

/// Every statement of this module: [`super::MetaDb::open`] prepares each
/// once, so a database whose tables have another shape is found at open.
pub(super) const ALL_STATEMENTS: [&str; 18] = [
    SELECT_ROW,
    SELECT_ROWS,
    SELECT_ROWS_OF_PARTITION,
    SELECT_ROWS_WITH_ROWID,
    UPSERT_ROW,
    DELETE_ROW,
    DELETE_ROWS_OF_PARTITION,
    DELETE_ROW_BY_ROWID,
    TOUCH_ROW,
    SELECT_EPOCH,
    UPSERT_EPOCH,
    SELECT_COUNTER,
    UPDATE_COUNTER,
    UPSERT_DISABLED,
    SELECT_DISABLED,
    DELETE_DISABLED_OF_PARTITION,
    DELETE_ALL_DISABLED,
    PURGE_DISABLED,
];

/// `MAX_SEALED_BYTES` as a parameter. The constant is far below
/// `i64::MAX`, so the fallback is never used.
fn sealed_limit() -> i64 {
    i64::try_from(MAX_SEALED_BYTES).unwrap_or(i64::MAX)
}

/// One open `BEGIN IMMEDIATE` transaction on the metadata database.
///
/// [`super::MetaDb::transaction`] gives it to a closure; the closure cannot
/// keep it. Every method returns [`rivqen_cache::driver::StoreErrorKind::Unavailable`]
/// when SQLite fails, and `Corrupted` when a stored value does not decode.
pub struct MetaTxn<'conn> {
    tx: Transaction<'conn>,
}

impl fmt::Debug for MetaTxn<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MetaTxn { .. }")
    }
}

impl<'conn> MetaTxn<'conn> {
    pub(super) const fn new(tx: Transaction<'conn>) -> Self {
        Self { tx }
    }

    pub(super) fn into_inner(self) -> Transaction<'conn> {
        self.tx
    }

    /// The row of `key` in `partition`.
    ///
    /// # Errors
    ///
    /// `Unavailable` when SQLite fails; `Corrupted` when the row does not
    /// decode (the caller removes it with [`Self::delete_row`]).
    pub fn row(&self, partition: Partition, key: &CacheKey) -> Result<Option<Row>, StoreError> {
        const OP: &str = "meta.row";
        let mut stmt = self.tx.prepare(SELECT_ROW).map_err(|_| unavailable(OP))?;
        let mut rows = stmt
            .query(params![
                partition_bytes(partition),
                key.as_bytes(),
                sealed_limit()
            ])
            .map_err(|_| unavailable(OP))?;
        match rows.next().map_err(|_| unavailable(OP))? {
            Some(found) => {
                let row = decode_row(found, OP)?;
                // The statement selects by both columns; a row of another
                // address is a broken database, never an answer.
                if row.partition != partition || row.key != *key {
                    return Err(corrupted(OP));
                }
                Ok(Some(row))
            }
            None => Ok(None),
        }
    }

    /// Writes `row` at its address; an old row of the address is replaced.
    ///
    /// # Errors
    ///
    /// `Unavailable` when SQLite fails, or when the version or the size is
    /// above `i64::MAX`, the host is longer than [`MAX_HOST_BYTES`] or
    /// `sealed` is longer than [`MAX_SEALED_BYTES`]. Nothing is written
    /// then.
    pub fn put_row(&mut self, row: &Row) -> Result<(), StoreError> {
        const OP: &str = "meta.put_row";
        if row.host.len() > MAX_HOST_BYTES || row.sealed.len() > MAX_SEALED_BYTES {
            return Err(unavailable(OP));
        }
        let version = sql_u64(row.version.0, OP)?;
        let content_bytes = sql_u64(row.content_bytes, OP)?;
        self.tx
            .execute(
                UPSERT_ROW,
                params![
                    partition_bytes(row.partition),
                    row.key.as_bytes(),
                    row.host,
                    version,
                    row.file_id,
                    content_bytes,
                    sql_time(row.stored_at),
                    sql_time(row.last_used_at),
                    row.sealed
                ],
            )
            .map_err(|_| unavailable(OP))?;
        Ok(())
    }

    /// Removes the row of `key` in `partition`. True when a row was there.
    ///
    /// # Errors
    ///
    /// `Unavailable` when SQLite fails.
    pub fn delete_row(&mut self, partition: Partition, key: &CacheKey) -> Result<bool, StoreError> {
        let removed = self
            .tx
            .execute(
                DELETE_ROW,
                params![partition_bytes(partition), key.as_bytes()],
            )
            .map_err(|_| unavailable("meta.delete_row"))?;
        Ok(removed > 0)
    }

    /// Removes every row of `partition`; gives the number removed.
    ///
    /// # Errors
    ///
    /// `Unavailable` when SQLite fails.
    pub fn delete_rows_of_partition(&mut self, partition: Partition) -> Result<u64, StoreError> {
        const OP: &str = "meta.delete_rows_of_partition";
        let removed = self
            .tx
            .execute(
                DELETE_ROWS_OF_PARTITION,
                params![partition_bytes(partition)],
            )
            .map_err(|_| unavailable(OP))?;
        count(removed)
    }

    /// Every row, ordered by partition column and key. The number of rows
    /// is bounded by the quota of the store.
    ///
    /// # Errors
    ///
    /// `Unavailable` when SQLite fails; `Corrupted` when one row does not
    /// decode (see [`Self::remove_undecodable_rows`]).
    pub fn rows(&self) -> Result<Vec<Row>, StoreError> {
        const OP: &str = "meta.rows";
        let mut stmt = self.tx.prepare(SELECT_ROWS).map_err(|_| unavailable(OP))?;
        let mut rows = stmt
            .query(params![sealed_limit()])
            .map_err(|_| unavailable(OP))?;
        let mut out = Vec::new();
        while let Some(found) = rows.next().map_err(|_| unavailable(OP))? {
            out.push(decode_row(found, OP)?);
        }
        Ok(out)
    }

    /// Every row of `partition`, ordered by key.
    ///
    /// # Errors
    ///
    /// `Unavailable` when SQLite fails; `Corrupted` when one row does not
    /// decode.
    pub fn rows_of_partition(&self, partition: Partition) -> Result<Vec<Row>, StoreError> {
        const OP: &str = "meta.rows_of_partition";
        let mut stmt = self
            .tx
            .prepare(SELECT_ROWS_OF_PARTITION)
            .map_err(|_| unavailable(OP))?;
        let mut rows = stmt
            .query(params![partition_bytes(partition), sealed_limit()])
            .map_err(|_| unavailable(OP))?;
        let mut out = Vec::new();
        while let Some(found) = rows.next().map_err(|_| unavailable(OP))? {
            let row = decode_row(found, OP)?;
            if row.partition != partition {
                return Err(corrupted(OP));
            }
            out.push(row);
        }
        Ok(out)
    }

    /// Removes every row that does not decode, also a row whose partition
    /// column is broken (no address can name it). Gives the number
    /// removed. Recovery runs it before [`Self::rows`] (R-CACHE-06: the
    /// other entries stay).
    ///
    /// # Errors
    ///
    /// `Unavailable` when SQLite fails.
    pub fn remove_undecodable_rows(&mut self) -> Result<u64, StoreError> {
        const OP: &str = "meta.remove_undecodable_rows";
        let mut broken: Vec<i64> = Vec::new();
        {
            let mut stmt = self
                .tx
                .prepare(SELECT_ROWS_WITH_ROWID)
                .map_err(|_| unavailable(OP))?;
            let mut rows = stmt
                .query(params![sealed_limit()])
                .map_err(|_| unavailable(OP))?;
            while let Some(found) = rows.next().map_err(|_| unavailable(OP))? {
                if decode_row(found, OP).is_err() {
                    let rowid = found.get_ref(ROWID_COLUMN).map_err(|_| unavailable(OP))?;
                    let rowid = rowid.as_i64().map_err(|_| unavailable(OP))?;
                    broken.push(rowid);
                }
            }
        }
        let mut removed: u64 = 0;
        for rowid in broken {
            let changed = self
                .tx
                .execute(DELETE_ROW_BY_ROWID, params![rowid])
                .map_err(|_| unavailable(OP))?;
            removed = removed.saturating_add(count(changed)?);
        }
        Ok(removed)
    }

    /// Sets `last_used_at` of the row of `key` in `partition`. True when a
    /// row was there.
    ///
    /// # Errors
    ///
    /// `Unavailable` when SQLite fails.
    pub fn touch(
        &mut self,
        partition: Partition,
        key: &CacheKey,
        now: UnixTime,
    ) -> Result<bool, StoreError> {
        let changed = self
            .tx
            .execute(
                TOUCH_ROW,
                params![partition_bytes(partition), key.as_bytes(), sql_time(now)],
            )
            .map_err(|_| unavailable("meta.touch"))?;
        Ok(changed > 0)
    }

    /// The epoch of `partition`. A partition with no epoch row has the base
    /// epoch of the store: 0 in a new store, a random value after a reset.
    ///
    /// # Errors
    ///
    /// `Unavailable` when SQLite fails; `Corrupted` when the stored value
    /// does not decode.
    pub fn partition_epoch(&self, partition: Partition) -> Result<Epoch, StoreError> {
        const OP: &str = "meta.partition_epoch";
        let stored = self
            .tx
            .query_row(SELECT_EPOCH, params![partition_bytes(partition)], |row| {
                row.get_ref(0).map(|value| decode_u64(value, OP))
            })
            .optional()
            .map_err(|_| unavailable(OP))?;
        match stored {
            Some(epoch) => epoch.map(Epoch),
            None => self.counter(COUNTER_PARTITION_EPOCH_BASE, OP).map(Epoch),
        }
    }

    /// Increments the epoch of `partition` by exactly 1 and gives the new
    /// epoch. The epoch row is never deleted.
    ///
    /// # Errors
    ///
    /// `Unavailable` when SQLite fails or the new epoch would be above
    /// `i64::MAX` (nothing is written then); `Corrupted` when the stored
    /// value does not decode.
    pub fn bump_partition_epoch(&mut self, partition: Partition) -> Result<Epoch, StoreError> {
        const OP: &str = "meta.bump_partition_epoch";
        let (next, column) = increment(self.partition_epoch(partition)?.0, OP)?;
        self.tx
            .execute(UPSERT_EPOCH, params![partition_bytes(partition), column])
            .map_err(|_| unavailable(OP))?;
        Ok(Epoch(next))
    }

    /// The epoch of the site clear.
    ///
    /// # Errors
    ///
    /// `Unavailable` when SQLite fails; `Corrupted` when the stored value
    /// does not decode or is missing.
    pub fn site_epoch(&self) -> Result<Epoch, StoreError> {
        self.counter(COUNTER_SITE_EPOCH, "meta.site_epoch")
            .map(Epoch)
    }

    /// Increments the site epoch by exactly 1 and gives the new epoch.
    ///
    /// # Errors
    ///
    /// As [`Self::bump_partition_epoch`].
    pub fn bump_site_epoch(&mut self) -> Result<Epoch, StoreError> {
        const OP: &str = "meta.bump_site_epoch";
        let (next, column) = increment(self.counter(COUNTER_SITE_EPOCH, OP)?, OP)?;
        self.set_counter(COUNTER_SITE_EPOCH, column, OP)?;
        Ok(Epoch(next))
    }

    /// A new entry version: the last one plus 1. The counter is never
    /// reset, so a version is never given twice in one store.
    ///
    /// # Errors
    ///
    /// As [`Self::bump_partition_epoch`].
    pub fn next_version(&mut self) -> Result<EntryVersion, StoreError> {
        const OP: &str = "meta.next_version";
        let (next, column) = increment(self.counter(COUNTER_VERSION, OP)?, OP)?;
        self.set_counter(COUNTER_VERSION, column, OP)?;
        Ok(EntryVersion(next))
    }

    /// Disables `key` in `partition` until `until` (CL-19); an earlier
    /// record of the key is replaced.
    ///
    /// # Errors
    ///
    /// `Unavailable` when SQLite fails.
    pub fn disable(
        &mut self,
        partition: Partition,
        key: &CacheKey,
        until: UnixTime,
    ) -> Result<(), StoreError> {
        self.tx
            .execute(
                UPSERT_DISABLED,
                params![partition_bytes(partition), key.as_bytes(), sql_time(until)],
            )
            .map_err(|_| unavailable("meta.disable"))?;
        Ok(())
    }

    /// The time of the disabled-key record of `key` in `partition`.
    ///
    /// # Errors
    ///
    /// `Unavailable` when SQLite fails; `Corrupted` when the stored value
    /// does not decode (the caller treats the key as disabled).
    pub fn disabled_until(
        &self,
        partition: Partition,
        key: &CacheKey,
    ) -> Result<Option<UnixTime>, StoreError> {
        const OP: &str = "meta.disabled_until";
        let stored = self
            .tx
            .query_row(
                SELECT_DISABLED,
                params![partition_bytes(partition), key.as_bytes()],
                |row| row.get_ref(0).map(|value| decode_time(value, OP)),
            )
            .optional()
            .map_err(|_| unavailable(OP))?;
        stored.transpose()
    }

    /// Removes every disabled-key record of `partition`; gives the number
    /// removed.
    ///
    /// # Errors
    ///
    /// `Unavailable` when SQLite fails.
    pub fn delete_disabled_of_partition(
        &mut self,
        partition: Partition,
    ) -> Result<u64, StoreError> {
        let removed = self
            .tx
            .execute(
                DELETE_DISABLED_OF_PARTITION,
                params![partition_bytes(partition)],
            )
            .map_err(|_| unavailable("meta.delete_disabled_of_partition"))?;
        count(removed)
    }

    /// Removes every disabled-key record; gives the number removed.
    ///
    /// # Errors
    ///
    /// `Unavailable` when SQLite fails.
    pub fn delete_all_disabled(&mut self) -> Result<u64, StoreError> {
        let removed = self
            .tx
            .execute(DELETE_ALL_DISABLED, params![])
            .map_err(|_| unavailable("meta.delete_all_disabled"))?;
        count(removed)
    }

    /// Removes the disabled-key records whose time is before `now`; a
    /// record whose time is `now` or later stays (ADR-004 section 4, step
    /// 5). Gives the number removed.
    ///
    /// # Errors
    ///
    /// `Unavailable` when SQLite fails.
    pub fn purge_disabled(&mut self, now: UnixTime) -> Result<u64, StoreError> {
        let removed = self
            .tx
            .execute(PURGE_DISABLED, params![sql_time(now)])
            .map_err(|_| unavailable("meta.purge_disabled"))?;
        count(removed)
    }

    /// The value of the counter `name`.
    fn counter(&self, name: &str, op: &'static str) -> Result<u64, StoreError> {
        let stored = self
            .tx
            .query_row(SELECT_COUNTER, params![name], |row| {
                row.get_ref(0).map(|value| decode_u64(value, op))
            })
            .optional()
            .map_err(|_| unavailable(op))?;
        // A counter row is written when the schema is made and never
        // deleted: a missing one is a broken database.
        stored.ok_or_else(|| corrupted(op))?
    }

    /// Writes the counter `name`.
    fn set_counter(&mut self, name: &str, value: i64, op: &'static str) -> Result<(), StoreError> {
        let changed = self
            .tx
            .execute(UPDATE_COUNTER, params![name, value])
            .map_err(|_| unavailable(op))?;
        if changed == 1 {
            Ok(())
        } else {
            Err(corrupted(op))
        }
    }
}

/// `current + 1` and its column value, when it is at most `i64::MAX`.
fn increment(current: u64, op: &'static str) -> Result<(u64, i64), StoreError> {
    let next = current.checked_add(1).ok_or_else(|| unavailable(op))?;
    Ok((next, sql_u64(next, op)?))
}

/// A number of changed rows as `u64`. Every Rust target has a `usize` of at
/// most 64 bits, so the error is never given.
fn count(changed: usize) -> Result<u64, StoreError> {
    u64::try_from(changed).map_err(|_| unavailable("meta.count"))
}

/// The index of the `rowid` column in [`SELECT_ROWS_WITH_ROWID`].
const ROWID_COLUMN: usize = 9;

/// The row of the first nine columns of `found` (see `row_columns!`).
fn decode_row(found: &rusqlite::Row<'_>, op: &'static str) -> Result<Row, StoreError> {
    let column = |index: usize| found.get_ref(index).map_err(|_| unavailable(op));
    Ok(Row {
        partition: decode_partition(column(0)?, op)?,
        key: decode_key(column(1)?, op)?,
        host: decode_text(column(2)?, MAX_HOST_BYTES, op)?,
        version: EntryVersion(decode_u64(column(3)?, op)?),
        file_id: decode_array::<FILE_ID_LEN>(column(4)?, op)?,
        content_bytes: decode_u64(column(5)?, op)?,
        stored_at: decode_time(column(6)?, op)?,
        last_used_at: decode_time(column(7)?, op)?,
        // `NULL` (the answer of the length guard for a value that is too
        // long) is not a blob and does not decode.
        sealed: decode_blob(column(8)?, MAX_SEALED_BYTES, op)?,
    })
}
