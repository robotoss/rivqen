// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! HTTP-date (RFC 9110 section 5.6.7) with the cache rules of RFC 9111
//! section 4.2.
//!
//! - The three forms: IMF-fixdate, the RFC 850 form, the asctime form. The
//!   grammar is exact: one SP where it has SP, no other white space (only
//!   the OWS at both ends of the value is removed).
//! - Letters match ASCII case-insensitively (RFC 9111 section 4.2), also
//!   `GMT`; any other zone is not valid.
//! - The day name must be one of the seven names; it is not checked against
//!   the date. The day of the month must exist in that month and year.
//! - `23:59:60` (a leap second) is the second `23:59:59`: the nearest time
//!   equal to or earlier than the value (RFC 9111 section 4.2). Second 60 at
//!   any other time is not valid.
//! - Years 1970 to 9999. A date before 1970 is not valid.
//!
//! Work: the trim reads the value once; then the parse reads at most 33 bytes
//! (`Wednesday, 06-Nov-94 08:49:37 GMT`) and does a fixed number of steps.

use rivqen_model::time::UnixTime;

use super::lex::trim_ows;

const DAY_NAMES: [&[u8]; 7] = [b"mon", b"tue", b"wed", b"thu", b"fri", b"sat", b"sun"];

const DAY_NAMES_LONG: [&[u8]; 7] = [
    b"monday",
    b"tuesday",
    b"wednesday",
    b"thursday",
    b"friday",
    b"saturday",
    b"sunday",
];

const MONTHS: [&[u8]; 12] = [
    b"jan", b"feb", b"mar", b"apr", b"may", b"jun", b"jul", b"aug", b"sep", b"oct", b"nov", b"dec",
];

/// Days before the first day of each month in a year that is not a leap
/// year.
const DAYS_BEFORE_MONTH: [u64; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];

const SECONDS_PER_DAY: u64 = 86_400;

/// The first and the last year of the range.
const FIRST_YEAR: u64 = 1970;
const LAST_YEAR: u64 = 9999;

/// The point in time from which the RFC 850 form finds the century of a
/// two-digit year: 2026-01-01T00:00:00Z. The contract function has no
/// current time; see [`parse_with_reference`].
pub(super) const RFC850_REFERENCE: UnixTime = UnixTime(1_767_225_600);

/// A date and a time of day as read, before the range checks.
#[derive(Debug, Clone, Copy)]
struct Parts {
    year: u64,
    month: u64,
    day: u64,
    hour: u64,
    minute: u64,
    second: u64,
}

/// A reader over the bytes of the value.
struct Cursor<'a> {
    rest: &'a [u8],
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let (head, tail) = self.rest.split_at_checked(n)?;
        self.rest = tail;
        Some(head)
    }

    /// One byte, ASCII case-insensitively.
    fn byte(&mut self, expected: u8) -> Option<()> {
        let (&first, tail) = self.rest.split_first()?;
        if first.eq_ignore_ascii_case(&expected) {
            self.rest = tail;
            Some(())
        } else {
            None
        }
    }

    /// Exactly `n` ASCII digits.
    fn digits(&mut self, n: usize) -> Option<u64> {
        self.take(n)?.iter().try_fold(0_u64, |acc, &b| {
            if b.is_ascii_digit() {
                acc.checked_mul(10)?
                    .checked_add(u64::from(b.wrapping_sub(b'0')))
            } else {
                None
            }
        })
    }

    /// One of `names`, all of length `len`; its index.
    fn one_of(&mut self, names: &[&[u8]], len: usize) -> Option<usize> {
        let word = self.take(len)?;
        names
            .iter()
            .position(|name| name.eq_ignore_ascii_case(word))
    }

    /// A long day name of the RFC 850 form.
    fn long_day_name(&mut self) -> Option<()> {
        for name in DAY_NAMES_LONG {
            if let Some((head, tail)) = self.rest.split_at_checked(name.len())
                && head.eq_ignore_ascii_case(name)
            {
                self.rest = tail;
                return Some(());
            }
        }
        None
    }

    /// A month name: 1 to 12.
    fn month(&mut self) -> Option<u64> {
        let index = self.one_of(&MONTHS, 3)?;
        u64::try_from(index).ok()?.checked_add(1)
    }

    /// `hour ":" minute ":" second`.
    fn time_of_day(&mut self) -> Option<(u64, u64, u64)> {
        let hour = self.digits(2)?;
        self.byte(b':')?;
        let minute = self.digits(2)?;
        self.byte(b':')?;
        let second = self.digits(2)?;
        Some((hour, minute, second))
    }

    fn gmt(&mut self) -> Option<()> {
        self.byte(b'g')?;
        self.byte(b'm')?;
        self.byte(b't')
    }

    fn end(&self) -> Option<()> {
        self.rest.is_empty().then_some(())
    }
}

