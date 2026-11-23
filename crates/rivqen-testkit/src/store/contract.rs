// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The driver contract suite (DL-181): checks that every
//! [`StorageDriver`](rivqen_cache::driver::StorageDriver) passes, through [`StorageDriver`](rivqen_cache::driver::StorageDriver) and
//! [`FaultInjection`] only.
//!
//! Each check takes a factory that gives a new, empty driver on new
//! storage, and returns the first [`Violation`]. A check compares the
//! driver with a simple model after every transaction: every entry of
//! the [`PARTITIONS`] and [`KEYS`] (read at [`NOW`]), every disabled
//! key, the epochs and [`StorageDriver::list`](rivqen_cache::driver::StorageDriver::list).
//!
//! Readings of the contract that the suite enforces:
//!
//! - A transaction with a key checks its preconditions in the order of
//!   `rivqen_cache::txn`: request partition epoch, target partition
//!   epoch, site epoch, host (`Replace`), entry; a revoke and a site clear
//!   are never refused (they only remove data).
//! - A session whose request partition was revoked commits nothing, also
//!   not into `Public`; a precondition without a request partition is not
//!   checked for it (DL-205).
//! - A `Replace` whose host fails `valid_host` is `Refused(InvalidHost)`
//!   and writes nothing, also when the commit stops (DL-205).
//! - `recover(now)` removes the disabled-key records whose time is before
//!   `now` and keeps the others (DL-205).
//! - `UpdateFields` without an entry is `Refused(Entry)`.
//! - A replace or a field update gives a version that differs from the
//!   version before; epochs grow by exactly one.
//! - A replace of a disabled key stores the entry; `read` hides it until
//!   the key is no longer disabled; `list` shows it.
//! - `ClearSite(None)` also removes the disabled keys; `ClearSite` with
//!   a domain keeps them (a disabled key has no host).
//! - After a stop at a step before `CommitMeta`, the store holds the old
//!   state; from `CommitMeta` on, the new state. A second `recover`
//!   removes nothing.

use std::error::Error;
use std::fmt;

use rivqen_cache::driver::{EntrySummary, RecoveryReport, StoreError, StoreErrorKind};
use rivqen_cache::header::CacheControl;
use rivqen_cache::key::{Partition, Protocol};
use rivqen_cache::partition::site_matches;
use rivqen_cache::policy::{EntryPolicy, Expires};
use rivqen_cache::txn::{
    CacheTransaction, CommitStep, EntryMeta, EntryVersion, Epoch, Expect, Preconditions, Refused,
    SnapshotBody, StoredEntry, TxnOp, TxnOutcome, Validators, Writer,
};
use rivqen_model::http::FieldLine;
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_model::safe::PageBytes;
use rivqen_model::time::UnixTime;

use super::{Damage, FaultInjection};
use std::collections::BTreeMap;

/// Calls a driver method of a harness and turns its error into a
/// [`Violation`] of the running check.
macro_rules! drive {
    ($harness:expr, $what:literal, $($call:tt)+) => {{
        let result = $harness.driver.$($call)+;
        $harness.call($what, result)
    }};
}

/// The ids of the two scoped partitions of the checks.
const SCOPED_IDS: [PartitionId; 2] = [
    PartitionId::from_bytes([0xA1; 32]),
    PartitionId::from_bytes([0xB2; 32]),
];

/// The partitions of the checks: two scoped partitions and `Public`.
pub const PARTITIONS: [Partition; 3] = [
    Partition::Scoped(SCOPED_IDS[0]),
    Partition::Scoped(SCOPED_IDS[1]),
    Partition::Public,
];

/// Hosts that a driver must refuse in a `Replace` (DL-205): a port, upper
/// case, an IDN in Unicode, an empty text, the URL form, a trailing or
/// leading dot, an empty label, an IPv6 literal with a port, in upper case
/// or with its zero groups written out (DL-208 a), user information, a path
/// and a space.
pub const INVALID_HOSTS: [&str; 16] = [
    "shop.example.com:443",
    "Shop.example.com",
    "SHOP.EXAMPLE.COM",
    "sh\u{f6}p.example.com",
    "",
    "https://shop.example.com/",
    "shop.example.com.",
    ".example.com",
    "shop..example.com",
    "[::1]:443",
    "[2001:DB8::1]",
    "[2001:db8:0:0:0:0:0:1]",
    "192.0.2.1:80",
    "user@shop.example.com",
    "shop.example.com/cart",
    "shop example.com",
];

/// Hosts that a driver must store: a name in its IDNA form, an IPv4
/// literal and two IPv6 literals.
pub const VALID_HOSTS: [&str; 4] = [
    "xn--bcher-kva.example",
    "192.0.2.1",
    "[2001:db8::1]",
    "[::1]",
];

/// The keys of the checks. The same key is used in every partition.
pub const KEYS: [CacheKey; 4] = [
    CacheKey::from_bytes([0x11; 32]),
    CacheKey::from_bytes([0x22; 32]),
    CacheKey::from_bytes([0x33; 32]),
    CacheKey::from_bytes([0x44; 32]),
];

/// The host of the entries of each key of [`KEYS`].
pub const HOSTS: [&str; 4] = [
    "shop.example.com",
    "example.com",
    "other.org",
    "notexample.com",
];

/// The domains of the site clears of a script.
pub const DOMAINS: [&str; 3] = ["example.com", "shop.example.com", "other.org"];

/// The time of every read of the checks.
pub const NOW: UnixTime = UnixTime(1_800_000_000);

/// A version that no driver gives.
pub const WRONG_VERSION: EntryVersion = EntryVersion(u64::MAX);

/// A check of the suite failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// The name of the check.
    pub check: &'static str,
    /// What differed.
    pub detail: String,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.check, self.detail)
    }
}

impl Error for Violation {}

/// The result of a check.
pub type Checked = Result<(), Violation>;

fn violation(check: &'static str, detail: String) -> Violation {
    Violation { check, detail }
}

fn call<T>(
    check: &'static str,
    what: &'static str,
    result: Result<T, StoreError>,
) -> Result<T, Violation> {
    result.map_err(|error| violation(check, format!("{what} failed: {error}")))
}

fn ensure(check: &'static str, holds: bool, detail: impl FnOnce() -> String) -> Checked {
    if holds {
        Ok(())
    } else {
        Err(violation(check, detail()))
    }
}

/// Freshness inputs with `max-age`.
#[must_use]
pub fn sample_policy(max_age: u64) -> EntryPolicy {
    EntryPolicy {
        cache_control: CacheControl {
            max_age: Some(max_age),
            ..CacheControl::default()
        },
        expires: Expires::Absent,
        date: Some(NOW),
        age: None,
        request_time: NOW,
        response_time: NOW,
        ttl_seconds: None,
        offline: false,
    }
}

/// An entry of `host` in `partition` whose every content part holds
/// `tag`; `variant` changes which parts exist and their sizes.
#[must_use]
pub fn sample_entry(
    partition: Partition,
    key: CacheKey,
    host: &str,
    tag: &str,
    variant: u8,
) -> (EntryMeta, SnapshotBody) {
    let meta = EntryMeta {
        key,
        partition,
        host: host.to_owned(),
        protocol: if variant & 1 == 0 {
            Protocol::Rqp
        } else {
            Protocol::Legacy
        },
        vary: Vec::new(),
        validators: Validators {
            etag: Some(format!("etag-{tag}")),
            template_tag: None,
            rqp: None,
        },
        policy: sample_policy(u64::from(variant)),
        stored_at: NOW,
        last_used_at: NOW,
    };
    let mut data = Vec::new();
    for n in 0..(variant & 3) {
        data.push((
            format!("{{b{n}}}"),
            PageBytes::new(format!("{tag}#{n}").into_bytes()),
        ));
    }
    let body = SnapshotBody {
        page: PageBytes::new(format!("{tag}/page").into_bytes()),
        template: (variant & 4 != 0).then(|| PageBytes::new(format!("{tag}/tpl").into_bytes())),
        data,
        fields: vec![FieldLine::new("content-type", "text/html; charset=utf-8")],
    };
    (meta, body)
}

/// The tag of the content of a script entry: it starts with the
/// partition index, so a read in another partition shows a leak.
fn tag(partition: usize, key: usize, seq: usize) -> String {
    format!("p{partition}/k{key}/n{seq}")
}

fn partition_tag(partition: usize) -> String {
    format!("p{partition}/")
}

fn after_commit_point(step: CommitStep) -> bool {
    matches!(step, CommitStep::CommitMeta | CommitStep::RemoveOld)
}

/// The operation of one script step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// `TxnOp::Replace` with a new tagged entry.
    Replace,
    /// `TxnOp::UpdateFields`.
    UpdateFields,
    /// `TxnOp::Remove`.
    Remove,
    /// `TxnOp::DisableKey` until `NOW + 1 + variant` seconds.
    DisableKey,
    /// `TxnOp::RevokePartition` of the partition of the step.
    RevokePartition,
    /// `TxnOp::ClearSite` with a domain of [`DOMAINS`], or `None`.
    ClearSite(Option<u8>),
    /// [`FaultInjection::damage`], then a read that must find it:
    /// `variant % 3` gives [`Damage::Content`], [`Damage::Missing`] or
    /// [`Damage::MovedFrom`] the next partition.
    Damage,
}

