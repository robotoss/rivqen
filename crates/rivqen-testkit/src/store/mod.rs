// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! A fake storage driver with a fault model (WP-07 S1, T-05; DL-181).
//!
//! Owner: T-05. [`FakeStore`] implements
//! [`rivqen_cache::driver::StorageDriver`] in memory. It runs each
//! transaction in the steps of [`CommitStep`], can stop after any step, drops
//! the data that was not synced, and recovers at the next start. No file, no
//! thread, no system time.
//!
//! The fault model (storage section 2):
//!
//! | Step | What the step does | Durable after a power loss |
//! |---|---|---|
//! | `WriteTemp` | writes the new content to a temporary file | no |
//! | `SyncTemp` | syncs the temporary file | yes (as a temporary file) |
//! | `Promote` | renames it to the entry place and syncs the directory | yes |
//! | `CommitMeta` | checks the preconditions (in the order of `rivqen_cache::txn`) and commits the metadata: the commit point | yes |
//! | `RemoveOld` | deletes the files that the commit no longer references | no (recovery deletes them) |
//!
//! A transaction that writes no content (every kind except `Replace`) has
//! nothing to do in `WriteTemp`, `SyncTemp` and `Promote`; a stop or a
//! failure at such a step still happens there.
//!
//! [`StorageDriver::recover`] removes the files without a row, the rows
//! without a fitting file and the disabled-key records whose time is
//! before its `now` (DL-205), and syncs what it removes.
//!
//! [`contract`] holds the driver contract suite: checks that every driver
//! passes, generic over a driver factory, through [`StorageDriver`] and
//! [`FaultInjection`] only.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use rivqen_cache::driver::{
    EntrySummary, RecoveryReport, StorageDriver, StoreError, StoreErrorKind,
};
use rivqen_cache::key::Partition;
use rivqen_cache::partition::{site_matches, valid_host};
use rivqen_cache::policy::EntryPolicy;
use rivqen_cache::txn::{
    CacheTransaction, CommitStep, EntryMeta, EntryVersion, Epoch, Expect, Preconditions, Refused,
    SnapshotBody, StoredEntry, TxnOp, TxnOutcome,
};
use rivqen_model::http::FieldLine;
use rivqen_model::ids::CacheKey;
use rivqen_model::safe::PageBytes;
use rivqen_model::time::UnixTime;

/// How a test damages the stored files of one entry. The metadata stays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Damage {
    /// One byte of the stored page changes (a byte is added to an empty
    /// page). The size of the files stays the same for a page that is not
    /// empty.
    Content,
    /// The files of the entry are gone.
    Missing,
    /// The files of another entry are copied over the files of this entry
    /// (a moved file; H-147 "move" test).
    MovedFrom {
        /// The partition of the source entry.
        partition: Partition,
        /// The key of the source entry.
        key: CacheKey,
    },
}

/// The fault hooks that the contract suite needs in addition to
/// [`StorageDriver`]. The reference driver of S2 implements them with
/// in-process stop points; [`FakeStore`] implements them in memory.
pub trait FaultInjection: StorageDriver {
    /// The next [`StorageDriver::apply`] stops after `step`, as if the
    /// process died there, and returns [`StoreErrorKind::Unavailable`].
    /// Every later call fails until [`FaultInjection::restart`].
    fn stop_after(&mut self, step: CommitStep);

    /// The next [`StorageDriver::apply`] gets an I/O failure at `step` (for
    /// example a full disk). Before the commit point it returns
    /// [`StoreErrorKind::Unavailable`] and the store holds the state before
    /// the call; at `RemoveOld` the commit stands and the call succeeds.
    fn fail_at(&mut self, step: CommitStep);

    /// Starts the driver again on the same storage. The next call must be
    /// [`StorageDriver::recover`].
    ///
    /// # Errors
    ///
    /// [`StoreErrorKind::Unavailable`] when the storage cannot be opened.
    fn restart(&mut self) -> Result<(), StoreError>;

