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
