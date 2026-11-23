// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The page fields of a response (legacy client CL-21, DL-129) and the merge
//! by name of a `304` (CL-17).

#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use rivqen_model::http::FieldLine;
use rivqen_proto_legacy::fields::{
    NOT_PAGE_FIELDS, PROTECTED_FIELD_PREFIX, PROTECTED_FIELDS, is_protected_field,
    is_valid_field_line, merge_page_fields, page_fields,
};

fn lines(pairs: &[(&str, &str)]) -> Vec<FieldLine> {
    pairs.iter().map(|(n, v)| FieldLine::new(*n, *v)).collect()
}

fn pairs(fields: &[FieldLine]) -> Vec<(&str, &str)> {
    fields
        .iter()
        .map(|f| (f.name.as_str(), f.value.as_str()))
        .collect()
}

#[test]
fn legacy_page_fields_drop_every_listed_name_in_any_case() {
    for name in NOT_PAGE_FIELDS {
        for spelled in [name.to_owned(), name.to_ascii_uppercase()] {
            let fields = lines(&[(spelled.as_str(), "v"), ("x-keep", "k")]);
            assert_eq!(pairs(&page_fields(&fields)), [("x-keep", "k")], "{spelled}");
        }
    }
}

#[test]
fn legacy_page_fields_list_is_the_list_of_cl_21() {
    let mut names = NOT_PAGE_FIELDS.to_vec();
    names.sort_unstable();
    let mut expected = vec![
        "set-cookie",
        "set-cookie2",
        "connection",
        "keep-alive",
        "proxy-connection",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
        "cache-control",
        "expires",
        "content-encoding",
        "content-length",
        "sonic-etag-key",
        "template-tag",
        "template-change",
        "cache-offline",
        "vary",
        "cdn-cache-control",
        "surrogate-control",
        "clear-site-data",
        "refresh",
        "www-authenticate",
        "authentication-info",
    ];
    expected.sort_unstable();
    assert_eq!(names, expected);
}

#[test]
fn legacy_protected_fields_are_the_list_of_dl_226() {
    let mut names = PROTECTED_FIELDS.to_vec();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "content-security-policy",
            "content-security-policy-report-only",
            "permissions-policy",
            "referrer-policy",
            "strict-transport-security",
            "x-content-type-options",
            "x-frame-options",
        ]
    );
    assert_eq!(PROTECTED_FIELD_PREFIX, "cross-origin-");
    for name in PROTECTED_FIELDS {
        assert!(is_protected_field(name), "{name}");
        assert!(is_protected_field(&name.to_ascii_uppercase()), "{name}");
    }
    for name in [
        "Cross-Origin-Opener-Policy",
        "CROSS-ORIGIN-EMBEDDER-POLICY",
        "cross-origin-",
    ] {
        assert!(is_protected_field(name), "{name}");
    }
    for name in [
        "",
        "cross-origin",
        "x-cross-origin-a",
        "content-security-policy ",
        "content-security",
        "x-frame-options-x",
        "content-type",
        "crößorigin-abcdef",
    ] {
        assert!(!is_protected_field(name), "{name}");
    }
}

#[test]
fn legacy_valid_field_line_is_a_token_name_and_a_value_without_controls() {
    let ok = |n: &str, v: &str| is_valid_field_line(&FieldLine::new(n, v));
    assert!(ok("x-a", ""));
    assert!(ok("x-a", " a\tb \u{e9}"));
    assert!(!ok("", "v"));
    assert!(!ok("x a", "v"));
    assert!(!ok("x-a", "\r"));
    assert!(!ok("x-a", "\n"));
    assert!(!ok("x-a", "\0"));
    assert!(!ok("x-a", "\u{1f}"));
    assert!(!ok("x-a", "\u{7f}"));
}

#[test]
fn legacy_page_fields_keep_security_and_content_fields() {
    let fields = lines(&[
        ("Content-Security-Policy", "frame-ancestors 'none'"),
        ("Content-Type", "text/html; charset=utf-8"),
        ("X-Frame-Options", "DENY"),
        ("Referrer-Policy", "no-referrer"),
        ("Set-Cookie", "sid=secret"),
        ("Set-Cookie2", "old=1"),
    ]);
    assert_eq!(
        pairs(&page_fields(&fields)),
        [
            ("content-security-policy", "frame-ancestors 'none'"),
            ("content-type", "text/html; charset=utf-8"),
            ("referrer-policy", "no-referrer"),
            ("x-frame-options", "DENY"),
        ]
    );
}

