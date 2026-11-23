// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The pre-scan: the checks that run on the input text before the WHATWG
//! parser of the crate `url` sees it.
//!
//! Why a pre-scan (H-156, H-68):
//!
//! - With `idna_adapter` 1.0.0 the host parser of `url` reaches a
//!   `debug_assert!` of the adapter for a host label that has an upper-case
//!   ASCII letter before a byte that is not ASCII (`Aé.example`, also in the
//!   percent-encoded form `A%C3%A9.example`). A build with debug assertions
//!   (tests, fuzzing) panics there; a release build gives an IDNA error. So no
//!   host byte over `0x7F`, raw or percent-encoded, may reach that parser.
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

use crate::{Scheme, UrlError};

/// The scheme name that is the longest accepted one; `http` is its prefix.
const HTTPS: &[u8] = b"https";

/// Check the input text. Gives the scheme for an `http` or `https` URL whose
/// authority has no at sign and no byte over `0x7F` (raw or as a `%XX`
/// escape).
///
/// # Errors
///
/// - [`UrlError::Syntax`]: the text has no scheme (the WHATWG parser reads it
///   as a relative URL, and [`crate::parse`] has no base URL).
/// - [`UrlError::Scheme`]: the scheme is not `http` or `https`.
/// - [`UrlError::UserInfo`]: the authority has an at sign (H-68).
/// - [`UrlError::Host`]: the authority has a byte over `0x7F` (H-156).
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
    let mut at_sign = false;
    let mut wide = false;
    let mut after_percent = false;
    for b in bytes.skip_while(|b| matches!(b, b'/' | b'\\')) {
        match b {
            b'/' | b'\\' | b'?' | b'#' => break,
            b'@' => at_sign = true,
            _ => {}
        }
        // A raw byte over 0x7F, or the first hex digit of an escape of a byte
        // over 0x7F (`%80` to `%FF`). The second digit is not checked: `%8z`
        // stays in the host as text, and the parser rejects `%` in a host.
        if b >= 0x80 || (after_percent && matches!(b, b'8' | b'9' | b'a'..=b'f' | b'A'..=b'F')) {
            wide = true;
        }
        after_percent = b == b'%';
    }
    if at_sign {
        Err(UrlError::UserInfo)
    } else if wide {
        Err(UrlError::Host)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::check;
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
        for tail in ["/@é", "\\@é", "?@é", "#@é"] {
            assert_eq!(
                check(&format!("https://a.example{tail}")),
                Ok(Scheme::Https)
            );
        }
        // Slashes and backslashes before the authority are skipped.
        assert_eq!(check("https:\\/\\a@b"), Err(UrlError::UserInfo));
        assert_eq!(check("https:a@b"), Err(UrlError::UserInfo));
        // Tabs and newlines do not end the authority.
        assert_eq!(check("https://a\t@b"), Err(UrlError::UserInfo));
        assert_eq!(check("https://a\n%\tC3/"), Err(UrlError::Host));
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
        // The at sign wins over a wide byte.
        assert_eq!(check("https://é@a/"), Err(UrlError::UserInfo));
    }
}
