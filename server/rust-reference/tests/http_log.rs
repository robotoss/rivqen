// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The rate limit of repeated log lines (WP-03 S2 §2 "Hardening" item 5; DL-097;
//! ERR-09), with a fixed clock.

#![allow(
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use std::io::ErrorKind;
use std::time::{Duration, Instant};

use rivqen_reference::http::server::{LOG_WINDOW, LogEvent, LogKind, LogLimiter, MAX_LOG_KINDS};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[test]
fn log_repeats_are_suppressed() {
    let limiter = LogLimiter::new();
    let t0 = Instant::now();
    let kind = LogKind::io(LogEvent::AcceptFailed, ErrorKind::ConnectionAborted);
    let line = |at: Duration| limiter.line(kind, t0 + at, format_args!("accept failed"));
    assert_eq!(line(ms(0)).as_deref(), Some("accept failed"));
    // Inside the window: suppressed and counted.
    assert_eq!(line(ms(1)), None);
    assert_eq!(line(ms(500)), None);
    assert_eq!(line(ms(999)), None);
    // The window starts at the last written line, not at the last suppressed one.
    assert_eq!(
        line(LOG_WINDOW).as_deref(),
        Some("accept failed (3 similar lines suppressed)")
    );
    // The count starts again.
    assert_eq!(line(LOG_WINDOW + ms(10)), None);
    assert_eq!(
        line(LOG_WINDOW * 2 + ms(10)).as_deref(),
        Some("accept failed (1 similar lines suppressed)")
    );
    // A line after a quiet window has no count.
    assert_eq!(line(LOG_WINDOW * 5).as_deref(), Some("accept failed"));
    assert_eq!(LOG_WINDOW, Duration::from_secs(1));
}

#[test]
fn log_kinds_are_limited_one_by_one() {
    let limiter = LogLimiter::new();
    let t0 = Instant::now();
    let accept = LogKind::io(LogEvent::AcceptFailed, ErrorKind::ConnectionAborted);
    let other_io = LogKind::io(LogEvent::AcceptFailed, ErrorKind::OutOfMemory);
    let read = LogKind::io(LogEvent::ReadFailed, ErrorKind::ConnectionAborted);
    let idle = LogKind::of(LogEvent::Idle);
    for kind in [accept, other_io, read, idle] {
        assert!(
            limiter.line(kind, t0, format_args!("x")).is_some(),
            "{kind:?}"
        );
    }
    for kind in [accept, other_io, read, idle] {
        assert!(
            limiter.line(kind, t0 + ms(1), format_args!("x")).is_none(),
            "{kind:?}"
        );
    }
}

#[test]
fn log_limiter_keeps_at_most_64_kinds() {
    let limiter = LogLimiter::new();
    let t0 = Instant::now();
    let kinds: Vec<LogKind> = [
        ErrorKind::NotFound,
        ErrorKind::PermissionDenied,
        ErrorKind::ConnectionRefused,
        ErrorKind::ConnectionReset,
        ErrorKind::ConnectionAborted,
        ErrorKind::NotConnected,
        ErrorKind::AddrInUse,
        ErrorKind::AddrNotAvailable,
        ErrorKind::BrokenPipe,
        ErrorKind::AlreadyExists,
        ErrorKind::WouldBlock,
        ErrorKind::InvalidInput,
        ErrorKind::InvalidData,
        ErrorKind::TimedOut,
        ErrorKind::WriteZero,
        ErrorKind::Interrupted,
        ErrorKind::Unsupported,
        ErrorKind::UnexpectedEof,
        ErrorKind::OutOfMemory,
        ErrorKind::Other,
    ]
    .into_iter()
    .flat_map(|io| {
        [
            LogEvent::AcceptFailed,
            LogEvent::ReadFailed,
            LogEvent::NoLocalAddress,
            LogEvent::Idle,
        ]
        .map(|event| LogKind::io(event, io))
    })
    .collect();
    assert!(kinds.len() > MAX_LOG_KINDS);
    // Each kind at its own time: the first 64 fill the table, the 65th replaces
    // the oldest (kinds[0]).
    for (n, kind) in kinds.iter().take(MAX_LOG_KINDS + 1).enumerate() {
        let at = t0 + ms(u64::try_from(n).unwrap());
        assert!(limiter.line(*kind, at, format_args!("x")).is_some());
    }
    // Inside the window: kinds[0] is no longer known, so it is written again;
    // kinds[1] and the 65th kind are known and suppressed.
    let late = t0 + ms(100);
    assert!(limiter.line(kinds[1], late, format_args!("x")).is_none());
    assert!(
        limiter
            .line(kinds[MAX_LOG_KINDS], late, format_args!("x"))
            .is_none()
    );
    assert!(limiter.line(kinds[0], late, format_args!("x")).is_some());
}
