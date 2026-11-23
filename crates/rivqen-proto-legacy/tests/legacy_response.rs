// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The legacy response head (legacy client CL-14, CL-15, CL-16): the cases
//! that the function fixtures do not reach (every canonical
//! `is_legacy_response` case has no `Content-Type`, so all are `false`), and
//! the field rules of DL-226.

#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use rivqen_model::http::FieldLine;
use rivqen_proto_legacy::fields::{NOT_PAGE_FIELDS, PROTECTED_FIELDS, page_fields};
use rivqen_proto_legacy::response::{
    LegacyHead, etag_field_name, etag_value, is_html_response, is_legacy_response, is_safe_head,
    is_utf8_text,
};

fn lines(pairs: &[(&str, &str)]) -> Vec<FieldLine> {
    pairs.iter().map(|(n, v)| FieldLine::new(*n, *v)).collect()
}

/// A legacy head of the canonical server (C-13) with `extra` lines added.
fn head(extra: &[(&str, &str)]) -> Vec<FieldLine> {
    let mut fields = lines(&[
        ("Content-Type", "text/html; charset=utf-8"),
        ("etag", "e1"),
        ("template-tag", "t1"),
        ("template-change", "false"),
        ("cache-offline", "true"),
    ]);
    fields.extend(lines(extra));
    fields
}

fn without(fields: &[FieldLine], name: &str) -> Vec<FieldLine> {
    fields.iter().filter(|f| !f.is(name)).cloned().collect()
}

#[test]
fn legacy_head_canonical_head_passes_with_its_values() {
    let fields = head(&[]);
    let h = LegacyHead::read(&fields).unwrap();
    assert!(is_legacy_response(&fields));
    assert_eq!(h.etag_field(), "etag");
    assert_eq!(h.etag(), "e1");
    assert_eq!(h.template_tag(), "t1");
    assert_eq!(h.template_change(), "false");
    assert!(!h.is_template_change());
    assert_eq!(h.cache_offline(), "true");
}

#[test]
fn legacy_head_each_field_is_needed() {
    for name in [
        "content-type",
        "etag",
        "template-tag",
        "template-change",
        "cache-offline",
    ] {
        assert!(!is_legacy_response(&without(&head(&[]), name)), "{name}");
    }
}

#[test]
fn legacy_head_empty_value_fails() {
    for name in ["etag", "template-tag", "template-change", "cache-offline"] {
        let mut fields = without(&head(&[]), name);
        fields.push(FieldLine::new(name, " \t"));
        assert!(!is_legacy_response(&fields), "{name}");
    }
}

#[test]
fn legacy_head_two_lines_with_different_values_fail() {
    // D-33: the traced `true` and `http` lines of CS-08.
    let fields = head(&[("cache-offline", "http")]);
    assert!(!is_legacy_response(&fields));
    for name in ["etag", "template-tag", "template-change"] {
        assert!(!is_legacy_response(&head(&[(name, "other")])), "{name}");
    }
}

#[test]
fn legacy_head_two_lines_with_equal_values_count_once() {
    let fields = head(&[("Cache-Offline", " true "), ("ETAG", "e1")]);
    let h = LegacyHead::read(&fields).unwrap();
    assert_eq!(h.cache_offline(), "true");
    assert_eq!(h.etag(), "e1");
}

#[test]
fn legacy_head_values_lose_ows_at_the_ends() {
    let mut fields = without(&head(&[]), "template-change");
    fields.push(FieldLine::new("Template-Change", "\t 0 "));
    let h = LegacyHead::read(&fields).unwrap();
    assert_eq!(h.template_change(), "0");
    assert!(!h.is_template_change());
}

#[test]
fn legacy_head_template_change_true() {
    let mut fields = without(&head(&[]), "template-change");
    fields.push(FieldLine::new("template-change", "true"));
    assert!(LegacyHead::read(&fields).unwrap().is_template_change());
}

#[test]
fn legacy_head_sonic_etag_key_names_the_etag_field() {
    let fields = without(
        &head(&[
            ("Sonic-Etag-Key", " Custom-ETag "),
            ("custom-etag", "W/\"c1\""),
        ]),
        "etag",
    );
    assert_eq!(etag_field_name(&fields), "Custom-ETag");
    assert_eq!(etag_value(&fields), Some("W/\"c1\""));
    let h = LegacyHead::read(&fields).unwrap();
    assert_eq!(h.etag_field(), "Custom-ETag");
    assert_eq!(h.etag(), "W/\"c1\"");
    assert_eq!(h.bare_etag(), "c1");
}

#[test]
fn legacy_head_sonic_etag_key_without_its_field_fails() {
    // The `etag` line does not count when `sonic-etag-key` names another field.
    let fields = head(&[("sonic-etag-key", "custom-etag")]);
    assert!(!is_legacy_response(&fields));
    assert_eq!(etag_value(&fields), None);
}

