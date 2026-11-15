// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Parsers of the response fields that the cache policy reads (RFC 9111
//! section 5, RFC 9110 sections 5.6.7 and 12.5.5, RFC 5861). Owner: WP-07 S1
//! T-02.
//!
//! Contract (architect): the public types and signatures. A parser never
//! fails: a value that it cannot read gives the result that makes the cache
//! stricter (no lifetime, no date). Field names match ASCII
//! case-insensitively; every field line of the name counts.

use rivqen_model::http::FieldLine;
use rivqen_model::time::UnixTime;

/// The `Cache-Control` directives that the cache uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(clippy::struct_excessive_bools)] // one flag per directive
pub struct CacheControl {
    /// `no-store`, also with an argument (DL-088 rule 5).
    pub no_store: bool,
    /// `no-cache`, with or without field names.
    pub no_cache: bool,
    /// `private`.
    pub private: bool,
    /// `public`.
    pub public: bool,
    /// `must-revalidate`.
    pub must_revalidate: bool,
    /// `max-age` in seconds.
    pub max_age: Option<u64>,
    /// `stale-while-revalidate` in seconds (RFC 5861).
    pub stale_while_revalidate: Option<u64>,
    /// `stale-if-error` in seconds (RFC 5861).
    pub stale_if_error: Option<u64>,
    /// A freshness directive was present but not valid (for example
    /// `max-age=abc`, or two different `max-age` values): the response is
    /// stale (RFC 9111 section 4.2.1).
    pub invalid_freshness: bool,
}

/// The `Vary` field of a response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Vary {
    /// No `Vary` field, or only empty members.
    None,
    /// The named request fields, in ASCII lower case, sorted, no name twice.
    Names(Vec<String>),
    /// `Vary: *`: the response is not stored.
    Any,
}

/// Reads all `Cache-Control` field lines of `fields`.
#[must_use]
pub fn parse_cache_control(fields: &[FieldLine]) -> CacheControl {
    // Stub (T-02): the strictest answer.
    let _ = fields;
    CacheControl {
        no_store: true,
        ..CacheControl::default()
    }
}

/// Reads an HTTP-date (RFC 9110 section 5.6.7: IMF-fixdate and the two
/// obsolete forms). `None` when the text is not a valid date.
#[must_use]
pub fn parse_http_date(value: &str) -> Option<UnixTime> {
    // Stub (T-02).
    let _ = value;
    None
}

/// Reads the `Age` field: `None` when it is absent or not valid.
#[must_use]
pub fn parse_age(fields: &[FieldLine]) -> Option<u64> {
    // Stub (T-02).
    let _ = fields;
    None
}

/// Reads all `Vary` field lines of `fields`.
#[must_use]
pub fn parse_vary(fields: &[FieldLine]) -> Vary {
    // Stub (T-02): the strictest answer.
    let _ = fields;
    Vary::Any
}

/// The value of the legacy `cache-offline` field (legacy wire section 7).
#[cfg(feature = "legacy")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheOffline {
    /// `true`: store and refresh.
    True,
    /// `false`, an absent field or any other value (CL-15, D-30).
    False,
    /// `store`: store, do not refresh.
    Store,
    /// `http`: no legacy mode for this URL for a time.
    Http,
}

/// Reads one `cache-offline` value: ASCII lower case, then an exact match
/// (CL-15). `None` is an absent field.
#[cfg(feature = "legacy")]
#[must_use]
pub fn parse_cache_offline(value: Option<&str>) -> CacheOffline {
    // Stub (T-02): the strictest answer.
    let _ = value;
    CacheOffline::False
}
