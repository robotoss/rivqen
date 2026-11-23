// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Legacy response head and data body (WP-08 S1, T-01 and F-03; legacy
//! client CL-14 to CL-17, CL-21, CL-24, CL-29; DL-226).
//!
//! Input: the first byte selects the body limits; the rest is the head and
//! the body, split at the first empty line (`\n\n`). Each head line is
//! `name:value` (split at the first `:`; a line without `:` is a name with
//! an empty value); bytes that are not UTF-8 are replaced.
//!
//! Invariants:
//! - No function panics.
//! - `LegacyHead::read` gives a head exactly when `is_legacy_response` is
//!   true; then there is one `Content-Type` line, and each of the four
//!   fields has lines whose values (without SP and HTAB at the ends) all
//!   equal the non-empty value of the head.
//! - A head that passes CL-14 follows DL-226: every name is a token, no
//!   value has a control byte other than HTAB, no non-empty
//!   `sonic-etag-key` value is a non-token, `Content-Type`, a protected
//!   field, a `NOT_PAGE_FIELDS` name or `isReload`, and no `Connection`
//!   token names `Content-Type` or a protected field.
//! - `page_fields` gives lowercase token names in order and values without
//!   a control byte other than HTAB, never a name of `NOT_PAGE_FIELDS` or
//!   the etag field, and each line is a line of the head. Every valid
//!   `Content-Type` or protected line of the head is in the page fields
//!   (so no protected field of a legacy response is lost). The merge of the
//!   page fields with the same head gives the page fields again; a merge
//!   never gives a line that is not a valid field line.
//! - `is_utf8_text` true means the body is UTF-8.
//! - `read_data_body` passes only within the limits; the decoded data is
//!   not longer than the body.
//! - A message is one JSON object text without a raw control byte.

#![no_main]

use libfuzzer_sys::fuzz_target;
use rivqen_model::http::FieldLine;
use rivqen_proto_legacy::body::{BodyLimits, read_data_body};
use rivqen_proto_legacy::fields::{NOT_PAGE_FIELDS, merge_page_fields, page_fields};
use rivqen_proto_legacy::message::{MessageInput, ResultCode, message};
use rivqen_proto_legacy::response::{
    LegacyHead, bare_etag, charset, etag_field_name, etag_value, is_legacy_response, is_safe_head,
    is_template_change, is_utf8_text,
};

fn trimmed(value: &str) -> &str {
    value.trim_matches([' ', '\t'])
}

/// RFC 9110 `token`, written apart from the crate.
fn is_token(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

/// No control byte other than HTAB, written apart from the crate.
fn has_no_control(value: &str) -> bool {
    !value.bytes().any(|b| (b < 0x20 && b != b'\t') || b == 0x7f)
}

fn is_valid_line(line: &FieldLine) -> bool {
    is_token(&line.name) && has_no_control(&line.value)
}

/// DL-226 (a): `Content-Type` or a protected field (ASCII
/// case-insensitive).
fn is_kept_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "content-type"
        || lower.starts_with("cross-origin-")
        || [
            "content-security-policy",
            "content-security-policy-report-only",
            "x-frame-options",
            "x-content-type-options",
            "permissions-policy",
            "referrer-policy",
            "strict-transport-security",
        ]
        .contains(&lower.as_str())
}

/// The field rules of DL-226 for a head that passed CL-14.
fn check_safe_head(fields: &[FieldLine]) {
    for line in fields {
        assert!(is_valid_line(line));
        if line.is("sonic-etag-key") {
            let key = trimmed(&line.value);
            if !key.is_empty() {
                assert!(is_token(key));
                assert!(!is_kept_name(key));
                assert!(!NOT_PAGE_FIELDS.iter().any(|n| key.eq_ignore_ascii_case(n)));
                assert!(!key.eq_ignore_ascii_case("isReload"));
            }
        }
        if line.is("connection") {
            assert!(line.value.split(',').all(|t| !is_kept_name(trimmed(t))));
        }
    }
}

fn head_lines(head: &[u8]) -> Vec<FieldLine> {
    String::from_utf8_lossy(head)
        .split('\n')
        .filter(|line| !line.is_empty())
        .map(|line| match line.split_once(':') {
            Some((name, value)) => FieldLine::new(name, trimmed(value)),
            None => FieldLine::new(line, ""),
        })
        .collect()
}

