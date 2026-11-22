// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Properties of the field parsers (WP-07 S1 T-02): no panic on any text,
//! round trips, the combination of field lines (RFC 9110 section 5.3) and
//! the direction of every change (a further directive is never less
//! strict).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::struct_excessive_bools
)]

use proptest::prelude::*;
use rivqen_cache::header::{
    CacheControl, Vary, parse_age, parse_cache_control, parse_http_date, parse_vary,
};
use rivqen_model::http::FieldLine;
use rivqen_model::time::UnixTime;

/// 2^31 (RFC 9111 section 1.2.2).
const CAP: u64 = 1 << 31;
/// The delimiters of RFC 9110 section 5.6.2.
const DELIMITERS: &[u8] = b"\"(),/:;<=>?@[\\]{}";
/// 9999-12-31T23:59:59Z.
const LAST_SECOND: u64 = 253_402_300_799;

fn cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(512)
}

fn lines(name: &str, values: &[String]) -> Vec<FieldLine> {
    values
        .iter()
        .map(|value| FieldLine::new(name, value.as_str()))
        .collect()
}

fn cc(values: &[String]) -> CacheControl {
    parse_cache_control(&lines("cache-control", values))
}

/// Text made of the pieces that the parsers react to.
fn field_text() -> impl Strategy<Value = String> {
    let pieces = prop::sample::select(vec![
        ",",
        ";",
        "=",
        "\"",
        "\\",
        " ",
        "\t",
        "*",
        "0",
        "1",
        "9",
        "a",
        "Z",
        "-",
        "\u{e9}",
        "\u{1}",
        "\u{7f}",
        "no-store",
        "no-cache",
        "private",
        "public",
        "must-revalidate",
        "max-age",
        "stale-while-revalidate",
        "stale-if-error",
        "s-maxage",
        "2147483648",
        "99999999999999999999",
        "GMT",
        "Sun",
        "Nov",
        "1994",
        "06",
        ":",
    ]);
    prop::collection::vec(pieces, 0..48).prop_map(|parts| parts.concat())
}

/// Any text, or text of the pieces above.
fn any_text() -> impl Strategy<Value = String> {
    prop_oneof![field_text(), any::<String>()]
}

// ---------------------------------------------------------------------------
// A serializer of `CacheControl` with random spelling.

#[derive(Debug, Clone)]
struct Style {
    upper: bool,
    quoted_numbers: bool,
    leading_zeros: bool,
    split_lines: bool,
    noise: bool,
}