/// The keyed kinds: the kinds that check preconditions.
const KEYED: [Action; 4] = [
    Action::Replace,
    Action::UpdateFields,
    Action::Remove,
    Action::DisableKey,
];

/// The entry precondition of a script step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptExpect {
    /// `Expect::Any`.
    Any,
    /// `Expect::Absent`.
    Absent,
    /// `Expect::Version` of the current entry ([`WRONG_VERSION`] when
    /// there is none).
    Current,
    /// `Expect::Version(WRONG_VERSION)`.
    Wrong,
}

/// Which epoch of a script step is old.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stale {
    /// Every epoch is current.
    Neither,
    /// The partition epoch differs from the current one.
    Partition,
    /// The site epoch differs from the current one.
    Site,
    /// The epoch of the request partition differs from the current one
    /// (DL-205). The request partition of a step is the target partition
    /// when it is scoped; for `Public` it is the scoped partition of index
    /// `key % 2`.
    Request,
}

/// A fault of a script step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// [`FaultInjection::stop_after`], then restart and recover.
    StopAfter(CommitStep),
    /// [`FaultInjection::fail_at`].
    FailAt(CommitStep),
}

/// One step of a script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptOp {
    /// The operation.
    pub action: Action,
    /// Index into [`PARTITIONS`].
    pub partition: u8,
    /// Index into [`KEYS`].
    pub key: u8,
    /// The entry precondition.
    pub expect: ScriptExpect,
    /// Which epoch is old.
    pub stale: Stale,
    /// The fault, if any.
    pub fault: Option<Fault>,
    /// Changes the entry parts, the fields, the lifetime and the time
    /// of a disable.
    pub variant: u8,
}

impl ScriptOp {
    /// A step without fault, stale epoch or entry precondition.
    #[must_use]
    pub const fn new(action: Action, partition: u8, key: u8) -> Self {
        Self {
            action,
            partition,
            key,
            expect: ScriptExpect::Any,
            stale: Stale::Neither,
            fault: None,
            variant: 0,
        }
    }

    /// The same step with `expect`.
    #[must_use]
    pub const fn expecting(mut self, expect: ScriptExpect) -> Self {
        self.expect = expect;
        self
    }

    /// The same step with `fault`.
    #[must_use]
    pub const fn with_fault(mut self, fault: Fault) -> Self {
        self.fault = Some(fault);
        self
    }

    /// The same step with `stale`.
    #[must_use]
    pub const fn with_stale(mut self, stale: Stale) -> Self {
        self.stale = stale;
        self
    }

    /// The same step with `variant`.
    #[must_use]
    pub const fn with_variant(mut self, variant: u8) -> Self {
        self.variant = variant;
        self
    }

    /// A step from any 32-bit word: every word gives a valid step, so a
    /// property test or a fuzz target can generate scripts.
    #[must_use]
    pub fn decode(word: u32) -> Self {
        let bits = |shift: u32, mask: u32| word.checked_shr(shift).unwrap_or(0) & mask;
        let small = |value: u32| u8::try_from(value).unwrap_or(0);
        let action = match bits(0, 7) {
            2 => Action::UpdateFields,
            3 => Action::Remove,
            4 => Action::DisableKey,
            5 => Action::RevokePartition,
            6 => Action::ClearSite(match bits(17, 3) {
                3 => None,
                domain => Some(small(domain)),
            }),
            7 => Action::Damage,
            _ => Action::Replace,
        };
        let partition = match bits(3, 3) {
            3 => 0,
            value => small(value),
        };
        let expect = match bits(7, 3) {
            0 => ScriptExpect::Any,
            1 => ScriptExpect::Absent,
            2 => ScriptExpect::Current,
            _ => ScriptExpect::Wrong,
        };
        let stale = match bits(9, 7) {
            0 => Stale::Partition,
            1 => Stale::Site,
            2 => Stale::Request,
            _ => Stale::Neither,
        };
        let step = |index: u32| {
            CommitStep::ALL
                .get(usize::try_from(index).unwrap_or(usize::MAX))
                .copied()
        };
        let fault = match bits(12, 31) {
            value @ 0..=4 => step(value).map(Fault::StopAfter),
            value @ 5..=9 => step(value.wrapping_sub(5)).map(Fault::FailAt),
            _ => None,
        };
        Self {
            action,
            partition,
            key: small(bits(5, 3)),
            expect,
            stale,
            fault,
            variant: small(bits(19, 0xFF)),
        }
    }
}

/// A version that the model knows, or a new one that it learns at the
/// next observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ver {
    Is(EntryVersion),
    New { not: Option<EntryVersion> },
}

#[derive(Debug, Clone)]
struct ModelEntry {
    meta: EntryMeta,
    body: SnapshotBody,
    version: Ver,
}

type Cell = (usize, usize);

/// The model of a store over the universe of the suite.
#[derive(Debug, Clone)]
struct Model {
    entries: BTreeMap<Cell, ModelEntry>,
    disabled: BTreeMap<Cell, UnixTime>,
    epochs: [u64; 3],
    site: u64,
}

impl Model {
    fn disabled_at(&self, cell: Cell, now: UnixTime) -> bool {
        self.disabled.get(&cell).is_some_and(|until| now < *until)
    }

    fn version(&self, cell: Cell) -> Option<EntryVersion> {
        self.entries
            .get(&cell)
            .and_then(|entry| match entry.version {
                Ver::Is(version) => Some(version),
                Ver::New { .. } => None,
            })
    }

    fn bump(&mut self, partition: usize) {
        if let Some(epoch) = self.epochs.get_mut(partition) {
            *epoch = epoch.saturating_add(1);
        }
    }

    /// What `recover(now)` removes from the model: the disabled-key
    /// records whose time is before `now`.
    fn expire(&mut self, now: UnixTime) {
        self.disabled.retain(|_, until| *until >= now);
    }
}

fn cell_of(meta: &EntryMeta) -> Option<Cell> {
    let partition = PARTITIONS.iter().position(|p| *p == meta.partition)?;
    let key = KEYS.iter().position(|k| *k == meta.key)?;
    Some((partition, key))
}

fn partition_at(index: usize) -> Partition {
    PARTITIONS.get(index).copied().unwrap_or(Partition::Public)
}

fn key_at(index: usize) -> CacheKey {
    KEYS.get(index)
        .copied()
        .unwrap_or(CacheKey::from_bytes([0; 32]))
}

fn host_at(index: usize) -> &'static str {
    HOSTS.get(index).copied().unwrap_or("invalid.invalid")
}

/// The request partition of a script step on `cell` (see
/// [`Stale::Request`]): two profiles write into `Public`.
fn request_of(cell: Cell) -> PartitionId {
    let [first, second] = SCOPED_IDS;
    let index = if cell.0 < SCOPED_IDS.len() {
        cell.0
    } else {
        cell.1 % 2
    };
    if index == 0 { first } else { second }
}

/// An epoch that differs from `epoch`: one below, or 1 for epoch 0.
fn older(epoch: Epoch) -> Epoch {
    Epoch(epoch.0.checked_sub(1).unwrap_or(1))
}

fn next_partition(index: usize) -> usize {
    match index {
        0 => 1,
        1 => 2,
        _ => 0,
    }
}

/// A driver under test, its model and the name of the running check.
struct Harness<'a, D> {
    driver: &'a mut D,
    model: Model,
    check: &'static str,
    seq: usize,
}

impl<'a, D: FaultInjection> Harness<'a, D> {
    fn start(driver: &'a mut D, check: &'static str) -> Result<Self, Violation> {
        call(check, "recover", driver.recover(NOW))?;
        let mut epochs = [0_u64; 3];
        for (index, epoch) in epochs.iter_mut().enumerate() {
            *epoch = call(
                check,
                "partition_epoch",
                driver.partition_epoch(partition_at(index)),
            )?
            .0;
        }
        let site = call(check, "site_epoch", driver.site_epoch())?.0;
        let mut harness = Self {
            driver,
            model: Model {
                entries: BTreeMap::new(),
                disabled: BTreeMap::new(),
                epochs,
                site,
            },
            check,
            seq: 0,
        };
        harness.settle()?;
        Ok(harness)
    }

    fn fail(&self, detail: impl fmt::Display) -> Violation {
        violation(self.check, format!("step {}: {detail}", self.seq))
    }

    fn ensure(&self, holds: bool, detail: impl FnOnce() -> String) -> Checked {
        if holds {
            Ok(())
        } else {
            Err(self.fail(detail()))
        }
    }

    fn ensure_eq<T: PartialEq + fmt::Debug>(
        &self,
        what: &str,
        actual: &T,
        expected: &T,
    ) -> Checked {
        self.ensure(actual == expected, || {
            format!("{what}: got {actual:?}, expected {expected:?}")
        })
    }

    fn call<T>(&self, what: &'static str, result: Result<T, StoreError>) -> Result<T, Violation> {
        result.map_err(|error| self.fail(format!("{what} failed: {error}")))
    }