    /// Damages the stored files of the entry of `key` in `partition`.
    /// Returns `false` and changes nothing when the entry (or, for
    /// [`Damage::MovedFrom`], the source entry) does not exist or is the
    /// same entry.
    ///
    /// # Errors
    ///
    /// [`StoreErrorKind::Unavailable`] when the storage cannot be written.
    fn damage(
        &mut self,
        partition: Partition,
        key: &CacheKey,
        damage: Damage,
    ) -> Result<bool, StoreError>;
}

/// What a stop loses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StopKind {
    /// Power loss: every write that no step synced is lost (DL-181). The
    /// default: the stricter model.
    #[default]
    PowerLoss,
    /// The process is killed: the operating system keeps the file writes;
    /// only the memory of the process is lost.
    ProcessKill,
}

/// A defect that a test builds into [`FakeStore`] to show that the
/// contract suite finds it. Never use one outside such a test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Defect {
    /// `Promote` deletes the files of the current entry: before the commit
    /// point.
    RemoveOldBeforeCommit,
    /// `Promote` does not sync the directory: the rename is lost at a power
    /// loss.
    UnsyncedPromote,
    /// `CommitMeta` does not check the preconditions.
    NoPreconditions,
    /// `read` finds an entry by its key in any partition.
    ReadIgnoresPartition,
    /// `CommitMeta` does not check the epoch of the request partition
    /// (DL-205).
    NoRequestCheck,
    /// `CommitMeta` stores a `Replace` with any host (DL-205).
    AnyHost,
    /// `recover` keeps the disabled-key records whose time has passed
    /// (DL-205).
    KeepsExpiredDisabledKeys,
}

/// The number of files in one layer of a [`FakeStore`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FileCount {
    /// Files at the temporary place.
    pub temp: usize,
    /// Files at an entry place.
    pub entry: usize,
}

/// A partition as a sortable map key: `Public` first, then the scoped ids.
type PartRank = (u8, [u8; 32]);
/// One entry place: partition and key.
type Slot = (PartRank, CacheKey);

fn rank(partition: Partition) -> PartRank {
    match partition {
        Partition::Public => (0, [0; 32]),
        Partition::Scoped(id) => (1, *id.as_bytes()),
    }
}

fn unavailable(op: &'static str) -> StoreError {
    StoreError::new(StoreErrorKind::Unavailable, op)
}

fn len_u64(len: usize) -> u64 {
    u64::try_from(len).unwrap_or(u64::MAX)
}

