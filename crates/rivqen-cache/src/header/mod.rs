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
//!
//! Work bounds (R-CORE-09): each parser reads each byte of the field values
//! a fixed number of times, never goes back, and has no recursion. Only
//! [`parse_vary`] allocates: one string for each name, so at most the length
//! of the `Vary` values, and one sort of the names. The adapter limits the
//! response head before the core reads it (`maxHeaderBytes`).

mod cache_control;
mod date;
mod lex;

use rivqen_model::http::FieldLine;
use rivqen_model::time::UnixTime;

use lex::{delta_seconds, is_token, trim_ows};

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
///
/// - Directives split at commas outside quoted strings; names match ASCII
///   case-insensitively; an argument is a token or a quoted-string (RFC 9111
///   section 5.2). Unknown directives and `s-maxage` are ignored.
/// - `no-store` counts with an argument and wherever the split of DL-088
///   (at every `,` and `;`, quotes not parsed) finds it. `no-cache` and
///   `private` count with and without an argument; `must-revalidate` also
///   with an argument; `public` only without one.
/// - `max-age`, `stale-while-revalidate`, `stale-if-error`: delta-seconds
///   as a token or a quoted-string without a quoted-pair; over 2^31 is 2^31.
///   No value, a value that is not digits, or two different values of one
///   directive: no number and `invalid_freshness`.
/// - A member that does not match the grammar sets `invalid_freshness`,
///   clears `public`, and its line also counts `no-cache`, `private` and
///   `must-revalidate` by the DL-088 split.
#[must_use]
pub fn parse_cache_control(fields: &[FieldLine]) -> CacheControl {
    cache_control::parse(fields)
}

/// Reads an HTTP-date (RFC 9110 section 5.6.7: IMF-fixdate and the two
/// obsolete forms). `None` when the text is not a valid date.
///
/// Letters match ASCII case-insensitively (RFC 9111 section 4.2); the zone
/// is `GMT`; the grammar is exact apart from the OWS at both ends. The day
/// name is checked against the list of names, not against the date; the
/// day must exist in the month. `23:59:60` gives `23:59:59`. A date before
/// 1970 gives `None`. A two-digit year of the RFC 850 form is the latest
/// year with those digits that is not more than 50 years after
/// 2026-01-01T00:00:00Z (the function has no current time).
#[must_use]
pub fn parse_http_date(value: &str) -> Option<UnixTime> {
    date::parse_with_reference(value, date::RFC850_REFERENCE)
}

/// Reads the `Age` field: `None` when it is absent or not valid.
///
/// The value is the first non-empty member of the field lines in order
/// (RFC 9111 section 5.1); it must be delta-seconds. A value too large for
/// `u64` is `u64::MAX` (RFC 9111 section 1.2.2: a larger age is the stricter
/// result).
#[must_use]
pub fn parse_age(fields: &[FieldLine]) -> Option<u64> {
    let first = fields
        .iter()
        .filter(|line| line.is("age"))
        .flat_map(|line| line.value.split(','))
        .map(trim_ows)
        .find(|member| !member.is_empty())?;
    delta_seconds(first, u64::MAX).map(|(_, seconds)| seconds)
}

/// Reads all `Vary` field lines of `fields`.
///
/// A member `*` gives [`Vary::Any`]. A member that is not a field name (a
/// token, RFC 9110 section 5.1) also gives [`Vary::Any`]: the cache cannot
/// tell which request field it names, so it does not store.
#[must_use]
pub fn parse_vary(fields: &[FieldLine]) -> Vary {
    let mut names = Vec::new();
    for line in fields.iter().filter(|line| line.is("vary")) {
        for member in line.value.split(',').map(trim_ows) {
            if member.is_empty() {
                continue;
            }
            if member == "*" || !is_token(member) {
                return Vary::Any;
            }
            names.push(member.to_ascii_lowercase());
        }
    }
    if names.is_empty() {
        return Vary::None;
    }
    names.sort_unstable();
    names.dedup();
    Vary::Names(names)
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
///
/// The value is not trimmed: the field line value is already without the
/// white space at its ends ([`FieldLine`]).
#[cfg(feature = "legacy")]
#[must_use]
pub fn parse_cache_offline(value: Option<&str>) -> CacheOffline {
    // `eq_ignore_ascii_case` is "ASCII lower case, then an exact match".
    match value {
        Some(text) if text.eq_ignore_ascii_case("true") => CacheOffline::True,
        Some(text) if text.eq_ignore_ascii_case("store") => CacheOffline::Store,
        Some(text) if text.eq_ignore_ascii_case("http") => CacheOffline::Http,
        Some(_) | None => CacheOffline::False,
    }
}
