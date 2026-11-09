// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! Fake clock and fake transport tests.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use proptest::prelude::*;
use rivqen_model::time::{Clock, Instant};
use rivqen_testkit::clock::FakeClock;
use rivqen_testkit::transport::FakeTransport;

#[test]
fn fake_clock_starts_where_told_and_advances() {
    let c = FakeClock::new(10);
    assert_eq!(c.now(), Instant(10));
    assert_eq!(c.advance(5), Instant(15));
    assert_eq!(c.now(), Instant(15));
}

#[test]
fn fake_clock_set_moves_forward_only() {
    let c = FakeClock::default();
    assert_eq!(c.set(Instant(100)), Instant(100));
    assert_eq!(c.set(Instant(40)), Instant(100));
    assert_eq!(c.now(), Instant(100));
}

#[test]
fn fake_clock_advance_saturates() {
    let c = FakeClock::new(u64::MAX - 1);
    assert_eq!(c.advance(10), Instant(u64::MAX));
}

#[test]
fn fake_clock_does_not_follow_the_system_time() {
    let c = FakeClock::new(7);
    std::thread::yield_now();
    assert_eq!(c.now(), Instant(7));
}

proptest! {
    #[test]
    fn fake_clock_time_never_goes_back(
        ops in proptest::collection::vec((any::<bool>(), any::<u64>()), 0..50)
    ) {
        let c = FakeClock::new(0);
        let mut last = c.now();
        for (set, v) in ops {
            if set { c.set(Instant(v)); } else { c.advance(v); }
            prop_assert!(c.now() >= last);
            last = c.now();
        }
    }
}

#[test]
fn fake_transport_answers_in_order_and_records_requests() {
    let mut t = FakeTransport::new(["one", "two"]);
    assert_eq!(t.send("a").unwrap(), "one");
    assert_eq!(t.remaining(), 1);
    assert_eq!(t.send("b").unwrap(), "two");
    assert_eq!(t.requests(), ["a", "b"]);
    assert_eq!(t.remaining(), 0);
}

#[test]
fn fake_transport_reports_unexpected_request_as_error() {
    let mut t: FakeTransport<&str, u8> = FakeTransport::new([1]);
    t.send("a").unwrap();
    let err = t.send("b").unwrap_err();
    assert_eq!(err.request, "b");
    assert_eq!(err.answered, 1);
    assert!(err.to_string().contains("unexpected request"));
    assert_eq!(t.requests(), ["a", "b"]);
}

#[test]
fn fake_transport_with_empty_script_fails_the_first_request() {
    let mut t: FakeTransport<u8, u8> = FakeTransport::new([]);
    assert_eq!(t.send(1).unwrap_err().answered, 0);
}