/// True for a leap year of the Gregorian calendar.
const fn is_leap(year: u64) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

/// The number of days of `month` (1 to 12) in `year`.
const fn days_in_month(year: u64, month: u64) -> u64 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Leap years from year 1 to `year`, both included.
fn leap_years_through(year: u64) -> u64 {
    // year / 4 >= year / 100 >= year / 400, so no step underflows.
    (year / 4)
        .saturating_sub(year / 100)
        .saturating_add(year / 400)
}

/// Seconds from 1970-01-01T00:00:00Z to the given time. The day and the
/// time are not checked for range: a value over the range counts on (day 29
/// of February in a year that is not a leap year is 1 March). `None` before
/// 1970 or on an overflow.
fn seconds_unchecked(year: u64, month: u64, day: u64, second_of_day: u64) -> Option<u64> {
    let years = year.checked_sub(FIRST_YEAR)?;
    let leap_days = leap_years_through(year.checked_sub(1)?)
        .checked_sub(leap_years_through(FIRST_YEAR.checked_sub(1)?))?;
    let month_index = usize::try_from(month.checked_sub(1)?).ok()?;
    let mut days = years
        .checked_mul(365)?
        .checked_add(leap_days)?
        .checked_add(*DAYS_BEFORE_MONTH.get(month_index)?)?
        .checked_add(day.checked_sub(1)?)?;
    if month > 2 && is_leap(year) {
        days = days.checked_add(1)?;
    }
    days.checked_mul(SECONDS_PER_DAY)?
        .checked_add(second_of_day)
}

/// The second of the day; `None` when the time is out of range. A leap
/// second (only `23:59:60`) is `23:59:59`.
fn second_of_day(hour: u64, minute: u64, second: u64) -> Option<u64> {
    let second = match (hour, minute, second) {
        (23, 59, 60) => 59,
        (0..=23, 0..=59, 0..=59) => second,
        _ => return None,
    };
    hour.checked_mul(3600)?
        .checked_add(minute.checked_mul(60)?)?
        .checked_add(second)
}

/// The checked time of `parts`.
fn to_unix(parts: Parts) -> Option<UnixTime> {
    if !(FIRST_YEAR..=LAST_YEAR).contains(&parts.year)
        || !(1..=12).contains(&parts.month)
        || parts.day == 0
        || parts.day > days_in_month(parts.year, parts.month)
    {
        return None;
    }
    let second = second_of_day(parts.hour, parts.minute, parts.second)?;
    seconds_unchecked(parts.year, parts.month, parts.day, second).map(UnixTime)
}

/// The average length of a Gregorian year in seconds (365.2425 days).
const AVERAGE_YEAR_SECONDS: u64 = 31_556_952;

