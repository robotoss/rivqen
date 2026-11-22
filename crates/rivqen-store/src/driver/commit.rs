// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The commit procedure of ADR-004 section 2: the five steps of
//! `CommitStep`, the precondition check inside the metadata transaction and
//! the fault points of the contract tests.

use std::fs::{self, File};
use std::path::{Path, PathBuf};

use rivqen_cache::driver::{StoreError, StoreErrorKind};
use rivqen_cache::key::Partition;
use rivqen_cache::partition::{site_matches, valid_host};
use rivqen_cache::policy::EntryPolicy;
use rivqen_cache::txn::{
    CacheTransaction, CommitStep, EntryMeta, Expect, Preconditions, Refused, SnapshotBody, TxnOp,
    TxnOutcome,
};
use rivqen_model::http::FieldLine;
use rivqen_model::ids::CacheKey;
use rivqen_model::time::UnixTime;

use super::entry::{aad_of, content_bytes, open_row};
use super::layout::{ensure_dir, remove_file, sync_dir, write_new};
use super::{FileStore, Life, as_unavailable, unavailable};
use crate::crypto::{
    DecodeLimits, FileId, KeyProvider, Role, SEAL_OVERHEAD, encode_file, encode_row, seal,
};
use crate::meta::{MAX_SEALED_BYTES, MetaTxn, Row};

const OP: &str = "driver.apply";

/// A transaction in the terms of the driver.
pub(super) enum Plan {
    Replace {
        meta: Box<EntryMeta>,
        body: Box<SnapshotBody>,
    },
    Update {
        partition: Partition,
        key: CacheKey,
        fields: Vec<FieldLine>,
        policy: EntryPolicy,
    },
    Remove {
        partition: Partition,
        key: CacheKey,
    },
    Disable {
        partition: Partition,
        key: CacheKey,
        until: UnixTime,
    },
    Revoke {
        partition: Partition,
    },
    Clear {
        domain: Option<String>,
    },
}

impl Plan {
    /// The plan of `op`; `None` for a kind that this driver does not know.
    fn of(op: TxnOp) -> Option<Self> {
        Some(match op {
            TxnOp::Replace { meta, body } => Self::Replace { meta, body },
            TxnOp::UpdateFields {
                partition,
                key,
                fields,
                policy,
            } => Self::Update {
                partition,
                key,
                fields,
                policy,
            },
            TxnOp::Remove { partition, key } => Self::Remove { partition, key },
            TxnOp::DisableKey {
                partition,
                key,
                until,
            } => Self::Disable {
                partition,
                key,
                until,
            },
            TxnOp::RevokePartition { partition } => Self::Revoke { partition },
            TxnOp::ClearSite { domain } => Self::Clear { domain },
            _ => return None,
        })
    }

    /// The address of a keyed kind. A revoke and a site clear have none:
    /// they are never refused.
    fn address(&self) -> Option<(Partition, CacheKey)> {
        match self {
            Self::Replace { meta, .. } => Some((meta.partition, meta.key)),
            Self::Update { partition, key, .. }
            | Self::Remove { partition, key }
            | Self::Disable { partition, key, .. } => Some((*partition, *key)),
            Self::Revoke { .. } | Self::Clear { .. } => None,
        }
    }
}

/// The new entry version of a `Replace`: its file and its sealed row.
struct Written {
    id: FileId,
    /// The open temporary file, for `SyncTemp`.
    temp: Option<File>,
    /// Where the file is now: in `tmp/` before `Promote`, in `files/`
    /// after it; `None` once a row names it.
    path: Option<PathBuf>,
    row_sealed: Vec<u8>,
    content_bytes: u64,
}

/// The state of one commit between its steps.
#[derive(Default)]
struct Work {
    written: Option<Written>,
    outcome: Option<TxnOutcome>,
    old: Vec<FileId>,
}

impl Work {
    /// Removes the new file of a commit that did not reach its commit
    /// point or was refused. A failure leaves an orphan that recovery
    /// removes (ADR-004 section 4), so it is not an error of the call: the
    /// call gives its own result.
    fn discard(&mut self) {
        if let Some(path) = self
            .written
            .as_mut()
            .and_then(|written| written.path.take())
        {
            remove_orphan(&path);
        }
    }
}

/// The current entry of a keyed kind, read inside the metadata
/// transaction.
enum Current {
    Absent,
    Present(Row),
    /// The row does not decode: an entry with no known version.
    Broken,
}

impl Current {
    fn read(txn: &MetaTxn<'_>, partition: Partition, key: &CacheKey) -> Result<Self, StoreError> {
        match txn.row(partition, key) {
            Ok(Some(row)) => Ok(Self::Present(row)),
            Ok(None) => Ok(Self::Absent),
            Err(error) if error.kind() == StoreErrorKind::Corrupted => Ok(Self::Broken),
            Err(error) => Err(error),
        }
    }

