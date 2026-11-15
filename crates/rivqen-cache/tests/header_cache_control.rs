// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! `parse_cache_control` (WP-07 S1 T-02): RFC 9111 section 5.2, RFC 5861,
//! DL-088 items 3 and 5. Each expected value is written from the RFC text or
//! the decision, not from the code.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rivqen_cache::header::{CacheControl, parse_cache_control};
use rivqen_model::http::FieldLine;

/// 2^31 (RFC 9111 section 1.2.2).
const CAP: u64 = 2_147_483_648;

fn cc(lines: &[&str]) -> CacheControl {
    let fields: Vec<FieldLine> = lines
        .iter()
        .map(|value| FieldLine::new("Cache-Control", *value))
        .collect();
    parse_cache_control(&fields)
}

fn one(value: &str) -> CacheControl {
    cc(&[value])
}

/// Only `invalid_freshness`.
const INVALID: CacheControl = CacheControl {
    no_store: false,
    no_cache: false,
    private: false,
    public: false,
    must_revalidate: false,
    max_age: None,
    stale_while_revalidate: None,
    stale_if_error: None,
    invalid_freshness: true,
};

#[test]
fn header_cache_control_absent_or_empty_gives_no_directive() {
    assert_eq!(parse_cache_control(&[]), CacheControl::default());
    assert_eq!(one(""), CacheControl::default());
    // Empty list elements are ignored (RFC 9110 section 5.6.1.2).
    assert_eq!(one(" , ,\t,"), CacheControl::default());
}

#[test]
fn header_cache_control_reads_each_directive() {
    let all = one(
        "no-store, no-cache, private, public, must-revalidate, max-age=60, \
         stale-while-revalidate=30, stale-if-error=1200",
    );
    assert_eq!(
        all,
        CacheControl {
            no_store: true,
            no_cache: true,
            private: true,
            public: true,
            must_revalidate: true,
            max_age: Some(60),
            stale_while_revalidate: Some(30),
            stale_if_error: Some(1200),
            invalid_freshness: false,
        }
    );
    // One directive at a time, so that each flag is its own.
    assert_eq!(
        one("no-store"),
        CacheControl {
            no_store: true,
            ..CacheControl::default()
        }
    );
    assert_eq!(
        one("no-cache"),
        CacheControl {
            no_cache: true,
            ..CacheControl::default()
        }
    );
    assert_eq!(
        one("private"),
        CacheControl {
            private: true,
            ..CacheControl::default()
        }
    );
    assert_eq!(
        one("public"),
        CacheControl {
            public: true,
            ..CacheControl::default()
        }
    );
    assert_eq!(
        one("must-revalidate"),
        CacheControl {
            must_revalidate: true,
            ..CacheControl::default()
        }
    );
    assert_eq!(
        one("stale-while-revalidate=30"),
        CacheControl {
            stale_while_revalidate: Some(30),
            ..CacheControl::default()
        }
    );
    assert_eq!(
        one("stale-if-error=1200"),
        CacheControl {
            stale_if_error: Some(1200),
            ..CacheControl::default()
        }
    );
}

#[test]
fn header_cache_control_rfc5861_examples() {
    // RFC 5861 sections 3.1 and 4.1.
    let swr = one("max-age=600, stale-while-revalidate=30");
    assert_eq!(swr.max_age, Some(600));
    assert_eq!(swr.stale_while_revalidate, Some(30));
    assert!(!swr.invalid_freshness);
    let sie = one("max-age=600, stale-if-error=1200");
    assert_eq!(sie.max_age, Some(600));
    assert_eq!(sie.stale_if_error, Some(1200));
    assert!(!sie.invalid_freshness);
}

#[test]
fn header_cache_control_every_field_line_counts() {
    let fields = [
        FieldLine::new("cache-control", "max-age=60"),
        FieldLine::new("Content-Type", "text/html"),
        FieldLine::new("CACHE-CONTROL", "no-cache"),
        FieldLine::new("Cache-Control", "private"),
    ];
    let parsed = parse_cache_control(&fields);
    assert_eq!(parsed.max_age, Some(60));
    assert!(parsed.no_cache);
    assert!(parsed.private);
    assert!(!parsed.invalid_freshness);
}

#[test]
fn header_cache_control_other_field_names_do_not_count() {
    let fields = [
        FieldLine::new("x-cache-control", "no-store, max-age=5"),
        FieldLine::new("cache-control2", "no-store"),
        FieldLine::new("pragma", "no-cache"),
        FieldLine::new("cdn-cache-control", "private"),
    ];
    assert_eq!(parse_cache_control(&fields), CacheControl::default());
}