    /// Compares the driver with the model and learns new versions.
    fn settle(&mut self) -> Checked {
        let listed = drive!(self, "list", list())?;
        self.ensure_eq(
            "number of listed entries",
            &listed.len(),
            &self.model.entries.len(),
        )?;
        for summary in &listed {
            self.settle_summary(summary)?;
        }
        for partition in 0..PARTITIONS.len() {
            for key in 0..KEYS.len() {
                self.settle_cell((partition, key))?;
            }
            let epoch = drive!(
                self,
                "partition_epoch",
                partition_epoch(partition_at(partition))
            )?;
            let expected = self.model.epochs.get(partition).copied().unwrap_or(0);
            self.ensure_eq("partition epoch", &epoch, &Epoch(expected))?;
        }
        let site = drive!(self, "site_epoch", site_epoch())?;
        self.ensure_eq("site epoch", &site, &Epoch(self.model.site))
    }

    fn settle_version(&mut self, cell: Cell, actual: EntryVersion) -> Checked {
        let Some(entry) = self.model.entries.get_mut(&cell) else {
            return Err(self.fail(format!("no model entry for {cell:?}")));
        };
        match entry.version {
            Ver::Is(expected) => {
                let detail = format!("version of {cell:?}: got {actual:?}, expected {expected:?}");
                if actual == expected {
                    Ok(())
                } else {
                    Err(self.fail(detail))
                }
            }
            Ver::New { not } => {
                if Some(actual) == not {
                    return Err(self.fail(format!("version of {cell:?} did not change")));
                }
                entry.version = Ver::Is(actual);
                Ok(())
            }
        }
    }

    fn settle_summary(&mut self, summary: &EntrySummary) -> Checked {
        let Some(cell) = cell_of(&summary.meta) else {
            return Err(self.fail("list has an entry outside the universe".to_owned()));
        };
        let Some(entry) = self.model.entries.get(&cell) else {
            return Err(self.fail(format!("list has {cell:?}, the model has no entry")));
        };
        let (meta, size) = (entry.meta.clone(), entry.body.size_bytes());
        self.ensure_eq("listed meta", &summary.meta, &meta)?;
        self.ensure_eq("listed size", &summary.size_bytes, &size)?;
        self.settle_version(cell, summary.version)
    }

    fn settle_cell(&mut self, cell: Cell) -> Checked {
        let (partition, key) = (partition_at(cell.0), key_at(cell.1));
        let disabled = self.model.disabled_at(cell, NOW);
        let got = drive!(self, "key_disabled", key_disabled(partition, &key, NOW))?;
        self.ensure_eq("key_disabled", &got, &disabled)?;
        let read = drive!(self, "read", read(partition, &key, NOW))?;
        let expected = if disabled {
            None
        } else {
            self.model.entries.get(&cell).cloned()
        };
        match (read, expected) {
            (None, None) => Ok(()),
            (Some(entry), Some(model)) => {
                self.ensure_eq("read meta", &entry.meta, &model.meta)?;
                self.ensure_eq("read body", &entry.body, &model.body)?;
                self.settle_version(cell, entry.version)
            }
            (Some(_), None) => {
                Err(self.fail(format!("read {cell:?} found an entry; the model has none")))
            }
            (None, Some(_)) => Err(self.fail(format!(
                "read {cell:?} found nothing; the model has an entry"
            ))),
        }
    }

    /// Recovers the driver at `now` and the model with it.
    fn recover_at(&mut self, now: UnixTime) -> Result<RecoveryReport, Violation> {
        let report = drive!(self, "recover", recover(now))?;
        self.model.expire(now);
        Ok(report)
    }

    fn preconditions(&mut self, op: &ScriptOp, cell: Cell) -> Result<Preconditions, Violation> {
        let request = request_of(cell);
        let request_epoch = drive!(
            self,
            "partition_epoch",
            partition_epoch(Partition::Scoped(request))
        )?;
        let partition = drive!(
            self,
            "partition_epoch",
            partition_epoch(partition_at(cell.0))
        )?;
        let site = drive!(self, "site_epoch", site_epoch())?;
        let entry = match op.expect {
            ScriptExpect::Any => Expect::Any,
            ScriptExpect::Absent => Expect::Absent,
            ScriptExpect::Current => {
                Expect::Version(self.model.version(cell).unwrap_or(WRONG_VERSION))
            }
            ScriptExpect::Wrong => Expect::Version(WRONG_VERSION),
        };
        Ok(Preconditions {
            entry,
            writer: Writer::request(
                request,
                if op.stale == Stale::Request {
                    older(request_epoch)
                } else {
                    request_epoch
                },
            ),
            partition_epoch: if op.stale == Stale::Partition {
                older(partition)
            } else {
                partition
            },
            site_epoch: if op.stale == Stale::Site {
                older(site)
            } else {
                site
            },
        })
    }

    /// What the model says about a keyed transaction.
    fn predicted_refusal(&self, op: &ScriptOp, cell: Cell) -> Option<Refused> {
        let keyed = matches!(
            op.action,
            Action::Replace | Action::UpdateFields | Action::Remove | Action::DisableKey
        );
        if !keyed {
            return None;
        }
        match op.stale {
            Stale::Request | Stale::Partition => return Some(Refused::PartitionRevoked),
            Stale::Site => return Some(Refused::SiteCleared),
            Stale::Neither => {}
        }
        let current = self.model.version(cell);
        let holds = match op.expect {
            ScriptExpect::Any => true,
            ScriptExpect::Absent => current.is_none(),
            ScriptExpect::Current => current.is_some(),
            ScriptExpect::Wrong => false,
        };
        let update_without_entry = op.action == Action::UpdateFields && current.is_none();
        (!holds || update_without_entry).then_some(Refused::Entry)
    }

    /// Runs one step and compares the driver with the model.
    fn run(&mut self, op: ScriptOp) -> Checked {
        self.seq = self.seq.saturating_add(1);
        self.run_op(op)?;
        self.settle()
    }

    fn run_op(&mut self, op: ScriptOp) -> Checked {
        let cell = (usize::from(op.partition), usize::from(op.key));
        if cell.0 >= PARTITIONS.len() || cell.1 >= KEYS.len() {
            return Err(self.fail(format!("script step out of the universe: {op:?}")));
        }
        if op.action == Action::Damage {
            return self.run_damage(&op, cell);
        }
        let pre = self.preconditions(&op, cell)?;
        let (txn_op, change) = self.build(&op, cell);
        let refused = self.predicted_refusal(&op, cell);
        match op.fault {
            Some(Fault::StopAfter(step)) => self.driver.stop_after(step),
            Some(Fault::FailAt(step)) => self.driver.fail_at(step),
            None => {}
        }
        let result = self.driver.apply(CacheTransaction { op: txn_op, pre });
        let old_version = self.model.version(cell);
        match op.fault {
            Some(Fault::StopAfter(step)) => {
                self.ensure_unavailable(&result)?;
                drive!(self, "restart", restart())?;
                self.recover_at(NOW)?;
                let again = drive!(self, "second recover", recover(NOW))?;
                self.ensure_eq("second recover", &again, &RecoveryReport::default())?;
                if refused.is_none() && after_commit_point(step) {
                    self.model_apply(change, cell, Ver::New { not: old_version });
                }
                Ok(())
            }
            Some(Fault::FailAt(step)) if step != CommitStep::RemoveOld => {
                self.ensure_unavailable(&result)
            }
            _ => {
                let outcome = self.call("apply", result)?;
                if let Some(refused) = refused {
                    return self.ensure_eq("outcome", &outcome, &TxnOutcome::Refused(refused));
                }
                let versioned = matches!(op.action, Action::Replace | Action::UpdateFields);
                match outcome {
                    TxnOutcome::Applied {
                        version: Some(version),
                    } if versioned => {
                        self.ensure(Some(version) != old_version, || {
                            format!("the new version equals the old one: {version:?}")
                        })?;
                        self.model_apply(change, cell, Ver::Is(version));
                        Ok(())
                    }
                    TxnOutcome::Applied { version: None } if !versioned => {
                        self.model_apply(change, cell, Ver::New { not: None });
                        Ok(())
                    }
                    other => Err(self.fail(format!("outcome {other:?} for {:?}", op.action))),
                }
            }
        }
    }

    fn ensure_unavailable<T: fmt::Debug>(&self, result: &Result<T, StoreError>) -> Checked {
        let kind = result.as_ref().err().map(StoreError::kind);
        self.ensure(kind == Some(StoreErrorKind::Unavailable), || {
            format!("a fault must give Unavailable, got {result:?}")
        })
    }

    fn build(&mut self, op: &ScriptOp, cell: Cell) -> (TxnOp, Change) {
        let (partition, key) = (partition_at(cell.0), key_at(cell.1));
        match op.action {
            Action::Replace => {
                let (meta, body) = sample_entry(
                    partition,
                    key,
                    host_at(cell.1),
                    &tag(cell.0, cell.1, self.seq),
                    op.variant,
                );
                let change = Change::Put(Box::new((meta.clone(), body.clone())));
                (
                    TxnOp::Replace {
                        meta: Box::new(meta),
                        body: Box::new(body),
                    },
                    change,
                )
            }
            Action::UpdateFields => {
                let fields = vec![
                    FieldLine::new("content-type", "text/html"),
                    FieldLine::new("x-step", format!("{}-{}", self.seq, op.variant)),
                ];
                let policy = sample_policy(u64::from(op.variant).saturating_add(1_000));
                let change = Change::Fields(fields.clone(), policy);
                (
                    TxnOp::UpdateFields {
                        partition,
                        key,
                        fields,
                        policy,
                    },
                    change,
                )
            }
            Action::Remove => (TxnOp::Remove { partition, key }, Change::Remove),
            Action::DisableKey => {
                let until = NOW.plus(u64::from(op.variant).saturating_add(1));
                (
                    TxnOp::DisableKey {
                        partition,
                        key,
                        until,
                    },
                    Change::Disable(until),
                )
            }
            Action::RevokePartition => (TxnOp::RevokePartition { partition }, Change::Revoke),
            Action::ClearSite(domain) => {
                let domain = domain
                    .and_then(|index| DOMAINS.get(usize::from(index)))
                    .map(|domain| (*domain).to_owned());
                (
                    TxnOp::ClearSite {
                        domain: domain.clone(),
                    },
                    Change::Clear(domain),
                )
            }
            // `run_op` handles a damage before it builds a transaction.
            Action::Damage => (TxnOp::ClearSite { domain: None }, Change::Clear(None)),
        }
    }

