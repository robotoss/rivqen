// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! Field line tests.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rivqen_model::http::FieldLine;

#[test]
fn model_http_field_is_matches_name_ignoring_ascii_case() {
    let line = FieldLine::new("Content-Type", "text/html");
    assert!(line.is("content-type"));
    assert!(line.is("CONTENT-TYPE"));
    assert!(!line.is("content-length"));
    assert!(!line.is("content-typ"));
}

#[test]
fn model_http_field_debug_shows_name_and_value_length_only() {
    let line = FieldLine::new("Cookie", "secret-value");
    let text = format!("{line:?}");
    assert_eq!(text, "FieldLine(Cookie, len=12)");
    assert!(!text.contains("secret"));
}
