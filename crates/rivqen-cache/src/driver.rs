// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The storage driver interface (ADR-001: adapters own storage; DL-180).
//! Contract of the architect: lanes do not change this file.
//!
//! A driver is synchronous and is called from one logical thread (core API
//! section 2, rule 1). The fake driver of `rivqen-testkit` and the reference
//! driver `rivqen-store` pass the same contract tests (DL-181).
//!
//! The rules of [`StorageDriver::apply`] are in the module documentation of
//! [`crate::txn`] (DL-200, DL-205). Three of them come from DL-205:
//!
//! - A keyed kind first checks the epoch of the request partition
//!   ([`crate::txn::Preconditions::writer`]); then the epoch of the target
//!   partition, the site epoch, the host (`Replace` only) and the entry, all
//!   at the commit point.
//! - A `Replace` whose host fails [`crate::partition::valid_host`] is
//!   [`crate::txn::Refused::InvalidHost`] and writes nothing.
//! - [`StorageDriver::recover`] removes the disabled-key records whose time
//!   is before its `now`; a record whose time is `now` or later stays.

use std::error::Error;
use std::fmt;

use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_model::ids::CacheKey;
use rivqen_model::time::UnixTime;

use crate::key::Partition;
use crate::txn::{CacheTransaction, EntryMeta, EntryVersion, Epoch, StoredEntry, TxnOutcome};

/// Why a driver call failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreErrorKind {
    /// The driver could not read or write: disk full, I/O error, key of the
    /// partition not available (`STORAGE_UNAVAILABLE`, H-148).
    Unavailable,
    /// An entry failed its integrity or schema check; the driver removed it
    /// (`CACHE_CORRUPTED`, R-CACHE-06).
    Corrupted,
}

/// The error of a driver call. It holds no path, key, URL or content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreError {
    kind: StoreErrorKind,
    op: &'static str,
}

impl StoreError {
    /// An error of `kind` in the driver operation `op` (a static name).
    #[must_use]
    pub const fn new(kind: StoreErrorKind, op: &'static str) -> Self {
        Self { kind, op }
    }

    /// The kind.
    #[must_use]
    pub const fn kind(&self) -> StoreErrorKind {
        self.kind
    }

    /// The driver operation.
    #[must_use]
    pub const fn op(&self) -> &'static str {
        self.op
    }
}

impl HasErrorCode for StoreError {
    fn code(&self) -> ErrorCode {
        match self.kind {
            StoreErrorKind::Unavailable => ErrorCode::StorageUnavailable,
            StoreErrorKind::Corrupted => ErrorCode::CacheCorrupted,
        }
    }
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} in {}", self.code(), self.op)
    }
}

impl Error for StoreError {}

/// One row of [`StorageDriver::list`]: what quotas and the site clear need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntrySummary {
    /// The metadata.
    pub meta: EntryMeta,
    /// The version.
    pub version: EntryVersion,
    /// The stored size in bytes.
    pub size_bytes: u64,
}

/// What a recovery removed (storage section 2, step 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RecoveryReport {
    /// Temporary files and entry versions without a metadata row.
    pub orphans_removed: u64,
    /// Entries whose files were missing or failed their check.
    pub entries_removed: u64,
}

/// A store of cache entries.
pub trait StorageDriver {
    /// Runs at start, before any other call: brings the store to a state
    /// where each entry is one committed version, and removes the
    /// disabled-key records whose time is before `now` (storage section 2;
    /// ADR-004 section 4). A record whose time is `now` or later stays.
    ///
    /// # Errors
    ///
    /// [`StoreErrorKind::Unavailable`] when the store cannot be read or
    /// written.
    fn recover(&mut self, now: UnixTime) -> Result<RecoveryReport, StoreError>;

    /// The entry of `key` in `partition`, checked against its stored
    /// hashes. A key that is disabled at `now` has no entry.
    ///
    /// # Errors
    ///
    /// [`StoreErrorKind::Corrupted`] when the entry failed its check (the
    /// driver removed it); [`StoreErrorKind::Unavailable`] for an I/O
    /// failure.
    fn read(
        &mut self,
        partition: Partition,
        key: &CacheKey,
        now: UnixTime,
    ) -> Result<Option<StoredEntry>, StoreError>;

    /// Applies one transaction atomically, or refuses it.
    ///
    /// # Errors
    ///
    /// [`StoreErrorKind::Unavailable`] when the store cannot be written;
    /// the store then holds the state before the call.
    fn apply(&mut self, txn: CacheTransaction) -> Result<TxnOutcome, StoreError>;

    /// The current epoch of `partition`.
    ///
    /// # Errors
    ///
    /// [`StoreErrorKind::Unavailable`] for an I/O failure.
    fn partition_epoch(&mut self, partition: Partition) -> Result<Epoch, StoreError>;

    /// The current epoch of the site clear.
    ///
    /// # Errors
    ///
    /// [`StoreErrorKind::Unavailable`] for an I/O failure.
    fn site_epoch(&mut self) -> Result<Epoch, StoreError>;

    /// True when `key` is disabled in `partition` at `now` (CL-19).
    ///
    /// # Errors
    ///
    /// [`StoreErrorKind::Unavailable`] for an I/O failure.
    fn key_disabled(
        &mut self,
        partition: Partition,
        key: &CacheKey,
        now: UnixTime,
    ) -> Result<bool, StoreError>;

    /// Every entry, without content.
    ///
    /// # Errors
    ///
    /// [`StoreErrorKind::Unavailable`] for an I/O failure.
    fn list(&mut self) -> Result<Vec<EntrySummary>, StoreError>;

    /// Records that an entry was used at `now` (the eviction order). A
    /// driver may keep this in memory and write it later.
    ///
    /// # Errors
    ///
    /// [`StoreErrorKind::Unavailable`] for an I/O failure.
    fn touch(
        &mut self,
        partition: Partition,
        key: &CacheKey,
        now: UnixTime,
    ) -> Result<(), StoreError>;
}