fn style() -> impl Strategy<Value = Style> {
    (
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(
            |(upper, quoted_numbers, leading_zeros, split_lines, noise)| Style {
                upper,
                quoted_numbers,
                leading_zeros,
                split_lines,
                noise,
            },
        )
}

/// A `CacheControl` that a field can express. A syntax error clears
/// `public` and `valid_private` and sets `invalid_freshness`. Two values
/// need one: an invalid freshness with all three numbers, and `private`
/// without `valid_private`.
fn cache_control() -> impl Strategy<Value = CacheControl> {
    let number = prop_oneof![Just(None), (0..=CAP).prop_map(Some)];
    (
        (
            any::<bool>(),
            any::<bool>(),
            any::<bool>(),
            any::<bool>(),
            any::<bool>(),
            any::<bool>(),
        ),
        number.clone(),
        number.clone(),
        number,
        any::<bool>(),
    )
        .prop_map(
            |(
                (no_store, no_cache, private, consent, public, must_revalidate),
                max_age,
                swr,
                sie,
                invalid,
            )| {
                let all_numbers = max_age.is_some() && swr.is_some() && sie.is_some();
                let syntax_error = (invalid && all_numbers) || (private && !consent);
                CacheControl {
                    no_store,
                    no_cache,
                    private,
                    valid_private: private && !syntax_error,
                    public: public && !syntax_error,
                    must_revalidate,
                    max_age,
                    stale_while_revalidate: swr,
                    stale_if_error: sie,
                    invalid_freshness: invalid || syntax_error,
                }
            },
        )
}

fn serialize(value: &CacheControl, style: &Style) -> Vec<String> {
    let name = |text: &str| {
        if style.upper {
            text.to_ascii_uppercase()
        } else {
            text.to_owned()
        }
    };
    let number = |seconds: u64| {
        let digits = if style.leading_zeros {
            format!("00{seconds}")
        } else {
            seconds.to_string()
        };
        if style.quoted_numbers {
            format!("\"{digits}\"")
        } else {
            digits
        }
    };
    let mut members = Vec::new();
    if style.noise {
        members.push("community=\"a, b\"".to_owned());
        members.push(String::new());
        members.push("s-maxage=5".to_owned());
    }
    for (on, directive) in [
        (value.no_store, "no-store"),
        (value.no_cache, "no-cache"),
        (value.private, "private"),
        (value.public, "public"),
        (value.must_revalidate, "must-revalidate"),
    ] {
        if on {
            members.push(name(directive));
        }
    }
    let mut free_slot = None;
    for (seconds, directive) in [
        (value.max_age, "max-age"),
        (value.stale_while_revalidate, "stale-while-revalidate"),
        (value.stale_if_error, "stale-if-error"),
    ] {
        match seconds {
            Some(seconds) => members.push(format!("{}={}", name(directive), number(seconds))),
            None => free_slot = free_slot.or(Some(directive)),
        }
    }
    // `private` that is no consent: only a syntax error gives it.
    let syntax_error = value.private && !value.valid_private;
    if value.invalid_freshness {
        match free_slot {
            Some(directive) if !syntax_error => {
                members.push(format!("{}=x", name(directive)));
            }
            _ => members.push("bad member".to_owned()),
        }
    }
    if style.split_lines {
        members
    } else {
        vec![members.join(", ")]
    }
}

// ---------------------------------------------------------------------------
// A formatter of HTTP-dates, from the civil calendar of the test.

const DAY_NAMES: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
const DAY_NAMES_LONG: [&str; 7] = [
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

struct Civil {
    year: u64,
    month: usize,
    day: u64,
    hour: u64,
    minute: u64,
    second: u64,
    weekday: usize,
}

fn is_leap(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

/// Walks years and months: slow, but independent of the code under test.
fn civil(seconds: u64) -> Civil {
    let mut days = seconds / 86_400;
    let weekday = (days % 7) as usize;
    let mut rest = seconds % 86_400;
    let mut year = 1970;
    loop {
        let length = if is_leap(year) { 366 } else { 365 };
        if days < length {
            break;
        }
        days -= length;
        year += 1;
    }
    let lengths = [
        31,
        if is_leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 0;
    while days >= lengths[month] {
        days -= lengths[month];
        month += 1;
    }
    let hour = rest / 3600;
    rest %= 3600;
    Civil {
        year,
        month,
        day: days + 1,
        hour,
        minute: rest / 60,
        second: rest % 60,
        weekday,
    }
}

fn imf_fixdate(seconds: u64) -> String {
    let c = civil(seconds);
    format!(
        "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
        DAY_NAMES[c.weekday], c.day, MONTHS[c.month], c.year, c.hour, c.minute, c.second
    )
}

fn asctime(seconds: u64) -> String {
    let c = civil(seconds);
    format!(
        "{} {} {:>2} {:02}:{:02}:{:02} {:04}",
        DAY_NAMES[c.weekday], MONTHS[c.month], c.day, c.hour, c.minute, c.second, c.year
    )
}

fn rfc850(seconds: u64) -> String {
    let c = civil(seconds);
    format!(
        "{}, {:02}-{}-{:02} {:02}:{:02}:{:02} GMT",
        DAY_NAMES_LONG[c.weekday],
        c.day,
        MONTHS[c.month],
        c.year % 100,
        c.hour,
        c.minute,
        c.second
    )
}

#[test]
fn header_prop_formatter_matches_the_rfc_example() {
    // The formatter of this file against RFC 9110 section 5.6.7.
    assert_eq!(imf_fixdate(784_111_777), "Sun, 06 Nov 1994 08:49:37 GMT");
    assert_eq!(asctime(784_111_777), "Sun Nov  6 08:49:37 1994");
    assert_eq!(rfc850(784_111_777), "Sunday, 06-Nov-94 08:49:37 GMT");
    assert_eq!(imf_fixdate(LAST_SECOND), "Fri, 31 Dec 9999 23:59:59 GMT");
}

proptest! {
    #![proptest_config(ProptestConfig { cases: cases(), ..ProptestConfig::default() })]

    #[test]
    fn header_prop_cache_control_any_text(values in prop::collection::vec(any_text(), 0..4)) {
        let parsed = cc(&values);
        for seconds in [parsed.max_age, parsed.stale_while_revalidate, parsed.stale_if_error]
            .into_iter()
            .flatten()
        {
            prop_assert!(seconds <= CAP);
        }
        // The permission implies the restrictive flag.
        prop_assert!(!parsed.valid_private || parsed.private);
        // The order of the field lines does not matter.
        let mut reversed = values.clone();
        reversed.reverse();
        prop_assert_eq!(cc(&reversed), parsed);
    }

    #[test]
    fn header_prop_cache_control_round_trip(value in cache_control(), style in style()) {
        let text = serialize(&value, &style);
        prop_assert_eq!(cc(&text), value, "{:?}", text);
    }

    #[test]
    fn header_prop_cache_control_lines_combine(first in field_text(), second in field_text()) {
        // RFC 9110 section 5.3: two lines are one line joined with a comma.
        // Quotes can open a string on one line and close it on the other,
        // so the property holds for lines without quotes.
        let first = first.replace('"', "");
        let second = second.replace('"', "");
        let joined = format!("{first}, {second}");
        prop_assert_eq!(cc(&[first, second]), cc(&[joined]));
    }

    #[test]
    fn header_prop_cache_control_a_further_line_is_never_less_strict(
        values in prop::collection::vec(any_text(), 0..3),
    ) {
        let base = cc(&values);
        let mut with = values.clone();
        with.push("no-store".to_owned());
        prop_assert_eq!(cc(&with), CacheControl { no_store: true, ..base });

        let mut with = values.clone();
        with.push("max-age=x".to_owned());
        prop_assert_eq!(
            cc(&with),
            CacheControl { max_age: None, invalid_freshness: true, ..base }
        );

        let mut with = values.clone();
        with.push("a b".to_owned());
        let broken = cc(&with);
        prop_assert!(broken.invalid_freshness);
        prop_assert!(!broken.public);
        // A syntax error on any line is no consent (H-150, DL-207).
        prop_assert!(!broken.valid_private);
        prop_assert_eq!(broken.max_age, base.max_age);
        prop_assert_eq!(broken.no_store, base.no_store);
        prop_assert_eq!(broken.private, base.private);

        let mut with = values;
        with.push("max-age=5".to_owned());
        let longer = cc(&with);
        // A further max-age never gives a number where there was an error.
        if base.invalid_freshness {
            prop_assert!(longer.invalid_freshness);
        }
        prop_assert!(longer.max_age.is_none() || longer.max_age == Some(5));
    }

    #[test]
    fn header_prop_date_imf_fixdate_every_second(seconds in 0..=LAST_SECOND) {
        let text = imf_fixdate(seconds);
        prop_assert_eq!(parse_http_date(&text), Some(UnixTime(seconds)), "{}", text);
        prop_assert_eq!(
            parse_http_date(&text.to_ascii_lowercase()),
            Some(UnixTime(seconds))
        );
        let text = asctime(seconds);
        prop_assert_eq!(parse_http_date(&text), Some(UnixTime(seconds)), "{}", text);
    }

    #[test]
    fn header_prop_date_rfc850_inside_the_window(
        // 1976-01-01T00:00:01Z to 2076-01-01T00:00:00Z: the century window of
        // the reference 2026-01-01T00:00:00Z.
        seconds in 189_302_401_u64..=3_345_062_400,
    ) {
        let text = rfc850(seconds);
        prop_assert_eq!(parse_http_date(&text), Some(UnixTime(seconds)), "{}", text);
    }

    #[test]
    fn header_prop_date_any_text(text in any_text()) {
        let parsed = parse_http_date(&text);
        prop_assert_eq!(parse_http_date(&text.to_ascii_uppercase()), parsed);
        prop_assert_eq!(parse_http_date(&format!(" {text}\t")), parsed);
        if let Some(UnixTime(seconds)) = parsed {
            prop_assert!(seconds <= LAST_SECOND);
            prop_assert_eq!(parse_http_date(&imf_fixdate(seconds)), parsed);
        }
    }

    #[test]
    fn header_prop_age_and_vary_any_text(values in prop::collection::vec(any_text(), 0..4)) {
        let age = parse_age(&lines("age", &values));
        let joined = values.join(",");
        prop_assert_eq!(parse_age(&lines("age", std::slice::from_ref(&joined))), age);
        // The first member that is only digits; members that are not valid
        // are skipped (DL-197 item 5).
        let first = joined
            .split(',')
            .map(|member| member.trim_matches([' ', '\t']))
            .find(|member| !member.is_empty() && member.bytes().all(|b| b.is_ascii_digit()));
        // Only digits: a parse error is an overflow, which saturates.
        prop_assert_eq!(first.map(|digits| digits.parse::<u64>().unwrap_or(u64::MAX)), age);

        let vary = parse_vary(&lines("vary", &values));
        prop_assert_eq!(parse_vary(&lines("vary", std::slice::from_ref(&joined))), vary.clone());
        let star = joined.split(',').any(|member| member.trim_matches([' ', '\t']) == "*");
        match vary {
            Vary::Names(names) => {
                prop_assert!(!star);
                prop_assert!(!names.is_empty());
                prop_assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
                for name in &names {
                    prop_assert!(!name.is_empty());
                    prop_assert!(!name.bytes().any(|b| b.is_ascii_uppercase()));
                    let token = name.bytes().all(|b| b.is_ascii_graphic() && !DELIMITERS.contains(&b));
                    prop_assert!(token, "not a token: {:?}", name);
                }
            }
            Vary::None => prop_assert!(!star),
            Vary::Any => {}
        }
    }
}

#[cfg(feature = "legacy")]
mod legacy {
    use super::{any_text, cases};
    use proptest::prelude::*;
    use rivqen_cache::header::{CacheOffline, parse_cache_offline};

    proptest! {
        #![proptest_config(ProptestConfig { cases: cases(), ..ProptestConfig::default() })]

        #[test]
        fn header_prop_cache_offline_any_text(text in any_text()) {
            let parsed = parse_cache_offline(Some(&text));
            prop_assert_eq!(parse_cache_offline(Some(&text.to_ascii_uppercase())), parsed);
            let expected = match text.to_ascii_lowercase().as_str() {
                "true" => CacheOffline::True,
                "store" => CacheOffline::Store,
                "http" => CacheOffline::Http,
                _ => CacheOffline::False,
            };
            prop_assert_eq!(parsed, expected);
        }

        #[test]
        fn header_prop_cache_offline_any_case(
            word in prop::sample::select(vec!["true", "false", "store", "http"]),
            mask in any::<u8>(),
        ) {
            let text: String = word
                .chars()
                .enumerate()
                .map(|(i, c)| if mask & (1 << (i % 8)) == 0 { c } else { c.to_ascii_uppercase() })
                .collect();
            let expected = match word {
                "true" => CacheOffline::True,
                "store" => CacheOffline::Store,
                "http" => CacheOffline::Http,
                _ => CacheOffline::False,
            };
            prop_assert_eq!(parse_cache_offline(Some(&text)), expected);
        }
    }
}