    fn file(&self) -> Option<FileId> {
        match self {
            Self::Present(row) => Some(FileId::from_bytes(row.file_id)),
            Self::Absent | Self::Broken => None,
        }
    }
}

/// The checks of a keyed kind at the commit point, in the order of
/// `rivqen_cache::txn` (DL-200, DL-205, DL-208): request partition epoch,
/// target partition epoch, site epoch, host (`Replace`), entry.
fn check(
    txn: &MetaTxn<'_>,
    plan: &Plan,
    pre: &Preconditions,
    partition: Partition,
    key: &CacheKey,
) -> Result<Result<Current, Refused>, StoreError> {
    if let Some((request, epoch)) = pre.request
        && txn.partition_epoch(Partition::Scoped(request))? != epoch
    {
        return Ok(Err(Refused::PartitionRevoked));
    }
    if txn.partition_epoch(partition)? != pre.partition_epoch {
        return Ok(Err(Refused::PartitionRevoked));
    }
    if txn.site_epoch()? != pre.site_epoch {
        return Ok(Err(Refused::SiteCleared));
    }
    if let Plan::Replace { meta, .. } = plan
        && !valid_host(&meta.host)
    {
        return Ok(Err(Refused::InvalidHost));
    }
    let current = Current::read(txn, partition, key)?;
    let holds = match pre.entry {
        Expect::Any => true,
        Expect::Absent => matches!(current, Current::Absent),
        Expect::Version(version) => {
            matches!(&current, Current::Present(row) if row.version == version)
        }
    };
    let update_without_entry =
        matches!(plan, Plan::Update { .. }) && !matches!(current, Current::Present(_));
    if !holds || update_without_entry {
        return Ok(Err(Refused::Entry));
    }
    Ok(Ok(current))
}

/// DL-211: the sealed row of a `Replace` must fit the `sealed` column.
fn check_row_size(plain_len: usize) -> Result<(), StoreError> {
    match plain_len.checked_add(SEAL_OVERHEAD) {
        Some(sealed) if sealed <= MAX_SEALED_BYTES => Ok(()),
        _ => Err(unavailable(OP)),
    }
}

impl<P: KeyProvider> FileStore<P> {
    /// `StorageDriver::apply` with the fault points `stop` and `fail`.
    pub(super) fn commit(
        &mut self,
        txn: CacheTransaction,
        stop: Option<CommitStep>,
        fail: Option<CommitStep>,
    ) -> Result<TxnOutcome, StoreError> {
        let CacheTransaction { op, pre } = txn;
        let plan = Plan::of(op).ok_or_else(|| unavailable(OP))?;
        // DL-211: before the first step, so nothing is written.
        if let Plan::Replace { meta, body } = &plan {
            check_row_size(encode_row(meta, &body.fields).len())?;
        }
        let mut work = Work::default();
        for step in CommitStep::ALL {
            if fail == Some(step) {
                if step == CommitStep::RemoveOld {
                    // The commit stands; recovery removes the old files.
                    return work.outcome.ok_or_else(|| unavailable(OP));
                }
                work.discard();
                return Err(unavailable(OP));
            }
            if let Err(error) = self.step(step, &plan, &pre, &mut work) {
                // Only the steps before the commit point fail; `RemoveOld`
                // never gives an error.
                work.discard();
                return Err(as_unavailable(error));
            }
            if stop == Some(step) {
                // As if the process died here: no cleanup.
                self.life = Life::Stopped;
                return Err(unavailable(OP));
            }
        }
        work.outcome.ok_or_else(|| unavailable(OP))
    }

    fn step(
        &mut self,
        step: CommitStep,
        plan: &Plan,
        pre: &Preconditions,
        work: &mut Work,
    ) -> Result<(), StoreError> {
        match step {
            CommitStep::WriteTemp => {
                // A `Replace` with a host that the check refuses writes no
                // file; the epochs are still checked first (DL-208).
                if let Plan::Replace { meta, body } = plan
                    && valid_host(&meta.host)
                {
                    work.written = Some(self.write_temp(meta, body)?);
                }
                Ok(())
            }
            CommitStep::SyncTemp => match work.written.as_ref().and_then(|w| w.temp.as_ref()) {
                // ADR-004 section 2, `SyncTemp`.
                Some(file) => file.sync_all().map_err(|_| unavailable(OP)),
                None => Ok(()),
            },
            CommitStep::Promote => match work.written.as_mut() {
                Some(written) => self.promote(written),
                None => Ok(()),
            },
            CommitStep::CommitMeta => {
                let (outcome, old) = self.commit_meta(plan, pre, work.written.as_ref())?;
                match outcome {
                    TxnOutcome::Refused(_) => work.discard(),
                    TxnOutcome::Applied { .. } => {
                        // A row names the new file now.
                        if let Some(written) = work.written.as_mut() {
                            written.path = None;
                        }
                    }
                }
                work.outcome = Some(outcome);
                work.old = old;
                Ok(())
            }
            CommitStep::RemoveOld => {
                // No sync (ADR-004 section 2): a file that comes back is an
                // orphan that recovery removes, so a failure here is not an
                // error of the call (DL-200 h).
                for id in &work.old {
                    remove_orphan(&self.layout.file_path(id));
                }
                Ok(())
            }
        }
    }

