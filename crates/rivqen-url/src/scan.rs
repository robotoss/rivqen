// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The pre-scan: the checks that run on the input text before the WHATWG
//! parser of the crate `url` sees it.
//!
//! Why a pre-scan (H-156, H-68, DL-225):
//!
//! - With `idna_adapter` 1.0.0 the host parser of `url` reaches a
//!   `debug_assert!` of the adapter for a host label that has an upper-case
//!   ASCII letter before a byte that is not ASCII (`Aé.example`, also in the
//!   percent-encoded form `A%C3%A9.example`). A build with debug assertions
//!   (tests, fuzzing) panics there; a release build gives an IDNA error. So no
//!   host byte over `0x7F`, raw or percent-encoded, may reach that parser.
//! - With `idna_adapter` 1.0.0 the Unicode text of a decoded punycode label
//!   is not checked (`normalize_validate` gives its input back). A label
//!   `xn--…` that decodes to a character that UTS 46 does not allow passes,
//!   and `idna` 1.1.0 then reaches `debug_assert_ne!(c, '\u{200F}')` in its
//!   bidi check (`https://xn--TTgTggTTTggTPTTTPTTThtTTtpTTs`, the fuzz finding
//!   of DL-225). So no host label that starts with `xn--` (ASCII
//!   case-insensitive) may reach that parser either.
//! - The URL `https://:@a.example/` has an empty user name and an empty
//!   password. The WHATWG serialization drops both and the `@`, so the parsed
//!   URL cannot show them. H-68 rejects them, so the at sign is found here.
//!
//! The scan follows the WHATWG URL Standard for a special scheme without a
//! base URL: it removes the leading C0 controls and spaces, ignores every tab
//! and newline, reads the scheme, skips the slashes and backslashes after it,
//! and reads the authority up to the first `/`, `\`, `?` or `#` (the "special
//! authority ignore slashes" and "authority" states). The authority that the
//! scan reads contains the user information, the host and the port that the
//! parser reads, so a rejection here is never too lenient. It can be too
//! strict for a text that the parser rejects anyway (for example `%8z` in a
//! host), which costs nothing.
//!
//! # The punycode rule (DL-225)
//!
//! The scan percent-decodes the authority once, as `url` 2.5.9 decodes the
//! host before it calls `idna` (`Host::parse_cow`, with
//! `percent_encoding::percent_decode`: a `%` and two hex digits give one byte,
//! every other byte stays). It splits the decoded bytes at each `.` and
//! rejects the text when a part starts with `xn--`, ASCII case-insensitive.
//! This is a superset of the labels that `idna` reads as punycode:
//!
//! 1. The host text of `url` is a prefix of the authority of the scan: the
//!    same bytes (tab, LF and CR removed in both), up to the first `:` outside
//!    square brackets. With the at sign rejected first, there is no user
//!    information before it.
//! 2. The decoding of that prefix is a prefix of the decoding of the
//!    authority: an escape needs two hex digits, and `:` is not a hex digit,
//!    so no escape crosses the end of the host. The trailing C0 controls and
//!    spaces that `url` also removes are not hex digits either.
//! 3. `idna` 1.1.0 splits an ASCII domain at the byte `.` only
//!    (`process_innermost`), and reads a label as punycode when its first
//!    four bytes match `xn--` with the case of `x` and `n` ignored
//!    (`has_punycode_prefix`). The other dots of UTS 46 (U+3002, U+FF0E,
//!    U+FF61) are not ASCII, so the wide-byte rule rejects them before this
//!    rule, also in their escaped form (`%E3%80%82`).
//!
//! So every label that starts with `xn--` in the decoded host also starts a
//! part of the decoded authority. A host in square brackets is an IPv6
//! address that `url` does not give to `idna`; a valid one has no `xn--`.
//! The decoding is done once, as in `url`: `%2578n--b` decodes to the text
//! `%78n--b`, which `idna` rejects for its `%`.

use crate::{Scheme, UrlError};

/// The scheme name that is the longest accepted one; `http` is its prefix.
const HTTPS: &[u8] = b"https";

/// The prefix of a punycode label (ASCII case-insensitive).
const PUNYCODE_PREFIX: &[u8] = b"xn--";

/// Check the input text. Gives the scheme for an `http` or `https` URL whose
/// authority has no at sign, no byte over `0x7F` (raw or as a `%XX` escape),
/// and no label that starts with `xn--` after the percent-decoding.
///
/// [`crate::parse`] calls it only for a text of at most
/// [`crate::MAX_URL_BYTES`] bytes, so the two buffers of the authority are
/// within that size.
///
/// # Errors
///
/// - [`UrlError::Syntax`]: the text has no scheme (the WHATWG parser reads it
///   as a relative URL, and [`crate::parse`] has no base URL).
/// - [`UrlError::Scheme`]: the scheme is not `http` or `https`.
/// - [`UrlError::UserInfo`]: the authority has an at sign (H-68).
/// - [`UrlError::Host`]: the authority has a byte over `0x7F` (H-156), or a
///   label that starts with `xn--` (DL-225).
pub(crate) fn check(input: &str) -> Result<Scheme, UrlError> {
    // The WHATWG parser removes leading C0 controls and spaces (bytes 00 to
    // 20) and ignores every tab and newline. A byte of a multi-byte UTF-8
    // character is never in this set.
    let start = input.bytes().position(|b| b > b' ').unwrap_or(input.len());
    let mut bytes = input
        .as_bytes()
        .get(start..)
        .unwrap_or_default()
        .iter()
        .copied()
        .filter(|b| !matches!(b, b'\t' | b'\n' | b'\r'));
    let scheme = scheme(&mut bytes)?;
    authority(bytes)?;
    Ok(scheme)
}

