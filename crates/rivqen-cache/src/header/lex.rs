// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Lexical rules of RFC 9110 section 5.6 that the field parsers share:
//! tokens, optional white space, quoted strings and delta-seconds (RFC 9111
//! section 1.2.2).
//!
//! Every function reads its input once from left to right and allocates
//! nothing.

/// The value of a `Cache-Control` delta-seconds argument that is larger
/// than 2^31 (RFC 9111 section 1.2.2).
pub(super) const DELTA_SECONDS_CAP: u64 = 1 << 31;

/// `tchar` of RFC 9110 section 5.6.2.
const fn is_tchar(b: u8) -> bool {
    matches!(
        b,
        b'!' | b'#'
            | b'$'
            | b'%'
            | b'&'
            | b'\''
            | b'*'
            | b'+'
            | b'-'
            | b'.'
            | b'^'
            | b'_'
            | b'`'
            | b'|'
            | b'~'
    ) || b.is_ascii_alphanumeric()
}

/// `token = 1*tchar` (RFC 9110 section 5.6.2).
pub(super) fn is_token(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_tchar)
}

/// `s` without the OWS (space and horizontal tab) at both ends (RFC 9110
/// section 5.6.3).
pub(super) fn trim_ows(s: &str) -> &str {
    s.trim_matches([' ', '\t'])
}

/// Reads `1*DIGIT`. Returns the digits without leading zeros (to compare two
/// values exactly, at any length) and the value, which saturates at `cap`.
pub(super) fn delta_seconds(s: &str, cap: u64) -> Option<(&str, u64)> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let value = s.bytes().fold(0_u64, |acc, b| {
        acc.saturating_mul(10)
            .saturating_add(u64::from(b.wrapping_sub(b'0')))
            .min(cap)
    });
    Some((s.trim_start_matches('0'), value))
}

/// A directive argument (RFC 9111 section 5.2: `token / quoted-string`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Arg<'a> {
    /// No `=`.
    None,
    /// A token.
    Token(&'a str),
    /// A quoted-string: the text between the quotes, with its quoted-pairs
    /// as they are (not unescaped).
    Quoted(&'a str),
    /// Text after `=` that is neither a token nor exactly one quoted-string.
    Bad,
}

/// `qdtext` of RFC 9110 section 5.6.4 (bytes 0x80 and over are `obs-text`).
const fn is_qdtext(b: u8) -> bool {
    matches!(b, b'\t' | b' ' | 0x21 | 0x23..=0x5B | 0x5D..=0x7E | 0x80..=0xFF)
}

/// The byte after a backslash in a `quoted-pair`: HTAB, SP, VCHAR or
/// `obs-text`.
const fn is_quoted_pair_byte(b: u8) -> bool {
    matches!(b, b'\t' | b' ' | 0x21..=0x7E | 0x80..=0xFF)
}

/// Reads the text after `=` of a directive. A quoted-string must close at
/// the last byte; anything after the closing quote makes the argument
/// [`Arg::Bad`].
pub(super) fn argument(text: &str) -> Arg<'_> {
    let Some(inner) = text.strip_prefix('"') else {
        return if is_token(text) {
            Arg::Token(text)
        } else {
            Arg::Bad
        };
    };
    let mut pending_escape = false;
    for (i, b) in inner.bytes().enumerate() {
        if pending_escape {
            if !is_quoted_pair_byte(b) {
                return Arg::Bad;
            }
            pending_escape = false;
        } else if b == b'\\' {
            pending_escape = true;
        } else if b == b'"' {
            // The closing quote: it must be the last byte.
            return match inner.split_at_checked(i) {
                Some((content, rest)) if rest.len() == 1 => Arg::Quoted(content),
                _ => Arg::Bad,
            };
        } else if !is_qdtext(b) {
            return Arg::Bad;
        }
    }
    // No closing quote.
    Arg::Bad
}

/// The members of one list field line (RFC 9110 section 5.6.1), split at
/// each comma that is not inside a quoted-string. A quote outside a
/// quoted-string opens one; a backslash inside one escapes the next byte. An
/// unclosed quoted-string runs to the end of the line (its member then fails
/// [`argument`]).
pub(super) struct Members<'a> {
    rest: Option<&'a str>,
}

impl<'a> Members<'a> {
    pub(super) const fn new(line: &'a str) -> Self {
        Self { rest: Some(line) }
    }
}

impl<'a> Iterator for Members<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        let rest = self.rest?;
        let mut quoted = false;
        let mut pending_escape = false;
        for (i, b) in rest.bytes().enumerate() {
            if quoted {
                if pending_escape {
                    pending_escape = false;
                } else if b == b'\\' {
                    pending_escape = true;
                } else if b == b'"' {
                    quoted = false;
                }
            } else if b == b'"' {
                quoted = true;
            } else if b == b',' {
                let (member, tail) = rest.split_at_checked(i)?;
                // `tail` starts with the comma (one byte).
                self.rest = tail.get(1..);
                return Some(member);
            }
        }
        self.rest = None;
        Some(rest)
    }
}