fn field_bytes(fields: &[FieldLine]) -> u64 {
    fields.iter().fold(0_u64, |total, line| {
        total
            .saturating_add(len_u64(line.name.len()))
            .saturating_add(len_u64(line.value.len()))
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Place {
    Temp,
    Entry,
}

/// The content files of one entry: everything of a [`SnapshotBody`] but the
/// page fields, which the metadata row holds.
#[derive(Clone, PartialEq, Eq)]
struct Content {
    page: PageBytes,
    template: Option<PageBytes>,
    data: Vec<(String, PageBytes)>,
}

impl Content {
    fn of(body: &SnapshotBody) -> Self {
        Self {
            page: body.page.clone(),
            template: body.template.clone(),
            data: body.data.clone(),
        }
    }

    /// The bytes of the content as [`SnapshotBody::size_bytes`] counts them.
    fn bytes(&self) -> u64 {
        let mut total = len_u64(self.page.len());
        if let Some(template) = &self.template {
            total = total.saturating_add(len_u64(template.len()));
        }
        for (key, value) in &self.data {
            total = total
                .saturating_add(len_u64(key.len()))
                .saturating_add(len_u64(value.len()));
        }
        total
    }
}

/// FNV-1a, 64 bits: a stand-in for the record integrity of S2 (ADR-011).
/// It finds every change of one byte; it is no defence against an attacker.
struct Fnv(u64);

impl Fnv {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    fn part(&mut self, bytes: &[u8]) {
        self.write(&len_u64(bytes.len()).to_be_bytes());
        self.write(bytes);
    }
}

/// The seal of a content file: over the place (partition, key, file id) as
/// associated data and over every byte of the content.
fn seal(slot: &Slot, file: u64, content: &Content) -> u64 {
    let mut hash = Fnv(Fnv::OFFSET);
    hash.write(b"rivqen-fake-store-seal\0");
    hash.write(&[slot.0.0]);
    hash.write(&slot.0.1);
    hash.write(slot.1.as_bytes());
    hash.write(&file.to_be_bytes());
    hash.part(content.page.as_slice());
    match &content.template {
        Some(template) => {
            hash.write(&[1]);
            hash.part(template.as_slice());
        }
        None => hash.write(&[0]),
    }
    hash.write(&len_u64(content.data.len()).to_be_bytes());
    for (key, value) in &content.data {
        hash.part(key.as_bytes());
        hash.part(value.as_slice());
    }
    hash.0
}

#[derive(Clone)]
struct File {
    place: Place,
    content: Content,
    seal: u64,
}

/// One metadata row.
#[derive(Clone)]
struct Row {
    meta: EntryMeta,
    version: EntryVersion,
    fields: Vec<FieldLine>,
    file: u64,
    content_bytes: u64,
}

/// One layer of the store: the files and the metadata database.
#[derive(Clone)]
struct Disk {
    files: BTreeMap<u64, File>,
    rows: BTreeMap<Slot, Row>,
    epochs: BTreeMap<PartRank, Epoch>,
    site_epoch: Epoch,
    disabled: BTreeMap<Slot, UnixTime>,
}

impl Disk {
    fn empty() -> Self {
        Self {
            files: BTreeMap::new(),
            rows: BTreeMap::new(),
            epochs: BTreeMap::new(),
            site_epoch: Epoch(0),
            disabled: BTreeMap::new(),
        }
    }

    fn count(&self) -> FileCount {
        let temp = self
            .files
            .values()
            .filter(|file| file.place == Place::Temp)
            .count();
        FileCount {
            temp,
            entry: self.files.len().saturating_sub(temp),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Life {
    NeedsRecover,
    Running,
    Stopped,
}

/// A transaction in the terms of the store. `None` from [`Plan::of`] for an
/// operation that this store does not know.
enum Plan {
    Replace {
        slot: Slot,
        meta: Box<EntryMeta>,
        body: Box<SnapshotBody>,
    },
    Update {
        slot: Slot,
        fields: Vec<FieldLine>,
        policy: EntryPolicy,
    },
    Remove {
        slot: Slot,
    },
    Disable {
        slot: Slot,
        until: UnixTime,
    },
    Revoke {
        part: PartRank,
    },
    Clear {
        domain: Option<String>,
    },
}

impl Plan {
    fn of(op: TxnOp) -> Option<Self> {
        Some(match op {
            TxnOp::Replace { meta, body } => Self::Replace {
                slot: (rank(meta.partition), meta.key),
                meta,
                body,
            },
            TxnOp::UpdateFields {
                partition,
                key,
                fields,
                policy,
            } => Self::Update {
                slot: (rank(partition), key),
                fields,
                policy,
            },
            TxnOp::Remove { partition, key } => Self::Remove {
                slot: (rank(partition), key),
            },
            TxnOp::DisableKey {
                partition,
                key,
                until,
            } => Self::Disable {
                slot: (rank(partition), key),
                until,
            },
            TxnOp::RevokePartition { partition } => Self::Revoke {
                part: rank(partition),
            },
            TxnOp::ClearSite { domain } => Self::Clear { domain },
            _ => return None,
        })
    }

    /// The entry place of a keyed transaction. A revoke and a site clear
    /// have none: they are never refused.
    fn slot(&self) -> Option<Slot> {
        match self {
            Self::Replace { slot, .. }
            | Self::Update { slot, .. }
            | Self::Remove { slot }
            | Self::Disable { slot, .. } => Some(*slot),
            Self::Revoke { .. } | Self::Clear { .. } => None,
        }
    }
}

#[derive(Default)]
struct Faults {
    stop: Option<CommitStep>,
    fail: Option<CommitStep>,
    between: Option<(CommitStep, CacheTransaction)>,
}

/// An in-memory storage driver with two layers: `durable` survives a stop;
/// `live` is what the running process sees (durable plus the pending
/// writes). A stop and [`FaultInjection::restart`] replace `live` with
/// `durable`.
///
/// A new store, and a store after a restart, answers every call with
/// [`StoreErrorKind::Unavailable`] until [`StorageDriver::recover`] runs
/// (the trait: recover runs before any other call).
pub struct FakeStore {
    durable: Disk,
    live: Disk,
    life: Life,
    stop: Option<CommitStep>,
    fail: Option<CommitStep>,
    between: Option<(CommitStep, CacheTransaction)>,
    between_outcome: Option<Result<TxnOutcome, StoreError>>,
    stop_kind: StopKind,
    defect: Option<Defect>,
    // File names and versions are never used twice, also across a restart
    // (a real driver uses random file names and a durable counter).
    next_file: u64,
    last_version: u64,
}

impl Default for FakeStore {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for FakeStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FakeStore")
            .field("life", &self.life)
            .field("entries", &self.live.rows.len())
            .field("files", &self.live.count())
            .field("stop_kind", &self.stop_kind)
            .field("defect", &self.defect)
            .finish_non_exhaustive()
    }
}

impl FakeStore {
    /// An empty store. Call [`StorageDriver::recover`] first.
    #[must_use]
    pub fn new() -> Self {
        Self {
            durable: Disk::empty(),
            live: Disk::empty(),
            life: Life::NeedsRecover,
            stop: None,
            fail: None,
            between: None,
            between_outcome: None,
            stop_kind: StopKind::PowerLoss,
            defect: None,
            next_file: 1,
            last_version: 0,
        }
    }

    /// The same store with another [`StopKind`].
    #[must_use]
    pub fn with_stop_kind(mut self, kind: StopKind) -> Self {
        self.stop_kind = kind;
        self
    }

    /// The same store with a built-in [`Defect`].
    #[must_use]
    pub fn with_defect(mut self, defect: Defect) -> Self {
        self.defect = Some(defect);
        self
    }

    /// The next [`StorageDriver::apply`] runs `txn` completely right after
    /// its step `after`: a second writer between two steps of a commit. The
    /// result is in [`FakeStore::between_outcome`].
    pub fn run_between(&mut self, after: CommitStep, txn: CacheTransaction) {
        self.between = Some((after, txn));
    }

    /// The result of the transaction of [`FakeStore::run_between`], once.
    pub fn between_outcome(&mut self) -> Option<Result<TxnOutcome, StoreError>> {
        self.between_outcome.take()
    }

    /// The files that the running store sees.
    #[must_use]
    pub fn files(&self) -> FileCount {
        self.live.count()
    }

    /// The files that survive a power loss now.
    #[must_use]
    pub fn durable_files(&self) -> FileCount {
        self.durable.count()
    }

    fn ready(&self, op: &'static str) -> Result<(), StoreError> {
        match self.life {
            Life::Running => Ok(()),
            Life::NeedsRecover | Life::Stopped => Err(unavailable(op)),
        }
    }

    fn epoch_of(&self, part: &PartRank) -> Epoch {
        self.live.epochs.get(part).copied().unwrap_or(Epoch(0))
    }

    fn disabled_at(&self, slot: &Slot, now: UnixTime) -> bool {
        self.live
            .disabled
            .get(slot)
            .is_some_and(|until| now < *until)
    }

    /// The metadata commit: the metadata of `live` becomes durable.
    fn sync_meta(&mut self) {
        self.durable.rows = self.live.rows.clone();
        self.durable.epochs = self.live.epochs.clone();
        self.durable.site_epoch = self.live.site_epoch;
        self.durable.disabled = self.live.disabled.clone();
    }

    fn new_version(&mut self) -> Result<EntryVersion, StoreError> {
        let next = self
            .last_version
            .checked_add(1)
            .ok_or_else(|| unavailable("apply"))?;
        self.last_version = next;
        Ok(EntryVersion(next))
    }

    fn write_temp(&mut self, plan: &Plan) -> Result<Option<u64>, StoreError> {
        let Plan::Replace { slot, body, .. } = plan else {
            return Ok(None);
        };
        let id = self.next_file;
        self.next_file = id.checked_add(1).ok_or_else(|| unavailable("apply"))?;
        let content = Content::of(body);
        let file = File {
            place: Place::Temp,
            seal: seal(slot, id, &content),
            content,
        };
        self.live.files.insert(id, file);
        Ok(Some(id))
    }

    fn sync_file(&mut self, id: Option<u64>) {
        if let Some(id) = id
            && let Some(file) = self.live.files.get(&id)
        {
            self.durable.files.insert(id, file.clone());
        }
    }

    fn promote(&mut self, id: Option<u64>, plan: &Plan) {
        if let Some(id) = id {
            if let Some(file) = self.live.files.get_mut(&id) {
                file.place = Place::Entry;
            }
            if self.defect != Some(Defect::UnsyncedPromote)
                && let Some(file) = self.durable.files.get_mut(&id)
            {
                file.place = Place::Entry;
            }
        }
        if self.defect == Some(Defect::RemoveOldBeforeCommit) {
            let old = plan
                .slot()
                .and_then(|slot| self.live.rows.get(&slot))
                .map(|row| row.file);
            if let Some(old) = old {
                self.live.files.remove(&old);
                self.durable.files.remove(&old);
            }
        }
    }

    /// An unsynced delete of the new file.
    fn drop_file(&mut self, id: Option<u64>) {
        if let Some(id) = id {
            self.live.files.remove(&id);
        }
    }

    /// The checks of a keyed kind in the order of `rivqen_cache::txn`:
    /// request partition epoch, target partition epoch, site epoch, host
    /// (`Replace`), entry.
    fn refusal(&self, plan: &Plan, pre: &Preconditions) -> Option<Refused> {
        let slot = plan.slot()?;
        if self.defect != Some(Defect::NoRequestCheck)
            && let Some((request, epoch)) = pre.writer.request_epoch()
            && self.epoch_of(&rank(Partition::Scoped(request))) != epoch
        {
            return Some(Refused::PartitionRevoked);
        }
        if self.epoch_of(&slot.0) != pre.partition_epoch {
            return Some(Refused::PartitionRevoked);
        }
        if self.live.site_epoch != pre.site_epoch {
            return Some(Refused::SiteCleared);
        }
        if self.defect != Some(Defect::AnyHost)
            && let Plan::Replace { meta, .. } = plan
            && !valid_host(&meta.host)
        {
            return Some(Refused::InvalidHost);
        }
        let current = self.live.rows.get(&slot).map(|row| row.version);
        let holds = match pre.entry {
            Expect::Any => true,
            Expect::Absent => current.is_none(),
            Expect::Version(version) => current == Some(version),
        };
        let update_without_entry = matches!(plan, Plan::Update { .. }) && current.is_none();
        if !holds || update_without_entry {
            return Some(Refused::Entry);
        }
        None
    }

    fn remove_row(&mut self, slot: &Slot, old: &mut Vec<u64>) {
        if let Some(row) = self.live.rows.remove(slot) {
            old.push(row.file);
        }
    }

    /// Step 4: the precondition check and the metadata change in one
    /// metadata transaction. Returns the outcome and the files that the
    /// commit no longer references.
    fn commit(
        &mut self,
        plan: &Plan,
        pre: &Preconditions,
        new_file: Option<u64>,
    ) -> Result<(TxnOutcome, Vec<u64>), StoreError> {
        if self.defect != Some(Defect::NoPreconditions)
            && let Some(refused) = self.refusal(plan, pre)
        {
            return Ok((TxnOutcome::Refused(refused), Vec::new()));
        }
        let mut old = Vec::new();
        let version = match plan {
            Plan::Replace { slot, meta, body } => {
                let file = new_file.ok_or_else(|| unavailable("apply"))?;
                let content_bytes = self
                    .live
                    .files
                    .get(&file)
                    .map(|file| file.content.bytes())
                    .ok_or_else(|| unavailable("apply"))?;
                let version = self.new_version()?;
                let row = Row {
                    meta: (**meta).clone(),
                    version,
                    fields: body.fields.clone(),
                    file,
                    content_bytes,
                };
                if let Some(previous) = self.live.rows.insert(*slot, row) {
                    old.push(previous.file);
                }
                Some(version)
            }
            Plan::Update {
                slot,
                fields,
                policy,
            } => {
                if !self.live.rows.contains_key(slot) {
                    return Ok((TxnOutcome::Refused(Refused::Entry), Vec::new()));
                }
                let version = self.new_version()?;
                if let Some(row) = self.live.rows.get_mut(slot) {
                    row.fields.clone_from(fields);
                    row.meta.policy = *policy;
                    row.version = version;
                }
                Some(version)
            }
            Plan::Remove { slot } => {
                self.remove_row(slot, &mut old);
                None
            }
            Plan::Disable { slot, until } => {
                self.remove_row(slot, &mut old);
                self.live.disabled.insert(*slot, *until);
                None
            }
            Plan::Revoke { part } => {
                self.revoke(part, &mut old)?;
                None
            }
            Plan::Clear { domain } => {
                self.clear(domain.as_deref(), &mut old)?;
                None
            }
        };
        self.sync_meta();
        Ok((TxnOutcome::Applied { version }, old))
    }

    fn revoke(&mut self, part: &PartRank, old: &mut Vec<u64>) -> Result<(), StoreError> {
        let next = self
            .epoch_of(part)
            .0
            .checked_add(1)
            .ok_or_else(|| unavailable("apply"))?;
        let slots: Vec<Slot> = self
            .live
            .rows
            .keys()
            .filter(|slot| slot.0 == *part)
            .copied()
            .collect();
        for slot in &slots {
            self.remove_row(slot, old);
        }
        self.live.disabled.retain(|slot, _| slot.0 != *part);
        self.live.epochs.insert(*part, Epoch(next));
        Ok(())
    }

    fn clear(&mut self, domain: Option<&str>, old: &mut Vec<u64>) -> Result<(), StoreError> {
        let next = self
            .live
            .site_epoch
            .0
            .checked_add(1)
            .ok_or_else(|| unavailable("apply"))?;
        let slots: Vec<Slot> = self
            .live
            .rows
            .iter()
            .filter(|(_, row)| domain.is_none_or(|domain| site_matches(&row.meta.host, domain)))
            .map(|(slot, _)| *slot)
            .collect();
        for slot in &slots {
            self.remove_row(slot, old);
        }
        if domain.is_none() {
            self.live.disabled.clear();
        }
        self.live.site_epoch = Epoch(next);
        Ok(())
    }

    fn execute(&mut self, txn: CacheTransaction, faults: Faults) -> Result<TxnOutcome, StoreError> {
        let CacheTransaction { op, pre } = txn;
        let plan = Plan::of(op).ok_or_else(|| unavailable("apply"))?;
        let mut between = faults.between;
        let mut new_file = None;
        let mut outcome = None;
        let mut old_files = Vec::new();
        for step in CommitStep::ALL {
            if faults.fail == Some(step) {
                if step == CommitStep::RemoveOld {
                    // The commit stands; recovery deletes the old files.
                    return outcome.ok_or_else(|| unavailable("apply"));
                }
                self.drop_file(new_file);
                return Err(unavailable("apply"));
            }
            match step {
                CommitStep::WriteTemp => new_file = self.write_temp(&plan)?,
                CommitStep::SyncTemp => self.sync_file(new_file),
                CommitStep::Promote => self.promote(new_file, &plan),
                CommitStep::CommitMeta => match self.commit(&plan, &pre, new_file) {
                    Ok((done, old)) => {
                        if matches!(done, TxnOutcome::Refused(_)) {
                            self.drop_file(new_file);
                        }
                        outcome = Some(done);
                        old_files = old;
                    }
                    Err(error) => {
                        self.drop_file(new_file);
                        return Err(error);
                    }
                },
                CommitStep::RemoveOld => {
                    // Unsynced deletes: a power loss brings them back as
                    // orphans.
                    for id in &old_files {
                        self.live.files.remove(id);
                    }
                }
            }
            if let Some((_, other)) = between.take_if(|(at, _)| *at == step) {
                let result = self.execute(other, Faults::default());
                self.between_outcome = Some(result);
            }
            if faults.stop == Some(step) {
                self.life = Life::Stopped;
                return Err(unavailable("apply"));
            }
        }
        outcome.ok_or_else(|| unavailable("apply"))
    }

    /// The entry of `slot` when its files pass every check.
    fn checked_entry(&self, slot: &Slot) -> Option<StoredEntry> {
        let row = self.live.rows.get(slot)?;
        let file = self.live.files.get(&row.file)?;
        let intact = file.place == Place::Entry
            && file.content.bytes() == row.content_bytes
            && seal(slot, row.file, &file.content) == file.seal;
        intact.then(|| StoredEntry {
            meta: row.meta.clone(),
            version: row.version,
            body: SnapshotBody {
                page: file.content.page.clone(),
                template: file.content.template.clone(),
                data: file.content.data.clone(),
                fields: row.fields.clone(),
            },
        })
    }

    /// Recovery check of one row: the file is at its entry place and has
    /// the size of the row (a `stat`, not a read of the content).
    fn file_fits(&self, row: &Row) -> bool {
        self.live.files.get(&row.file).is_some_and(|file| {
            file.place == Place::Entry && file.content.bytes() == row.content_bytes
        })
    }
}

impl StorageDriver for FakeStore {
    fn recover(&mut self, now: UnixTime) -> Result<RecoveryReport, StoreError> {
        if self.life == Life::Stopped {
            return Err(unavailable("recover"));
        }
        if self.defect != Some(Defect::KeepsExpiredDisabledKeys) {
            // A record whose time is `now` or later stays.
            self.live.disabled.retain(|_, until| *until >= now);
        }
        let mut report = RecoveryReport::default();
        let broken: Vec<Slot> = self
            .live
            .rows
            .iter()
            .filter(|(_, row)| !self.file_fits(row))
            .map(|(slot, _)| *slot)
            .collect();
        for slot in &broken {
            if let Some(row) = self.live.rows.remove(slot) {
                self.live.files.remove(&row.file);
                report.entries_removed = report.entries_removed.saturating_add(1);
            }
        }
        let used: BTreeSet<u64> = self.live.rows.values().map(|row| row.file).collect();
        let before = self.live.files.len();
        self.live
            .files
            .retain(|id, file| file.place == Place::Entry && used.contains(id));
        report.orphans_removed = len_u64(before.saturating_sub(self.live.files.len()));
        // Recovery syncs what it removes.
        self.durable = self.live.clone();
        self.life = Life::Running;
        Ok(report)
    }

    fn read(
        &mut self,
        partition: Partition,
        key: &CacheKey,
        now: UnixTime,
    ) -> Result<Option<StoredEntry>, StoreError> {
        self.ready("read")?;
        let slot = (rank(partition), *key);
        if self.disabled_at(&slot, now) {
            return Ok(None);
        }
        let found = if self.defect == Some(Defect::ReadIgnoresPartition) {
            self.live
                .rows
                .keys()
                .find(|candidate| candidate.1 == *key)
                .copied()
        } else {
            self.live.rows.contains_key(&slot).then_some(slot)
        };
        let Some(found) = found else {
            return Ok(None);
        };
        if let Some(entry) = self.checked_entry(&found) {
            return Ok(Some(entry));
        }
        // R-CACHE-06: remove the damaged entry, keep every other one.
        if let Some(row) = self.live.rows.remove(&found) {
            self.live.files.remove(&row.file);
        }
        self.sync_meta();
        Err(StoreError::new(StoreErrorKind::Corrupted, "read"))
    }

    fn apply(&mut self, txn: CacheTransaction) -> Result<TxnOutcome, StoreError> {
        self.ready("apply")?;
        let faults = Faults {
            stop: self.stop.take(),
            fail: self.fail.take(),
            between: self.between.take(),
        };
        self.execute(txn, faults)
    }

    fn partition_epoch(&mut self, partition: Partition) -> Result<Epoch, StoreError> {
        self.ready("partition_epoch")?;
        Ok(self.epoch_of(&rank(partition)))
    }

    fn site_epoch(&mut self) -> Result<Epoch, StoreError> {
        self.ready("site_epoch")?;
        Ok(self.live.site_epoch)
    }

    fn key_disabled(
        &mut self,
        partition: Partition,
        key: &CacheKey,
        now: UnixTime,
    ) -> Result<bool, StoreError> {
        self.ready("key_disabled")?;
        Ok(self.disabled_at(&(rank(partition), *key), now))
    }

    fn list(&mut self) -> Result<Vec<EntrySummary>, StoreError> {
        self.ready("list")?;
        Ok(self
            .live
            .rows
            .values()
            .map(|row| EntrySummary {
                meta: row.meta.clone(),
                version: row.version,
                size_bytes: row.content_bytes.saturating_add(field_bytes(&row.fields)),
            })
            .collect())
    }

    fn touch(
        &mut self,
        partition: Partition,
        key: &CacheKey,
        now: UnixTime,
    ) -> Result<(), StoreError> {
        self.ready("touch")?;
        // Kept in memory until the next metadata commit (the trait allows
        // it); a stop loses it.
        if let Some(row) = self.live.rows.get_mut(&(rank(partition), *key)) {
            row.meta.last_used_at = now;
        }
        Ok(())
    }
}

fn damage_content(content: &mut Content) {
    let mut bytes = content.page.as_slice().to_vec();
    match bytes.first_mut() {
        Some(first) => *first ^= 0x01,
        None => bytes.push(0),
    }
    content.page = PageBytes::new(bytes);
}

impl FaultInjection for FakeStore {
    fn stop_after(&mut self, step: CommitStep) {
        self.stop = Some(step);
    }

    fn fail_at(&mut self, step: CommitStep) {
        self.fail = Some(step);
    }

    fn restart(&mut self) -> Result<(), StoreError> {
        if self.stop_kind == StopKind::ProcessKill {
            // The operating system keeps the file writes of the process.
            self.durable.files = self.live.files.clone();
        }
        self.live = self.durable.clone();
        self.life = Life::NeedsRecover;
        self.stop = None;
        self.fail = None;
        self.between = None;
        Ok(())
    }

    fn damage(
        &mut self,
        partition: Partition,
        key: &CacheKey,
        damage: Damage,
    ) -> Result<bool, StoreError> {
        let slot = (rank(partition), *key);
        let Some(target) = self.live.rows.get(&slot).map(|row| row.file) else {
            return Ok(false);
        };
        match damage {
            Damage::Content => {
                for disk in [&mut self.live, &mut self.durable] {
                    if let Some(file) = disk.files.get_mut(&target) {
                        damage_content(&mut file.content);
                    }
                }
            }
            Damage::Missing => {
                self.live.files.remove(&target);
                self.durable.files.remove(&target);
            }
            Damage::MovedFrom { partition, key } => {
                let source_slot = (rank(partition), key);
                if source_slot == slot {
                    return Ok(false);
                }
                let source = self
                    .live
                    .rows
                    .get(&source_slot)
                    .and_then(|row| self.live.files.get(&row.file))
                    .cloned();
                let Some(source) = source else {
                    return Ok(false);
                };
                for disk in [&mut self.live, &mut self.durable] {
                    if let Some(file) = disk.files.get_mut(&target) {
                        file.content = source.content.clone();
                        file.seal = source.seal;
                    }
                }
            }
        }
        Ok(true)
    }
}

pub mod contract;