fn check_head(fields: &[FieldLine]) {
    let head = LegacyHead::read(fields);
    assert_eq!(head.is_some(), is_legacy_response(fields));
    let etag_field = etag_field_name(fields);
    let first_etag = fields
        .iter()
        .find(|f| f.is(etag_field))
        .map(|f| trimmed(&f.value));
    assert_eq!(etag_value(fields), first_etag);
    let Some(head) = head else {
        return;
    };
    assert!(is_safe_head(fields));
    check_safe_head(fields);
    assert_eq!(fields.iter().filter(|f| f.is("content-type")).count(), 1);
    for (field, value) in [
        (etag_field, head.etag()),
        ("template-tag", head.template_tag()),
        ("template-change", head.template_change()),
        ("cache-offline", head.cache_offline()),
    ] {
        assert!(!value.is_empty());
        let mut lines = fields.iter().filter(|f| f.is(field)).peekable();
        assert!(lines.peek().is_some());
        assert!(lines.all(|f| trimmed(&f.value) == value));
    }
    assert_eq!(head.bare_etag(), bare_etag(head.etag()));
    assert_eq!(
        head.is_template_change(),
        is_template_change(head.template_change())
    );
    for f in fields.iter().filter(|f| f.is("content-type")) {
        assert!(!charset(&f.value).bytes().any(|b| b.is_ascii_uppercase()));
    }
}

fn check_page_fields(fields: &[FieldLine]) {
    let etag_field = etag_field_name(fields).to_ascii_lowercase();
    let page = page_fields(fields);
    assert!(page.len() <= fields.len());
    for line in fields
        .iter()
        .filter(|f| is_valid_line(f) && is_kept_name(&f.name))
    {
        let lower = line.name.to_ascii_lowercase();
        assert!(
            page.iter()
                .any(|p| p.name == lower && p.value == line.value)
        );
    }
    for line in &page {
        assert!(is_valid_line(line));
        assert_eq!(line.name, line.name.to_ascii_lowercase());
        assert!(!NOT_PAGE_FIELDS.contains(&line.name.as_str()));
        if !is_kept_name(&etag_field) {
            assert_ne!(line.name, etag_field);
        }
        assert!(
            fields
                .iter()
                .any(|f| f.name.eq_ignore_ascii_case(&line.name) && f.value == line.value)
        );
    }
    assert!(
        page.windows(2)
            .all(|w| w.first().map(|a| &a.name) <= w.get(1).map(|b| &b.name))
    );
    assert_eq!(merge_page_fields(&page, fields), page);
    // The head as a stored record: a merge drops its invalid lines.
    assert!(merge_page_fields(fields, &page).iter().all(is_valid_line));
}

fn check_body(selector: u8, body: &[u8]) {
    let max_bytes = usize::from(selector & 0x0f).saturating_mul(256);
    let max_depth = usize::from(selector >> 4);
    if let Ok(read) = read_data_body(body, &BodyLimits::new(max_bytes, max_depth)) {
        assert!(body.len() <= max_bytes);
        let decoded = read.data().iter().fold(0_usize, |n, b| {
            n.saturating_add(b.key.len()).saturating_add(b.value.len())
        });
        assert!(decoded <= body.len());
    }
}

fn check_message(fields: &[FieldLine], body: &[u8]) {
    let diff = [rivqen_template::legacy::DataBlock {
        key: "{a}".to_owned(),
        value: String::from_utf8_lossy(body).into_owned(),
    }];
    for code in [
        ResultCode::DataUpdate,
        ResultCode::NotModified,
        ResultCode::FirstLoad,
    ] {
        let input = MessageInput {
            src: code,
            final_code: code,
            diff: &diff,
            fields,
            reload: false,
        };
        let text = message(&input);
        let Some(text) = text else {
            panic!("a message for a code other than -1");
        };
        assert!(text.starts_with("{\"") && text.ends_with("}}"));
        assert!(!text.bytes().any(|b| b < 0x20));
    }
}

fuzz_target!(|data: &[u8]| {
    let Some((&selector, rest)) = data.split_first() else {
        return;
    };
    let (head, body) = match rest.windows(2).position(|w| w == b"\n\n") {
        Some(at) => (
            rest.get(..at).unwrap_or_default(),
            rest.get(at.saturating_add(2)..).unwrap_or_default(),
        ),
        None => (rest, &[][..]),
    };
    let fields = head_lines(head);
    check_head(&fields);
    if is_utf8_text(&fields, body) {
        assert!(core::str::from_utf8(body).is_ok());
    }
    check_page_fields(&fields);
    check_body(selector, body);
    check_message(&fields, body);
});
