// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Rivqen cache engine (WP-07; docs/engineering/cache/).
//!
//! The crate is sans-I/O (ADR-001): it decides what is stored, for whom and
//! for how long, and it plans transactions. A storage driver executes them
//! ([`driver::StorageDriver`]); the reference driver is the crate
//! `rivqen-store` (DL-180).
//!
//! - [`key`]: the cache key and the partition of an entry (cache identity
//!   section 1; DL-184).
//! - [`partition`]: the choice between the partition of the request and the
//!   shared `public` partition, and the site match of a site clear (cache
//!   identity section 3).
//! - [`header`]: parsers of the response fields that the policy reads.
//! - [`policy`]: freshness and the store decision (freshness sections 2 and
//!   3; RFC 9111, RFC 5861), and the legacy `cache-offline` rules behind the
//!   feature `legacy`.
//! - [`txn`]: cache transactions with their preconditions (DL-185).
//! - [`driver`]: the storage driver interface and its error.
//! - [`memory`], [`quota`]: the L1 memory cache and the L2 eviction plan.
//! - [`inflight`]: one request for identical opens (R-CACHE-07).
//!
//! Every parser takes untrusted text, never panics and is bounded by the
//! response head limit that the adapter already applied.

#![forbid(unsafe_code)]

pub mod driver;
pub mod header;
pub mod inflight;
pub mod key;
pub mod memory;
pub mod partition;
pub mod policy;
pub mod quota;
pub mod txn;