/// The year of a two-digit RFC 850 year (RFC 9110 section 5.6.7): the latest
/// year with those last two digits whose time is not more than 50 years
/// after `reference`. "50 years after" is calendar years: the time is more
/// than 50 years after `reference` when the same date and time 50 years
/// earlier is after `reference`.
fn rfc850_year(two_digits: u64, parts: Parts, reference: UnixTime) -> Option<u64> {
    let second = second_of_day(parts.hour, parts.minute, parts.second)?;
    let more_than_50_years_ahead = |year: u64| -> bool {
        // A year before 2020 is less than 50 years after any reference,
        // which is never before 1970.
        year.checked_sub(50)
            .filter(|&earlier| earlier >= FIRST_YEAR)
            .and_then(|earlier| seconds_unchecked(earlier, parts.month, parts.day, second))
            .is_some_and(|then| then > reference.0)
    };
    // The year of the reference, within one year: the average year differs
    // from the calendar by less than one day over this range.
    let reference_year = FIRST_YEAR.checked_add(reference.0 / AVERAGE_YEAR_SECONDS)?;
    // The latest year with these digits not after reference_year + 52, which
    // is later than any answer; then at most one step of 100 years back.
    let bound = reference_year.checked_add(52)?;
    let candidate = (bound / 100).checked_mul(100)?.checked_add(two_digits)?;
    let candidate = if candidate > bound {
        candidate.checked_sub(100)?
    } else {
        candidate
    };
    if more_than_50_years_ahead(candidate) {
        candidate.checked_sub(100)
    } else {
        Some(candidate)
    }
}

/// IMF-fixdate: `Sun, 06 Nov 1994 08:49:37 GMT`.
fn imf_fixdate(mut c: Cursor<'_>) -> Option<UnixTime> {
    c.one_of(&DAY_NAMES, 3)?;
    c.byte(b',')?;
    c.byte(b' ')?;
    let day = c.digits(2)?;
    c.byte(b' ')?;
    let month = c.month()?;
    c.byte(b' ')?;
    let year = c.digits(4)?;
    c.byte(b' ')?;
    let (hour, minute, second) = c.time_of_day()?;
    c.byte(b' ')?;
    c.gmt()?;
    c.end()?;
    to_unix(Parts {
        year,
        month,
        day,
        hour,
        minute,
        second,
    })
}

/// The RFC 850 form: `Sunday, 06-Nov-94 08:49:37 GMT`.
fn rfc850(mut c: Cursor<'_>, reference: UnixTime) -> Option<UnixTime> {
    c.long_day_name()?;
    c.byte(b',')?;
    c.byte(b' ')?;
    let day = c.digits(2)?;
    c.byte(b'-')?;
    let month = c.month()?;
    c.byte(b'-')?;
    let two_digits = c.digits(2)?;
    c.byte(b' ')?;
    let (hour, minute, second) = c.time_of_day()?;
    c.byte(b' ')?;
    c.gmt()?;
    c.end()?;
    let mut parts = Parts {
        year: 0,
        month,
        day,
        hour,
        minute,
        second,
    };
    parts.year = rfc850_year(two_digits, parts, reference)?;
    to_unix(parts)
}

/// The asctime form: `Sun Nov  6 08:49:37 1994`.
fn asctime(mut c: Cursor<'_>) -> Option<UnixTime> {
    c.one_of(&DAY_NAMES, 3)?;
    c.byte(b' ')?;
    let month = c.month()?;
    c.byte(b' ')?;
    let day = if c.byte(b' ').is_some() {
        c.digits(1)?
    } else {
        c.digits(2)?
    };
    c.byte(b' ')?;
    let (hour, minute, second) = c.time_of_day()?;
    c.byte(b' ')?;
    let year = c.digits(4)?;
    c.end()?;
    to_unix(Parts {
        year,
        month,
        day,
        hour,
        minute,
        second,
    })
}

