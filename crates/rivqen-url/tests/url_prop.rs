// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Properties of `rivqen-url` (WP-08 S1 T-04): no panic for any text; a
//! result is ASCII and is its own parse; the origin and the path are parts of
//! U; the canonical URL of the cache key is a fixed point; user information
//! and a host byte over 0x7F are always rejected.
//!
//! Fixed seeds; `PROPTEST_CASES` raises the case count.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use rivqen_url::{ParsedUrl, UrlError, parse};

fn config(cases: u32, seed: u64) -> ProptestConfig {
    ProptestConfig {
        cases: std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(cases),
        rng_seed: RngSeed::Fixed(seed),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

/// Pieces that make the parser take its edge paths.
const PIECES: &[&str] = &[
    "http",
    "https",
    "HTTPS",
    "hTtP",
    "ftp",
    "ws",
    ":",
    "//",
    "/",
    "\\",
    "@",
    "?",
    "#",
    "&",
    "=",
    "%",
    "%4",
    "%41",
    "%7e",
    "%2e",
    "%2E",
    "%c3%a9",
    "%C3",
    "%80",
    "%25",
    "%00",
    "%2F",
    ".",
    "..",
    "a",
    "A",
    "z",
    "0",
    "9",
    "-",
    "_",
    "~",
    "é",
    "ß",
    "ａ",
    "。",
    "\u{1F600}",
    "xn--",
    "XN--",
    "fa-hia",
    "bcher-kva",
    "example",
    "[",
    "]",
    "::1",
    "2001:db8::",
    "127.1",
    "0x7f",
    "4294967295",
    " ",
    "\t",
    "\n",
    "\r",
    "\u{0}",
    "\u{1f}",
    "\u{7f}",
    "\u{a0}",
    "443",
    "80",
    ":0",
    "'",
    "\"",
    "<",
    ">",
    "{",
    "}",
    "|",
    "^",
    "`",
    "+",
];

fn pieces() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(PIECES), 0..40).prop_map(|parts| parts.concat())
}

/// A text that starts like an `http` or `https` URL.
fn url_like() -> impl Strategy<Value = String> {
    (
        prop::sample::select(
            &[
                "http://",
                "https://",
                "HTTPS://",
                "https:",
                "http:\\\\",
                " https://",
            ][..],
        ),
        pieces(),
    )
        .prop_map(|(head, tail)| format!("{head}{tail}"))
}

fn label() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9][a-zA-Z0-9_-]{0,8}"
}

/// A host the parser accepts: a name, an IPv4 form or an IPv6 literal.
fn host() -> impl Strategy<Value = String> {
    prop_oneof![
        // The last label is not a number (WHATWG reads such a host as IPv4).
        (
            prop::collection::vec(label(), 0..3),
            prop::sample::select(&["example", "EXAMPLE", "test", "localhost"][..]),
        )
            .prop_map(|(labels, last)| {
                let mut all = labels;
                all.push(last.to_owned());
                all.join(".")
            }),
        (0_u8..=255, 0_u8..=255).prop_map(|(a, b)| format!("10.{a}.{b}.1")),
        (0_u16..=0xffff).prop_map(|g| format!("[2001:DB8::{g:x}]")),
        Just("xn--fa-hia.example".to_owned()),
    ]
}

fn segment() -> impl Strategy<Value = String> {
    "([a-zA-Z0-9._~!$'()*+,;=:@ \"<>`{}|^\\[\\]-]|%[0-9a-fA-F]{2}|%|é|\\.\\.|\\.){0,6}"
}

/// A query parameter with a name of a small set, so that names repeat.
fn param() -> impl Strategy<Value = (String, Option<String>)> {
    (
        prop::sample::select(&["a", "b", "utm_source", "utm%5Fsource", "A", ""][..])
            .prop_map(str::to_owned),
        prop::option::of("([a-zA-Z0-9._~ '%=]|%[0-9a-fA-F]{2}){0,5}"),
    )
}

#[derive(Debug, Clone)]
struct Structured {
    text: String,
    params: Vec<(String, Option<String>)>,
}

