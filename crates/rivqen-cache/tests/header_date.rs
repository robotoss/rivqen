// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! `parse_http_date` (WP-07 S1 T-02): RFC 9110 section 5.6.7, RFC 9111
//! section 4.2. The expected seconds were computed with the system `date`
//! tool, not with the code under test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rivqen_cache::header::parse_http_date;
use rivqen_model::time::UnixTime;

/// 1994-11-06T08:49:37Z, the example of RFC 9110 section 5.6.7.
const RFC_EXAMPLE: u64 = 784_111_777;

fn at(value: &str) -> Option<u64> {
    parse_http_date(value).map(|time: UnixTime| time.0)
}

#[test]
fn header_date_rfc9110_examples_in_the_three_forms() {
    assert_eq!(at("Sun, 06 Nov 1994 08:49:37 GMT"), Some(RFC_EXAMPLE));
    assert_eq!(at("Sunday, 06-Nov-94 08:49:37 GMT"), Some(RFC_EXAMPLE));
    assert_eq!(at("Sun Nov  6 08:49:37 1994"), Some(RFC_EXAMPLE));
    // `date1` example "02 Jun 1982".
    assert_eq!(at("Wed, 02 Jun 1982 00:00:00 GMT"), Some(391_824_000));
    // `date3` example "Jun  2".
    assert_eq!(at("Wed Jun  2 00:00:00 1982"), Some(391_824_000));
    // `date2` example "02-Jun-82".
    assert_eq!(at("Wednesday, 02-Jun-82 00:00:00 GMT"), Some(391_824_000));
}

#[test]
fn header_date_letters_match_case_insensitively() {
    // RFC 9111 section 4.2.
    for value in [
        "SUN, 06 NOV 1994 08:49:37 GMT",
        "sun, 06 nov 1994 08:49:37 gmt",
        "sUn, 06 nOv 1994 08:49:37 GmT",
        "SUNDAY, 06-NOV-94 08:49:37 GMT",
        "sunday, 06-nov-94 08:49:37 gmt",
        "SUN NOV  6 08:49:37 1994",
    ] {
        assert_eq!(at(value), Some(RFC_EXAMPLE), "{value}");
    }
}

#[test]
fn header_date_day_name_is_checked_for_range_not_agreement() {
    // 1994-11-06 was a Sunday; another name still parses.
    for name in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] {
        let value = format!("{name}, 06 Nov 1994 08:49:37 GMT");
        assert_eq!(at(&value), Some(RFC_EXAMPLE), "{value}");
        let value = format!("{name} Nov  6 08:49:37 1994");
        assert_eq!(at(&value), Some(RFC_EXAMPLE), "{value}");
    }
    for name in [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ] {
        let value = format!("{name}, 06-Nov-94 08:49:37 GMT");
        assert_eq!(at(&value), Some(RFC_EXAMPLE), "{value}");
    }
    // Not a day name, or the name of the other form.
    for value in [
        "Sux, 06 Nov 1994 08:49:37 GMT",
        "Sunday, 06 Nov 1994 08:49:37 GMT",
        "Sun, 06-Nov-94 08:49:37 GMT",
        "Sundy, 06-Nov-94 08:49:37 GMT",
        "Sundays, 06-Nov-94 08:49:37 GMT",
        "Sux Nov  6 08:49:37 1994",
        "Sunday Nov  6 08:49:37 1994",
    ] {
        assert_eq!(at(value), None, "{value}");
    }
}

#[test]
fn header_date_each_month() {
    let expected = [
        ("Jan", 1_704_067_200),
        ("Feb", 1_706_745_600),
        ("Mar", 1_709_251_200),
        ("Apr", 1_711_929_600),
        ("May", 1_714_521_600),
        ("Jun", 1_717_200_000),
        ("Jul", 1_719_792_000),
        ("Aug", 1_722_470_400),
        ("Sep", 1_725_148_800),
        ("Oct", 1_727_740_800),
        ("Nov", 1_730_419_200),
        ("Dec", 1_733_011_200),
    ];
    for (month, seconds) in expected {
        let value = format!("Mon, 01 {month} 2024 00:00:00 GMT");
        assert_eq!(at(&value), Some(seconds), "{value}");
    }
    assert_eq!(at("Mon, 01 Jux 2024 00:00:00 GMT"), None);
}

#[test]
fn header_date_day_of_month_range() {
    // The last day of each month of 2023 (not a leap year), and one more.
    let last = [
        ("Jan", 31),
        ("Feb", 28),
        ("Mar", 31),
        ("Apr", 30),
        ("May", 31),
        ("Jun", 30),
        ("Jul", 31),
        ("Aug", 31),
        ("Sep", 30),
        ("Oct", 31),
        ("Nov", 30),
        ("Dec", 31),
    ];
    for (month, days) in last {
        let ok = format!("Mon, {days:02} {month} 2023 00:00:00 GMT");
        assert!(at(&ok).is_some(), "{ok}");
        let over = format!("Mon, {:02} {month} 2023 00:00:00 GMT", days + 1);
        assert_eq!(at(&over), None, "{over}");
    }
    assert_eq!(at("Mon, 00 Jan 2023 00:00:00 GMT"), None);
    assert_eq!(at("Sun, 01 Jan 2023 00:00:00 GMT"), Some(1_672_531_200));
    assert_eq!(at("Mon Jan  0 00:00:00 2023"), None);
}

