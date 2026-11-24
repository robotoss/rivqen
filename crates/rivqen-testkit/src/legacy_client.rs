// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The runner of the canonical legacy client fixtures (WP-08 S2, T-03):
//! it drives `rivqen_session::legacy::engine::LegacyEngine` with the steps
//! of `case.json` and the bytes of `exchanges.json`, executes the effects on
//! a fake web view, a fake network and the fake store, and builds the
//! projection of DL-110 that `expected.json` holds.
//!
//! Owner: WP-08 S2, lane L3.
