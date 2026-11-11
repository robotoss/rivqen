// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Events from the adapter (data-model section 5). Owner: WP-05 S2 T-04.
//!
//! Skeleton: the variants are the contract; the payloads are opaque
//! placeholders (see [`crate::action`]). The session generation and the
//! request id of each event (INV-12, core API section 2 rule 3) come with the
//! session in WP-08.

use rivqen_model::time::Instant;

use crate::action::placeholder;

placeholder! {
    /// A stored snapshot that a cache lookup found (`CacheResult`). Fields: WP-07.
    Snapshot
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
placeholder! {
    /// How much memory the OS asks to free (`MemoryPressure`). Levels: WP-07.
    PressureLevel
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
                Event::CacheResult(Some(Snapshot::placeholder())),
                "CacheResult(Some(Snapshot(..)))",
            ),
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
                Event::MemoryPressure(PressureLevel::placeholder()),
                "MemoryPressure(PressureLevel(..))",
            ),
        ];
        for (event, want) in &events {
            assert_eq!(format!("{event:?}"), *want);
        }
    }
}