#[test]
fn legacy_head_empty_sonic_etag_key_means_etag() {
    let fields = head(&[("sonic-etag-key", ""), ("sonic-etag-key", "custom")]);
    assert_eq!(etag_field_name(&fields), "etag");
    assert_eq!(etag_value(&fields), Some("e1"));
    assert!(is_legacy_response(&fields));
}

#[test]
fn legacy_head_etag_value_is_the_first_line() {
    let fields = lines(&[("ETag", " \"a\" "), ("etag", "b")]);
    assert_eq!(etag_value(&fields), Some("\"a\""));
    assert_eq!(etag_value(&[]), None);
}

#[test]
fn legacy_head_media_type_must_be_one_text_html_line() {
    let ok = [
        "text/html",
        "TEXT/HTML",
        " Text/Html ;charset=gbk",
        "text/html;",
    ];
    for value in ok {
        assert!(
            is_html_response(&lines(&[("content-type", value)])),
            "{value}"
        );
    }
    let bad = [
        "text/plain",
        "application/xhtml+xml",
        "text/htmlx",
        "",
        "text /html",
    ];
    for value in bad {
        assert!(
            !is_html_response(&lines(&[("content-type", value)])),
            "{value}"
        );
    }
    let two = lines(&[("content-type", "text/html"), ("Content-Type", "text/html")]);
    assert!(!is_html_response(&two));
    assert!(!is_html_response(&[]));
    // DL-125: a JSON resource with all legacy fields is not a legacy response.
    let mut json = without(&head(&[]), "content-type");
    json.push(FieldLine::new("content-type", "application/json"));
    assert!(!is_legacy_response(&json));
}

#[test]
fn legacy_head_utf8_text_needs_utf8_charset_and_valid_bytes() {
    let html = |ct: &str| lines(&[("content-type", ct)]);
    assert!(is_utf8_text(&html("text/html"), b"<p>caf\xc3\xa9</p>"));
    assert!(is_utf8_text(&html("text/html; charset=\"UTF-8\""), b"a"));
    assert!(is_utf8_text(&[], b"a"));
    assert!(!is_utf8_text(&html("text/html; charset=gbk"), b"a"));
    assert!(!is_utf8_text(&html("text/html; charset=utf8"), b"a"));
    assert!(!is_utf8_text(&html("text/html"), b"caf\xe9"));
    // The first `charset` parameter counts.
    assert!(is_utf8_text(
        &html("text/html; charset=utf-8; charset=gbk"),
        b"a"
    ));
    assert!(!is_utf8_text(
        &html("text/html; charset=gbk; charset=utf-8"),
        b"a"
    ));
}

#[test]
fn legacy_head_debug_has_no_value() {
    let fields = lines(&[
        ("content-type", "text/html"),
        ("etag", "secret-etag-value"),
        ("template-tag", "secret-tag-value"),
        ("template-change", "secret-change"),
        ("cache-offline", "secret-offline"),
    ]);
    let text = format!("{:?}", LegacyHead::read(&fields).unwrap());
    assert!(!text.contains("secret"), "{text}");
    assert!(text.contains("etag_len: 17"), "{text}");
}

/// A canonical head whose etag field is `custom-etag` and whose
/// `sonic-etag-key` is `key`, with a line of the field `key`.
fn keyed_head(key: &str) -> Vec<FieldLine> {
    let mut fields = without(&head(&[]), "etag");
    fields.extend(lines(&[
        ("sonic-etag-key", key),
        (key, "e1"),
        ("x-keep", "k"),
    ]));
    fields
}

#[test]
fn legacy_head_sonic_etag_key_may_name_etag_or_another_token() {
    // `etag` is the default name; naming it is allowed (DL-226 (a)).
    for key in [
        "etag",
        "ETag",
        "eTag",
        "custom-etag",
        "x-cross-origin-a",
        "x.~!",
    ] {
        let fields = keyed_head(key);
        assert!(is_safe_head(&fields), "{key}");
        let h = LegacyHead::read(&fields).unwrap();
        assert_eq!(h.etag_field(), key);
        assert_eq!(h.etag(), "e1");
    }
}

#[test]
fn legacy_head_sonic_etag_key_that_names_a_kept_or_control_field_fails() {
    // DL-226 (a), L-1 and I-1. Each key has its own line with the etag
    // value, so only the key rule rejects the head.
    let mut keys: Vec<String> = ["content-type", "Content-Type", "isReload", "ISRELOAD"]
        .iter()
        .map(|k| (*k).to_owned())
        .collect();
    keys.extend(PROTECTED_FIELDS.iter().map(|k| k.to_ascii_uppercase()));
    keys.extend([
        "cross-origin-opener-policy".to_owned(),
        "Cross-Origin-X".to_owned(),
    ]);
    keys.extend(NOT_PAGE_FIELDS.iter().map(|k| (*k).to_owned()));
    for key in &keys {
        let fields = keyed_head(key);
        assert!(!is_safe_head(&fields), "{key}");
        assert!(LegacyHead::read(&fields).is_none(), "{key}");
        assert!(!is_legacy_response(&fields), "{key}");
    }
}

