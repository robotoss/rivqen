// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Shared helpers of the fixture tests (DL-145, DL-146).
//!
//! The helpers live once in `rivqen_testkit::rqp_client` (WP-08 S1, T-06).
//! Each function here calls it and stops the test with the message of the
//! error. Each test binary uses a part of it.

#![allow(dead_code, clippy::panic)]

use rivqen_proto::types::{BlockDescriptor, Limits, Nonce, Policy, SnapshotMeta};
use rivqen_testkit::fixtures::{BlockData, Fixture, Members, RqpProtocol, SnapshotData};
use rivqen_testkit::ledger::Ledger;
use rivqen_testkit::rqp_client;

/// All fixtures of `fixtures/rqp/protocol`, loaded once per test binary.
pub fn protocol() -> &'static RqpProtocol {
    rqp_client::protocol().unwrap_or_else(|e| panic!("{e}"))
}

/// The ledger of the client fixtures; it lists exactly the client fixtures.
pub fn ledger() -> &'static Ledger {
    rqp_client::ledger().unwrap_or_else(|e| panic!("{e}"))
}

/// The client fixture with this id.
pub fn client_fixture(id: &str) -> &'static Fixture {
    rqp_client::client_fixture(id).unwrap_or_else(|e| panic!("{e}"))
}

/// `policy` and `limits` with member presence kept.
pub fn members(members: &Members) -> (Option<Policy>, Option<Limits>) {
    rqp_client::members(members).unwrap_or_else(|e| panic!("{e}"))
}

/// One block of a fixture snapshot.
pub fn descriptor(block: &BlockData) -> BlockDescriptor {
    rqp_client::descriptor(block).unwrap_or_else(|e| panic!("{}: {e}", block.id))
}

/// The values that the client keeps for a fixture snapshot.
pub fn snapshot_meta(data: &SnapshotData) -> SnapshotMeta {
    rqp_client::snapshot_meta(data).unwrap_or_else(|e| panic!("{e}"))
}

/// The bytes of base64url text without padding.
pub fn b64u_decode(text: &str) -> Vec<u8> {
    rqp_client::b64u_decode(text).unwrap_or_else(|e| panic!("{text}: {e}"))
}

/// The nonce of the text N (22 base64url characters, no padding).
pub fn nonce(text: &str) -> Nonce {
    rqp_client::nonce(text).unwrap_or_else(|e| panic!("{text}: {e}"))
}

/// U of the binding for a fixture final URL (manifest-patch section 2.2).
pub fn canonical_url(url: &str) -> String {
    rqp_client::canonical_url(url).unwrap_or_else(|e| panic!("{url}: {e}"))
}