    /// `WriteTemp`: seals the content and the row, writes the file to
    /// `tmp/<file-id>.tmp`.
    fn write_temp(&mut self, meta: &EntryMeta, body: &SnapshotBody) -> Result<Written, StoreError> {
        let id = FileId::new(&mut self.keys)?;
        let data_key = self.keys.data_key(meta.partition)?;
        let file_aad = aad_of(Role::File, meta.partition, &meta.key, &id);
        let sealed = seal(&data_key, &mut self.keys, &file_aad, &encode_file(body))?;
        let row_aad = aad_of(Role::Row, meta.partition, &meta.key, &id);
        let row_sealed = seal(
            &data_key,
            &mut self.keys,
            &row_aad,
            &encode_row(meta, &body.fields),
        )?;
        let path = self.layout.temp_path(&id);
        // A part of the file may be on disk after a failure.
        let temp = write_new(&path, &sealed, OP).inspect_err(|_| remove_orphan(&path))?;
        Ok(Written {
            id,
            temp: Some(temp),
            path: Some(path),
            row_sealed,
            content_bytes: content_bytes(&body.page, body.template.as_ref(), &body.data),
        })
    }

    /// `Promote`: renames the file to `files/<hh>/<file-id>.rqf` and syncs
    /// `files/<hh>` and `tmp`.
    fn promote(&self, written: &mut Written) -> Result<(), StoreError> {
        let Some(from) = written.path.clone() else {
            return Err(unavailable(OP));
        };
        drop(written.temp.take());
        let bucket = self.layout.bucket(&written.id);
        // A new bucket: create it and sync `files/`.
        ensure_dir(&bucket, OP)?;
        let to = self.layout.file_path(&written.id);
        // A file id is 16 random bytes; a name that exists is never
        // replaced.
        if fs::symlink_metadata(&to).is_ok() {
            return Err(unavailable(OP));
        }
        fs::rename(&from, &to).map_err(|_| unavailable(OP))?;
        written.path = Some(to);
        sync_dir(&bucket, OP)?;
        sync_dir(&self.layout.tmp, OP)
    }

    /// `CommitMeta`: deletes a key first when the plan says so, then runs
    /// the checks and the change in one metadata transaction. Gives the
    /// outcome and the files that no row names any more.
    fn commit_meta(
        &mut self,
        plan: &Plan,
        pre: &Preconditions,
        written: Option<&Written>,
    ) -> Result<(TxnOutcome, Vec<FileId>), StoreError> {
        // ADR-004 section 2 and ADR-011 section 4: the key goes first, so
        // a file that a power loss brings back cannot be opened.
        match plan {
            Plan::Revoke { partition } => self.keys.delete_key(*partition)?,
            Plan::Clear { domain: None } => self.keys.delete_key(Partition::Public)?,
            _ => {}
        }
        let Self {
            db, keys, limits, ..
        } = self;
        let db = db.as_mut().ok_or_else(|| unavailable(OP))?;
        db.transaction(|txn| {
            let current = match plan.address() {
                Some((partition, key)) => match check(txn, plan, pre, partition, &key)? {
                    Ok(current) => Some(current),
                    Err(refused) => return Ok((TxnOutcome::Refused(refused), Vec::new())),
                },
                None => None,
            };
            change(txn, plan, current, written, keys, limits)
        })
    }
}