/// True when a part of `domain` between dots starts with `xn--`, ASCII
/// case-insensitive (DL-225). `domain` is a decoded host, or a text that has
/// the decoded host as its prefix.
pub(crate) fn has_punycode_label(domain: &[u8]) -> bool {
    domain.split(|b| *b == b'.').any(|label| {
        label
            .get(..PUNYCODE_PREFIX.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(PUNYCODE_PREFIX))
    })
}

/// Read the scheme and its `:` (WHATWG "scheme start state" and "scheme
/// state").
fn scheme(bytes: &mut impl Iterator<Item = u8>) -> Result<Scheme, UrlError> {
    let mut len = 0_usize;
    // True while the bytes so far are a prefix of `https` (ASCII
    // case-insensitive).
    let mut prefix = true;
    for b in bytes.by_ref() {
        if b == b':' && len > 0 {
            return match (prefix, len) {
                (true, 4) => Ok(Scheme::Http),
                (true, 5) => Ok(Scheme::Https),
                _ => Err(UrlError::Scheme),
            };
        }
        let valid = if len == 0 {
            b.is_ascii_alphabetic()
        } else {
            b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.')
        };
        if !valid {
            return Err(UrlError::Syntax);
        }
        prefix = prefix && HTTPS.get(len) == Some(&b.to_ascii_lowercase());
        len = len.saturating_add(1);
    }
    Err(UrlError::Syntax)
}

/// Read the authority after the scheme of a special URL.
fn authority(bytes: impl Iterator<Item = u8>) -> Result<(), UrlError> {
    let text: Vec<u8> = bytes
        .skip_while(|b| matches!(b, b'/' | b'\\'))
        .take_while(|b| !matches!(b, b'/' | b'\\' | b'?' | b'#'))
        .collect();
    if text.contains(&b'@') {
        Err(UrlError::UserInfo)
    } else if has_wide_byte(&text) || has_punycode_label(&percent_decode(&text)) {
        Err(UrlError::Host)
    } else {
        Ok(())
    }
}

/// True when `text` has a raw byte over 0x7F, or the first hex digit of an
/// escape of a byte over 0x7F (`%80` to `%FF`). The second digit is not
/// checked: `%8z` stays in the host as text, and the parser rejects `%` in a
/// host.
fn has_wide_byte(text: &[u8]) -> bool {
    let mut after_percent = false;
    text.iter().any(|&b| {
        let wide =
            b >= 0x80 || (after_percent && matches!(b, b'8' | b'9' | b'a'..=b'f' | b'A'..=b'F'));
        after_percent = b == b'%';
        wide
    })
}

/// Percent-decode `text` once, as `percent_encoding::percent_decode` does
/// for `url`: a `%` and two hex digits give one byte; every other byte,
/// also a `%` that starts no escape, stays.
fn percent_decode(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut rest = text;
    while let Some((&b, tail)) = rest.split_first() {
        if b == b'%'
            && let [high, low, after @ ..] = tail
            && let (Some(high), Some(low)) = (hex_value(*high), hex_value(*low))
        {
            out.push((high << 4) | low);
            rest = after;
            continue;
        }
        out.push(b);
        rest = tail;
    }
    out
}

