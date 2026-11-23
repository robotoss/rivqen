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