    fn model_apply(&mut self, change: Change, cell: Cell, version: Ver) {
        let model = &mut self.model;
        match change {
            Change::Put(entry) => {
                let (meta, body) = *entry;
                model.entries.insert(
                    cell,
                    ModelEntry {
                        meta,
                        body,
                        version,
                    },
                );
            }
            Change::Fields(fields, policy) => {
                if let Some(entry) = model.entries.get_mut(&cell) {
                    entry.body.fields = fields;
                    entry.meta.policy = policy;
                    entry.version = version;
                }
            }
            Change::Remove => {
                model.entries.remove(&cell);
            }
            Change::Disable(until) => {
                model.entries.remove(&cell);
                model.disabled.insert(cell, until);
            }
            Change::Revoke => {
                model.entries.retain(|other, _| other.0 != cell.0);
                model.disabled.retain(|other, _| other.0 != cell.0);
                model.bump(cell.0);
            }
            Change::Clear(domain) => {
                model.entries.retain(|_, entry| {
                    domain
                        .as_deref()
                        .is_some_and(|domain| !site_matches(&entry.meta.host, domain))
                });
                if domain.is_none() {
                    model.disabled.clear();
                }
                model.site = model.site.saturating_add(1);
            }
        }
    }

    fn run_damage(&mut self, op: &ScriptOp, cell: Cell) -> Checked {
        if self.model.disabled_at(cell, NOW) {
            // A hidden entry is not read: skip.
            return Ok(());
        }
        let source = (next_partition(cell.0), cell.1);
        let (damage, expected) = match op.variant % 3 {
            0 => (Damage::Content, self.model.entries.contains_key(&cell)),
            1 => (Damage::Missing, self.model.entries.contains_key(&cell)),
            _ => (
                Damage::MovedFrom {
                    partition: partition_at(source.0),
                    key: key_at(source.1),
                },
                self.model.entries.contains_key(&cell) && self.model.entries.contains_key(&source),
            ),
        };
        let (partition, key) = (partition_at(cell.0), key_at(cell.1));
        let damaged = drive!(self, "damage", damage(partition, &key, damage))?;
        self.ensure_eq("damage done", &damaged, &expected)?;
        if !damaged {
            return Ok(());
        }
        let read = self.driver.read(partition, &key, NOW);
        let kind = read.as_ref().err().map(StoreError::kind);
        self.ensure(kind == Some(StoreErrorKind::Corrupted), || {
            format!("a read of a damaged entry ({damage:?}) must give Corrupted, got {read:?}")
        })?;
        self.model.entries.remove(&cell);
        Ok(())
    }

    fn read(&mut self, cell: Cell, now: UnixTime) -> Result<Option<StoredEntry>, Violation> {
        drive!(
            self,
            "read",
            read(partition_at(cell.0), &key_at(cell.1), now)
        )
    }

    fn apply(&mut self, txn: CacheTransaction) -> Result<TxnOutcome, Violation> {
        drive!(self, "apply", apply(txn))
    }

    /// The preconditions of a session that starts now on `cell`, with
    /// the request partition of [`Stale::Request`].
    fn current_pre(&mut self, cell: Cell, entry: Expect) -> Result<Preconditions, Violation> {
        let request = request_of(cell);
        let request_epoch = drive!(
            self,
            "partition_epoch",
            partition_epoch(Partition::Scoped(request))
        )?;
        let partition = drive!(
            self,
            "partition_epoch",
            partition_epoch(partition_at(cell.0))
        )?;
        let site = drive!(self, "site_epoch", site_epoch())?;
        Ok(Preconditions {
            entry,
            writer: Writer::request(request, request_epoch),
            partition_epoch: partition,
            site_epoch: site,
        })
    }

    /// Applies `txn` and expects `want`.
    fn expect_outcome(&mut self, what: &str, txn: CacheTransaction, want: TxnOutcome) -> Checked {
        let outcome = self.apply(txn)?;
        self.ensure_eq(what, &outcome, &want)
    }