#[test]
fn header_cache_control_names_are_ascii_case_insensitive() {
    let parsed = one(
        "NO-STORE, No-Cache, PRIVATE, Public, Must-Revalidate, MAX-AGE=5, \
         Stale-While-Revalidate=6, STALE-IF-ERROR=7",
    );
    assert!(parsed.no_store);
    assert!(parsed.no_cache);
    assert!(parsed.private);
    assert!(parsed.public);
    assert!(parsed.must_revalidate);
    assert_eq!(parsed.max_age, Some(5));
    assert_eq!(parsed.stale_while_revalidate, Some(6));
    assert_eq!(parsed.stale_if_error, Some(7));
    // The seed `fixtures/malformed/seeds/fields/cache-control-no-store-case.txt`.
    let seed = one("max-age=1, NO-STORE");
    assert!(seed.no_store);
    assert_eq!(seed.max_age, Some(1));
    // Not ASCII: the Kelvin sign is not a `k`; the name is not a token.
    assert!(!one("no-\u{212A}ache").no_cache);
}

#[test]
fn header_cache_control_ows_around_members() {
    let parsed = one(" \tmax-age=5 ,\tprivate\t, public ");
    assert_eq!(parsed.max_age, Some(5));
    assert!(parsed.private);
    assert!(parsed.public);
    assert!(!parsed.invalid_freshness);
}

