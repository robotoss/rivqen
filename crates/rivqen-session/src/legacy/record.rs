// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The cache record of a legacy session (WP-08 S2 contract; CL-12, CL-38,
//! CL-40, legacy client section 9.12, H-160).
//!
//! Owner of the bodies and of the private items: lane L2 (T-02). The public
//! items are the contract. Until L2 is integrated, no entry is visible and
//! every write is a removal (fail closed).
//!
//! H-160: a URL has one entry in a partition. Its key has no `Vary` entry;
//! the entry stores the `Vary` names of its response with the keyed digests
//! of the request values, and it is visible only to a request with the same
//! values. A commit for other values replaces it.

use core::fmt;

use rivqen_cache::key::{Partition, VaryEntry};
use rivqen_cache::txn::{
    CacheTransaction, EntryVersion, Epoch, Expect, Preconditions, StoredEntry, TxnOp, Writer,
};
use rivqen_integrity::legacy::Sha1Hex;
use rivqen_model::http::FieldLine;
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_model::time::UnixTime;
use rivqen_template::legacy::DataBlock;
use rivqen_url::ParsedUrl;

use crate::legacy::types::VaryKey;

/// The key of the one entry of `url` in `partition` (H-160): the cache key
/// of identity section 1 with no `Vary` entry, the protocol legacy and no
/// locale.
#[must_use]
pub fn entry_key(partition: PartitionId, url: &ParsedUrl) -> CacheKey {
    let _ = (partition, url);
    CacheKey::from_bytes([0; 32])
}

/// The request values that the `Vary` names of a response read (CL-12): the
/// fields of the session request (CL-09) and the fields that the platform
/// adds. The names `accept-diff`, `if-none-match` and `template-tag` do not
/// count.
#[derive(Clone, Copy)]
pub struct RequestValues<'a> {
    /// The legacy request fields of the session, the `cookie` field
    /// included.
    pub session_fields: &'a [FieldLine],
    /// The fields that the platform adds.
    pub platform_fields: &'a [FieldLine],
}

impl fmt::Debug for RequestValues<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestValues")
            .field("session_fields", &self.session_fields.len())
            .field("platform_fields", &self.platform_fields.len())
            .finish()
    }
}

/// A visible entry (CL-38): it passed the `Vary` match (CL-12) and the
/// SHA-1 checks of its page and template (CL-40).
#[derive(Clone, PartialEq, Eq)]
pub struct LegacyRecord {
    /// The version of the stored entry (the precondition of a replace).
    pub version: EntryVersion,
    /// The page.
    pub page: String,
    /// The template.
    pub template: String,
    /// The data blocks in stored order.
    pub data: Vec<DataBlock>,
    /// The page fields (CL-21).
    pub fields: Vec<FieldLine>,
    /// The `Vary` entries of the entry.
    pub vary: Vec<VaryEntry>,
    /// The etag: the SHA-1 of the page.
    pub etag: Sha1Hex,
    /// The template tag: the SHA-1 of the template.
    pub template_tag: Sha1Hex,
}

impl fmt::Debug for LegacyRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LegacyRecord")
            .field("version", &self.version)
            .field("page_len", &self.page.len())
            .field("template_len", &self.template.len())
            .field("data", &self.data.len())
            .field("fields", &self.fields.len())
            .field("vary", &self.vary.len())
            .finish_non_exhaustive()
    }
}

/// The visible record of `entry` for a request with `values` (CL-12,
/// CL-40), or `None`: an entry of another protocol, a `Vary` mismatch, a
/// page or template whose SHA-1 is not its etag or template tag, or text
/// that is not UTF-8. The session removes an entry that fails CL-40.
#[must_use]
pub fn visible(
    entry: &StoredEntry,
    values: &RequestValues<'_>,
    key: &VaryKey,
) -> Option<LegacyRecord> {
    let _ = (entry, values, key);
    None
}

/// Why an entry is not visible, when the session must remove it (CL-40).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NotVisible {
    /// The entry is not visible to these request values; it stays.
    OtherVariant,
    /// The entry fails the checks of CL-40; the session removes it.
    Damaged,
}

