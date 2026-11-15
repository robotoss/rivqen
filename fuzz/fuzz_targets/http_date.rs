// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! HTTP-date parser of the cache (WP-07 S1, T-02).
//!
//! Invariants beyond "no panic":
//! - ASCII case and the OWS at both ends do not change the result (RFC 9111
//!   section 4.2);
//! - a date is at most 9999-12-31T23:59:59Z;
//! - the IMF-fixdate and the asctime text of a parsed date parse to the same
//!   second (the formatter below has its own calendar).
#![no_main]

use libfuzzer_sys::fuzz_target;
use rivqen_cache::header::parse_http_date;
use rivqen_model::time::UnixTime;

const LAST_SECOND: u64 = 253_402_300_799;
const DAY_NAMES: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn is_leap(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

/// Year, month index, day, weekday index and second of the day, by a walk
/// over the years (slow, but independent of the parser).
fn civil(seconds: u64) -> (u64, usize, u64, usize, u64) {
    let mut days = seconds / 86_400;
    let weekday = usize::try_from(days % 7).unwrap_or_default();
    let mut year = 1970;
    loop {
        let length = if is_leap(year) { 366 } else { 365 };
        if days < length {
            break;
        }
        days -= length;
        year += 1;
    }
    let february = if is_leap(year) { 29 } else { 28 };
    let lengths = [31, february, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 0;
    for length in lengths {
        if days < length {
            break;
        }
        days -= length;
        month += 1;
    }
    (year, month, days + 1, weekday, seconds % 86_400)
}

fn formats(seconds: u64) -> [String; 2] {
    let (year, month, day, weekday, second) = civil(seconds);
    let (h, m, s) = (second / 3600, second % 3600 / 60, second % 60);
    let name = DAY_NAMES.get(weekday).copied().unwrap_or_default();
    let month = MONTHS.get(month).copied().unwrap_or_default();
    [
        format!("{name}, {day:02} {month} {year:04} {h:02}:{m:02}:{s:02} GMT"),
        format!("{name} {month} {day:>2} {h:02}:{m:02}:{s:02} {year:04}"),
    ]
}

fuzz_target!(|data: &[u8]| {
    let Ok(text) = core::str::from_utf8(data) else {
        return;
    };
    let parsed = parse_http_date(text);
    assert_eq!(parse_http_date(&text.to_ascii_uppercase()), parsed);
    assert_eq!(parse_http_date(&text.to_ascii_lowercase()), parsed);
    assert_eq!(parse_http_date(&format!(" \t{text} ")), parsed);
    if let Some(UnixTime(seconds)) = parsed {
        assert!(seconds <= LAST_SECOND);
        for form in formats(seconds) {
            assert_eq!(parse_http_date(&form), parsed, "{form}");
        }
    }
});