#[test]
fn header_date_leap_years() {
    assert_eq!(at("Thu, 29 Feb 2024 00:00:00 GMT"), Some(1_709_164_800));
    assert_eq!(at("Fri, 01 Mar 2024 00:00:00 GMT"), Some(1_709_251_200));
    assert_eq!(at("Tue, 29 Feb 2000 12:00:00 GMT"), Some(951_825_600));
    assert_eq!(at("Wed, 29 Feb 2023 00:00:00 GMT"), None);
    // 2100 is not a leap year (divisible by 100, not by 400).
    assert_eq!(at("Sun, 28 Feb 2100 00:00:00 GMT"), Some(4_107_456_000));
    assert_eq!(at("Mon, 29 Feb 2100 00:00:00 GMT"), None);
    assert_eq!(at("Mon, 01 Mar 2100 00:00:00 GMT"), Some(4_107_542_400));
    assert_eq!(at("Sun Feb 29 00:00:00 2024"), Some(1_709_164_800));
    assert_eq!(at("Sun Feb 29 00:00:00 2023"), None);
}

#[test]
fn header_date_time_of_day_range() {
    assert_eq!(at("Thu, 01 Jan 1970 23:59:59 GMT"), Some(86_399));
    assert_eq!(at("Thu, 01 Jan 1970 24:00:00 GMT"), None);
    assert_eq!(at("Thu, 01 Jan 1970 00:60:00 GMT"), None);
    assert_eq!(at("Thu, 01 Jan 1970 12:00:60 GMT"), None);
    assert_eq!(at("Thu, 01 Jan 1970 00:00:61 GMT"), None);
    assert_eq!(at("Thu, 01 Jan 1970 99:99:99 GMT"), None);
}

#[test]
fn header_date_leap_second_is_the_second_before() {
    // RFC 9110 section 5.6.7 allows 23:59:60; RFC 9111 section 4.2: the
    // nearest time equal to or earlier than the value.
    assert_eq!(at("Sat, 31 Dec 2016 23:59:60 GMT"), Some(1_483_228_799));
    assert_eq!(at("Sat, 31 Dec 2016 23:59:59 GMT"), Some(1_483_228_799));
    assert_eq!(at("Sat Dec 31 23:59:60 2016"), Some(1_483_228_799));
    assert_eq!(at("Sat, 31 Dec 2016 23:58:60 GMT"), None);
    assert_eq!(at("Sat, 31 Dec 2016 22:59:60 GMT"), None);
}

