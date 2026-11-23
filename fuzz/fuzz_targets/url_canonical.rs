// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! URL parser `rivqen_url::parse` (WP-08 S1, T-04; H-156, DL-218).
//!
//! The input is the URL text (UTF-8; other bytes are skipped). Invariants
//! beyond "no panic" (also with debug assertions, which `cargo fuzz` turns
//! on: `idna_adapter` 1.0.0 has a `debug_assert!` that a host with a byte over
//! 0x7F can reach):
//!
//! - `TooLong` if and only if the text is over `MAX_URL_BYTES` bytes.
//! - A result is ASCII, has no `#`, and is `origin`, `path`, then nothing or
//!   `?` and the query; the origin has no `@`.
//! - Idempotence: the parse of `binding_url` gives the same result.
//! - `cache_url` with no names, and with the name of the first query
//!   parameter: it is ASCII, at most 3 times U, the parse of it gives it back
//!   as U, and its `cache_url` with the same names is the same text. With the
//!   name, no parameter of that name is left.
//! - U and the cache URL can be longer than the input (percent-encoding; a
//!   `%` that starts no escape becomes `%25`). A text over `MAX_URL_BYTES`
//!   gives `TooLong` again, so the two parse checks above apply only to texts
//!   within the limit.

#![no_main]

use libfuzzer_sys::fuzz_target;
use rivqen_url::{MAX_URL_BYTES, UrlError, parse};

fuzz_target!(|data: &[u8]| {
    let Ok(text) = core::str::from_utf8(data) else {
        return;
    };
    let result = parse(text);
    assert_eq!(result == Err(UrlError::TooLong), text.len() > MAX_URL_BYTES);
    let Ok(parsed) = result else {
        return;
    };

    let binding = parsed.binding_url();
    assert!(binding.is_ascii());
    assert!(!binding.contains('#'));
    let origin = parsed.origin();
    assert!(!origin.contains('@'));
    let rest = binding
        .strip_prefix(origin)
        .expect("U starts with the origin");
    let after_path = rest
        .strip_prefix(parsed.path())
        .expect("the path follows the origin");
    assert!(parsed.path().starts_with('/'));
    assert!(after_path.is_empty() || after_path.starts_with('?'));
    if binding.len() > MAX_URL_BYTES {
        assert_eq!(parse(binding), Err(UrlError::TooLong));
    } else {
        assert_eq!(parse(binding).as_ref(), Ok(&parsed));
    }

    let first_name = after_path
        .strip_prefix('?')
        .and_then(|query| query.split('&').next())
        .map(|param| {
            param
                .split_once('=')
                .map_or(param, |(name, _)| name)
                .to_owned()
        });
    let lists: [Vec<&str>; 2] = [Vec::new(), first_name.iter().map(String::as_str).collect()];
    for names in &lists {
        let cache = parsed.cache_url(names);
        assert!(cache.is_ascii());
        assert!(cache.len() <= binding.len().saturating_mul(3));
        if cache.len() <= MAX_URL_BYTES {
            let again = parse(&cache).expect("the cache URL is a URL");
            assert_eq!(again.binding_url(), cache);
            assert_eq!(again.cache_url(names), cache);
        }
        if let Some(query) = cache.split_once('?').map(|(_, query)| query) {
            for param in query.split('&') {
                let name = param.split_once('=').map_or(param, |(name, _)| name);
                assert!(!names.iter().any(|n| !n.is_empty() && *n == name));
            }
        }
    }
});
