// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Time as the core sees it: an injected clock (data-model section 2, `clock`).
//!
//! The core never reads the system time. The adapter gives a [`Clock`]; tests
//! use the fake clock of `rivqen-testkit`.

/// A point in time in milliseconds on a monotonic clock of the adapter.
///
/// The origin is not defined; only differences have a meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instant(pub u64);

/// The time source of the core.
pub trait Clock {
    /// The current time.
    fn now(&self) -> Instant;
}

/// A wall-clock time in whole seconds since 1970-01-01T00:00:00Z (DL-183).
///
/// The adapter gives it with each call that needs freshness. It can go back
/// (the user changes the clock); the cache then treats an entry as stale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnixTime(pub u64);

impl UnixTime {
    /// The seconds from `earlier` to `self`, or `None` when `earlier` is
    /// after `self`.
    #[must_use]
    pub const fn since(self, earlier: Self) -> Option<u64> {
        self.0.checked_sub(earlier.0)
    }

    /// `self` plus `seconds`; the largest time when the sum does not fit.
    #[must_use]
    pub const fn plus(self, seconds: u64) -> Self {
        Self(self.0.saturating_add(seconds))
    }
}
