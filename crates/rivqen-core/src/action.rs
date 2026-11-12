// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Actions and outcomes of the core (data-model section 5; core API section
//! 3). Owner: WP-05 S2 T-04.
//!
//! Skeleton: the variants are the contract; the payloads are opaque
//! placeholders until the work package that owns them defines their fields.
//! A placeholder has one private field and no public constructor, so no code
//! outside this crate can make one, and its `Debug` prints only the type name.

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
placeholder! {
    /// A cached document to show (`ShowCachedDocument`). Fields: WP-07.
    DocumentRef
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
    /// A cache transaction (`CommitCache`). Fields: WP-07.
    CacheTxn
}
placeholder! {
    /// The key of a cache entry (`InvalidateCache`; data-model section 1).
    /// Fields: WP-07.
    CacheKey
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

    #[test]
    fn core_action_placeholders_print_the_type_name_only() {
        let actions = [
            (
                Action::SendHttpRequest(RequestSpec::placeholder()),
                "SendHttpRequest(RequestSpec(..))",
            ),
            (
                Action::ShowCachedDocument(DocumentRef::placeholder()),
                "ShowCachedDocument(DocumentRef(..))",
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
                Action::CommitCache(CacheTxn::placeholder()),
                "CommitCache(CacheTxn(..))",
            ),
            (
                Action::InvalidateCache(CacheKey::placeholder()),
                "InvalidateCache(CacheKey(..))",
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