fn structured() -> impl Strategy<Value = Structured> {
    (
        prop::sample::select(&["http", "https", "HTTP", "HtTpS"][..]),
        host(),
        prop::option::of(prop::sample::select(&["80", "443", "8080", "0", ""][..])),
        prop::collection::vec(segment(), 0..5),
        prop::option::of(prop::collection::vec(param(), 0..6)),
        prop::option::of("[a-z #%]{0,5}"),
    )
        .prop_map(|(scheme, host, port, segments, query, fragment)| {
            let mut text = format!("{scheme}://{host}");
            if let Some(port) = port {
                text.push(':');
                text.push_str(port);
            }
            for segment in &segments {
                text.push('/');
                text.push_str(segment);
            }
            let params = query.clone().unwrap_or_default();
            if let Some(query) = query {
                text.push('?');
                let joined: Vec<String> = query
                    .iter()
                    .map(|(name, value)| match value {
                        Some(value) => format!("{name}={value}"),
                        None => name.clone(),
                    })
                    .collect();
                text.push_str(&joined.join("&"));
            }
            if let Some(fragment) = fragment {
                text.push('#');
                text.push_str(&fragment);
            }
            Structured { text, params }
        })
}

/// What every result must hold.
fn check_result(input: &str, parsed: &ParsedUrl) -> Result<(), TestCaseError> {
    let binding = parsed.binding_url();
    prop_assert!(binding.is_ascii(), "{input:?}");
    prop_assert!(!binding.contains('#'), "{input:?}");
    prop_assert!(!binding.contains(['\t', '\n', '\r', ' ']), "{input:?}");
    prop_assert!(!parsed.origin().contains('@'), "{input:?}");
    prop_assert!(binding.starts_with(parsed.origin()), "{input:?}");
    let rest = &binding[parsed.origin().len()..];
    prop_assert!(
        rest.starts_with('/') && rest.starts_with(parsed.path()),
        "{input:?}"
    );
    let after_path = &rest[parsed.path().len()..];
    prop_assert!(
        after_path.is_empty() || after_path.starts_with('?'),
        "{input:?}"
    );
    prop_assert!(!parsed.path().contains('?'), "{input:?}");
    // Idempotence: U is its own parse. (The generated texts are short, so U
    // is within `MAX_URL_BYTES`; `url_errors_result_over_the_limit` has the
    // other case.)
    let reparsed = parse(binding);
    prop_assert_eq!(reparsed.as_ref(), Ok(parsed), "{:?}", input);
    // The origin of the WHATWG parser of the crate `url`.
    let reference = url::Url::parse(input).unwrap();
    prop_assert_eq!(parsed.origin(), reference.origin().ascii_serialization());
    // The canonical URL is ASCII, at most 3 times U (a `%` becomes `%25`),
    // decodes to the bytes of U, and is a fixed point.
    let cache = parsed.cache_url(&[]);
    prop_assert!(cache.is_ascii() && cache.len() <= 3 * binding.len());
    prop_assert_eq!(decode(&cache), decode(binding));
    let again = parse(&cache).unwrap();
    prop_assert_eq!(again.binding_url(), cache.as_str());
    prop_assert_eq!(again.cache_url(&[]), cache);
    Ok(())
}