/// The metadata change of an applied plan.
fn change<P: KeyProvider>(
    txn: &mut MetaTxn<'_>,
    plan: &Plan,
    current: Option<Current>,
    written: Option<&Written>,
    keys: &mut P,
    limits: &DecodeLimits,
) -> Result<(TxnOutcome, Vec<FileId>), StoreError> {
    let old: Vec<FileId> = current
        .as_ref()
        .and_then(Current::file)
        .into_iter()
        .collect();
    match plan {
        Plan::Replace { meta, .. } => {
            let written = written.ok_or_else(|| unavailable(OP))?;
            let version = txn.next_version()?;
            txn.put_row(&Row {
                partition: meta.partition,
                key: meta.key,
                host: meta.host.clone(),
                version,
                file_id: *written.id.as_bytes(),
                content_bytes: written.content_bytes,
                stored_at: meta.stored_at,
                last_used_at: meta.last_used_at,
                sealed: written.row_sealed.clone(),
            })?;
            Ok((
                TxnOutcome::Applied {
                    version: Some(version),
                },
                old,
            ))
        }
        Plan::Update {
            partition,
            fields,
            policy,
            ..
        } => {
            let Some(Current::Present(row)) = current else {
                return Err(unavailable(OP));
            };
            update_fields(txn, row, *partition, fields, *policy, keys, limits)
        }
        Plan::Remove { partition, key } => {
            txn.delete_row(*partition, key)?;
            Ok((TxnOutcome::Applied { version: None }, old))
        }
        Plan::Disable {
            partition,
            key,
            until,
        } => {
            txn.delete_row(*partition, key)?;
            txn.disable(*partition, key, *until)?;
            Ok((TxnOutcome::Applied { version: None }, old))
        }
        Plan::Revoke { partition } => {
            // A row that does not decode is removed with the others; its
            // file is an orphan for recovery.
            let old = match txn.rows_of_partition(*partition) {
                Ok(rows) => rows
                    .iter()
                    .map(|row| FileId::from_bytes(row.file_id))
                    .collect(),
                Err(error) if error.kind() == StoreErrorKind::Corrupted => Vec::new(),
                Err(error) => return Err(error),
            };
            txn.delete_rows_of_partition(*partition)?;
            txn.delete_disabled_of_partition(*partition)?;
            txn.bump_partition_epoch(*partition)?;
            Ok((TxnOutcome::Applied { version: None }, old))
        }
        Plan::Clear { domain } => clear_site(txn, domain.as_deref()),
    }
}

/// `UpdateFields`: opens the row, sets the fields and the freshness inputs
/// and seals it again with the same file id (ADR-011 section 3).
fn update_fields<P: KeyProvider>(
    txn: &mut MetaTxn<'_>,
    row: Row,
    partition: Partition,
    fields: &[FieldLine],
    policy: EntryPolicy,
    keys: &mut P,
    limits: &DecodeLimits,
) -> Result<(TxnOutcome, Vec<FileId>), StoreError> {
    let data_key = keys.data_key(partition)?;
    // A row that does not open cannot be updated; the next read finds it
    // and removes it.
    let mut content = open_row(&data_key, &row, limits).map_err(as_unavailable)?;
    content.fields = fields.to_vec();
    content.policy = policy;
    let row_plain = content.encode();
    // DL-211: an update whose row does not fit writes nothing.
    check_row_size(row_plain.len())?;
    let id = FileId::from_bytes(row.file_id);
    let sealed = seal(
        &data_key,
        keys,
        &aad_of(Role::Row, row.partition, &row.key, &id),
        &row_plain,
    )?;
    let version = txn.next_version()?;
    txn.put_row(&Row {
        version,
        sealed,
        ..row
    })?;
    Ok((
        TxnOutcome::Applied {
            version: Some(version),
        },
        Vec::new(),
    ))
}

/// `ClearSite`: removes the rows of the site in every partition (every row
/// for `None`) and increments the site epoch. `None` also removes every
/// disabled-key record (DL-200 f).
fn clear_site(
    txn: &mut MetaTxn<'_>,
    domain: Option<&str>,
) -> Result<(TxnOutcome, Vec<FileId>), StoreError> {
    // A row whose host cannot be read may be of the site: remove it (fail
    // closed, R-CACHE-09).
    let rows = match txn.rows() {
        Ok(rows) => rows,
        Err(error) if error.kind() == StoreErrorKind::Corrupted => {
            txn.remove_undecodable_rows()?;
            txn.rows()?
        }
        Err(error) => return Err(error),
    };
    let mut old = Vec::new();
    for row in rows
        .iter()
        .filter(|row| domain.is_none_or(|domain| site_matches(&row.host, domain)))
    {
        txn.delete_row(row.partition, &row.key)?;
        old.push(FileId::from_bytes(row.file_id));
    }
    if domain.is_none() {
        txn.delete_all_disabled()?;
    }
    txn.bump_site_epoch()?;
    Ok((TxnOutcome::Applied { version: None }, old))
}

/// Removes a file that no row names, on a path where the call gives its
/// own result. A failure leaves an orphan: recovery removes every file
/// that no row names (ADR-004 section 4, step 3), which is how this error
/// is handled.
fn remove_orphan(path: &Path) {
    match remove_file(path, OP) {
        Ok(()) | Err(_) => {}
    }
}
