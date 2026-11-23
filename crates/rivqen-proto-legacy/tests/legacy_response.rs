// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The legacy response head (legacy client CL-14, CL-15, CL-16): the cases
//! that the function fixtures do not reach (every canonical
//! `is_legacy_response` case has no `Content-Type`, so all are `false`).

#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use rivqen_model::http::FieldLine;
use rivqen_proto_legacy::response::{
    LegacyHead, etag_field_name, etag_value, is_html_response, is_legacy_response, is_utf8_text,
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