#[test]
fn legacy_page_fields_drop_the_etag_field() {
    let plain = lines(&[("ETag", "e1"), ("x-a", "1")]);
    assert_eq!(pairs(&page_fields(&plain)), [("x-a", "1")]);
    // With `sonic-etag-key`, the named field goes and `etag` stays.
    let custom = lines(&[
        ("sonic-etag-key", "My-ETag"),
        ("my-etag", "c1"),
        ("etag", "e1"),
    ]);
    assert_eq!(pairs(&page_fields(&custom)), [("etag", "e1")]);
}

#[test]
fn legacy_page_fields_drop_the_names_that_connection_lists() {
    let fields = lines(&[
        ("Connection", " X-Hop , ,x-other\t"),
        ("connection", "X-Third"),
        ("x-hop", "1"),
        ("X-OTHER", "2"),
        ("x-third", "3"),
        ("x-keep", "4"),
    ]);
    assert_eq!(pairs(&page_fields(&fields)), [("x-keep", "4")]);
}

#[test]
fn legacy_page_fields_drop_a_line_whose_name_is_not_a_token() {
    // DL-226 (b), L-2: a later consumer could trim such a name to a field
    // that CL-21 drops. An empty `Connection` token does not keep the line.
    let names = [
        "",
        "Set-Cookie ",
        " set-cookie",
        "set-cookie\0",
        "x a",
        "x:a",
        "x\u{e9}",
        "\"x\"",
        "x\r",
    ];
    for name in names {
        let fields = lines(&[(name, "v"), ("Connection", " , "), ("x-keep", "k")]);
        assert_eq!(pairs(&page_fields(&fields)), [("x-keep", "k")], "{name:?}");
    }
    // Every `tchar` is a name byte.
    let token = "!#$%&'*+-.^_`|~09AZaz";
    assert_eq!(
        pairs(&page_fields(&lines(&[(token, "v")]))),
        [(token.to_ascii_lowercase().as_str(), "v")]
    );
}

#[test]
fn legacy_page_fields_drop_a_value_with_a_control_byte_other_than_htab() {
    for byte in (0_u8..0x20).chain([0x7f]) {
        let value = format!("a{}b", char::from(byte));
        let fields = lines(&[("x-a", value.as_str()), ("x-keep", "k")]);
        let expected: &[(&str, &str)] = if byte == b'\t' {
            &[("x-a", "a\tb"), ("x-keep", "k")]
        } else {
            &[("x-keep", "k")]
        };
        assert_eq!(pairs(&page_fields(&fields)), expected, "{byte:#04x}");
    }
    // SP and bytes from 0x80 (obs-text) stay.
    let fields = lines(&[("x-a", " caf\u{e9} \u{80} ")]);
    assert_eq!(
        pairs(&page_fields(&fields)),
        [("x-a", " caf\u{e9} \u{80} ")]
    );
}

#[test]
fn legacy_page_fields_never_lose_a_protected_field_to_sonic_etag_key() {
    // DL-226 (a), L-1: the head test rejects such a response; the page
    // fields still keep the protected field and `Content-Type`.
    for key in [
        "Content-Security-Policy",
        "content-security-policy-report-only",
        "X-Frame-Options",
        "x-content-type-options",
        "permissions-policy",
        "Referrer-Policy",
        "strict-transport-security",
        "Cross-Origin-Opener-Policy",
        "content-type",
    ] {
        let fields = lines(&[
            ("sonic-etag-key", key),
            (key, "p"),
            ("etag", "e1"),
            ("x-keep", "k"),
        ]);
        let lower = key.to_ascii_lowercase();
        let mut expected = vec![("etag", "e1"), (lower.as_str(), "p"), ("x-keep", "k")];
        expected.sort_unstable();
        assert_eq!(pairs(&page_fields(&fields)), expected, "{key}");
    }
}