/// Percent-decoding as a server does it: a `%` and two hex digits give one
/// byte; any other `%` is the byte `%`.
fn decode(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()));
        match (bytes[i], hex) {
            (b'%', Some(h)) => {
                out.push(u8::from_str_radix(h, 16).unwrap());
                i += 3;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    out
}

proptest! {
    #![proptest_config(config(2048, 0x5eed_0001))]

    #[test]
    fn url_prop_no_panic_any_text(input in any::<String>()) {
        if let Ok(parsed) = parse(&input) {
            check_result(&input, &parsed)?;
        }
    }

    #[test]
    fn url_prop_no_panic_pieces(input in pieces()) {
        if let Ok(parsed) = parse(&input) {
            check_result(&input, &parsed)?;
        }
    }

    #[test]
    fn url_prop_url_like_result_is_ascii_and_idempotent(input in url_like()) {
        if let Ok(parsed) = parse(&input) {
            check_result(&input, &parsed)?;
        }
    }
}

proptest! {
    #![proptest_config(config(1024, 0x5eed_0002))]

    #[test]
    fn url_prop_structured_is_accepted(url in structured()) {
        let parsed = parse(&url.text);
        prop_assert!(parsed.is_ok(), "{:?}: {:?}", url.text, parsed);
        check_result(&url.text, &parsed.unwrap())?;
    }

    #[test]
    fn url_prop_cache_removes_only_listed_names(
        url in structured(),
        ignored in prop::collection::vec(prop::sample::select(&["a", "utm_source", "A", ""][..]), 0..3),
    ) {
        let parsed = parse(&url.text).unwrap();
        let cache = parsed.cache_url(&ignored);
        let plain = parsed.cache_url(&[]);
        // The text before the query does not change.
        let head = |text: &str| text.split_once('?').map_or(text.to_owned(), |(head, _)| head.to_owned());
        prop_assert_eq!(head(&cache), head(&plain));
        let names = |text: &str| -> Option<Vec<String>> {
            text.split_once('?').map(|(_, query)| {
                query.split('&').map(|p| p.split_once('=').map_or(p, |(n, _)| n).to_owned()).collect()
            })
        };
        let expected: Option<Vec<String>> = names(&plain).and_then(|all| {
            let kept: Vec<String> = all
                .iter()
                .filter(|n| !ignored.iter().any(|i| !i.is_empty() && *i == n.as_str()))
                .cloned()
                .collect();
            if kept.is_empty() && kept.len() < all.len() { None } else { Some(kept) }
        });
        prop_assert_eq!(names(&cache), expected);
        // Every generated parameter is in the query of U, in order.
        if !url.params.is_empty() {
            prop_assert_eq!(names(parsed.binding_url()).map(|n| n.len()), Some(url.params.len()));
        }
        // Removal is idempotent.
        prop_assert_eq!(parse(&cache).unwrap().cache_url(&ignored), cache);
    }

    /// No collision: one key means the same decoded bytes. (The converse is
    /// not wanted: `/` and `%2F` decode to one byte and keep two keys.)
    #[test]
    fn url_prop_cache_key_equal_only_for_equal_bytes(
        a in "([a/%2F4AJ]|%[24][0-9a-fA-F]){0,8}",
        b in "([a/%2F4AJ]|%[24][0-9a-fA-F]){0,8}",
    ) {
        let first = parse(&format!("https://k.example/{a}?{a}")).unwrap();
        let second = parse(&format!("https://k.example/{b}?{b}")).unwrap();
        if first.cache_url(&[]) == second.cache_url(&[]) {
            prop_assert_eq!(decode(first.binding_url()), decode(second.binding_url()), "{:?} {:?}", a, b);
        }
    }

    #[test]
    fn url_prop_user_info_is_rejected(
        url in structured(),
        userinfo in prop::sample::select(&["@", ":@", "u@", "u:p@", "%40@", "é@", "\t@"][..]),
    ) {
        let (scheme, rest) = url.text.split_once("://").unwrap();
        let text = format!("{scheme}://{userinfo}{rest}");
        prop_assert_eq!(parse(&text), Err(UrlError::UserInfo), "{:?}", text);
    }

    #[test]
    fn url_prop_wide_host_is_rejected(
        url in structured(),
        wide in prop::sample::select(&["é", "ß", "ａ", "\u{a0}", "%C3%A9", "%c3%a9", "%80", "%fF", "A%C3%A9", "Aé"][..]),
        at_end in any::<bool>(),
    ) {
        let (scheme, rest) = url.text.split_once("://").unwrap();
        let host_end = if rest.starts_with('[') {
            rest.find(']').unwrap() + 1
        } else {
            rest.find(['/', '?', '#', ':']).unwrap_or(rest.len())
        };
        let (host, tail) = rest.split_at(host_end);
        let text = if at_end {
            format!("{scheme}://{host}{wide}{tail}")
        } else {
            format!("{scheme}://{wide}{host}{tail}")
        };
        prop_assert_eq!(parse(&text), Err(UrlError::Host), "{:?}", text);
    }
}
