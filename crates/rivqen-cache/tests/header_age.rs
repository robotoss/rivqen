// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! `parse_age` (WP-07 S1 T-02): RFC 9111 sections 5.1 and 1.2.2.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rivqen_cache::header::parse_age;
use rivqen_model::http::FieldLine;

fn age(lines: &[&str]) -> Option<u64> {
    let fields: Vec<FieldLine> = lines
        .iter()
        .map(|value| FieldLine::new("Age", *value))
        .collect();
    parse_age(&fields)
}

#[test]
fn header_age_absent_gives_none() {
    assert_eq!(parse_age(&[]), None);
    assert_eq!(parse_age(&[FieldLine::new("x-age", "5")]), None);
    assert_eq!(parse_age(&[FieldLine::new("ages", "5")]), None);
}

#[test]
fn header_age_one_non_negative_integer() {
    assert_eq!(age(&["0"]), Some(0));
    assert_eq!(age(&["60"]), Some(60));
    assert_eq!(age(&["007"]), Some(7));
    assert_eq!(age(&[" \t60\t "]), Some(60));
    assert_eq!(parse_age(&[FieldLine::new("AGE", "9")]), Some(9));
}

#[test]
fn header_age_list_or_lines_give_the_first_value() {
    // RFC 9111 section 5.1: the first member, the others discarded.
    assert_eq!(age(&["5, 10"]), Some(5));
    assert_eq!(age(&["5,x"]), Some(5));
    assert_eq!(age(&["5", "10"]), Some(5));
    assert_eq!(age(&["5", "x"]), Some(5));
    // Empty list elements do not count (RFC 9110 section 5.6.1.2).
    assert_eq!(age(&[", 5"]), Some(5));
    assert_eq!(age(&["", " ", "7"]), Some(7));
    // Another field between the lines does not change the order.
    let fields = [
        FieldLine::new("age", "3"),
        FieldLine::new("date", "x"),
        FieldLine::new("age", "4"),
    ];
    assert_eq!(parse_age(&fields), Some(3));
}

#[test]
fn header_age_not_valid_gives_none() {
    // RFC 9111 section 5.1: the cache ignores the field.
    for value in [
        "", ",", "-1", "+1", "1.5", "abc", "\"5\"", "5 6", "5s", "0x10", "\u{FF15}", "x, 5",
    ] {
        assert_eq!(age(&[value]), None, "{value:?}");
    }
    assert_eq!(age(&["x", "5"]), None);
}

#[test]
fn header_age_large_values_saturate_at_u64_max() {
    // RFC 9111 section 1.2.2 allows the greatest integer the cache can
    // represent; a larger age is the stricter result, so no cap at 2^31.
    assert_eq!(age(&["2147483649"]), Some(2_147_483_649));
    assert_eq!(age(&["18446744073709551614"]), Some(u64::MAX - 1));
    assert_eq!(age(&["18446744073709551615"]), Some(u64::MAX));
    assert_eq!(age(&["18446744073709551616"]), Some(u64::MAX));
    assert_eq!(age(&[&"9".repeat(4096)]), Some(u64::MAX));
}
