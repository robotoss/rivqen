// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! A fake clock. It never reads the system time.

use std::cell::Cell;

use rivqen_model::time::{Clock, Instant};

/// A clock that tests set and advance by hand.
///
/// Time never goes back: [`FakeClock::set`] ignores a value before the current time and
/// [`FakeClock::advance`] saturates at `u64::MAX`.
#[derive(Debug, Default)]
pub struct FakeClock {
    now: Cell<u64>,
}

impl FakeClock {
    /// A clock at `start` milliseconds.
    #[must_use]
    pub fn new(start: u64) -> Self {
        Self {
            now: Cell::new(start),
        }
    }

    /// Move to `to` when it is not before the current time. Returns the time after the call.
    pub fn set(&self, to: Instant) -> Instant {
        self.now.set(self.now.get().max(to.0));
        self.now()
    }

    /// Move forward by `millis`. Returns the time after the call.
    pub fn advance(&self, millis: u64) -> Instant {
        self.now.set(self.now.get().saturating_add(millis));
        self.now()
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Instant {
        Instant(self.now.get())
    }
}
