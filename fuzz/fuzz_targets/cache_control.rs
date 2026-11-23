// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! `Cache-Control`, `Age`, `Vary` and `cache-offline` parsers of the cache
//! (WP-07 S1, T-02).
//!
//! The input is UTF-8 text; the first LF splits it into two field lines.
//! Invariants beyond "no panic":
//! - every delta-seconds value is at most 2^31 (RFC 9111 section 1.2.2);
//! - the order of the field lines does not matter;
//! - two lines without quotes equal one line joined with a comma (RFC 9110
//!   section 5.3);
//! - a further `no-store` line sets only `no_store`; a further `max-age=x`
//!   line sets only `invalid_freshness` and removes `max_age`;
//! - a serialized result parses to the same result (also `valid_private`);
//! - `valid_private` implies `private`;
//! - `Age` and `Vary` give the same result for the lines and the joined
//!   line; `Vary` names are sorted, unique, lower case tokens, and a `*`
//!   member gives `Any`;
//! - `cache-offline` ignores ASCII case and gives `False` for any other
//!   value.
#![no_main]

use libfuzzer_sys::fuzz_target;
use rivqen_cache::header::{
    CacheControl, CacheOffline, Vary, parse_age, parse_cache_control, parse_cache_offline,
    parse_vary,
};
use rivqen_model::http::FieldLine;

const CAP: u64 = 1 << 31;
const DELIMITERS: &[u8] = b"\"(),/:;<=>?@[\\]{}";

fn lines(name: &str, values: &[&str]) -> Vec<FieldLine> {
    values
        .iter()
        .map(|value| FieldLine::new(name, *value))
        .collect()
}

fn cc(values: &[&str]) -> CacheControl {
    parse_cache_control(&lines("cache-control", values))
}

/// One field line that gives `value` again.
fn serialize(value: &CacheControl) -> String {
    let mut members: Vec<String> = Vec::new();
    for (on, name) in [
        (value.no_store, "no-store"),
        (value.no_cache, "no-cache"),
        (value.private, "private"),
        (value.public, "public"),
        (value.must_revalidate, "must-revalidate"),
    ] {
        if on {
            members.push(name.to_owned());
        }
    }
    let mut free = None;
    for (seconds, name) in [
        (value.max_age, "max-age"),
        (value.stale_while_revalidate, "stale-while-revalidate"),
        (value.stale_if_error, "stale-if-error"),
    ] {
        match seconds {
            Some(seconds) => members.push(format!("{name}={seconds}")),
            None => free = free.or(Some(name)),
        }
    }
    // `private` without `valid_private`: only a syntax error gives it, and
    // the parser cleared `public` for it.
    let syntax_error = value.private && !value.valid_private;
    if value.invalid_freshness {
        match free {
            Some(name) if !syntax_error => members.push(format!("{name}=x")),
            // All three numbers and invalid: only a syntax error can say it,
            // and the parser cleared `public` for it.
            _ => members.push("a b".to_owned()),
        }
    }
    members.join(", ")
}

fn check_cache_control(first: &str, second: &str) {
    let parsed = cc(&[first, second]);
    for seconds in [
        parsed.max_age,
        parsed.stale_while_revalidate,
        parsed.stale_if_error,
    ]
    .into_iter()
    .flatten()
    {
        assert!(seconds <= CAP);
    }
    assert!(!parsed.valid_private || parsed.private);
    assert_eq!(cc(&[second, first]), parsed);
    if !first.contains('"') && !second.contains('"') {
        assert_eq!(cc(&[&format!("{first}, {second}")]), parsed);
    }
    assert_eq!(
        cc(&[first, second, "no-store"]),
        CacheControl {
            no_store: true,
            ..parsed
        }
    );
    assert_eq!(
        cc(&[first, second, "max-age=x"]),
        CacheControl {
            max_age: None,
            invalid_freshness: true,
            ..parsed
        }
    );
    let text = serialize(&parsed);
    assert_eq!(cc(&[&text]), parsed, "{text:?}");
}

fn check_age_and_vary(first: &str, second: &str) {
    let joined = format!("{first},{second}");
    let age = parse_age(&lines("age", &[first, second]));
    assert_eq!(parse_age(&lines("age", &[&joined])), age);
    // The first member that is only digits; members that are not valid are
    // skipped (DL-197 item 5).
    let member = joined
        .split(',')
        .map(|member| member.trim_matches([' ', '\t']))
        .find(|member| !member.is_empty() && member.bytes().all(|b| b.is_ascii_digit()));
    assert_eq!(
        member.map(|digits| digits.parse::<u64>().unwrap_or(u64::MAX)),
        age
    );

    let vary = parse_vary(&lines("vary", &[first, second]));
    assert_eq!(parse_vary(&lines("vary", &[&joined])), vary);
    let star = joined
        .split(',')
        .any(|member| member.trim_matches([' ', '\t']) == "*");
    match vary {
        Vary::Names(names) => {
            assert!(!star);
            assert!(!names.is_empty());
            assert!(names.windows(2).all(|pair| pair.first() < pair.get(1)));
            for name in &names {
                assert!(!name.is_empty());
                assert!(name.bytes().all(|b| b.is_ascii_graphic()
                    && !b.is_ascii_uppercase()
                    && !DELIMITERS.contains(&b)));
            }
        }
        Vary::None => assert!(!star),
        Vary::Any => {}
    }
}

fn check_cache_offline(text: &str) {
    let parsed = parse_cache_offline(Some(text));
    assert_eq!(
        parse_cache_offline(Some(&text.to_ascii_uppercase())),
        parsed
    );
    let expected = match text.to_ascii_lowercase().as_str() {
        "true" => CacheOffline::True,
        "store" => CacheOffline::Store,
        "http" => CacheOffline::Http,
        _ => CacheOffline::False,
    };
    assert_eq!(parsed, expected);
}

fuzz_target!(|data: &[u8]| {
    let Ok(text) = core::str::from_utf8(data) else {
        return;
    };
    let (first, second) = text.split_once('\n').unwrap_or((text, ""));
    check_cache_control(first, second);
    check_age_and_vary(first, second);
    check_cache_offline(text);
});
