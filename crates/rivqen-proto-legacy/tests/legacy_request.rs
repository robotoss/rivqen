// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The legacy request fields (legacy client CL-09, CL-11 item 1).

#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use rivqen_integrity::legacy::sha1_hex;
use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_model::http::FieldLine;
use rivqen_proto_legacy::request::{EntryValidators, LegacyRequest, RequestError, request_fields};

fn entry() -> EntryValidators {
    EntryValidators {
        etag: sha1_hex(b"<html>page</html>"),
        template_tag: sha1_hex(b"<html>{title}</html>"),
    }
}

fn pairs(fields: &[FieldLine]) -> Vec<(&str, &str)> {
    fields
        .iter()
        .map(|f| (f.name.as_str(), f.value.as_str()))
        .collect()
}

#[test]
fn legacy_request_without_entry_has_accept_diff_only() {
    let fields = request_fields(&LegacyRequest {
        accept_diff: true,
        entry: None,
        cookie: None,
    })
    .unwrap();
    assert_eq!(pairs(&fields), [("accept-diff", "true")]);
}

#[test]
fn legacy_request_with_entry_sends_both_validators_in_order() {
    let e = entry();
    let fields = request_fields(&LegacyRequest {
        accept_diff: true,
        entry: Some(e),
        cookie: None,
    })
    .unwrap();
    assert_eq!(
        pairs(&fields),
        [
            ("accept-diff", "true"),
            ("if-none-match", e.etag.as_str()),
            ("template-tag", e.template_tag.as_str()),
        ]
    );
}

#[test]
fn legacy_request_accept_diff_off_sends_false() {
    let fields = request_fields(&LegacyRequest {
        accept_diff: false,
        entry: Some(entry()),
        cookie: None,
    })
    .unwrap();
    assert_eq!(pairs(&fields)[0], ("accept-diff", "false"));
    assert_eq!(fields.len(), 3);
}

#[test]
fn legacy_request_cookie_is_last_and_unchanged() {
    let e = entry();
    let fields = request_fields(&LegacyRequest {
        accept_diff: true,
        entry: Some(e),
        cookie: Some("a=1; b=2"),
    })
    .unwrap();
    assert_eq!(
        pairs(&fields),
        [
            ("accept-diff", "true"),
            ("if-none-match", e.etag.as_str()),
            ("template-tag", e.template_tag.as_str()),
            ("cookie", "a=1; b=2"),
        ]
    );
}

#[test]
fn legacy_request_empty_cookie_sends_no_cookie_field() {
    for cookie in [None, Some("")] {
        let fields = request_fields(&LegacyRequest {
            accept_diff: true,
            entry: None,
            cookie,
        })
        .unwrap();
        assert!(!fields.iter().any(|f| f.is("cookie")), "{cookie:?}");
    }
}

#[test]
fn legacy_request_has_no_sdk_version_and_no_empty_value() {
    for entry in [None, Some(entry())] {
        for cookie in [None, Some("c=1")] {
            for accept_diff in [false, true] {
                let fields = request_fields(&LegacyRequest {
                    accept_diff,
                    entry,
                    cookie,
                })
                .unwrap();
                for f in &fields {
                    assert!(!f.is("sonic-sdk-version"));
                    assert!(!f.value.is_empty(), "{}", f.name);
                    assert_eq!(f.name, f.name.to_ascii_lowercase());
                }
            }
        }
    }
}

#[test]
fn legacy_request_validators_are_lowercase_hex() {
    let fields = request_fields(&LegacyRequest {
        accept_diff: true,
        entry: Some(entry()),
        cookie: None,
    })
    .unwrap();
    for f in &fields[1..] {
        assert_eq!(f.value.len(), 40);
        assert!(
            f.value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
    }
}

#[test]
fn legacy_request_cookie_with_control_character_is_an_error() {
    for cookie in ["a=1\r\nx-evil: 1", "a\n", "a\0", "a\x7f", "\x01"] {
        let result = request_fields(&LegacyRequest {
            accept_diff: true,
            entry: None,
            cookie: Some(cookie),
        });
        assert_eq!(result, Err(RequestError::CookieValue), "{cookie:?}");
    }
    let error = RequestError::CookieValue;
    assert_eq!(error.code(), ErrorCode::InternalError);
    assert_eq!(
        error.to_string(),
        "the cookie of the platform store has a control character"
    );
}

#[test]
fn legacy_request_cookie_with_htab_and_non_ascii_passes() {
    let fields = request_fields(&LegacyRequest {
        accept_diff: true,
        entry: None,
        cookie: Some("a=\tcafé~"),
    })
    .unwrap();
    assert_eq!(pairs(&fields)[1], ("cookie", "a=\tcafé~"));
}

#[test]
fn legacy_request_debug_has_no_cookie_or_digest() {
    let e = entry();
    let request = LegacyRequest {
        accept_diff: true,
        entry: Some(e),
        cookie: Some("secret-cookie-value"),
    };
    let text = format!("{request:?}");
    assert!(!text.contains("secret-cookie-value"), "{text}");
    assert!(!text.contains(&e.etag.as_str()[..8]), "{text}");
    assert!(!text.contains(&e.template_tag.as_str()[..8]), "{text}");
    assert!(text.contains("cookie_len: Some(19)"), "{text}");
}