#[test]
fn legacy_page_fields_never_lose_a_protected_field_to_connection() {
    let fields = lines(&[
        (
            "Connection",
            "close, content-security-policy, X-FRAME-OPTIONS, content-type, \
             cross-origin-resource-policy, x-hop",
        ),
        ("Content-Security-Policy", "default-src 'self'"),
        ("x-frame-options", "DENY"),
        ("Content-Type", "text/html"),
        ("Cross-Origin-Resource-Policy", "same-origin"),
        ("x-hop", "1"),
        ("close", "2"),
    ]);
    assert_eq!(
        pairs(&page_fields(&fields)),
        [
            ("content-security-policy", "default-src 'self'"),
            ("content-type", "text/html"),
            ("cross-origin-resource-policy", "same-origin"),
            ("x-frame-options", "DENY"),
        ]
    );
}

#[test]
fn legacy_page_fields_drop_a_listed_name_that_only_looks_protected() {
    // The prefix rule is a prefix: `x-cross-origin-a` is not protected.
    let fields = lines(&[
        ("Connection", "x-cross-origin-a, cross-origin"),
        ("x-cross-origin-a", "1"),
        ("cross-origin", "2"),
    ]);
    assert_eq!(page_fields(&fields), Vec::new());
}

#[test]
fn legacy_page_fields_lowercase_names_and_keep_the_order_of_one_name() {
    let fields = lines(&[
        ("X-B", "1"),
        ("Link", "<a>"),
        ("x-b", "2"),
        ("link", "<b>"),
        ("X-A", " v "),
    ]);
    assert_eq!(
        pairs(&page_fields(&fields)),
        [
            ("link", "<a>"),
            ("link", "<b>"),
            ("x-a", " v "),
            ("x-b", "1"),
            ("x-b", "2"),
        ]
    );
}

#[test]
fn legacy_page_fields_of_nothing_is_nothing() {
    assert_eq!(page_fields(&[]), Vec::new());
}

#[test]
fn legacy_merge_fields_replace_the_stored_lines_of_each_name() {
    let stored = lines(&[
        ("content-type", "text/html"),
        ("link", "<old1>"),
        ("link", "<old2>"),
        ("x-old", "keep"),
    ]);
    let response = lines(&[
        ("Link", "<new>"),
        ("X-New", "n"),
        ("cache-offline", "store"),
    ]);
    assert_eq!(
        pairs(&merge_page_fields(&stored, &response)),
        [
            ("content-type", "text/html"),
            ("link", "<new>"),
            ("x-new", "n"),
            ("x-old", "keep"),
        ]
    );
}

#[test]
fn legacy_merge_fields_ignore_names_that_are_not_page_fields() {
    // A `304` with only control fields changes nothing; its `Set-Cookie`
    // never enters the entry.
    let stored = lines(&[("content-type", "text/html"), ("x-a", "1")]);
    let response = lines(&[
        ("Set-Cookie", "sid=1"),
        ("cache-offline", "store"),
        ("Content-Length", "0"),
        ("Cache-Control", "no-cache"),
    ]);
    assert_eq!(merge_page_fields(&stored, &response), stored);
}

#[test]
fn legacy_merge_fields_compare_names_ascii_case_insensitively() {
    let stored = lines(&[("X-Policy", "old"), ("Other", "o")]);
    let response = lines(&[("x-POLICY", "new")]);
    assert_eq!(
        pairs(&merge_page_fields(&stored, &response)),
        [("other", "o"), ("x-policy", "new")]
    );
}

#[test]
fn legacy_merge_fields_drop_a_stored_line_that_is_not_a_valid_field_line() {
    // DL-226 (b): a stored record can be old or damaged.
    let stored = lines(&[
        ("x-a", "1"),
        ("Set-Cookie ", "sid=1"),
        ("x-b", "a\r\nset-cookie: sid=2"),
        ("x-c", "a\tb"),
    ]);
    let response = lines(&[("x-d", "bad\0"), ("x-e", "e")]);
    assert_eq!(
        pairs(&merge_page_fields(&stored, &response)),
        [("x-a", "1"), ("x-c", "a\tb"), ("x-e", "e")]
    );
}

#[test]
fn legacy_merge_fields_with_an_empty_side() {
    let stored = lines(&[("x-a", "1")]);
    assert_eq!(merge_page_fields(&stored, &[]), stored);
    let response = lines(&[("X-A", "2")]);
    assert_eq!(pairs(&merge_page_fields(&[], &response)), [("x-a", "2")]);
}