#[test]
fn header_cache_control_splits_at_commas_outside_quoted_strings() {
    // The comma inside the quoted-string does not split: `max-age=5` is
    // part of the field names of `no-cache`.
    let parsed = one("no-cache=\"a, max-age=5\", private");
    assert!(parsed.no_cache);
    assert!(parsed.private);
    assert_eq!(parsed.max_age, None);
    assert!(!parsed.invalid_freshness);
    // An escaped quote does not close the string.
    let escaped = one(r#"community="a\", max-age=9", max-age=3"#);
    assert_eq!(escaped.max_age, Some(3));
    assert!(!escaped.invalid_freshness);
    // RFC 9111 section 5.2.3 example: the unknown directive is ignored.
    assert_eq!(
        one("private, community=\"UCI\""),
        CacheControl {
            private: true,
            ..CacheControl::default()
        }
    );
}

#[test]
fn header_cache_control_argument_as_token_or_quoted_string() {
    assert_eq!(one("max-age=7").max_age, Some(7));
    // RFC 9111 section 5.2: a recipient accepts the quoted form too.
    assert_eq!(one("max-age=\"7\"").max_age, Some(7));
    assert!(!one("max-age=\"7\"").invalid_freshness);
    assert_eq!(
        one("stale-while-revalidate=\"8\"").stale_while_revalidate,
        Some(8)
    );
    assert_eq!(one("stale-if-error=\"9\"").stale_if_error, Some(9));
    assert!(one("no-cache=set-cookie").no_cache);
    assert!(one("no-cache=\"set-cookie, x-a\"").no_cache);
}

#[test]
fn header_cache_control_no_store_with_and_without_argument() {
    // DL-088 item 3: the name before any `=` counts.
    for value in [
        "no-store",
        "no-store=1",
        "no-store=\"x\"",
        "NO-STORE",
        " no-store ",
        "max-age=5,no-store",
        "no-store,max-age=5",
    ] {
        assert!(one(value).no_store, "{value}");
    }
    // DL-088 item 5: a split at every `,` and `;`, quotes not parsed; the
    // name loses SP, HTAB and `"` at its ends.
    for value in [
        "private=\"a, no-store\"",
        "private=\"a, no-store \"",
        "x;no-store",
        "max-age=5; no-store",
        "community=\"a;no-store\"",
        "\"no-store\"",
        "no-store =1",
    ] {
        assert!(one(value).no_store, "{value}");
    }
    // Not `no-store`.
    for value in [
        "no-stores",
        "no_store",
        "x-no-store",
        "nostore",
        "no-store\u{1}",
        "community=\"no-store\"",
        "no store",
    ] {
        assert!(!one(value).no_store, "{value}");
    }
}

#[test]
fn header_cache_control_no_cache_and_private_count_with_an_argument() {
    assert!(one("no-cache=\"set-cookie\"").no_cache);
    assert!(one("no-cache=x").no_cache);
    assert!(one("private=\"set-cookie\"").private);
    assert!(one("private=x").private);
    // A valid line: a name inside a quoted string is not a directive.
    let quoted = one("community=\"private, no-cache, must-revalidate\"");
    assert!(!quoted.private);
    assert!(!quoted.no_cache);
    assert!(!quoted.must_revalidate);
    assert!(!quoted.invalid_freshness);
}

#[test]
fn header_cache_control_public_counts_only_without_an_argument() {
    assert!(one("public").public);
    assert!(!one("public=1").public);
    assert!(!one("public=\"\"").public);
    // The argument is valid grammar: not a syntax error.
    assert!(!one("public=1").invalid_freshness);
    // One plain `public` is enough.
    assert!(one("public=1, public").public);
}

#[test]
fn header_cache_control_must_revalidate_counts_with_an_argument() {
    assert!(one("must-revalidate=1").must_revalidate);
    assert!(one("must-revalidate=\"x\"").must_revalidate);
}

#[test]
fn header_cache_control_unknown_directives_are_ignored() {
    assert_eq!(
        one(
            "s-maxage=100, proxy-revalidate, immutable, no-transform, must-understand, \
             foo, bar=baz, qux=\"a, b\", max-stale=5, min-fresh=1, only-if-cached"
        ),
        CacheControl::default()
    );
    // Every tchar in a name (RFC 9110 section 5.6.2).
    assert_eq!(
        one("!#$%&'*+-.^_`|~09azAZ=!#$%&'*+-.^_`|~09azAZ"),
        CacheControl::default()
    );
}

#[test]
fn header_cache_control_zero_and_leading_zeros() {
    assert_eq!(one("max-age=0").max_age, Some(0));
    assert!(!one("max-age=0").invalid_freshness);
    assert_eq!(one("max-age=000").max_age, Some(0));
    assert_eq!(one("max-age=0060").max_age, Some(60));
}

#[test]
fn header_cache_control_the_same_value_twice_counts() {
    // RFC 9111 section 4.2.1: the first occurrence, which equals the other.
    assert_eq!(one("max-age=10, max-age=10").max_age, Some(10));
    assert_eq!(one("max-age=10, max-age=010").max_age, Some(10));
    assert_eq!(one("max-age=10, max-age=\"10\"").max_age, Some(10));
    assert_eq!(cc(&["max-age=10", "max-age=10"]).max_age, Some(10));
    assert!(!one("max-age=10, max-age=10").invalid_freshness);
    assert_eq!(one("max-age=0, max-age=00").max_age, Some(0));
}

#[test]
fn header_cache_control_invalid_not_digits() {
    for name in ["max-age", "stale-while-revalidate", "stale-if-error"] {
        for arg in [
            "abc", "-1", "+1", "1.5", "0x10", "1e3", "5s", "\u{FF15}", "\"5 \"", "\" 5\"", "\"-1\"",
        ] {
            let value = format!("{name}={arg}");
            assert_eq!(one(&value), INVALID, "{value}");
        }
    }
}

#[test]
fn header_cache_control_invalid_empty_value() {
    for name in ["max-age", "stale-while-revalidate", "stale-if-error"] {
        for value in [name.to_owned(), format!("{name}="), format!("{name}=\"\"")] {
            assert_eq!(one(&value), INVALID, "{value}");
        }
    }
}

#[test]
fn header_cache_control_invalid_two_different_values() {
    for name in ["max-age", "stale-while-revalidate", "stale-if-error"] {
        let same_line = format!("{name}=10, {name}=20");
        assert_eq!(one(&same_line), INVALID, "{same_line}");
        let two_lines = cc(&[&format!("{name}=10"), &format!("{name}=20")]);
        assert_eq!(two_lines, INVALID, "{name} on two lines");
        // A later valid value does not repair it.
        let three = format!("{name}=10, {name}=20, {name}=10");
        assert_eq!(one(&three), INVALID, "{three}");
        // A value and a value that is not valid.
        let bad_second = format!("{name}=10, {name}=x");
        assert_eq!(one(&bad_second), INVALID, "{bad_second}");
        let bad_first = format!("{name}=x, {name}=10");
        assert_eq!(one(&bad_first), INVALID, "{bad_first}");
    }
    // Two values over the cap are still two different values.
    assert_eq!(one("max-age=2147483649, max-age=2147483650"), INVALID);
}

#[test]
fn header_cache_control_invalid_leaves_the_other_directives() {
    let parsed = one("max-age=5, stale-if-error=x, private, stale-while-revalidate=6");
    assert_eq!(
        parsed,
        CacheControl {
            private: true,
            max_age: Some(5),
            stale_while_revalidate: Some(6),
            invalid_freshness: true,
            ..CacheControl::default()
        }
    );
}

#[test]
fn header_cache_control_delta_seconds_cap() {
    // RFC 9111 section 1.2.2: over 2^31 is 2^31.
    assert_eq!(one("max-age=2147483647").max_age, Some(CAP - 1));
    assert_eq!(one("max-age=2147483648").max_age, Some(CAP));
    assert_eq!(one("max-age=2147483649").max_age, Some(CAP));
    assert_eq!(one("max-age=18446744073709551616").max_age, Some(CAP));
    let long = format!("max-age={}", "9".repeat(4096));
    assert_eq!(one(&long).max_age, Some(CAP));
    assert!(!one(&long).invalid_freshness);
    assert_eq!(
        one("stale-while-revalidate=99999999999").stale_while_revalidate,
        Some(CAP)
    );
    assert_eq!(one("stale-if-error=99999999999").stale_if_error, Some(CAP));
    // Leading zeros do not count against the cap.
    let zeros = format!("max-age={}5", "0".repeat(4096));
    assert_eq!(one(&zeros).max_age, Some(5));
}

#[test]
fn header_cache_control_invalid_quoted_pair_in_a_number() {
    // Stricter than RFC 9110 section 5.6.4 (which would unescape it).
    assert_eq!(one(r#"max-age="1\0""#), INVALID);
}

#[test]
fn header_cache_control_invalid_syntax_sets_invalid_freshness_and_clears_public() {
    for value in [
        // White space around `=` (no BWS in the grammar).
        "public, max-age =5",
        "public, max-age= 5",
        // A name that is not a token.
        "public, a b",
        "public, x\"y\"",
        "public, (comment)",
        "public, max-age=5;foo",
        // Text after the closing quote.
        "public, no-cache=\"a\"b",
        // An unclosed quoted-string.
        "public, no-cache=\"a",
        // A control byte or DEL inside a quoted-string.
        "public, foo=\"a\u{1}\"",
        "public, foo=\"a\u{7f}\"",
        // A quoted-pair with a control byte or DEL.
        "public, foo=\"a\\\u{1}\"",
        "public, foo=\"a\\\u{7f}\"",
        // An argument that is neither a token nor a quoted-string.
        "public, foo=a b",
        "public, foo=a\"b\"",
        "public, foo=",
        // No name.
        "public, =5",
        "public, =",
    ] {
        let parsed = one(value);
        assert!(parsed.invalid_freshness, "{value}");
        assert!(!parsed.public, "{value}");
    }
    // Each delimiter of RFC 9110 section 5.6.2 that does not split.
    for delimiter in [
        "(", ")", "/", ":", ";", "<", ">", "?", "@", "[", "\\", "]", "{", "}",
    ] {
        let value = format!("public, a{delimiter}b");
        let parsed = one(&value);
        assert!(parsed.invalid_freshness, "{value}");
        assert!(!parsed.public, "{value}");
    }
    // A syntax error on another line also clears `public`.
    let two = cc(&["public, max-age=60", "a b"]);
    assert!(!two.public);
    assert!(two.invalid_freshness);
    assert_eq!(two.max_age, Some(60));
}

#[test]
fn header_cache_control_valid_quoted_strings() {
    for value in [
        "foo=\"\"",
        "foo=\"\t !#[]~\"",
        "foo=\"\u{e9}\"",
        r#"foo="\"""#,
        r#"foo="\\""#,
        "foo=\"\\\u{e9}\"",
        "foo=\"\\\t\"",
        "foo=\"\\ \"",
        "foo=\"\\!\"",
        "foo=\"\\~\"",
        "foo=\"a,b;c=d\"",
    ] {
        let parsed = one(&format!("public, {value}"));
        assert!(!parsed.invalid_freshness, "{value}");
        assert!(parsed.public, "{value}");
    }
}

#[test]
fn header_cache_control_invalid_line_counts_hidden_restrictive_directives() {
    // The broken member `x"y, ..."` hides directives from the list grammar;
    // the DL-088 split of that line still finds them.
    let parsed = one("max-age=5, x\"y, private, must-revalidate, no-cache\"");
    assert!(parsed.private);
    assert!(parsed.must_revalidate);
    assert!(parsed.no_cache);
    assert!(parsed.invalid_freshness);
    assert_eq!(parsed.max_age, Some(5));
    // `private` with white space before `=` is not a token: also found.
    assert!(one("private =\"x\"").private);
    assert!(one("no-cache =\"x\"").no_cache);
    assert!(one("must-revalidate =1").must_revalidate);
    // On a valid line the split of DL-088 does not add these three.
    let valid = cc(&[
        "max-age=5",
        "community=\"private; no-cache, must-revalidate\"",
    ]);
    assert!(!valid.private);
    assert!(!valid.no_cache);
    assert!(!valid.must_revalidate);
    // The split is per line: the error of line 2 does not read line 1.
    let per_line = cc(&["community=\"private\"", "a b"]);
    assert!(!per_line.private);
    assert!(per_line.invalid_freshness);
}

#[test]
fn header_cache_control_unclosed_quote_keeps_the_directive_name() {
    let parsed = one("max-age=5, no-cache=\"a, private");
    assert!(parsed.no_cache);
    // `private` is inside the unclosed string, found by the DL-088 split.
    assert!(parsed.private);
    assert!(parsed.invalid_freshness);
    assert_eq!(parsed.max_age, Some(5));
}
