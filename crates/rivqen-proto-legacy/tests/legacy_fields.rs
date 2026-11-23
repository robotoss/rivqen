// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The page fields of a response (legacy client CL-21, DL-129) and the merge
//! by name of a `304` (CL-17).

#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use rivqen_model::http::FieldLine;
use rivqen_proto_legacy::fields::{NOT_PAGE_FIELDS, merge_page_fields, page_fields};

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
        ("", "empty name"),
    ]);
    // An empty token names no field: a line with an empty name stays.
    assert_eq!(
        pairs(&page_fields(&fields)),
        [("", "empty name"), ("x-keep", "4")]
    );
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
fn legacy_merge_fields_with_an_empty_side() {
    let stored = lines(&[("x-a", "1")]);
    assert_eq!(merge_page_fields(&stored, &[]), stored);
    let response = lines(&[("X-A", "2")]);
    assert_eq!(pairs(&merge_page_fields(&[], &response)), [("x-a", "2")]);
}
