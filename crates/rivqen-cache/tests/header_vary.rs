// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! `parse_vary` (WP-07 S1 T-02): RFC 9110 section 12.5.5; CL-12.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rivqen_cache::header::{Vary, parse_vary};
use rivqen_model::http::FieldLine;

fn vary(lines: &[&str]) -> Vary {
    let fields: Vec<FieldLine> = lines
        .iter()
        .map(|value| FieldLine::new("Vary", *value))
        .collect();
    parse_vary(&fields)
}

fn names(list: &[&str]) -> Vary {
    Vary::Names(list.iter().map(|name| (*name).to_owned()).collect())
}

#[test]
fn header_vary_absent_or_empty_gives_none() {
    assert_eq!(parse_vary(&[]), Vary::None);
    assert_eq!(vary(&[""]), Vary::None);
    assert_eq!(vary(&[" , ,\t", ""]), Vary::None);
    assert_eq!(
        parse_vary(&[FieldLine::new("x-vary", "accept")]),
        Vary::None
    );
}

#[test]
fn header_vary_names_lower_case_sorted_once() {
    assert_eq!(vary(&["Accept-Encoding"]), names(&["accept-encoding"]));
    assert_eq!(
        vary(&["accept-language, Accept-Encoding, ACCEPT-ENCODING"]),
        names(&["accept-encoding", "accept-language"])
    );
    assert_eq!(
        vary(&["Cookie", "accept", " template-tag , cookie"]),
        names(&["accept", "cookie", "template-tag"])
    );
    assert_eq!(vary(&["b, a, c, a, b"]), names(&["a", "b", "c"]));
    assert_eq!(
        parse_vary(&[FieldLine::new("VARY", "X-A"), FieldLine::new("vary", "x-a")]),
        names(&["x-a"])
    );
    // `**` is a token, not the member `*`.
    assert_eq!(vary(&["**"]), names(&["**"]));
}

#[test]
fn header_vary_star_in_any_member_gives_any() {
    for lines in [
        &["*"][..],
        &["accept, *"],
        &["*, accept"],
        &[" * "],
        &["accept", "*"],
        &["*", ""],
    ] {
        assert_eq!(vary(lines), Vary::Any, "{lines:?}");
    }
    // The seed `fixtures/malformed/seeds/fields/vary-star-list.txt`.
    let seed = include_str!("../../../fixtures/malformed/seeds/fields/vary-star-list.txt");
    assert_eq!(vary(&[seed]), Vary::Any);
}

#[test]
fn header_vary_member_that_is_not_a_field_name_gives_any() {
    // The cache cannot tell which request field it names: not stored.
    for value in [
        "accept encoding",
        "\"accept\"",
        "accept;q=1",
        "accept, user agent",
        "acc\u{e9}pt",
        "accept/x",
    ] {
        assert_eq!(vary(&[value]), Vary::Any, "{value}");
    }
}
