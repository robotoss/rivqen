// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Recovery (ADR-004 section 4). The steps run in this order:
//!
//! 1. remove the directories of other schema versions;
//! 2. remove every entry of `tmp/`;
//! 3. in one metadata transaction: remove the rows that do not decode,
//!    the disabled-key records whose time is before `now` (step 5), and
//!    the rows whose file is missing or has a length that no file of
//!    `content_bytes` has (step 4);
//! 4. after that commit: remove the files of the rows of step 4, then
//!    every entry of `files/` that no row names (step 3).
//!
//! Step 3 of the ADR runs last so that it sees the final set of rows, and
//! no file is removed before the removal of its row is durable. A removal
//! is not synced: a file that a power loss brings back is removed again
//! by the next recovery.

use std::collections::HashSet;

use rivqen_cache::driver::{RecoveryReport, StoreError};
use rivqen_model::time::UnixTime;

use super::entry::FileShape;
use super::layout::{
    Layout, ensure_dir, file_len, is_bucket_name, is_other_schema_name, list_dir, parse_file_name,
    remove_entry, remove_file,
};
use super::{FileStore, Life, as_unavailable};
use crate::crypto::{FileId, KeyProvider};
use crate::meta::MetaTxn;

const OP: &str = "driver.recover";

/// What the metadata step of recovery found.
struct RowsChecked {
    /// The file ids that a row names.
    named: HashSet<FileId>,
    /// The files of the removed rows that are still on disk.
    files_of_removed: Vec<FileId>,
    /// The rows removed (undecodable, file missing, size).
    removed: u64,
}

impl<P: KeyProvider> FileStore<P> {
    /// `StorageDriver::recover`.
    pub(super) fn recover_store(&mut self, now: UnixTime) -> Result<RecoveryReport, StoreError> {
        let layout = self.layout.clone();
        let shape = self.shape;
        remove_other_schemas(&layout)?;
        ensure_dir(&layout.tmp, OP)?;
        ensure_dir(&layout.files, OP)?;
        let mut report = RecoveryReport {
            orphans_removed: clear_tmp(&layout)?,
            entries_removed: 0,
        };
        let checked = self
            .db_mut(OP)?
            .transaction(|txn| check_rows(txn, &layout, shape, now))
            .map_err(as_unavailable)?;
        report.entries_removed = checked.removed;
        for id in &checked.files_of_removed {
            remove_file(&layout.file_path(id), OP)?;
        }
        report.orphans_removed = report
            .orphans_removed
            .saturating_add(remove_unnamed_files(&layout, &checked.named)?);
        self.life = Life::Running;
        Ok(report)
    }
}

/// Step 1: removes `<root>/schema-<n>` for every other `n`. Nothing else
/// in the root is touched: the root belongs to the adapter.
fn remove_other_schemas(layout: &Layout) -> Result<(), StoreError> {
    for entry in list_dir(&layout.root, OP)? {
        if entry
            .name
            .as_deref()
            .is_some_and(|name| is_other_schema_name(name, &layout.schema))
        {
            remove_entry(&entry.path, entry.is_dir, OP)?;
        }
    }
    Ok(())
}

/// Step 2: removes every entry of `tmp/`; gives the number removed.
fn clear_tmp(layout: &Layout) -> Result<u64, StoreError> {
    let mut removed: u64 = 0;
    for entry in list_dir(&layout.tmp, OP)? {
        remove_entry(&entry.path, entry.is_dir, OP)?;
        removed = removed.saturating_add(1);
    }
    Ok(removed)
}

/// Steps 4 and 5 inside the metadata transaction.
fn check_rows(
    txn: &mut MetaTxn<'_>,
    layout: &Layout,
    shape: FileShape,
    now: UnixTime,
) -> Result<RowsChecked, StoreError> {
    let mut removed = txn.remove_undecodable_rows()?;
    // Not counted in the report (DL-208 e).
    txn.purge_disabled(now)?;
    let mut named = HashSet::new();
    let mut files_of_removed = Vec::new();
    for row in txn.rows()? {
        let id = FileId::from_bytes(row.file_id);
        let len = file_len(&layout.file_path(&id), OP)?;
        if len.is_some_and(|len| shape.fits(len, row.content_bytes)) {
            named.insert(id);
            continue;
        }
        txn.delete_row(row.partition, &row.key)?;
        removed = removed.saturating_add(1);
        if len.is_some() {
            files_of_removed.push(id);
        }
    }
    Ok(RowsChecked {
        named,
        files_of_removed,
        removed,
    })
}

/// Step 3: removes every entry of `files/` that is not the file of a row:
/// a bucket with another name, a name that is not `<file-id>.rqf` of its
/// bucket, a file id that no row names, anything that is not a regular
/// file. Gives the number removed. An empty bucket stays.
fn remove_unnamed_files(layout: &Layout, named: &HashSet<FileId>) -> Result<u64, StoreError> {
    let mut removed: u64 = 0;
    for bucket in list_dir(&layout.files, OP)? {
        let Some(bucket_name) = bucket
            .name
            .as_deref()
            .filter(|name| bucket.is_dir && is_bucket_name(name))
        else {
            remove_entry(&bucket.path, bucket.is_dir, OP)?;
            removed = removed.saturating_add(1);
            continue;
        };
        for entry in list_dir(&bucket.path, OP)? {
            let keep = entry.is_file
                && entry
                    .name
                    .as_deref()
                    .and_then(|name| parse_file_name(bucket_name, name))
                    .is_some_and(|id| named.contains(&id));
            if !keep {
                remove_entry(&entry.path, entry.is_dir, OP)?;
                removed = removed.saturating_add(1);
            }
        }
    }
    Ok(removed)
}
