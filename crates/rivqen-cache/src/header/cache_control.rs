// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! `Cache-Control` (RFC 9111 section 5.2; RFC 5861 sections 3 and 4).
//!
//! Each field line is read twice, both times from left to right without
//! allocation:
//!
//! 1. The directive list of RFC 9111 section 5.2 (`#cache-directive`), split
//!    at commas outside quoted strings. A member that does not match the
//!    grammar is a syntax error: the response gets `invalid_freshness` and
//!    loses `public`.
//! 2. The split of DL-088 (items 3 and 5 of legacy wire section 10): at every
//!    `,` and `;`, quotes not parsed; a piece counts by its name before any
//!    `=`, without `"` at its ends, ASCII case-insensitive. The split also
//!    cuts at SP and HTAB, so directives that a misconfigured server
//!    separates with spaces (`no-cache no-store`) are found (stricter than
//!    item 5). `no-store` always counts this way, so `private="a, no-store"`
//!    and `foo="a no-store"` are `no-store`. That follows the text of item 5;
//!    the two reference models of the legacy fixtures differ for this input
//!    (they split only at commas and keep the quote in the name; sprint
//!    record WP-07 S1, section 5). On a line with a syntax error,
//!    `no-cache`, `private` and `must-revalidate` also count this way: the
//!    broken member can hide them.
//!
//! A flag that restricts the cache may come from either reading. A flag that
//! permits more (`public`, and `valid_private`: the consent of H-150) comes
//! only from the grammar of step 1, and only when no member of any line has
//! a syntax error. So `private` and `valid_private` differ: `private=`,
//! `no-transform private` and an unclosed quote that holds `private` set
//! `private` (restrictive) but not `valid_private`.

use rivqen_model::http::FieldLine;

use super::CacheControl;
use super::lex::{Arg, DELTA_SECONDS_CAP, Members, argument, delta_seconds, is_token, trim_ows};

/// The field name.
const NAME: &str = "cache-control";

/// The state of one delta-seconds directive over all field lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Slot<'a> {
    #[default]
    Absent,
    /// One value so far: its digits without leading zeros, and the capped
    /// number.
    Value { digits: &'a str, seconds: u64 },
    /// Not valid, or two different values (RFC 9111 section 4.2.1).
    Invalid,
}

impl<'a> Slot<'a> {
    /// Adds one occurrence; `None` is an occurrence that is not valid.
    fn add(self, value: Option<(&'a str, u64)>) -> Self {
        match (self, value) {
            (Self::Invalid, _) | (_, None) => Self::Invalid,
            (Self::Absent, Some((digits, seconds))) => Self::Value { digits, seconds },
            (Self::Value { digits, seconds }, Some((other, _))) => {
                if digits == other {
                    Self::Value { digits, seconds }
                } else {
                    Self::Invalid
                }
            }
        }
    }

    const fn seconds(self) -> Option<u64> {
        match self {
            Self::Value { seconds, .. } => Some(seconds),
            Self::Absent | Self::Invalid => None,
        }
    }

    const fn is_invalid(self) -> bool {
        matches!(self, Self::Invalid)
    }
}

/// The delta-seconds value of an argument: a token or a quoted-string of
/// digits (RFC 9111 section 5.2: a recipient accepts both forms). The
/// content of the quoted-string is not unescaped, so a quoted-pair (its
/// backslash is not a digit) makes the value not valid (stricter than RFC
/// 9110 section 5.6.4).
fn delta_argument(arg: Arg<'_>) -> Option<(&str, u64)> {
    match arg {
        Arg::Token(text) | Arg::Quoted(text) => delta_seconds(text, DELTA_SECONDS_CAP),
        Arg::None | Arg::Bad => None,
    }
}

/// The restrictive directives that the DL-088 split of one line names.
#[derive(Debug, Default)]
#[allow(clippy::struct_excessive_bools)] // one flag per directive
struct Named {
    no_store: bool,
    no_cache: bool,
    private: bool,
    must_revalidate: bool,
}

/// The DL-088 split of one field line, also at SP and HTAB. The name of a
/// piece is the text before any `=` without `"` at its ends, so that a name
/// at the edge of a quoted string counts (`private="a, no-store"`, DL-088
/// item 5), and so do directives separated only by white space
/// (`private no-store`). It finds every name that the comma split of the
/// legacy reference models finds, and more.
fn dl088_names(value: &str) -> Named {
    let mut named = Named::default();
    for piece in value.split([',', ';', ' ', '\t']) {
        let name = piece
            .split_once('=')
            .map_or(piece, |(name, _)| name)
            .trim_matches('"');
        if name.eq_ignore_ascii_case("no-store") {
            named.no_store = true;
        } else if name.eq_ignore_ascii_case("no-cache") {
            named.no_cache = true;
        } else if name.eq_ignore_ascii_case("private") {
            named.private = true;
        } else if name.eq_ignore_ascii_case("must-revalidate") {
            named.must_revalidate = true;
        }
    }
    named
}

#[derive(Debug, Default)]
#[allow(clippy::struct_excessive_bools)] // one flag per directive
struct Reader<'a> {
    no_store: bool,
    no_cache: bool,
    private: bool,
    /// The grammar read a `private` member (the split of DL-088 never sets
    /// this flag).
    grammar_private: bool,
    public: bool,
    must_revalidate: bool,
    /// Some member of some line does not match the grammar.
    syntax_error: bool,
    max_age: Slot<'a>,
    stale_while_revalidate: Slot<'a>,
    stale_if_error: Slot<'a>,
}