#[test]
fn header_date_range_of_years() {
    assert_eq!(at("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
    assert_eq!(at("Wed, 31 Dec 1969 23:59:59 GMT"), None);
    assert_eq!(at("Mon, 01 Jan 0001 00:00:00 GMT"), None);
    assert_eq!(at("Mon, 01 Jan 0000 00:00:00 GMT"), None);
    assert_eq!(at("Fri, 31 Dec 9999 23:59:59 GMT"), Some(253_402_300_799));
    assert_eq!(at("Fri Dec 31 23:59:59 9999"), Some(253_402_300_799));
    assert_eq!(at("Wed Dec 31 23:59:59 1969"), None);
    // The second 2^31 (no 32-bit limit).
    assert_eq!(at("Tue, 19 Jan 2038 03:14:08 GMT"), Some(2_147_483_648));
}

#[test]
fn header_date_rfc850_two_digit_year() {
    // RFC 9110 section 5.6.7: a year more than 50 years after the reference
    // (2026-01-01T00:00:00Z) is the year 100 years earlier.
    assert_eq!(at("Thursday, 01-Jan-26 00:00:00 GMT"), Some(1_767_225_600));
    assert_eq!(at("Saturday, 01-Jan-00 00:00:00 GMT"), Some(946_684_800));
    assert_eq!(at("Friday, 31-Dec-99 23:59:59 GMT"), Some(946_684_799));
    assert_eq!(at("Tuesday, 31-Dec-75 23:59:59 GMT"), Some(3_345_062_399));
    // 2076-01-01T00:00:00Z is exactly 50 years after: still 2076.
    assert_eq!(at("Wednesday, 01-Jan-76 00:00:00 GMT"), Some(3_345_062_400));
    // One second later: 1976.
    assert_eq!(at("Thursday, 01-Jan-76 00:00:01 GMT"), Some(189_302_401));
    assert_eq!(at("Wednesday, 15-Jun-77 10:20:30 GMT"), Some(235_218_030));
    // "70" is 2070 (44 years after the reference), not 1970.
    assert_eq!(at("Wednesday, 01-Jan-70 00:00:00 GMT"), Some(3_155_760_000));
}

#[test]
fn header_date_rfc850_needs_two_digits_and_dashes() {
    for value in [
        "Sunday, 06-Nov-1994 08:49:37 GMT",
        "Sunday, 06-Nov-4 08:49:37 GMT",
        "Sunday, 6-Nov-94 08:49:37 GMT",
        "Sunday, 06 Nov 94 08:49:37 GMT",
        "Sunday, 06-Nov-94 08:49:37",
        "Sunday,06-Nov-94 08:49:37 GMT",
    ] {
        assert_eq!(at(value), None, "{value}");
    }
}

#[test]
fn header_date_asctime_day_forms() {
    assert_eq!(at("Sun Nov  6 08:49:37 1994"), Some(RFC_EXAMPLE));
    assert_eq!(at("Sun Nov 06 08:49:37 1994"), Some(RFC_EXAMPLE));
    for value in [
        "Sun Nov 6 08:49:37 1994",
        "Sun Nov   6 08:49:37 1994",
        "Sun Nov  06 08:49:37 1994",
        "Sun Nov  6 08:49:37 94",
        "Sun Nov  6 08:49:37 1994 GMT",
        "Sun, Nov  6 08:49:37 1994",
    ] {
        assert_eq!(at(value), None, "{value}");
    }
}

#[test]
fn header_date_zone_must_be_gmt() {
    // RFC 9111 section 4.2: another zone is not valid for expiration.
    for zone in ["UTC", "UT", "Z", "+0000", "EST", "GMT+1", "GM", "GMTX"] {
        let value = format!("Sun, 06 Nov 1994 08:49:37 {zone}");
        assert_eq!(at(&value), None, "{value}");
    }
}

#[test]
fn header_date_white_space() {
    // OWS at both ends is removed.
    assert_eq!(at(" \tSun, 06 Nov 1994 08:49:37 GMT\t "), Some(RFC_EXAMPLE));
    for value in [
        "Sun,  06 Nov 1994 08:49:37 GMT",
        "Sun, 06  Nov 1994 08:49:37 GMT",
        "Sun, 06 Nov 1994  08:49:37 GMT",
        "Sun, 06 Nov 1994 08:49:37  GMT",
        "Sun,\t06 Nov 1994 08:49:37 GMT",
        "Sun, 06 Nov 1994 08:49:37\tGMT",
        "Sun, 06 Nov 1994 08 :49:37 GMT",
        "Sun,06 Nov 1994 08:49:37 GMT",
        "\u{a0}Sun, 06 Nov 1994 08:49:37 GMT",
        "Sun, 06 Nov 1994 08:49:37 GMT\n",
    ] {
        assert_eq!(at(value), None, "{value:?}");
    }
}

#[test]
fn header_date_digits_are_exact_ascii() {
    for value in [
        "Sun, 6 Nov 1994 08:49:37 GMT",
        "Sun, 006 Nov 1994 08:49:37 GMT",
        "Sun, 06 Nov 994 08:49:37 GMT",
        "Sun, 06 Nov 19940 08:49:37 GMT",
        "Sun, 06 Nov 1994 8:49:37 GMT",
        "Sun, 06 Nov 1994 08:49:7 GMT",
        "Sun, \u{FF10}6 Nov 1994 08:49:37 GMT",
        "Sun, +6 Nov 1994 08:49:37 GMT",
        "Sun, 06 Nov 19a4 08:49:37 GMT",
    ] {
        assert_eq!(at(value), None, "{value}");
    }
}

#[test]
fn header_date_text_that_is_not_a_date() {
    // `Expires: 0` (RFC 9111 section 5.3) and other text.
    for value in [
        "",
        " ",
        "0",
        "-1",
        "Sun",
        "Sun,",
        "Sun, 06 Nov 1994",
        "Sun, 06 Nov 1994 08:49:37 GMT x",
        "1994-11-06T08:49:37Z",
        "784111777",
        "\u{e9}\u{e9}\u{e9}, 06 Nov 1994 08:49:37 GMT",
    ] {
        assert_eq!(at(value), None, "{value}");
    }
}

/// The window of the two-digit year is closed at exactly 50 years: a date
/// 50 years after the reference to the second is not "more than 50 years in
/// the future" (RFC 9110 section 5.6.7; reference 2026-01-01T00:00:00Z,
/// DL-197 item 8).
#[test]
fn header_date_rfc850_year_exactly_50_years_ahead_stays() {
    assert_eq!(
        parse_http_date("Thursday, 01-Jan-76 00:00:00 GMT"),
        Some(UnixTime(3_345_062_400))
    );
    // One second later is more than 50 years ahead: the earlier century.
    assert_eq!(
        parse_http_date("Thursday, 01-Jan-76 00:00:01 GMT"),
        Some(UnixTime(189_302_401))
    );
}
