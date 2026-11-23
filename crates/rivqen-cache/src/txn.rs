// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Cache transactions (data model section 3; storage section 2; legacy
//! client section 9.12; DL-181, DL-185, DL-205). Contract of the architect:
//! lanes do not change this file.
//!
//! A transaction is one atomic change of the store. It carries its
//! preconditions; a driver refuses a transaction whose precondition is
//! false, and a refused transaction writes nothing.
//!
//! Rules that every driver follows (DL-200, DL-205; the driver contract
//! suite of `rivqen-testkit` checks them):
//!
//! - `RevokePartition` and `ClearSite` ignore the preconditions and are
//!   never refused.
//! - A keyed kind (`Replace`, `UpdateFields`, `Remove`, `DisableKey`) runs
//!   its checks at the commit point (`CommitStep::CommitMeta`), not before
//!   the first step, in this order; the first false check gives the result:
//!   1. the epoch of the request partition ([`Preconditions::writer`],
//!      when it is `Some`): [`Refused::PartitionRevoked`]. A session of a
//!      revoked partition commits nothing, also not into `public` (cache
//!      identity rule 4, RSK-06);
//!   2. the epoch of the target partition: [`Refused::PartitionRevoked`];
//!   3. the site epoch: [`Refused::SiteCleared`];
//!   4. for `Replace` only, the host of the entry
//!      ([`crate::partition::valid_host`]): [`Refused::InvalidHost`]. A
//!      site clear matches every host that passes (R-CACHE-09);
//!   5. the entry: [`Refused::Entry`].
//!
//!   An epoch that differs from the current one is refused, also a higher
//!   one.
//! - `UpdateFields` without an entry is `Refused::Entry` for every `Expect`;
//!   it replaces the whole field list. `Remove` and `DisableKey` without an
//!   entry are applied, with no version.
//! - A version differs from every earlier version of its key. An epoch grows
//!   by exactly 1.
//! - `ClearSite` with a domain keeps the disabled-key records: they have no
//!   host and no page data. `ClearSite` of all sites and `RevokePartition`
//!   remove them.
//! - A `Replace` of a disabled key is stored; a read hides it until the time
//!   of the record. `StorageDriver::recover` removes the records whose time
//!   is before its `now`.

use std::fmt;

use rivqen_model::http::FieldLine;
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_model::safe::PageBytes;
use rivqen_model::time::UnixTime;
use rivqen_proto::types::SnapshotMeta;

use crate::key::{Partition, Protocol, VaryEntry};
use crate::policy::EntryPolicy;

/// The schema version of stored entries (freshness section 4, "App update
/// with a new cache schema").
pub const SCHEMA_VERSION: u32 = 1;

/// The version of one entry: a new value for each replace or field update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntryVersion(pub u64);

/// A counter that a revoke of a partition or a site clear increments. A
/// writer reads it when its session starts and gives it back with its
/// transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Epoch(pub u64);

/// The validators of an entry (data model section 3).
///
/// [`fmt::Debug`] writes the first 8 characters of each tag: a tag can be an
/// unkeyed hash of the content (errors-logging section 4, H-143).
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Validators {
    /// Legacy `etag` (lower-case hex) or the HTTP entity tag.
    pub etag: Option<String>,
    /// Legacy `template-tag`.
    pub template_tag: Option<String>,
    /// The computed values of an RQP snapshot.
    pub rqp: Option<SnapshotMeta>,
}

impl fmt::Debug for Validators {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Validators")
            .field("etag", &self.etag.as_deref().map(ShortTag))
            .field("template_tag", &self.template_tag.as_deref().map(ShortTag))
            .field("rqp", &self.rqp)
            .finish()
    }
}

/// A tag in a `Debug` text: at most its first 8 characters.
struct ShortTag<'a>(&'a str);

impl fmt::Debug for ShortTag<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prefix: String = self.0.chars().take(8).collect();
        write!(f, "{prefix}...")
    }
}

/// The metadata of one entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryMeta {
    /// The cache key.
    pub key: CacheKey,
    /// The partition.
    pub partition: Partition,
    /// The host of the origin, in clear text, for the site clear (DL-184).
    pub host: String,
    /// The protocol variant.
    pub protocol: Protocol,
    /// The `Vary` entries of the key.
    pub vary: Vec<VaryEntry>,
    /// The validators.
    pub validators: Validators,
    /// The freshness inputs.
    pub policy: EntryPolicy,
    /// When the entry was stored.
    pub stored_at: UnixTime,
    /// When the entry was last used; the eviction order.
    pub last_used_at: UnixTime,
}

/// The content of one entry. Immutable once committed.
///
/// [`fmt::Debug`] writes sizes only (errors-logging section 2.4).
#[derive(Clone, PartialEq, Eq)]
pub struct SnapshotBody {
    /// The document as the web view gets it.
    pub page: PageBytes,
    /// The template (legacy) or nothing (RQP: the page holds the blocks).
    pub template: Option<PageBytes>,
    /// The data blocks in document order (legacy), key and value.
    pub data: Vec<(String, PageBytes)>,
    /// The page fields (CL-21): never `Set-Cookie`, hop-by-hop or cache
    /// fields.
    pub fields: Vec<FieldLine>,
}

impl SnapshotBody {
    /// The bytes that count for the L1 and L2 budgets.
    #[must_use]
    pub fn size_bytes(&self) -> u64 {
        let mut total = self.page.len();
        if let Some(template) = &self.template {
            total = total.saturating_add(template.len());
        }
        for (key, value) in &self.data {
            total = total.saturating_add(key.len()).saturating_add(value.len());
        }
        for line in &self.fields {
            total = total
                .saturating_add(line.name.len())
                .saturating_add(line.value.len());
        }
        u64::try_from(total).unwrap_or(u64::MAX)
    }
}