/// The value of one ASCII hex digit.
fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => b.checked_sub(b'0'),
        b'a'..=b'f' => b.checked_sub(b'a' - 10),
        b'A'..=b'F' => b.checked_sub(b'A' - 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{check, has_punycode_label, percent_decode};
    use crate::{Scheme, UrlError};

    #[test]
    fn scan_scheme_forms() {
        assert_eq!(check("http://a"), Ok(Scheme::Http));
        assert_eq!(check("HTTPS://a"), Ok(Scheme::Https));
        assert_eq!(check(" \u{1}\th\tT\ntp\rs:a"), Ok(Scheme::Https));
        assert_eq!(check("https:"), Ok(Scheme::Https));
        for other in [
            "ftp://a",
            "httpss://a",
            "htt://a",
            "h://a",
            "ws://a",
            "hxxps://a",
        ] {
            assert_eq!(check(other), Err(UrlError::Scheme), "{other}");
        }
        for relative in [
            "",
            "   ",
            ":",
            "://a",
            "1http://a",
            "ht_tp://a",
            "https",
            "//a/b",
            "é:",
        ] {
            assert_eq!(check(relative), Err(UrlError::Syntax), "{relative:?}");
        }
        assert_eq!(check("a+b-c.d:x"), Err(UrlError::Scheme));
    }

    #[test]
    fn scan_authority_bounds() {
        // The authority ends at each of these bytes; what follows is not read.
        for tail in [
            "/@é", "\\@é", "?@é", "#@é", "/xn--a", "\\xn--a", "?xn--a", "#xn--a",
        ] {
            assert_eq!(
                check(&format!("https://a.example{tail}")),
                Ok(Scheme::Https)
            );
        }
        // Slashes and backslashes before the authority are skipped.
        assert_eq!(check("https:\\/\\a@b"), Err(UrlError::UserInfo));
        assert_eq!(check("https:a@b"), Err(UrlError::UserInfo));
        assert_eq!(check("https:\\/\\xn--a"), Err(UrlError::Host));
        assert_eq!(check("https:xn--a"), Err(UrlError::Host));
        // Tabs and newlines do not end the authority.
        assert_eq!(check("https://a\t@b"), Err(UrlError::UserInfo));
        assert_eq!(check("https://a\n%\tC3/"), Err(UrlError::Host));
        assert_eq!(check("https://x\tn\n-\r-a/"), Err(UrlError::Host));
    }

    #[test]
    fn scan_wide_bytes() {
        assert_eq!(check("https://bücher.example/"), Err(UrlError::Host));
        for escape in ["%80", "%9f", "%a0", "%C3", "%fF", "%8z", "%%e9"] {
            assert_eq!(
                check(&format!("https://a{escape}.example/")),
                Err(UrlError::Host),
                "{escape}"
            );
        }
        for escape in ["%41", "%2E", "%7f", "%", "%g0", "%0é"] {
            let text = format!("https://a{escape}.example/");
            let expected = if escape.contains('é') {
                Err(UrlError::Host)
            } else {
                Ok(Scheme::Https)
            };
            assert_eq!(check(&text), expected, "{escape}");
        }
        // A wide byte after a `%` that is not the first hex digit.
        assert_eq!(check("https://a%0%9z/"), Err(UrlError::Host));
        // The at sign wins over a wide byte and over a punycode label.
        assert_eq!(check("https://é@a/"), Err(UrlError::UserInfo));
        assert_eq!(check("https://xn--a@b/"), Err(UrlError::UserInfo));
    }

    #[test]
    fn scan_punycode_labels() {
        for host in [
            "xn--a",
            "XN--a",
            "Xn--a",
            "xN--a",
            "xn--",
            "a.xn--b",
            "a.b.xn--c.d",
            "xn--a:443",
            "a.example.xn--b.",
            // Percent-encoded forms: decoded before the split, as in `url`.
            "%78n--b",
            "x%6E--b",
            "x%6e--b",
            "xn%2D-b",
            "xn-%2db",
            "%58%4E%2D%2Db",
            "a%2Exn--b",
            "a%2exn--b",
            "a%2E%78%6E%2D%2Db",
        ] {
            assert_eq!(
                check(&format!("https://{host}/")),
                Err(UrlError::Host),
                "{host}"
            );
        }
        for host in [
            "xn-a",
            "xn.example",
            "axn--b",
            "ab--c",
            "a-xn--b",
            "x-n--b",
            "xn_-b",
            "xm--b",
            "yn--b",
            "xo--b",
            "x.n--b",
            // One decoding only: the text `%78n--b`, which the parser
            // rejects for its `%`.
            "%2578n--b",
            "x%n--b",
            "a.b:8080",
        ] {
            assert_eq!(
                check(&format!("https://{host}/")),
                Ok(Scheme::Https),
                "{host}"
            );
        }
    }

    #[test]
    fn scan_punycode_label_split() {
        assert!(has_punycode_label(b"xn--"));
        assert!(has_punycode_label(b"a.XN--"));
        assert!(has_punycode_label(b"a..xN--b."));
        assert!(!has_punycode_label(b""));
        assert!(!has_punycode_label(b"xn-"));
        assert!(!has_punycode_label(b"a.xn-.b"));
        assert!(!has_punycode_label(b"axn--"));
        assert!(!has_punycode_label(b"[::1]"));
    }

    #[test]
    fn scan_percent_decode_like_url() {
        assert_eq!(percent_decode(b"%41%2e%2E%7a%7A%30%39"), b"A..zz09");
        assert_eq!(percent_decode(b"%af%AF%Fa"), [0xAF, 0xAF, 0xFA]);
        assert_eq!(percent_decode(b"%"), b"%");
        assert_eq!(percent_decode(b"%4"), b"%4");
        assert_eq!(percent_decode(b"%4g%g4%%41"), b"%4g%g4%A");
        assert_eq!(percent_decode(b"%2541"), b"%41");
        assert_eq!(percent_decode(b"a%"), b"a%");
        assert_eq!(percent_decode(b""), b"");
        // Each byte value, with lower-case and upper-case hex digits.
        for value in 0_u8..=255 {
            for text in [format!("%{value:02x}"), format!("%{value:02X}")] {
                assert_eq!(percent_decode(text.as_bytes()), [value], "{text}");
            }
        }
    }
}