/// Like [`visible`], with the reason of a `None`.
///
/// # Errors
///
/// [`NotVisible`] when the entry is not visible.
pub fn check_visible(
    entry: &StoredEntry,
    values: &RequestValues<'_>,
    key: &VaryKey,
) -> Result<LegacyRecord, NotVisible> {
    visible(entry, values, key).ok_or(NotVisible::Damaged)
}

/// The `Vary` entries of a commit for a response with `response_fields`
/// (CL-12): the names of every `Vary` line except the three legacy names,
/// in lower case and sorted, each with the keyed digest of the request
/// value, or no value for an absent field. `None` for `Vary: *` (no
/// commit).
#[must_use]
pub fn vary_entries(
    response_fields: &[FieldLine],
    values: &RequestValues<'_>,
    key: &VaryKey,
) -> Option<Vec<VaryEntry>> {
    let _ = (response_fields, values, key);
    None
}

/// Who writes and what it saw (DL-220, legacy client section 9.12): the
/// request partition of the session and the epochs of its lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteBasis {
    /// The partition of the session.
    pub partition: PartitionId,
    /// The key of the entry (H-160).
    pub key: CacheKey,
    /// The epoch of the partition at the lookup.
    pub partition_epoch: Epoch,
    /// The epoch of the site clear at the lookup.
    pub site_epoch: Epoch,
    /// The state of the entry that the session saw.
    pub expect: Expect,
}

impl WriteBasis {
    /// The preconditions of a write of the session: always a request writer
    /// (DL-220).
    #[must_use]
    pub const fn preconditions(&self) -> Preconditions {
        Preconditions {
            entry: self.expect,
            writer: Writer::request(self.partition, self.partition_epoch),
            partition_epoch: self.partition_epoch,
            site_epoch: self.site_epoch,
        }
    }
}

/// The content of a new record (CL-38) and the inputs of its policy.
#[derive(Clone, Copy)]
pub struct NewRecord<'a> {
    /// The host of the URL (site clear, CL-45).
    pub host: &'a str,
    /// The `Vary` entries ([`vary_entries`]).
    pub vary: &'a [VaryEntry],
    /// The page.
    pub page: &'a str,
    /// The template.
    pub template: &'a str,
    /// The data blocks in order.
    pub data: &'a [DataBlock],
    /// The page fields (CL-21).
    pub fields: &'a [FieldLine],
    /// The etag (the SHA-1 of the page).
    pub etag: Sha1Hex,
    /// The template tag (the SHA-1 of the template).
    pub template_tag: Sha1Hex,
    /// The response field lines (the freshness inputs of the policy).
    pub response_fields: &'a [FieldLine],
    /// When the request was sent.
    pub request_time: UnixTime,
    /// When the response head arrived.
    pub response_time: UnixTime,
}

impl fmt::Debug for NewRecord<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NewRecord")
            .field("page_len", &self.page.len())
            .field("template_len", &self.template.len())
            .field("data", &self.data.len())
            .field("fields", &self.fields.len())
            .field("vary", &self.vary.len())
            .finish_non_exhaustive()
    }
}

/// The transaction that stores `record` under the key of `basis` (CL-26):
/// a replace of the entry with the expectation of `basis`. The field update
/// of CL-17 is a commit of the stored record with the merged fields.
#[must_use]
pub fn commit(basis: &WriteBasis, record: &NewRecord<'_>) -> CacheTransaction {
    let _ = record;
    remove(basis)
}

/// The transaction that removes the entry of `basis` (CL-14, CL-17, CL-18,
/// CL-19, CL-27, CL-29, CL-31, CL-40, CL-44).
#[must_use]
pub fn remove(basis: &WriteBasis) -> CacheTransaction {
    CacheTransaction {
        op: TxnOp::Remove {
            partition: Partition::Scoped(basis.partition),
            key: basis.key,
        },
        pre: basis.preconditions(),
    }
}

/// The transaction that disables the key of `basis` until `until` (CL-19).
#[must_use]
pub fn disable(basis: &WriteBasis, until: UnixTime) -> CacheTransaction {
    let _ = until;
    remove(basis)
}