impl fmt::Debug for SnapshotBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SnapshotBody")
            .field("size_bytes", &self.size_bytes())
            .field("blocks", &self.data.len())
            .field("fields", &self.fields.len())
            .finish_non_exhaustive()
    }
}

/// One entry as a driver returns it: checked against its stored hashes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredEntry {
    /// The metadata.
    pub meta: EntryMeta,
    /// The version.
    pub version: EntryVersion,
    /// The content.
    pub body: SnapshotBody,
}

/// What must hold for the entry of the key when a transaction runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expect {
    /// Any state.
    Any,
    /// No entry.
    Absent,
    /// The entry has this version.
    Version(EntryVersion),
}

/// The writer of a transaction (DL-220). There is no default: each
/// transaction names its writer, so a session write cannot leave out the
/// check of its request partition (RSK-06).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Writer {
    /// A request session. `partition` is the partition of its request and
    /// `epoch` is the epoch of `Partition::Scoped(partition)` when the
    /// session started.
    Request {
        /// The partition of the request.
        partition: PartitionId,
        /// The epoch of that partition at the session start.
        epoch: Epoch,
    },
    /// A change that no request session makes (a revoke, a site clear,
    /// eviction): no request partition is checked.
    Maintenance,
}

impl Writer {
    /// A request writer.
    #[must_use]
    pub const fn request(partition: PartitionId, epoch: Epoch) -> Self {
        Self::Request { partition, epoch }
    }

    /// The request partition and its epoch, or `None` for
    /// [`Writer::Maintenance`].
    #[must_use]
    pub const fn request_epoch(&self) -> Option<(PartitionId, Epoch)> {
        match *self {
            Self::Request { partition, epoch } => Some((partition, epoch)),
            Self::Maintenance => None,
        }
    }
}

/// The preconditions of a transaction (legacy client section 9.12,
/// "Partition race"). The module documentation gives the order of the
/// checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preconditions {
    /// The state of the entry.
    pub entry: Expect,
    /// Who makes the change (DL-205, DL-220). For a [`Writer::Request`] a
    /// keyed kind is refused with [`Refused::PartitionRevoked`] when the
    /// current epoch of the request partition differs, whatever the target
    /// partition is.
    pub writer: Writer,
    /// The epoch of the target partition that the writer saw.
    pub partition_epoch: Epoch,
    /// The epoch of the site clear that the writer saw.
    pub site_epoch: Epoch,
}

/// One change of the store.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TxnOp {
    /// Store a new version of an entry.
    Replace {
        /// The metadata of the new version.
        meta: Box<EntryMeta>,
        /// The content of the new version.
        body: Box<SnapshotBody>,
    },
    /// Replace the page fields and the freshness inputs of an entry after a
    /// `304` (CL-17; RFC 9111 section 4.3.4).
    UpdateFields {
        /// The partition.
        partition: Partition,
        /// The key.
        key: CacheKey,
        /// The new page fields.
        fields: Vec<FieldLine>,
        /// The new freshness inputs.
        policy: EntryPolicy,
    },
    /// Remove an entry.
    Remove {
        /// The partition.
        partition: Partition,
        /// The key.
        key: CacheKey,
    },
    /// Remove the entry and disable the key until a time (CL-19).
    DisableKey {
        /// The partition.
        partition: Partition,
        /// The key.
        key: CacheKey,
        /// The key opens no legacy session before this time.
        until: UnixTime,
    },
    /// Remove every entry and key of a partition and increment its epoch
    /// (R-CACHE-04).
    RevokePartition {
        /// The partition.
        partition: Partition,
    },
    /// Remove the entries of a site in every partition and increment the
    /// site epoch (R-CACHE-09). `None` clears every entry.
    ClearSite {
        /// The cleared domain, lower-case ASCII.
        domain: Option<String>,
    },
}

/// A cache transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheTransaction {
    /// The change.
    pub op: TxnOp,
    /// What must hold.
    pub pre: Preconditions,
}

/// Why a driver refused a transaction. Nothing was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Refused {
    /// The entry is not in the expected state.
    Entry,
    /// The partition of the request or the target partition was revoked
    /// after the writer read its epoch.
    PartitionRevoked,
    /// The site was cleared after the writer read the site epoch.
    SiteCleared,
    /// The host of a `Replace` is not a lower-case ASCII host name or an IP
    /// literal ([`crate::partition::valid_host`]): a site clear could miss
    /// it (R-CACHE-09).
    InvalidHost,
}

/// The result of a transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxnOutcome {
    /// The change is committed. `version` is the new version of the entry
    /// for [`TxnOp::Replace`] and [`TxnOp::UpdateFields`].
    Applied {
        /// The new version of the entry.
        version: Option<EntryVersion>,
    },
    /// A precondition was false.
    Refused(Refused),
}

/// The steps of a commit, in order (storage section 2). A fault test stops a
/// driver after each of them (DL-181).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommitStep {
    /// Step 1: the new files are written to the temporary place.
    WriteTemp,
    /// Step 2: the new files are synced.
    SyncTemp,
    /// Step 3: the new files are at their final place.
    Promote,
    /// Step 4: the metadata is committed. The commit point.
    CommitMeta,
    /// Step 5: the old version is removed.
    RemoveOld,
}

impl CommitStep {
    /// Every step, in order.
    pub const ALL: [Self; 5] = [
        Self::WriteTemp,
        Self::SyncTemp,
        Self::Promote,
        Self::CommitMeta,
        Self::RemoveOld,
    ];
}