    /// Applies a `Replace` of `meta` and `body` with `pre` and puts the new
    /// entry into the model.
    fn put_explicit(
        &mut self,
        cell: Cell,
        meta: EntryMeta,
        body: SnapshotBody,
        pre: Preconditions,
    ) -> Checked {
        let change = Change::Put(Box::new((meta.clone(), body.clone())));
        let outcome = self.apply(replace_txn(meta, body, pre))?;
        match outcome {
            TxnOutcome::Applied {
                version: Some(version),
            } => {
                self.model_apply(change, cell, Ver::Is(version));
                self.settle()
            }
            other => Err(self.fail(format!("a replace gave {other:?}"))),
        }
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

/// A model change.
enum Change {
    Put(Box<(EntryMeta, SnapshotBody)>),
    Fields(Vec<FieldLine>, EntryPolicy),
    Remove,
    Disable(UnixTime),
    Revoke,
    Clear(Option<String>),
}

fn put(partition: u8, key: u8) -> ScriptOp {
    ScriptOp::new(Action::Replace, partition, key)
}

fn with_driver<D, F>(
    make: &mut F,
    check: &'static str,
    body: impl FnOnce(&mut Harness<'_, D>) -> Checked,
) -> Checked
where
    D: FaultInjection,
    F: FnMut() -> D,
{
    let mut driver = make();
    let mut harness = Harness::start(&mut driver, check)?;
    body(&mut harness)
}

/// Replace then read gives the new version and body; a second replace
/// gives a new version; `list` shows the entry with its size.
///
/// # Errors
///
/// The first [`Violation`].
pub fn replace_then_read<D, F>(make: &mut F) -> Checked
where
    D: FaultInjection,
    F: FnMut() -> D,
{
    with_driver(make, "replace_then_read", |h| {
        h.run(put(0, 0).expecting(ScriptExpect::Absent).with_variant(7))?;
        h.run(put(0, 0).expecting(ScriptExpect::Current).with_variant(2))?;
        h.run(put(0, 0).with_variant(5))?;
        h.run(put(1, 1).expecting(ScriptExpect::Absent))?;
        let entry = h.read((0, 0), NOW)?;
        let ok = entry.is_some_and(|entry| {
            entry
                .body
                .page
                .as_slice()
                .starts_with(partition_tag(0).as_bytes())
                && entry.body.page.as_slice().ends_with(b"/n3/page")
        });
        h.ensure(ok, || {
            "read does not give the body of the last replace".to_owned()
        })
    })
}

/// `UpdateFields` changes the page fields, the freshness inputs and the
/// version, not the content; without an entry it is refused.
///
/// # Errors
///
/// The first [`Violation`].
pub fn update_fields<D, F>(make: &mut F) -> Checked
where
    D: FaultInjection,
    F: FnMut() -> D,
{
    with_driver(make, "update_fields", |h| {
        h.run(put(0, 0).with_variant(7))?;
        let before = h.read((0, 0), NOW)?;
        h.run(
            ScriptOp::new(Action::UpdateFields, 0, 0)
                .expecting(ScriptExpect::Current)
                .with_variant(9),
        )?;
        let after = h.read((0, 0), NOW)?;
        match (before, after) {
            (Some(before), Some(after)) => {
                h.ensure_eq("page", &after.body.page, &before.body.page)?;
                h.ensure_eq("template", &after.body.template, &before.body.template)?;
                h.ensure_eq("data", &after.body.data, &before.body.data)?;
                h.ensure(after.body.fields != before.body.fields, || {
                    "fields unchanged".to_owned()
                })?;
                h.ensure_eq(
                    "max-age",
                    &after.meta.policy.cache_control.max_age,
                    &Some(1_009),
                )?;
                h.ensure(after.version != before.version, || {
                    "version unchanged".to_owned()
                })?;
            }
            _ => return Err(h.fail("the entry is missing".to_owned())),
        }
        h.run(ScriptOp::new(Action::UpdateFields, 1, 0))?;
        let pre = h.current_pre((1, 0), Expect::Any)?;
        let outcome = h.apply(CacheTransaction {
            op: TxnOp::UpdateFields {
                partition: partition_at(1),
                key: key_at(0),
                fields: Vec::new(),
                policy: sample_policy(1),
            },
            pre,
        })?;
        h.ensure_eq(
            "update without entry",
            &outcome,
            &TxnOutcome::Refused(Refused::Entry),
        )?;
        h.settle()
    })
}

/// `Remove` removes one entry; a remove without an entry is applied and
/// changes nothing.
///
/// # Errors
///
/// The first [`Violation`].
pub fn remove<D, F>(make: &mut F) -> Checked
where
    D: FaultInjection,
    F: FnMut() -> D,
{
    with_driver(make, "remove", |h| {
        h.run(put(0, 0))?;
        h.run(put(1, 0))?;
        h.run(put(0, 1))?;
        h.run(ScriptOp::new(Action::Remove, 0, 0).expecting(ScriptExpect::Current))?;
        h.run(ScriptOp::new(Action::Remove, 0, 0))?;
        h.run(ScriptOp::new(Action::Remove, 2, 3).expecting(ScriptExpect::Absent))?;
        let gone = h.read((0, 0), NOW)?;
        h.ensure(gone.is_none(), || {
            "the removed entry is readable".to_owned()
        })
    })
}

/// `DisableKey` removes the entry and disables the key until the time:
/// `key_disabled` is true before it and false from it on; a replace
/// during the period stays hidden until the time.
///
/// # Errors
///
/// The first [`Violation`].
pub fn disable_key<D, F>(make: &mut F) -> Checked
where
    D: FaultInjection,
    F: FnMut() -> D,
{
    with_driver(make, "disable_key", |h| {
        h.run(put(0, 0))?;
        h.run(put(1, 0))?;
        // until = NOW + 10
        h.run(ScriptOp::new(Action::DisableKey, 0, 0).with_variant(9))?;
        let until = NOW.plus(10);
        let (partition, key) = (partition_at(0), key_at(0));
        let before = drive!(
            h,
            "key_disabled",
            key_disabled(partition, &key, UnixTime(until.0.saturating_sub(1)))
        )?;
        h.ensure(before, || {
            "the key is not disabled one second before the time".to_owned()
        })?;
        let at = drive!(h, "key_disabled", key_disabled(partition, &key, until))?;
        h.ensure(!at, || "the key is still disabled at the time".to_owned())?;
        let other = drive!(h, "key_disabled", key_disabled(partition_at(1), &key, NOW))?;
        h.ensure(!other, || {
            "the key is disabled in another partition".to_owned()
        })?;
        let entry = h.read((0, 0), until)?;
        h.ensure(entry.is_none(), || {
            "the disable did not remove the entry".to_owned()
        })?;
        h.run(put(0, 0))?;
        let hidden = h.read((0, 0), UnixTime(until.0.saturating_sub(1)))?;
        h.ensure(hidden.is_none(), || {
            "an entry of a disabled key is readable".to_owned()
        })?;
        let shown = h.read((0, 0), until)?;
        h.ensure(shown.is_some(), || {
            "the entry is hidden after the time".to_owned()
        })
    })
}

/// `RevokePartition` removes every entry and disabled key of that
/// partition only and increments its epoch, also for `Public`.
///
/// # Errors
///
/// The first [`Violation`].
pub fn revoke_partition<D, F>(make: &mut F) -> Checked
where
    D: FaultInjection,
    F: FnMut() -> D,
{
    with_driver(make, "revoke_partition", |h| {
        for partition in 0..3 {
            h.run(put(partition, 0))?;
            h.run(put(partition, 1))?;
        }
        h.run(ScriptOp::new(Action::DisableKey, 0, 2).with_variant(50))?;
        h.run(ScriptOp::new(Action::DisableKey, 1, 2).with_variant(50))?;
        h.run(ScriptOp::new(Action::RevokePartition, 0, 0))?;
        for key in 0..KEYS.len() {
            let entry = h.read((0, key), NOW)?;
            h.ensure(entry.is_none(), || {
                format!("key {key} of the revoked partition is readable")
            })?;
        }
        let listed = drive!(h, "list", list())?;
        h.ensure(
            listed.iter().all(|s| s.meta.partition != partition_at(0)),
            || "list shows an entry of the revoked partition".to_owned(),
        )?;
        h.run(ScriptOp::new(Action::RevokePartition, 2, 0))?;
        h.run(ScriptOp::new(Action::RevokePartition, 0, 0))
    })
}

/// `ClearSite` removes the entries of the site in every partition, also
/// `Public`, and increments the site epoch, not the partition epochs.
/// Needs a [`site_matches`] that matches a domain and its subdomains.
///
/// # Errors
///
/// The first [`Violation`].
pub fn clear_site_every_partition<D, F>(make: &mut F) -> Checked
where
    D: FaultInjection,
    F: FnMut() -> D,
{
    with_driver(make, "clear_site_every_partition", |h| {
        for partition in 0..3 {
            h.run(put(partition, 0))?;
            h.run(put(partition, 1))?;
        }
        h.run(ScriptOp::new(Action::ClearSite(Some(0)), 0, 0))?;
        for partition in 0..3 {
            for key in 0..2 {
                let entry = h.read((partition, key), NOW)?;
                h.ensure(entry.is_none(), || {
                    format!("entry {key} of the cleared site is readable in partition {partition}")
                })?;
            }
        }
        Ok(())
    })
}

/// A site clear keeps the entries of other sites: `other.org` and
/// `notexample.com` stay when `example.com` is cleared.
///
/// # Errors
///
/// The first [`Violation`].
pub fn clear_site_keeps_other_sites<D, F>(make: &mut F) -> Checked
where
    D: FaultInjection,
    F: FnMut() -> D,
{
    with_driver(make, "clear_site_keeps_other_sites", |h| {
        for partition in [0, 2] {
            for key in 0..4 {
                h.run(put(partition, key))?;
            }
        }
        h.run(ScriptOp::new(Action::ClearSite(Some(0)), 0, 0))?;
        for partition in [0, 2] {
            for key in 0..4 {
                let kept = h.read((partition, key), NOW)?.is_some();
                h.ensure_eq(&format!("entry of {}", host_at(key)), &kept, &(key >= 2))?;
            }
        }
        Ok(())
    })
}

/// `ClearSite(None)` removes every entry and every disabled key and
/// increments the site epoch.
///
/// # Errors
///
/// The first [`Violation`].
pub fn clear_site_all<D, F>(make: &mut F) -> Checked
where
    D: FaultInjection,
    F: FnMut() -> D,
{
    with_driver(make, "clear_site_all", |h| {
        for partition in 0..3 {
            h.run(put(partition, partition))?;
            h.run(put(partition, 3))?;
        }
        h.run(ScriptOp::new(Action::DisableKey, 1, 2).with_variant(50))?;
        h.run(ScriptOp::new(Action::ClearSite(None), 0, 0))?;
        let listed = drive!(h, "list", list())?;
        h.ensure(listed.is_empty(), || "entries are left".to_owned())?;
        let disabled = drive!(
            h,
            "key_disabled",
            key_disabled(partition_at(1), &key_at(2), NOW)
        )?;
        h.ensure(!disabled, || "a disabled key is left".to_owned())
    })
}

/// `Expect::Absent`, `Expect::Version` and `Expect::Any` for every kind
/// with a key; a refused transaction changes nothing.
///
/// # Errors
///
/// The first [`Violation`].
pub fn refused_entry<D, F>(make: &mut F) -> Checked
where
    D: FaultInjection,
    F: FnMut() -> D,
{
    with_driver(make, "refused_entry", |h| {
        h.run(put(0, 0))?;
        let kinds = [
            Action::Replace,
            Action::UpdateFields,
            Action::Remove,
            Action::DisableKey,
        ];
        for action in kinds {
            h.run(ScriptOp::new(action, 0, 0).expecting(ScriptExpect::Absent))?;
            h.run(ScriptOp::new(action, 0, 0).expecting(ScriptExpect::Wrong))?;
            h.run(ScriptOp::new(action, 0, 1).expecting(ScriptExpect::Current))?;
        }
        // Explicit, not through the model.
        let pre = h.current_pre((0, 0), Expect::Absent)?;
        let (meta, body) = sample_entry(partition_at(0), key_at(0), host_at(0), "explicit", 0);
        let outcome = h.apply(CacheTransaction {
            op: TxnOp::Replace {
                meta: Box::new(meta),
                body: Box::new(body),
            },
            pre,
        })?;
        h.ensure_eq(
            "Absent with an entry",
            &outcome,
            &TxnOutcome::Refused(Refused::Entry),
        )?;
        let pre = h.current_pre((0, 0), Expect::Version(WRONG_VERSION))?;
        let outcome = h.apply(CacheTransaction {
            op: TxnOp::Remove {
                partition: partition_at(0),
                key: key_at(0),
            },
            pre,
        })?;
        h.ensure_eq(
            "a wrong version",
            &outcome,
            &TxnOutcome::Refused(Refused::Entry),
        )?;
        h.settle()?;
        for action in kinds {
            h.run(ScriptOp::new(action, 0, 0).expecting(ScriptExpect::Current))?;
            h.run(put(0, 0).expecting(ScriptExpect::Any))?;
        }
        h.run(put(0, 3).expecting(ScriptExpect::Absent))
    })
}

fn refused_epoch<D, F>(make: &mut F, check: &'static str, stale: Stale, reason: Refused) -> Checked
where
    D: FaultInjection,
    F: FnMut() -> D,
{
    with_driver(make, check, |h| {
        h.run(put(0, 0))?;
        h.run(put(1, 0))?;
        let kinds = [
            Action::Replace,
            Action::UpdateFields,
            Action::Remove,
            Action::DisableKey,
        ];
        for action in kinds {
            // An epoch that differs from the current one (above it at
            // epoch 0, below it later).
            h.run(ScriptOp::new(action, 0, 0).with_stale(stale))?;
            h.run(ScriptOp::new(action, 0, 1).with_stale(stale))?;
        }
        // A writer reads the epochs, a revoke or a clear runs, then the
        // writer commits: refused, explicit.
        let pre = h.current_pre((0, 2), Expect::Absent)?;
        match stale {
            Stale::Site => h.run(ScriptOp::new(Action::ClearSite(Some(2)), 0, 0))?,
            Stale::Partition | Stale::Request | Stale::Neither => {
                h.run(ScriptOp::new(Action::RevokePartition, 0, 0))?;
            }
        }
        let (meta, body) = sample_entry(partition_at(0), key_at(2), host_at(2), "late", 0);
        let outcome = h.apply(CacheTransaction {
            op: TxnOp::Replace {
                meta: Box::new(meta),
                body: Box::new(body),
            },
            pre,
        })?;
        h.ensure_eq("late writer", &outcome, &TxnOutcome::Refused(reason))?;
        h.settle()?;
        // An epoch one above the current one.
        let mut ahead = h.current_pre((1, 0), Expect::Any)?;
        match stale {
            Stale::Site => ahead.site_epoch = Epoch(ahead.site_epoch.0.saturating_add(1)),
            Stale::Partition | Stale::Request | Stale::Neither => {
                ahead.partition_epoch = Epoch(ahead.partition_epoch.0.saturating_add(1));
            }
        }
        let outcome = h.apply(CacheTransaction {
            op: TxnOp::Remove {
                partition: partition_at(1),
                key: key_at(0),
            },
            pre: ahead,
        })?;
        h.ensure_eq("epoch ahead", &outcome, &TxnOutcome::Refused(reason))?;
        h.settle()
    })
}

/// An old (or unknown) partition epoch gives `Refused(PartitionRevoked)`
/// and changes nothing.
///
/// # Errors
///
/// The first [`Violation`].
pub fn refused_partition_revoked<D, F>(make: &mut F) -> Checked
where
    D: FaultInjection,
    F: FnMut() -> D,
{
    refused_epoch(
        make,
        "refused_partition_revoked",
        Stale::Partition,
        Refused::PartitionRevoked,
    )
}

/// An old (or unknown) site epoch gives `Refused(SiteCleared)` and
/// changes nothing.
///
/// # Errors
///
/// The first [`Violation`].
pub fn refused_site_cleared<D, F>(make: &mut F) -> Checked
where
    D: FaultInjection,
    F: FnMut() -> D,
{
    refused_epoch(
        make,
        "refused_site_cleared",
        Stale::Site,
        Refused::SiteCleared,
    )
}

/// A session whose request partition was revoked after it read the epoch
/// commits nothing (DL-205; cache identity rule 4, RSK-06): every keyed
/// kind is `Refused(PartitionRevoked)`, into `Public` and into its own
/// partition, also with every other epoch current; an epoch ahead of the
/// current one is refused too; a revoke of another partition refuses
/// nothing.
///
/// # Errors
///
/// The first [`Violation`].
pub fn refused_request_partition_revoked<D, F>(make: &mut F) -> Checked
where
    D: FaultInjection,
    F: FnMut() -> D,
{
    with_driver(make, "refused_request_partition_revoked", |h| {
        h.run(put(0, 0))?;
        h.run(put(2, 0))?;
        h.run(put(2, 1))?;
        for action in KEYED {
            // `Public` with the request partition 0 and 1, then partition 0.
            h.run(ScriptOp::new(action, 2, 0).with_stale(Stale::Request))?;
            h.run(ScriptOp::new(action, 2, 1).with_stale(Stale::Request))?;
            h.run(ScriptOp::new(action, 0, 0).with_stale(Stale::Request))?;
        }
        // A session of partition 0 reads the epochs, a logout revokes
        // partition 0, then the session commits into `Public`: refused.
        let late = h.current_pre((2, 2), Expect::Absent)?;
        h.run(ScriptOp::new(Action::RevokePartition, 0, 0))?;
        let (meta, body) = sample_entry(Partition::Public, key_at(2), host_at(2), "late", 0);
        h.expect_outcome(
            "a session of a revoked partition into Public",
            replace_txn(meta, body, late),
            TxnOutcome::Refused(Refused::PartitionRevoked),
        )?;
        h.settle()?;
        // A revoke of partition 1 does not touch a session of partition 0.
        let pre = h.current_pre((2, 2), Expect::Absent)?;
        h.run(ScriptOp::new(Action::RevokePartition, 1, 0))?;
        let (meta, body) = sample_entry(Partition::Public, key_at(2), host_at(2), "p2/ok", 0);
        h.put_explicit((2, 2), meta, body, pre)?;
        // An epoch one above the current one.
        let mut ahead = h.current_pre((2, 0), Expect::Any)?;
        if let Some((id, epoch)) = ahead.writer.request_epoch() {
            ahead.writer = Writer::request(id, Epoch(epoch.0.saturating_add(1)));
        }
        h.expect_outcome(
            "request epoch ahead",
            CacheTransaction {
                op: TxnOp::Remove {
                    partition: Partition::Public,
                    key: key_at(0),
                },
                pre: ahead,
            },
            TxnOutcome::Refused(Refused::PartitionRevoked),
        )?;
        h.settle()
    })
}

/// A precondition without a request partition (`Writer::Maintenance`) is not
/// checked for it: the change is applied after a revoke of any
/// partition other than the target.
///
/// # Errors
///
/// The first [`Violation`].
pub fn refused_request_partition_none_is_not_checked<D, F>(make: &mut F) -> Checked
where
    D: FaultInjection,
    F: FnMut() -> D,
{
    with_driver(make, "refused_request_partition_none_is_not_checked", |h| {
        h.run(put(2, 0))?;
        let mut pre = h.current_pre((2, 0), Expect::Any)?;
        pre.writer = Writer::Maintenance;
        h.run(ScriptOp::new(Action::RevokePartition, 0, 0))?;
        h.run(ScriptOp::new(Action::RevokePartition, 1, 0))?;
        let (meta, body) = sample_entry(Partition::Public, key_at(0), host_at(0), "p2/none", 3);
        h.put_explicit((2, 0), meta, body, pre)
    })
}

/// The order of the checks of a keyed kind: request partition epoch,
/// target partition epoch, site epoch, host (`Replace`), entry. Each row
/// makes every later check false too, so only the first one decides.
///
/// # Errors
///
/// The first [`Violation`].
pub fn refusal_order<D, F>(make: &mut F) -> Checked
where
    D: FaultInjection,
    F: FnMut() -> D,
{
    with_driver(make, "refusal_order", |h| {
        h.run(put(2, 0))?;
        h.run(put(1, 1))?;
        // The request partition of (2, 1) is partition 1: not the target.
        let fresh = h.current_pre((2, 1), Expect::Version(WRONG_VERSION))?;
        let ahead = |epoch: Epoch| Epoch(epoch.0.saturating_add(1));
        let stale_request = Preconditions {
            writer: match fresh.writer {
                Writer::Request { partition, epoch } => Writer::request(partition, ahead(epoch)),
                Writer::Maintenance => Writer::Maintenance,
            },
            ..fresh
        };
        let stale_target = Preconditions {
            partition_epoch: ahead(fresh.partition_epoch),
            ..fresh
        };
        let stale_site = Preconditions {
            site_epoch: ahead(fresh.site_epoch),
            ..fresh
        };
        let all_stale = Preconditions {
            partition_epoch: stale_target.partition_epoch,
            site_epoch: stale_site.site_epoch,
            ..stale_request
        };
        let request_and_site = Preconditions {
            site_epoch: stale_site.site_epoch,
            ..stale_request
        };
        let target_and_site = Preconditions {
            site_epoch: stale_site.site_epoch,
            ..stale_target
        };
        let bad_host = INVALID_HOSTS.first().copied().unwrap_or_default();
        let rows = [
            (all_stale, bad_host, Refused::PartitionRevoked),
            (request_and_site, bad_host, Refused::PartitionRevoked),
            (target_and_site, bad_host, Refused::PartitionRevoked),
            (stale_site, bad_host, Refused::SiteCleared),
            (fresh, bad_host, Refused::InvalidHost),
            (fresh, host_at(1), Refused::Entry),
        ];
        for (pre, host, reason) in rows {
            let (meta, body) = sample_entry(Partition::Public, key_at(1), host, "order", 0);
            h.expect_outcome(
                &format!("{reason:?} first"),
                replace_txn(meta, body, pre),
                TxnOutcome::Refused(reason),
            )?;
        }
        h.settle()
    })
}

/// A `Replace` whose host fails `valid_host` is `Refused(InvalidHost)`
/// and writes nothing, in a scoped partition and in `Public`, also with
/// a stop after each step; every host of [`VALID_HOSTS`] is stored, and
/// a site clear of that host removes it (R-CACHE-09).
///
/// # Errors
///
/// The first [`Violation`].
pub fn refused_invalid_host<D, F>(make: &mut F) -> Checked
where
    D: FaultInjection,
    F: FnMut() -> D,
{
    with_driver(make, "refused_invalid_host", |h| {
        h.run(put(0, 0))?;
        for host in INVALID_HOSTS {
            for cell in [(0, 3), (2, 3), (0, 0)] {
                let pre = h.current_pre(cell, Expect::Any)?;
                let (meta, body) =
                    sample_entry(partition_at(cell.0), key_at(cell.1), host, "host", 5);
                h.expect_outcome(
                    &format!("host {host:?}"),
                    replace_txn(meta, body, pre),
                    TxnOutcome::Refused(Refused::InvalidHost),
                )?;
            }
            h.settle()?;
        }
        for step in CommitStep::ALL {
            let pre = h.current_pre((0, 0), Expect::Any)?;
            let (meta, body) = sample_entry(partition_at(0), key_at(0), "Shop.example.com", "x", 7);
            h.driver.stop_after(step);
            let result = h.driver.apply(replace_txn(meta, body, pre));
            h.ensure_unavailable(&result)?;
            drive!(h, "restart", restart())?;
            h.recover_at(NOW)?;
            h.settle()?;
        }
        for host in VALID_HOSTS {
            let pre = h.current_pre((2, 3), Expect::Any)?;
            let (meta, body) = sample_entry(Partition::Public, key_at(3), host, "p2/valid", 1);
            h.put_explicit((2, 3), meta, body, pre)?;
            let pre = h.current_pre((2, 3), Expect::Any)?;
            let domain = Some(host.to_owned());
            h.expect_outcome(
                &format!("site clear of {host:?}"),
                CacheTransaction {
                    op: TxnOp::ClearSite {
                        domain: domain.clone(),
                    },
                    pre,
                },
                TxnOutcome::Applied { version: None },
            )?;
            h.model_apply(Change::Clear(domain), (2, 3), Ver::New { not: None });
            let left = h.read((2, 3), NOW)?;
            h.ensure(left.is_none(), || {
                format!("a site clear of {host:?} left its entry")
            })?;
            h.settle()?;
        }
        Ok(())
    })
}

/// `recover(now)` removes the disabled-key records whose time is before
/// `now`, keeps a record whose time is `now` or later, and keeps the
/// entry that was stored for an expired key; the removal is durable.
///
/// # Errors
///
/// The first [`Violation`].
pub fn recover_removes_expired_disabled_keys<D, F>(make: &mut F) -> Checked
where
    D: FaultInjection,
    F: FnMut() -> D,
{
    with_driver(make, "recover_removes_expired_disabled_keys", |h| {
        // Until NOW + 5, NOW + 10 and NOW + 20.
        h.run(ScriptOp::new(Action::DisableKey, 0, 0).with_variant(4))?;
        h.run(ScriptOp::new(Action::DisableKey, 1, 0).with_variant(9))?;
        h.run(ScriptOp::new(Action::DisableKey, 2, 1).with_variant(19))?;
        // A replace of a disabled key: stored and hidden.
        h.run(put(0, 0))?;
        let now = NOW.plus(10);
        drive!(h, "restart", restart())?;
        h.recover_at(now)?;
        // The model reads at NOW: only the record of NOW + 5 is gone.
        h.settle()?;
        let checks = [
            ((0, 0), false, "a record before now stays"),
            ((1, 0), true, "a record at now was removed"),
            ((2, 1), true, "a record after now was removed"),
        ];
        for (cell, disabled, detail) in checks {
            let got = drive!(
                h,
                "key_disabled",
                key_disabled(partition_at(cell.0), &key_at(cell.1), NOW)
            )?;
            h.ensure(got == disabled, || detail.to_owned())?;
        }
        let entry = h.read((0, 0), NOW)?;
        h.ensure(entry.is_some(), || {
            "the entry of an expired key is not readable".to_owned()
        })?;
        drive!(h, "restart", restart())?;
        h.recover_at(NOW)?;
        h.settle()
    })
}

/// A revoke and a site clear are applied with any preconditions: a
/// removal is never refused.
///
/// # Errors
///
/// The first [`Violation`].
pub fn removals_are_never_refused<D, F>(make: &mut F) -> Checked
where
    D: FaultInjection,
    F: FnMut() -> D,
{
    with_driver(make, "removals_are_never_refused", |h| {
        for partition in 0..3 {
            h.run(put(partition, 0))?;
        }
        for stale in [Stale::Partition, Stale::Site] {
            h.run(put(0, 0))?;
            h.run(
                ScriptOp::new(Action::RevokePartition, 0, 0)
                    .with_stale(stale)
                    .expecting(ScriptExpect::Wrong),
            )?;
            h.run(put(1, 0))?;
            h.run(
                ScriptOp::new(Action::ClearSite(None), 1, 0)
                    .with_stale(stale)
                    .expecting(ScriptExpect::Absent),
            )?;
        }
        Ok(())
    })
}

/// The same key in every partition: each read gives the entry of its
/// own partition; a change in one partition leaves the others.
///
/// # Errors
///
/// The first [`Violation`].
pub fn partitions_never_mix<D, F>(make: &mut F) -> Checked
where
    D: FaultInjection,
    F: FnMut() -> D,
{
    with_driver(make, "partitions_never_mix", |h| {
        for partition in 0..3 {
            h.run(put(partition, 0).with_variant(partition.wrapping_mul(3)))?;
        }
        for partition in 0..3_u8 {
            let entry = h.read((usize::from(partition), 0), NOW)?;
            let own = entry.is_some_and(|entry| {
                entry.meta.partition == partition_at(usize::from(partition))
                    && entry
                        .body
                        .page
                        .as_slice()
                        .starts_with(partition_tag(usize::from(partition)).as_bytes())
            });
            h.ensure(own, || {
                format!("partition {partition} does not read its own entry")
            })?;
        }
        h.run(ScriptOp::new(Action::Remove, 0, 0))?;
        h.run(ScriptOp::new(Action::UpdateFields, 1, 0))?;
        h.run(ScriptOp::new(Action::DisableKey, 2, 0))?;
        h.run(put(0, 0))?;
        h.run(ScriptOp::new(Action::Damage, 1, 0))?;
        h.run(ScriptOp::new(Action::RevokePartition, 0, 0))?;
        h.run(put(2, 1))?;
        h.run(put(0, 1))?;
        // The file of partition 0 is moved over the entry of `Public`.
        h.run(ScriptOp::new(Action::Damage, 2, 1).with_variant(2))
    })
}

/// Every [`Damage`]: `read` gives `Corrupted`, the entry is gone, also
/// after a restart; every other entry is unchanged.
///
/// # Errors
///
/// The first [`Violation`].
pub fn corrupted<D, F>(make: &mut F) -> Checked
where
    D: FaultInjection,
    F: FnMut() -> D,
{
    for variant in 0..3 {
        with_driver(make, "corrupted", |h| {
            h.run(put(0, 0).with_variant(variant))?;
            h.run(put(1, 0).with_variant(variant))?;
            h.run(put(0, 1))?;
            h.run(ScriptOp::new(Action::Damage, 0, 0).with_variant(variant))?;
            let again = h.read((0, 0), NOW)?;
            h.ensure(again.is_none(), || {
                "the damaged entry is readable".to_owned()
            })?;
            drive!(h, "restart", restart())?;
            h.recover_at(NOW)?;
            h.settle()?;
            // A damage of an entry that does not exist does nothing.
            h.run(ScriptOp::new(Action::Damage, 0, 0).with_variant(variant))
        })?;
    }
    Ok(())
}

/// `touch` records the time of use in the metadata; a touch without an
/// entry does nothing.
///
/// # Errors
///
/// The first [`Violation`].
pub fn touch<D, F>(make: &mut F) -> Checked
where
    D: FaultInjection,
    F: FnMut() -> D,
{
    with_driver(make, "touch", |h| {
        h.run(put(0, 0))?;
        let later = NOW.plus(5);
        drive!(h, "touch", touch(partition_at(0), &key_at(0), later))?;
        drive!(h, "touch", touch(partition_at(1), &key_at(0), later))?;
        if let Some(entry) = h.model.entries.get_mut(&(0, 0)) {
            entry.meta.last_used_at = later;
        }
        h.settle()
    })
}

/// The transactions of the fault table: every kind of [`TxnOp`], each
/// on the state of [`seed_script`].
#[must_use]
pub fn fault_table() -> Vec<ScriptOp> {
    vec![
        put(0, 0).expecting(ScriptExpect::Current).with_variant(6),
        put(0, 2).expecting(ScriptExpect::Absent).with_variant(5),
        ScriptOp::new(Action::UpdateFields, 0, 0).expecting(ScriptExpect::Current),
        ScriptOp::new(Action::Remove, 0, 0).expecting(ScriptExpect::Current),
        ScriptOp::new(Action::DisableKey, 0, 0).with_variant(30),
        ScriptOp::new(Action::RevokePartition, 0, 0),
        ScriptOp::new(Action::RevokePartition, 2, 0),
        ScriptOp::new(Action::ClearSite(Some(0)), 0, 0),
        ScriptOp::new(Action::ClearSite(None), 0, 0),
        // A refused transaction with a stop: always the old state.
        put(0, 0).with_stale(Stale::Partition),
        // A session of a revoked request partition, into `Public`.
        put(2, 0).with_stale(Stale::Request),
    ]
}

/// The state before each row of [`fault_table`]: entries in every
/// partition, the same key in two of them, and a disabled key.
#[must_use]
pub fn seed_script() -> Vec<ScriptOp> {
    vec![
        put(0, 0).with_variant(7),
        put(1, 0).with_variant(1),
        put(2, 1).with_variant(4),
        put(0, 1),
        put(1, 2),
        put(0, 3),
        ScriptOp::new(Action::DisableKey, 1, 3).with_variant(40),
    ]
}

fn fault_matrix<D, F>(make: &mut F, check: &'static str, fault: fn(CommitStep) -> Fault) -> Checked
where
    D: FaultInjection,
    F: FnMut() -> D,
{
    for row in fault_table() {
        for step in CommitStep::ALL {
            with_driver(make, check, |h| {
                for op in seed_script() {
                    h.run(op)?;
                }
                h.run(row.with_fault(fault(step))).map_err(|mut violation| {
                    violation.detail =
                        format!("{:?} at {step:?}: {}", row.action, violation.detail);
                    violation
                })
            })?;
        }
    }
    Ok(())
}

/// For every row of [`fault_table`] and every [`CommitStep`]: a stop
/// after the step, a restart and a recovery leave exactly the old state
/// before `CommitMeta` and exactly the new state from `CommitMeta` on;
/// every other entry is unchanged; a second recovery removes nothing.
///
/// # Errors
///
/// The first [`Violation`].
pub fn old_or_new<D, F>(make: &mut F) -> Checked
where
    D: FaultInjection,
    F: FnMut() -> D,
{
    fault_matrix(make, "old_or_new", Fault::StopAfter)
}

/// For every row of [`fault_table`] and every [`CommitStep`]: an I/O
/// failure at the step gives `Unavailable` and the old state up to
/// `CommitMeta`; at `RemoveOld` the commit stands.
///
/// # Errors
///
/// The first [`Violation`].
pub fn failed_step_keeps_the_old_state<D, F>(make: &mut F) -> Checked
where
    D: FaultInjection,
    F: FnMut() -> D,
{
    fault_matrix(make, "failed_step_keeps_the_old_state", Fault::FailAt)
}

/// Runs `script` on a new driver and compares it with the model after
/// every step.
///
/// # Errors
///
/// The first [`Violation`].
pub fn run_script<D: FaultInjection>(driver: &mut D, script: &[ScriptOp]) -> Checked {
    let mut harness = Harness::start(driver, "run_script")?;
    for op in script {
        harness.run(*op).map_err(|mut violation| {
            violation.detail = format!("{op:?}: {}", violation.detail);
            violation
        })?;
    }
    Ok(())
}

/// Runs `script` on a new driver without the model and checks only the
/// partition rule after every step: a read in a partition gives only an
/// entry that was stored in that partition, and after an applied
/// revoke nothing of that partition is readable or listed.
///
/// # Errors
///
/// The first [`Violation`].
pub fn script_keeps_partitions_apart<D: FaultInjection>(
    driver: &mut D,
    script: &[ScriptOp],
) -> Checked {
    const CHECK: &str = "script_keeps_partitions_apart";
    call(CHECK, "recover", driver.recover(NOW))?;
    for (seq, op) in script.iter().enumerate() {
        let cell = (usize::from(op.partition), usize::from(op.key));
        let partition = partition_at(cell.0);
        let revoked = apply_blind(driver, op, cell, seq)?;
        for (index, partition_here) in PARTITIONS.iter().enumerate() {
            for key_here in &KEYS {
                let read = driver.read(*partition_here, key_here, NOW);
                match read {
                    Ok(Some(entry)) => {
                        let own = entry.meta.partition == *partition_here
                            && entry.meta.key == *key_here
                            && entry
                                .body
                                .page
                                .as_slice()
                                .starts_with(partition_tag(index).as_bytes());
                        ensure(CHECK, own, || {
                            format!("step {seq}: a read in partition {index} gave another entry")
                        })?;
                        ensure(CHECK, !(revoked && *partition_here == partition), || {
                            format!(
                                "step {seq}: an entry of the revoked partition {index} is readable"
                            )
                        })?;
                    }
                    Ok(None) => {}
                    Err(error) if error.kind() == StoreErrorKind::Corrupted => {}
                    Err(error) => {
                        return Err(violation(
                            CHECK,
                            format!("step {seq}: read failed: {error}"),
                        ));
                    }
                }
            }
        }
        if revoked {
            let listed = call(CHECK, "list", driver.list())?;
            ensure(
                CHECK,
                listed.iter().all(|s| s.meta.partition != partition),
                || format!("step {seq}: list shows an entry of the revoked partition"),
            )?;
        }
    }
    Ok(())
}

/// Applies one script step without a model. Returns true when it was an
/// applied revoke.
fn apply_blind<D: FaultInjection>(
    driver: &mut D,
    op: &ScriptOp,
    cell: Cell,
    seq: usize,
) -> Result<bool, Violation> {
    const CHECK: &str = "script_keeps_partitions_apart";
    let partition = partition_at(cell.0);
    let key = key_at(cell.1);
    if op.action == Action::Damage {
        let damage = match op.variant % 3 {
            0 => Damage::Content,
            1 => Damage::Missing,
            _ => Damage::MovedFrom {
                partition: partition_at(next_partition(cell.0)),
                key,
            },
        };
        call(CHECK, "damage", driver.damage(partition, &key, damage))?;
        return Ok(false);
    }
    let current = call(CHECK, "list", driver.list())?
        .into_iter()
        .find(|s| s.meta.partition == partition && s.meta.key == key)
        .map(|s| s.version);
    let entry = match op.expect {
        ScriptExpect::Any => Expect::Any,
        ScriptExpect::Absent => Expect::Absent,
        ScriptExpect::Current => Expect::Version(current.unwrap_or(WRONG_VERSION)),
        ScriptExpect::Wrong => Expect::Version(WRONG_VERSION),
    };
    let request = request_of(cell);
    let request_epoch = call(
        CHECK,
        "partition_epoch",
        driver.partition_epoch(Partition::Scoped(request)),
    )?;
    let partition_epoch = call(CHECK, "partition_epoch", driver.partition_epoch(partition))?;
    let site_epoch = call(CHECK, "site_epoch", driver.site_epoch())?;
    let pre = Preconditions {
        entry,
        writer: Writer::request(
            request,
            if op.stale == Stale::Request {
                older(request_epoch)
            } else {
                request_epoch
            },
        ),
        partition_epoch: if op.stale == Stale::Partition {
            older(partition_epoch)
        } else {
            partition_epoch
        },
        site_epoch: if op.stale == Stale::Site {
            older(site_epoch)
        } else {
            site_epoch
        },
    };
    let txn_op = match op.action {
        Action::Replace => {
            let (meta, body) = sample_entry(
                partition,
                key,
                host_at(cell.1),
                &tag(cell.0, cell.1, seq),
                op.variant,
            );
            TxnOp::Replace {
                meta: Box::new(meta),
                body: Box::new(body),
            }
        }
        Action::UpdateFields => TxnOp::UpdateFields {
            partition,
            key,
            fields: vec![FieldLine::new("x-step", seq.to_string())],
            policy: sample_policy(u64::from(op.variant)),
        },
        Action::Remove => TxnOp::Remove { partition, key },
        Action::DisableKey => TxnOp::DisableKey {
            partition,
            key,
            until: NOW.plus(u64::from(op.variant).saturating_add(1)),
        },
        Action::RevokePartition | Action::Damage => TxnOp::RevokePartition { partition },
        Action::ClearSite(domain) => TxnOp::ClearSite {
            domain: domain
                .and_then(|index| DOMAINS.get(usize::from(index)))
                .map(|d| (*d).to_owned()),
        },
    };
    match op.fault {
        Some(Fault::StopAfter(step)) => driver.stop_after(step),
        Some(Fault::FailAt(step)) => driver.fail_at(step),
        None => {}
    }
    let result = driver.apply(CacheTransaction { op: txn_op, pre });
    if let Some(Fault::StopAfter(_)) = op.fault {
        call(CHECK, "restart", driver.restart())?;
        call(CHECK, "recover", driver.recover(NOW))?;
    }
    Ok(op.action == Action::RevokePartition && matches!(result, Ok(TxnOutcome::Applied { .. })))
}

/// Every check of the suite, in order.
///
/// # Errors
///
/// The first [`Violation`].
pub fn run_all<D, F>(make: &mut F) -> Checked
where
    D: FaultInjection,
    F: FnMut() -> D,
{
    replace_then_read(make)?;
    update_fields(make)?;
    remove(make)?;
    disable_key(make)?;
    revoke_partition(make)?;
    clear_site_every_partition(make)?;
    clear_site_keeps_other_sites(make)?;
    clear_site_all(make)?;
    refused_entry(make)?;
    refused_partition_revoked(make)?;
    refused_site_cleared(make)?;
    refused_request_partition_revoked(make)?;
    refused_request_partition_none_is_not_checked(make)?;
    refusal_order(make)?;
    refused_invalid_host(make)?;
    recover_removes_expired_disabled_keys(make)?;
    removals_are_never_refused(make)?;
    partitions_never_mix(make)?;
    corrupted(make)?;
    touch(make)?;
    old_or_new(make)?;
    failed_step_keeps_the_old_state(make)
}
