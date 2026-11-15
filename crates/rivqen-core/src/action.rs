// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Actions and outcomes of the core (data-model section 5; core API section
//! 3). Owner: WP-05 S2 T-04.
//!
//! Skeleton: the variants are the contract; the payloads are opaque
//! placeholders until the work package that owns them defines their fields.
//! A placeholder has one private field and no public constructor, so no code
//! outside this crate can make one, and its `Debug` prints only the type name.

use core::fmt;

use rivqen_cache::key::Partition;
use rivqen_cache::txn::{CacheTransaction, EntryVersion, TxnOp};
use rivqen_model::ids::CacheKey as ModelCacheKey;
use rivqen_model::time::Instant;

/// Defines an opaque placeholder payload: a private unit field, `Debug` as
/// `Name(..)`. The work package that defines the fields replaces it and writes
/// a `Debug` that passes the marker scan (errors-logging section 4).
macro_rules! placeholder {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, PartialEq, Eq)]
        pub struct $name(());

        impl $name {
            /// A value for the tests of this crate.
            #[cfg(test)]
            pub(crate) const fn placeholder() -> Self {
                Self(())
            }
        }

        impl ::core::fmt::Debug for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.write_str(concat!(stringify!($name), "(..)"))
            }
        }
    };
}
pub(crate) use placeholder;

placeholder! {
    /// The request to send (`SendHttpRequest`). Fields: WP-08.
    RequestSpec
}
/// A cached document to show (`ShowCachedDocument`): the stored entry that is
/// named by partition, key and version. The adapter reads the entry from the
/// storage driver; the version lets it detect a replaced entry.
///
/// [`fmt::Debug`] writes the short forms of the ids and the version, no URL.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DocumentRef {
    /// The key of the entry (partition and hash).
    pub key: CacheKey,
    /// The version that the lookup found.
    pub version: EntryVersion,
}

impl DocumentRef {
    /// A reference to the entry of `key` at `version`.
    #[must_use]
    pub const fn new(key: CacheKey, version: EntryVersion) -> Self {
        Self { key, version }
    }
}

impl fmt::Debug for DocumentRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DocumentRef")
            .field("key", &self.key)
            .field("version", &self.version)
            .finish()
    }
}

/// A cache transaction (`CommitCache`): one atomic change of the store with
/// its preconditions.
///
/// [`fmt::Debug`] writes the kind of the change only: the change can hold a
/// host, a field value or a domain.
#[derive(Clone, PartialEq, Eq)]
pub struct CacheTxn(Box<CacheTransaction>);

impl CacheTxn {
    /// Wraps a transaction.
    #[must_use]
    pub fn new(txn: CacheTransaction) -> Self {
        Self(Box::new(txn))
    }

    /// The transaction.
    #[must_use]
    pub fn transaction(&self) -> &CacheTransaction {
        &self.0
    }

    /// The transaction, by value.
    #[must_use]
    pub fn into_transaction(self) -> CacheTransaction {
        *self.0
    }

    const fn kind(&self) -> &'static str {
        match &self.0.op {
            TxnOp::Replace { .. } => "Replace",
            TxnOp::UpdateFields { .. } => "UpdateFields",
            TxnOp::Remove { .. } => "Remove",
            TxnOp::DisableKey { .. } => "DisableKey",
            TxnOp::RevokePartition { .. } => "RevokePartition",
            TxnOp::ClearSite { .. } => "ClearSite",
            // A kind added later: no name, no content.
            _ => "Other",
        }
    }
}

impl fmt::Debug for CacheTxn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CacheTxn")
            .field("op", &format_args!("{}", self.kind()))
            .finish_non_exhaustive()
    }
}

/// The key of a cache entry (`InvalidateCache`; data-model section 1): the
/// partition and the hash of the cache identity.
///
/// [`fmt::Debug`] writes the first 8 hex characters of each id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CacheKey {
    /// The partition that holds the entry.
    pub partition: Partition,
    /// The key hash.
    pub key: ModelCacheKey,
}

impl CacheKey {
    /// The key of the entry of `key` in `partition`.
    #[must_use]
    pub const fn new(partition: Partition, key: ModelCacheKey) -> Self {
        Self { partition, key }
    }
}

