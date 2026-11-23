// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Linear time of the field parsers (WP-07 S1 T-02; R-CORE-09).
//!
//! `header_linear_time` measures 64 KiB inputs against 50 ms. It needs a
//! release build, so it is ignored by default. Run it with:
//!
//! ```text
//! RUST_TEST_THREADS=1 cargo test -p rivqen-cache --release --test header_linear -- --ignored header_linear_time
//! ```
//!
//! `header_linear_debug_bound` runs in every build with the same inputs and
//! a wide limit: a quadratic parse of 64 KiB takes seconds.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects
)]

use std::fmt::Write as _;
use std::hint::black_box;
use std::time::{Duration, Instant};

use rivqen_cache::header::{parse_age, parse_cache_control, parse_http_date, parse_vary};
use rivqen_model::http::FieldLine;

const KIB_64: usize = 64 * 1024;

/// Adversarial values of about `n` bytes: runs of commas, quotes,
/// backslashes and the other bytes that the parsers react to.
fn adversarial_values(n: usize) -> Vec<String> {
    let run = |s: &str| s.repeat(n / s.len());
    let mut names = String::new();
    let mut i = 0_u32;
    while names.len() < n {
        write!(names, "x-{i},").unwrap();
        i += 1;
    }
    vec![
        run(","),
        run("\""),
        run("\\"),
        run("\\\""),
        run(",\"\\"),
        format!("a=\"{}", run("\\\"")),
        format!("a=\"{}", run(",")),
        run("a=\"\","),
        run("a="),
        run("="),
        run(";"),
        run("no-store;"),
        run("max-age=1,"),
        run("max-age=1,max-age=2,"),
        format!("max-age={}", run("9")),
        format!("max-age=\"{}\"", run("0")),
        run("x\"y,"),
        run(" "),
        run("\t,"),
        run("*,"),
        run("a,"),
        names,
        run("\u{e9}"),
        run("Sun, 06 Nov 1994 08:49:37 GMT"),
        run("Sunday, "),
    ]
}

/// The time of one call of every parser on `value` (one field line each,
/// and the value split into 64 field lines).
fn run_all(value: &str) -> Duration {
    let one = [
        FieldLine::new("cache-control", value),
        FieldLine::new("age", value),
        FieldLine::new("vary", value),
    ];
    let chunk = value.len() / 64 + 1;
    let mut many = Vec::new();
    let mut rest = value;
    while !rest.is_empty() {
        let mut end = chunk.min(rest.len());
        while !rest.is_char_boundary(end) {
            end += 1;
        }
        let (head, tail) = rest.split_at(end);
        many.push(FieldLine::new("cache-control", head));
        many.push(FieldLine::new("vary", head));
        rest = tail;
    }
    let start = Instant::now();
    black_box(parse_cache_control(black_box(&one)));
    black_box(parse_age(black_box(&one)));
    black_box(parse_vary(black_box(&one)));
    black_box(parse_http_date(black_box(value)));
    black_box(parse_cache_control(black_box(&many)));
    black_box(parse_vary(black_box(&many)));
    #[cfg(feature = "legacy")]
    black_box(rivqen_cache::header::parse_cache_offline(black_box(Some(
        value,
    ))));
    start.elapsed()
}

#[test]
#[ignore = "release build only: cargo test --release -- --ignored header_linear_time"]
fn header_linear_time() {
    for value in adversarial_values(KIB_64) {
        assert!(value.len() >= KIB_64 - 64, "input too short");
        let elapsed = run_all(&value);
        assert!(
            elapsed < Duration::from_millis(50),
            "{elapsed:?} for an input that starts with {:?}",
            value.chars().take(16).collect::<String>()
        );
    }
}

#[test]
fn header_linear_debug_bound() {
    for value in adversarial_values(KIB_64) {
        let elapsed = run_all(&value);
        assert!(
            elapsed < Duration::from_secs(2),
            "{elapsed:?} for an input that starts with {:?}",
            value.chars().take(16).collect::<String>()
        );
    }
}
