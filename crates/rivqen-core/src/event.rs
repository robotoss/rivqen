// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Events from the adapter (data-model section 5). Owner: WP-05 S2 T-04.
//!
//! Skeleton: the variants are the contract; the payloads are opaque
//! placeholders (see [`crate::action`]). The session generation and the
//! request id of each event (INV-12, core API section 2 rule 3) come with the
//! session in WP-08.

use core::fmt;

use rivqen_cache::txn::StoredEntry;
use rivqen_model::time::Instant;

use crate::action::placeholder;

/// How much memory the OS asks to free (`MemoryPressure`; storage section 4).
/// Its `Debug` writes the level name only.
pub use rivqen_cache::memory::PressureLevel;

/// A stored snapshot that a cache lookup found (`CacheResult`): the entry as
/// the driver returned it, checked against its stored hashes.
///
/// [`fmt::Debug`] writes the version and the size in bytes only: the entry
/// holds the host, the validators and the page.
#[derive(Clone, PartialEq, Eq)]
pub struct Snapshot(Box<StoredEntry>);

impl Snapshot {
    /// Wraps a stored entry.
    #[must_use]
    pub fn new(entry: StoredEntry) -> Self {
        Self(Box::new(entry))
    }

    /// The entry.
    #[must_use]
    pub fn entry(&self) -> &StoredEntry {
        &self.0
    }

    /// The entry, by value.
    #[must_use]
    pub fn into_entry(self) -> StoredEntry {
        *self.0
    }
}

impl fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Snapshot")
            .field("version", &self.0.version)
            .field("size_bytes", &self.0.body.size_bytes())
            .finish_non_exhaustive()
    }
}
placeholder! {
    /// Status and response field lines (`HttpResponseHead`). Fields: WP-08.
    ResponseMeta
}
placeholder! {
    /// One body chunk of bounded size (`HttpBodyChunk`). Fields: WP-08.
    BodyChunk
}
placeholder! {
    /// Why the body failed (`HttpFailed`). Fields: WP-08.
    HttpFailure
}

/// What the adapter reports (data-model section 5, table "Event").
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Event {
    /// The `WebView` can accept a document.
    WebViewReady,
    /// The storage driver finished a lookup; `None` when nothing is stored.
    CacheResult(Option<Snapshot>),
    /// Status and headers arrived.
    HttpResponseHead(ResponseMeta),
    /// A body chunk arrived.
    HttpBodyChunk(BodyChunk),
    /// The body ended.
    HttpCompleted,
    /// The body failed.
    HttpFailed(HttpFailure),
    /// The web SDK in the page connected.
    BridgeReady,
    /// The page reported first useful render.
    PageRendered,
    /// The user or app left the page.
    NavigationCancelled,
    /// The account changed (logout, switch).
    PartitionChanged,
    /// The OS asked to reduce memory.
    MemoryPressure(PressureLevel),
    /// Timer event from the adapter's clock.
    Tick(Instant),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_event_placeholders_print_the_type_name_only() {
        let events = [
            (
                Event::HttpResponseHead(ResponseMeta::placeholder()),
                "HttpResponseHead(ResponseMeta(..))",
            ),
            (
                Event::HttpBodyChunk(BodyChunk::placeholder()),
                "HttpBodyChunk(BodyChunk(..))",
            ),
            (
                Event::HttpFailed(HttpFailure::placeholder()),
                "HttpFailed(HttpFailure(..))",
            ),
            (
                Event::MemoryPressure(PressureLevel::Critical),
                "MemoryPressure(Critical)",
            ),
        ];
        for (event, want) in &events {
            assert_eq!(format!("{event:?}"), *want);
        }
    }
}