placeholder! {
    /// A document stream for the `WebView` (`StreamDocument`). Fields: WP-09.
    StreamToken
}
placeholder! {
    /// A patch that passed every check (`ApplyDataPatch`). Fields: WP-08.
    ValidatedPatch
}
placeholder! {
    /// The URL of a normal load (`LoadNormally`). Fields: WP-08.
    LoadUrl
}
placeholder! {
    /// A local metric (`EmitMetric`). Fields: a later work package.
    Metric
}
placeholder! {
    /// Why a navigation fell back to a normal load (`Outcome::Fallback`).
    /// Fields: WP-08.
    FallbackReason
}

/// What the adapter must do (data-model section 5, table "Action").
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Action {
    /// Send this request with the platform stack.
    SendHttpRequest(RequestSpec),
    /// Load this cached document into the `WebView`.
    ShowCachedDocument(DocumentRef),
    /// Feed the document stream to the `WebView`.
    StreamDocument(StreamToken),
    /// Deliver this validated patch to the web SDK.
    ApplyDataPatch(ValidatedPatch),
    /// Execute this cache transaction atomically.
    CommitCache(CacheTxn),
    /// Remove this entry.
    InvalidateCache(CacheKey),
    /// Fall back to a normal `WebView` load.
    LoadNormally(LoadUrl),
    /// Record this metric locally.
    EmitMetric(Metric),
    /// Send a `Tick` at this time (adapter clock).
    ScheduleTimer(Instant),
}

/// What happened to a navigation (core API section 3).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Outcome {
    /// No cache. Full document from network.
    FirstLoad,
    /// The cached document is current.
    CacheHitNotModified,
    /// Same template, data blocks updated.
    DataUpdated,
    /// New template, full reload.
    TemplateChanged,
    /// Cached document shown under `stale-while-revalidate` or offline policy.
    ServedStale,
    /// Normal HTTPS load.
    Fallback(FallbackReason),
}

#[cfg(test)]
mod tests {
    use super::*;
    use rivqen_cache::txn::{Epoch, Expect, Preconditions};

    fn key() -> CacheKey {
        CacheKey::new(Partition::Public, ModelCacheKey::from_bytes([7; 32]))
    }

    fn txn() -> CacheTransaction {
        CacheTransaction {
            op: TxnOp::ClearSite { domain: None },
            pre: Preconditions {
                entry: Expect::Any,
                partition_epoch: Epoch(0),
                site_epoch: Epoch(0),
            },
        }
    }

    #[test]
    fn core_action_placeholders_print_the_type_name_only() {
        let actions = [
            (
                Action::SendHttpRequest(RequestSpec::placeholder()),
                "SendHttpRequest(RequestSpec(..))",
            ),
            (
                Action::ShowCachedDocument(DocumentRef::new(key(), EntryVersion(3))),
                "ShowCachedDocument(DocumentRef { key: CacheKey { partition: Public, key: CacheKey(07070707) }, version: EntryVersion(3) })",
            ),
            (
                Action::StreamDocument(StreamToken::placeholder()),
                "StreamDocument(StreamToken(..))",
            ),
            (
                Action::ApplyDataPatch(ValidatedPatch::placeholder()),
                "ApplyDataPatch(ValidatedPatch(..))",
            ),
            (
                Action::CommitCache(CacheTxn::new(txn())),
                "CommitCache(CacheTxn { op: ClearSite, .. })",
            ),
            (
                Action::InvalidateCache(key()),
                "InvalidateCache(CacheKey { partition: Public, key: CacheKey(07070707) })",
            ),
            (
                Action::LoadNormally(LoadUrl::placeholder()),
                "LoadNormally(LoadUrl(..))",
            ),
            (
                Action::EmitMetric(Metric::placeholder()),
                "EmitMetric(Metric(..))",
            ),
            (
                Action::ScheduleTimer(Instant(7)),
                "ScheduleTimer(Instant(7))",
            ),
        ];
        for (action, want) in &actions {
            assert_eq!(format!("{action:?}"), *want);
        }
        assert_eq!(
            format!("{:?}", Outcome::Fallback(FallbackReason::placeholder())),
            "Fallback(FallbackReason(..))"
        );
    }
}
