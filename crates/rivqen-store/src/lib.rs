// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The reference storage driver of Rivqen (ADR-004, ADR-011; H-146, H-147).
//!
//! This crate is adapter code: it reads and writes files and a SQLite
//! database. No core crate depends on it (ADR-001, DL-180); the core talks
//! to a driver only through `rivqen_cache::driver::StorageDriver`.
//!
//! - [`crypto`]: the key port, the entry file format and its AEAD seal.
//! - [`meta`]: the SQLite metadata (rows, epochs, versions, disabled keys).
//! - [`driver`]: the commit procedure, recovery and the driver itself.

#![forbid(unsafe_code)]

pub mod crypto;
pub mod driver;
pub mod meta;