impl<'a> Reader<'a> {
    fn line(&mut self, value: &'a str) {
        let mut line_error = false;
        for member in Members::new(value) {
            if !self.member(member) {
                line_error = true;
            }
        }
        let named = dl088_names(value);
        self.no_store |= named.no_store;
        if line_error {
            self.syntax_error = true;
            self.no_cache |= named.no_cache;
            self.private |= named.private;
            self.must_revalidate |= named.must_revalidate;
        }
    }

    /// Reads one member. False when it does not match the grammar of RFC
    /// 9111 section 5.2 (`token [ "=" ( token / quoted-string ) ]`).
    fn member(&mut self, raw: &'a str) -> bool {
        let member = trim_ows(raw);
        if member.is_empty() {
            // An empty list element (RFC 9110 section 5.6.1.2).
            return true;
        }
        let (name, arg) = match member.split_once('=') {
            Some((name, text)) => (name, argument(text)),
            None => (member, Arg::None),
        };
        if !is_token(name) {
            return false;
        }
        if name.eq_ignore_ascii_case("no-cache") {
            // With or without field names: the qualified form counts as the
            // unqualified one (stricter; RFC 9111 section 5.2.2.4, note).
            self.no_cache = true;
        } else if name.eq_ignore_ascii_case("private") {
            // Also the qualified form (RFC 9111 section 5.2.2.7, note).
            self.private = true;
            // Only the grammar sets the permission. An argument that is not
            // valid is a syntax error, which clears it in `finish`.
            self.grammar_private = true;
        } else if name.eq_ignore_ascii_case("must-revalidate") {
            // An argument is not allowed; the directive still counts.
            self.must_revalidate = true;
        } else if name.eq_ignore_ascii_case("public") {
            // An argument is not allowed: then `public` does not count.
            if arg == Arg::None {
                self.public = true;
            }
        } else if name.eq_ignore_ascii_case("max-age") {
            self.max_age = self.max_age.add(delta_argument(arg));
        } else if name.eq_ignore_ascii_case("stale-while-revalidate") {
            self.stale_while_revalidate = self.stale_while_revalidate.add(delta_argument(arg));
        } else if name.eq_ignore_ascii_case("stale-if-error") {
            self.stale_if_error = self.stale_if_error.add(delta_argument(arg));
        }
        // Not listed: `no-store` (read by the DL-088 split), `s-maxage` (a
        // private cache ignores it) and every unknown directive (RFC 9111
        // section 5.2.3).
        arg != Arg::Bad
    }

    fn finish(self) -> CacheControl {
        let invalid_slot = self.max_age.is_invalid()
            || self.stale_while_revalidate.is_invalid()
            || self.stale_if_error.is_invalid();
        CacheControl {
            no_store: self.no_store,
            no_cache: self.no_cache,
            private: self.private,
            // The permissive flags: no syntax error anywhere (H-150).
            valid_private: self.grammar_private && !self.syntax_error,
            public: self.public && !self.syntax_error,
            must_revalidate: self.must_revalidate,
            max_age: self.max_age.seconds(),
            stale_while_revalidate: self.stale_while_revalidate.seconds(),
            stale_if_error: self.stale_if_error.seconds(),
            invalid_freshness: self.syntax_error || invalid_slot,
        }
    }
}

/// Reads every `Cache-Control` field line of `fields`.
pub(super) fn parse(fields: &[FieldLine]) -> CacheControl {
    let mut reader = Reader::default();
    for line in fields.iter().filter(|line| line.is(NAME)) {
        reader.line(&line.value);
    }
    reader.finish()
}
