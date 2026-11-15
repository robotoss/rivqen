// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! A fake storage driver with a fault model (WP-07 S1, T-05; DL-181).
//!
//! Owner: T-05. `FakeStore` implements `rivqen_cache::driver::StorageDriver`
//! in memory. It runs each transaction in the steps of
//! `rivqen_cache::txn::CommitStep`, can stop after any step, drops the data
//! that was not synced, and recovers at the next start. The contract tests
//! that every driver passes are in this module too.
