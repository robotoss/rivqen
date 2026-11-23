// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Contract of `rivqen-url` (WP-08 S1): what the stub and the product
//! both guarantee.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use rivqen_url::{MAX_URL_BYTES, UrlError, parse};

#[test]
fn url_too_long_is_rejected_before_the_parse() {
    let long = format!("https://a.example/{}", "a".repeat(MAX_URL_BYTES));
    assert_eq!(parse(&long), Err(UrlError::TooLong));
}

#[test]
fn url_error_text_has_no_input_bytes() {
    for error in [
        UrlError::TooLong,
        UrlError::Syntax,
        UrlError::Scheme,
        UrlError::UserInfo,
        UrlError::Host,
    ] {
        assert!(!error.to_string().contains("example"));
    }
}

#[test]
fn url_error_texts_are_distinct_and_not_empty() {
    let texts: Vec<String> = [
        UrlError::TooLong,
        UrlError::Syntax,
        UrlError::Scheme,
        UrlError::UserInfo,
        UrlError::Host,
    ]
    .iter()
    .map(ToString::to_string)
    .collect();
    for (i, text) in texts.iter().enumerate() {
        assert_ne!(text, "");
        assert!(
            !texts.iter().skip(i + 1).any(|other| other == text),
            "{text}"
        );
    }
}

#[test]
fn url_debug_of_a_parsed_url_has_the_scheme_and_the_length_only() {
    let parsed = parse("https://a.example/secret?token=1").unwrap();
    let text = format!("{parsed:?}");
    assert_eq!(text, "ParsedUrl { scheme: Https, len: 32, .. }");
}
