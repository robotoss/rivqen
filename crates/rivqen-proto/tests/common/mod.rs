// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Shared helpers of the fixture tests (DL-145, DL-146).
//!
//! `rivqen-testkit` reads the fixtures and the ledger; it does not depend on
//! `rivqen-proto` (no dependency cycle). This module converts its data to the
//! core types: [`snapshot_meta`], [`members`], [`nonce`]. Each test binary
//! uses a part of it.

#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects
)]

use std::sync::OnceLock;

use rivqen_integrity::{PageRevision, Sha256Digest, TemplateRevision, b64u_encode};
use rivqen_proto::types::{
    BlockDescriptor, BlockFormat, BlockId, Limits, Mode, NONCE_BYTES, Nonce, Policy, SnapshotMeta,
};
use rivqen_testkit::fixtures::{
    BlockData, Fixture, Members, RqpProtocol, SnapshotData, load_rqp_protocol,
};
use rivqen_testkit::ledger::Ledger;

/// All fixtures of `fixtures/rqp/protocol`, loaded once per test binary.
pub fn protocol() -> &'static RqpProtocol {
    static PROTOCOL: OnceLock<RqpProtocol> = OnceLock::new();
    PROTOCOL.get_or_init(|| load_rqp_protocol().unwrap_or_else(|e| panic!("{e}")))
}

/// The ledger of the client fixtures; it lists exactly the client fixtures.
pub fn ledger() -> &'static Ledger {
    static LEDGER: OnceLock<Ledger> = OnceLock::new();
    LEDGER.get_or_init(|| {
        let ledger = Ledger::embedded().unwrap_or_else(|e| panic!("{e}"));
        ledger
            .verify(protocol().client.iter().map(|f| f.id.as_str()))
            .unwrap_or_else(|e| panic!("{e}"));
        ledger
    })
}

/// The client fixture with this id.
pub fn client_fixture(id: &str) -> &'static Fixture {
    protocol()
        .client
        .iter()
        .find(|f| f.id == id)
        .unwrap_or_else(|| panic!("no client fixture {id}"))
}

/// `policy` and `limits` with member presence kept.
pub fn members(members: &Members) -> (Option<Policy>, Option<Limits>) {
    let policy = members.policy.as_ref().map(|p| Policy {
        mode: p.mode.as_deref().map(|m| match m {
            "private" => Mode::Private,
            "public" => Mode::Public,
            other => panic!("mode {other:?}"),
        }),
        ttl_seconds: p
            .ttl_seconds
            .map(|t| u32::try_from(t).expect("ttl_seconds")),
        offline: p.offline,
    });
    let limits = members.limits.as_ref().map(|l| Limits {
        max_patch_bytes: l
            .max_patch_bytes
            .map(|m| u32::try_from(m).expect("max_patch_bytes")),
    });
    (policy, limits)
}

/// One block of a fixture snapshot.
pub fn descriptor(block: &BlockData) -> BlockDescriptor {
    BlockDescriptor {
        id: BlockId::parse(&block.id).unwrap_or_else(|e| panic!("{}: {e}", block.id)),
        format: match block.format.as_str() {
            "html" => BlockFormat::Html,
            "json" => BlockFormat::Json,
            other => panic!("format {other}"),
        },
        sha256: Sha256Digest::from_b64u(&block.sha256).unwrap(),
    }
}

/// The values that the client keeps for a fixture snapshot.
pub fn snapshot_meta(data: &SnapshotData) -> SnapshotMeta {
    let (policy, limits) = members(&data.members);
    SnapshotMeta {
        template_revision: TemplateRevision::parse(&data.template_revision).unwrap(),
        page_revision: PageRevision::parse(&data.page_revision).unwrap(),
        blocks: data.blocks.iter().map(descriptor).collect(),
        policy,
        limits,
    }
}

/// The bytes of base64url text without padding (an independent decoder for
/// the tests).
pub fn b64u_decode(text: &str) -> Vec<u8> {
    let value = |c: u8| -> u32 {
        match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'-' => 62,
            b'_' => 63,
            _ => panic!("not base64url: {c}"),
        }
    };
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0_u32, 0_u32);
    for &c in text.as_bytes() {
        acc = (acc << 6) | value(c);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from(acc >> bits).unwrap());
            acc &= (1 << bits) - 1;
        }
    }
    out
}

/// The nonce of the text N (22 base64url characters, no padding).
pub fn nonce(text: &str) -> Nonce {
    let nonce = Nonce(
        b64u_decode(text)
            .try_into()
            .unwrap_or_else(|_| panic!("nonce {text} is not {NONCE_BYTES} bytes")),
    );
    // The decoder is right when the encoder of the crate writes N again.
    assert_eq!(b64u_encode(&nonce.0), text);
    nonce
}

/// U of the binding for a fixture final URL (manifest-patch section 2.2):
/// scheme and host in lower case, no default port, no fragment.
///
/// A test helper for the URL forms of the fixtures only (an `http` or
/// `https` URL with an ASCII host and no user information); it panics on any
/// other form. The URL canonicalization of the product is WP-08.
pub fn canonical_url(url: &str) -> String {
    let (scheme, rest) = url
        .split_once("://")
        .unwrap_or_else(|| panic!("{url}: not an absolute URL"));
    let scheme = scheme.to_ascii_lowercase();
    let default_port = match scheme.as_str() {
        "https" => ":443",
        "http" => ":80",
        _ => panic!("{url}: scheme"),
    };
    let rest = rest.split_once('#').map_or(rest, |(before, _)| before);
    let end = rest.find(['/', '?']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    assert!(
        !authority.is_empty()
            && authority
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b':'),
        "{url}: host form not supported by the test helper"
    );
    assert!(
        tail.bytes().all(|b| b.is_ascii_graphic() && b != b'\\')
            && !tail.contains("/./")
            && !tail.contains("/../"),
        "{url}: path form not supported by the test helper"
    );
    let authority = authority.to_ascii_lowercase();
    let authority = authority.strip_suffix(default_port).unwrap_or(&authority);
    let path = if tail.starts_with('/') { "" } else { "/" };
    format!("{scheme}://{authority}{path}{tail}")
}