/// Reads an HTTP-date; the RFC 850 form finds its century from `reference`.
pub(super) fn parse_with_reference(value: &str, reference: UnixTime) -> Option<UnixTime> {
    let bytes = trim_ows(value).as_bytes();
    let cursor = Cursor { rest: bytes };
    // The fourth byte tells the form: `Sun,` / `Sun ` / `Sunday,`.
    match bytes.get(3) {
        Some(b',') => imf_fixdate(cursor),
        Some(b' ') => asctime(cursor),
        Some(_) => rfc850(cursor, reference),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::arithmetic_side_effects)]

    use super::*;

    /// 2076-01-01T00:00:00Z.
    const Y2076: u64 = 3_345_062_400;

    fn rfc850_at(value: &str, reference: u64) -> Option<u64> {
        parse_with_reference(value, UnixTime(reference)).map(|t| t.0)
    }

    #[test]
    fn header_date_rfc850_window_of_the_constant_reference() {
        // 2076-01-01 is exactly 50 years after the reference: not more.
        assert_eq!(
            rfc850_at("Wednesday, 01-Jan-76 00:00:00 GMT", RFC850_REFERENCE.0),
            Some(Y2076)
        );
        // One second later is more than 50 years ahead: 1976.
        assert_eq!(
            rfc850_at("Thursday, 01-Jan-76 00:00:01 GMT", RFC850_REFERENCE.0),
            Some(189_302_401)
        );
        assert_eq!(
            rfc850_at("Monday, 31-Dec-75 23:59:59 GMT", RFC850_REFERENCE.0),
            Some(Y2076 - 1)
        );
    }

    #[test]
    fn header_date_rfc850_window_moves_with_the_reference() {
        // Reference 2060-06-01T00:00:00Z: "05" is 2105 (45 years ahead), "11"
        // is 2011 (2111 is 51 years ahead).
        let reference = 2_853_273_600;
        assert_eq!(
            rfc850_at("Monday, 01-Jun-05 00:00:00 GMT", reference),
            Some(4_273_257_600)
        );
        assert_eq!(
            rfc850_at("Monday, 01-Jun-11 00:00:00 GMT", reference),
            Some(1_306_886_400)
        );
        // Reference 1970-01-01: "19" is 2019, "20" is 2020 (exactly 50
        // years), "21" is 1921 and not valid (before 1970).
        assert_eq!(
            rfc850_at("Monday, 01-Jan-19 00:00:00 GMT", 0),
            Some(1_546_300_800)
        );
        assert_eq!(
            rfc850_at("Monday, 01-Jan-20 00:00:00 GMT", 0),
            Some(1_577_836_800)
        );
        assert_eq!(rfc850_at("Monday, 01-Jan-20 00:00:01 GMT", 0), None);
        assert_eq!(rfc850_at("Monday, 01-Jan-21 00:00:00 GMT", 0), None);
    }

    #[test]
    fn header_date_rfc850_february_29() {
        // Reference 2026: "00" is 2000, a leap year.
        assert_eq!(
            rfc850_at("Tuesday, 29-Feb-00 00:00:00 GMT", RFC850_REFERENCE.0),
            Some(951_782_400)
        );
        // "99" is 1999: no 29 February.
        assert_eq!(
            rfc850_at("Monday, 29-Feb-99 00:00:00 GMT", RFC850_REFERENCE.0),
            None
        );
    }

    #[test]
    fn header_date_rfc850_a_large_reference_gives_no_panic() {
        assert_eq!(rfc850_at("Monday, 01-Jan-26 00:00:00 GMT", u64::MAX), None);
    }

    #[test]
    fn header_date_rfc850_matches_a_search_over_all_years() {
        // For references in each decade: the result is the latest year with
        // the digits that is not more than 50 calendar years ahead.
        for reference_year in (1970..=9900).step_by(37) {
            let reference = seconds_unchecked(reference_year, 7, 15, 43_200).unwrap();
            for two_digits in [0, 1, 25, 49, 50, 51, 69, 70, 99] {
                let value = format!("Monday, 15-Jul-{two_digits:02} 12:00:00 GMT");
                let expected = (FIRST_YEAR..=LAST_YEAR)
                    .filter(|year| year % 100 == two_digits)
                    .filter(|&year| {
                        year < 2020
                            || seconds_unchecked(year - 50, 7, 15, 43_200).unwrap() <= reference
                    })
                    .max()
                    .map(|year| seconds_unchecked(year, 7, 15, 43_200).unwrap());
                assert_eq!(
                    rfc850_at(&value, reference),
                    expected,
                    "{value} at {reference_year}"
                );
            }
        }
    }
}