#[test]
fn legacy_head_sonic_etag_key_that_is_not_a_token_fails() {
    for key in ["a b", "x:a", "\"x\"", "x,y", "caf\u{e9}"] {
        let fields = keyed_head(key);
        assert!(!is_legacy_response(&fields), "{key:?}");
    }
}

#[test]
fn legacy_head_every_sonic_etag_key_line_counts() {
    // The first non-empty line names the etag field; a later line still
    // may not name a protected field.
    let fields = head(&[
        ("sonic-etag-key", ""),
        ("sonic-etag-key", "content-security-policy"),
    ]);
    assert_eq!(etag_field_name(&fields), "etag");
    assert!(!is_legacy_response(&fields));
    let fields = head(&[
        ("sonic-etag-key", "etag"),
        ("sonic-etag-key", "x-frame-options"),
    ]);
    assert!(!is_legacy_response(&fields));
}

#[test]
fn legacy_head_l1_probe_keeps_the_policy_in_the_page_fields() {
    // The probe of the review: the response is no longer legacy, and its
    // page fields would still have the policy.
    let fields = head(&[
        ("sonic-etag-key", "Content-Security-Policy"),
        ("Content-Security-Policy", "e1"),
    ]);
    assert!(!is_legacy_response(&fields));
    let page = page_fields(&fields);
    assert!(
        page.iter()
            .any(|f| f.name == "content-security-policy" && f.value == "e1")
    );
}

#[test]
fn legacy_head_connection_that_names_a_kept_field_fails() {
    for token in [
        "content-type",
        "Content-Security-Policy",
        "content-security-policy-report-only",
        "X-Frame-Options",
        "x-content-type-options",
        "permissions-policy",
        "referrer-policy",
        "Strict-Transport-Security",
        "cross-origin-embedder-policy",
    ] {
        let value = format!("close, {token}");
        let fields = head(&[("Connection", value.as_str())]);
        assert!(!is_safe_head(&fields), "{token}");
        assert!(!is_legacy_response(&fields), "{token}");
        // The token is found on every `Connection` line.
        let fields = head(&[
            ("connection", "close"),
            ("CONNECTION", &format!(" {token}\t")),
        ]);
        assert!(!is_legacy_response(&fields), "{token}");
    }
}

#[test]
fn legacy_head_connection_with_other_tokens_passes() {
    let fields = head(&[
        (
            "Connection",
            "close, keep-alive, , x-hop, etag, cross-origin",
        ),
        ("x-hop", "1"),
    ]);
    assert!(is_safe_head(&fields));
    assert!(is_legacy_response(&fields));
}

#[test]
fn legacy_head_with_a_line_that_is_not_a_valid_field_line_fails() {
    // DL-226 (b), L-2.
    for name in ["Set-Cookie ", "set-cookie\0", "", "x a", "x\u{e9}"] {
        let fields = head(&[(name, "v")]);
        assert!(!is_safe_head(&fields), "{name:?}");
        assert!(!is_legacy_response(&fields), "{name:?}");
    }
    for byte in (0_u8..0x20).chain([0x7f]) {
        let value = format!("a{}b", char::from(byte));
        let fields = head(&[("x-a", value.as_str())]);
        assert_eq!(is_legacy_response(&fields), byte == b'\t', "{byte:#04x}");
    }
    // A control byte in a legacy field value fails too.
    let mut fields = without(&head(&[]), "template-tag");
    fields.push(FieldLine::new("template-tag", "t1\r\nx-a: 1"));
    assert!(!is_legacy_response(&fields));
}

#[test]
fn legacy_head_page_fields_keep_every_protected_line_of_a_legacy_response() {
    let mut fields = head(&[
        ("Connection", "close, x-hop"),
        ("x-hop", "1"),
        ("Content-Security-Policy", "default-src 'self'"),
        ("X-Frame-Options", "DENY"),
        ("Cross-Origin-Opener-Policy", "same-origin"),
    ]);
    fields.push(FieldLine::new("Referrer-Policy", "no-referrer"));
    assert!(is_legacy_response(&fields));
    let page = page_fields(&fields);
    for line in fields.iter().filter(|f| {
        f.is("content-type")
            || f.is("content-security-policy")
            || f.is("x-frame-options")
            || f.is("cross-origin-opener-policy")
            || f.is("referrer-policy")
    }) {
        let name = line.name.to_ascii_lowercase();
        assert!(
            page.iter().any(|p| p.name == name && p.value == line.value),
            "{name}"
        );
    }
    assert!(!page.iter().any(|p| p.name == "x-hop"));
}
