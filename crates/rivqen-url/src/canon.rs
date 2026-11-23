// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Steps 4 to 6 of cache identity section 2 on the binding URL U.
//!
//! Step 4 reads the text as escapes `%XX` (a `%` and two hex digits) and other
//! characters. The escape of an unreserved character (RFC 3986 section 2.3:
//! `A`-`Z`, `a`-`z`, `0`-`9`, `-`, `.`, `_`, `~`) becomes that character, and
//! every other escape gets upper-case hex digits. A `%` that starts no escape
//! becomes `%25`, the escape of the text `%`. No other character changes.
//!
//! So every `%` of the result starts an escape of a byte that is not
//! unreserved, and the result decodes to the same bytes as the input. Two
//! texts get the same result only when they decode to the same bytes; the
//! literal rule ("decode unreserved characters") without the `%25` would give
//! `%2%46` (the text `%2F`) and `%2F` (the byte `/`) one key. The result is
//! its own normalization. A decoded character is never `/`, `?`, `#`, `&` or
//! `=`, so the path segments and the query parameters do not change.
//!
//! Step 5 keeps the query parameters in their order. Step 6 removes a
//! parameter whose name (the text before its first `=`, after step 4) is
//! byte for byte equal to a name in the list. An empty name in the list
//! removes nothing.

/// The canonical URL of the cache key. `binding` is the ASCII text of U;
/// `path_start` and `path_end` are the byte offsets of the path in it.
pub(crate) fn cache_url(
    binding: &str,
    path_start: usize,
    path_end: usize,
    ignored_query_params: &[&str],
) -> String {
    let origin = binding.get(..path_start).unwrap_or_default();
    let path = binding.get(path_start..path_end).unwrap_or_default();
    let query = binding
        .get(path_end..)
        .and_then(|rest| rest.strip_prefix('?'));

    let mut out = String::with_capacity(binding.len());
    out.push_str(origin);
    push_normalized(&mut out, path);
    let Some(query) = query else {
        return out;
    };
    let mut normalized = String::with_capacity(query.len());
    push_normalized(&mut normalized, query);

    let mut kept = String::with_capacity(normalized.len());
    let mut first = true;
    let mut removed = false;
    for param in normalized.split('&') {
        let name = param.split_once('=').map_or(param, |(name, _)| name);
        if ignored_query_params
            .iter()
            .any(|ignored| !ignored.is_empty() && *ignored == name)
        {
            removed = true;
            continue;
        }
        if !first {
            kept.push('&');
        }
        kept.push_str(param);
        first = false;
    }
    // A query whose parameters were all removed leaves no `?`, so the URL
    // gets the key of the same URL without them. An empty query that the
    // URL had (`/p?`) stays.
    if !(removed && first) {
        out.push('?');
        out.push_str(&kept);
    }
    out
}

/// Append `text` with the percent-encoding normalization of step 4.
fn push_normalized(out: &mut String, text: &str) {
    let mut rest = text;
    while let Some((before, escape)) = rest.find('%').and_then(|at| rest.split_at_checked(at)) {
        out.push_str(before);
        // Two hex digits; `from_str_radix` alone also accepts a sign (`+1`).
        let escaped = escape
            .get(1..3)
            .filter(|hex| hex.bytes().all(|b| b.is_ascii_hexdigit()))
            .and_then(|hex| Some((hex, u8::from_str_radix(hex, 16).ok()?)));
        if let Some((hex, value)) = escaped {
            if is_unreserved(value) {
                out.push(char::from(value));
            } else {
                out.push('%');
                out.push_str(&hex.to_ascii_uppercase());
            }
            rest = escape.get(3..).unwrap_or_default();
        } else {
            // A `%` that starts no escape is the text `%`: write its escape,
            // so that it cannot form an escape with the next bytes (`%2%46`
            // would else give the key of `%2F`).
            out.push_str("%25");
            rest = escape.get(1..).unwrap_or_default();
        }
    }
    out.push_str(rest);
}

/// RFC 3986 section 2.3.
const fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

#[cfg(test)]
mod tests {
    use super::push_normalized;

    fn normalized(text: &str) -> String {
        let mut out = String::new();
        push_normalized(&mut out, text);
        out
    }

    #[test]
    fn canon_escapes() {
        assert_eq!(normalized("%41%7e%2D%5f%2e%30"), "A~-_.0");
        assert_eq!(normalized("%c3%a9%2f%3D%26%25"), "%C3%A9%2F%3D%26%25");
        assert_eq!(normalized("%zz%%4%+1%-1x%"), "%25zz%25%254%25+1%25-1x%25");
        assert_eq!(normalized("%%41"), "%25A");
        assert_eq!(normalized("%2541"), "%2541");
        assert_eq!(normalized("a%4"), "a%254");
        assert_eq!(normalized(""), "");
    }

    #[test]
    fn canon_no_new_escape() {
        // The text `%2F` and the byte `/` (`%2F`) keep two keys.
        assert_eq!(normalized("a%2%46b"), "a%252Fb");
        assert_eq!(normalized("a%2Fb"), "a%2Fb");
        // The text `%` and its escape get one key: both decode to `%`.
        assert_eq!(normalized("%"), normalized("%25"));
        for text in ["%4%41", "%a%30", "%%34%41", "%2%46"] {
            let once = normalized(text);
            assert_eq!(normalized(&once), once, "{text}");
        }
    }
}
