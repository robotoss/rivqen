// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Cache transactions (data model section 3; storage section 2; legacy
//! client section 9.12; DL-181, DL-185). Contract of the architect: lanes do
//! not change this file.
//!
//! A transaction is one atomic change of the store. It carries its
//! preconditions; a driver refuses a transaction whose precondition is
//! false, and a refused transaction writes nothing.
//!
//! Rules that every driver follows (DL-200; the driver contract suite of
//! `rivqen-testkit` checks them):
//!
//! - `RevokePartition` and `ClearSite` ignore the preconditions and are
//!   never refused.
//! - A keyed kind checks the partition epoch, then the site epoch, then the
//!   entry, at the commit point (`CommitStep::CommitMeta`), not before the
//!   first step. An epoch that differs from the current one is refused, also
//!   a higher one.
//! - `UpdateFields` without an entry is `Refused::Entry` for every `Expect`;
//!   it replaces the whole field list. `Remove` and `DisableKey` without an
//!   entry are applied, with no version.
//! - A version differs from every earlier version of its key. An epoch grows
//!   by exactly 1.
//! - `ClearSite` with a domain keeps the disabled-key records: they have no
//!   host and no page data. `ClearSite` of all sites and `RevokePartition`
//!   remove them.
//! - A `Replace` of a disabled key is stored; a read hides it until the time
//!   of the record.

use std::fmt;

use rivqen_model::http::FieldLine;
use rivqen_model::ids::CacheKey;
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
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Validators {
    /// Legacy `etag` (lower-case hex) or the HTTP entity tag.
    pub etag: Option<String>,
    /// Legacy `template-tag`.
    pub template_tag: Option<String>,
    /// The computed values of an RQP snapshot.
    pub rqp: Option<SnapshotMeta>,
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

/// The preconditions of a transaction (legacy client section 9.12,
/// "Partition race").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preconditions {
    /// The state of the entry.
    pub entry: Expect,
    /// The epoch of the partition that the writer saw.
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
pub enum Refused {
    /// The entry is not in the expected state.
    Entry,
    /// The partition was revoked after the writer read its epoch.
    PartitionRevoked,
    /// The site was cleared after the writer read the site epoch.
    SiteCleared,
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
