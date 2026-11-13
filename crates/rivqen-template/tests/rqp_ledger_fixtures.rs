// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The WP-06 checks of the fixture ledger (DL-145, H-142) on the RQP
//! protocol client fixtures: the document part of every fixture that the
//! ledger gives to WP-06.
//!
//! - first visit: `rqp::full::read_full_document` on the response body.
//! - delta: `delta::verify_delta` (as in the WP-05 test), then
//!   `rqp::patch::check_patch` on the stored page of the fixture.
//!
//! The ledger selects the fixtures; `expected.json` (oracle:
//! `tools/rqp-protocol`) gives the expected values. No list of fixture ids
//! and no skip list is in this file. A fixture whose expected values the
//! document part cannot decide fails with its id.
//!
//! Partition, URL canonicalization, allowlist, build flags, `next` and
//! `metric` are session decisions (WP-08) and are not checked here.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::too_many_lines
)]

use std::collections::BTreeMap;
use std::sync::OnceLock;

use rivqen_integrity::{PageRevision, Sha256Digest, TemplateRevision, b64u_encode, sha256};
use rivqen_model::error::HasErrorCode;
use rivqen_proto::delta::verify_delta;
use rivqen_proto::types::{
    BlockDescriptor, BlockFormat, BlockId, Limits, MAX_BLOCK_BYTES, MAX_PATCH_BYTES, Mode,
    NONCE_BYTES, Nonce, Policy, SnapshotMeta,
};
use rivqen_template::limits::TemplateLimits;
use rivqen_template::rqp::full::{FullDocumentError, read_full_document};
use rivqen_template::rqp::patch::{PatchError, check_patch};
use rivqen_template::rqp::{RqpDocument, split_rqp};
use rivqen_testkit::fixtures::{
    BlockData, Fixture, Members, RqpProtocol, SnapshotData, load_rqp_protocol,
};
use rivqen_testkit::ledger::{Ledger, Owner};
use serde_json::Value;

/// The DL-145 counts of the WP-06 part of the ledger.
const WP06: usize = 104;
const MARKUP: usize = 68;
const CONTENT: usize = 34;
const LOCATOR: usize = 2;
/// The branches of the document part (measured from `expected.json`).
const FULL_STORED: usize = 28;
const FULL_REJECTED: usize = 16;
const DELTA_PATCHED: usize = 26;
const DELTA_REJECTED: usize = 34;

// ---------------------------------------------------------------------------
// Fixture data

fn protocol() -> &'static RqpProtocol {
    static PROTOCOL: OnceLock<RqpProtocol> = OnceLock::new();
    PROTOCOL.get_or_init(|| load_rqp_protocol().unwrap_or_else(|e| panic!("{e}")))
}

fn ledger() -> &'static Ledger {
    static LEDGER: OnceLock<Ledger> = OnceLock::new();
    LEDGER.get_or_init(|| {
        let ledger = Ledger::embedded().unwrap_or_else(|e| panic!("{e}"));
        ledger
            .verify(protocol().client.iter().map(|f| f.id.as_str()))
            .unwrap_or_else(|e| panic!("{e}"));
        ledger
    })
}

fn client_fixture(id: &str) -> &'static Fixture {
    protocol()
        .client
        .iter()
        .find(|f| f.id == id)
        .unwrap_or_else(|| panic!("no client fixture {id}"))
}

fn descriptor(block: &BlockData) -> BlockDescriptor {
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

fn members(members: &Members) -> (Option<Policy>, Option<Limits>) {
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

fn snapshot_meta(data: &SnapshotData) -> SnapshotMeta {
    let (policy, limits) = members(&data.members);
    SnapshotMeta {
        template_revision: TemplateRevision::parse(&data.template_revision).unwrap(),
        page_revision: PageRevision::parse(&data.page_revision).unwrap(),
        blocks: data.blocks.iter().map(descriptor).collect(),
        policy,
        limits,
    }
}

/// The bytes of base64url text without padding (an independent decoder).
fn b64u_decode(text: &str) -> Vec<u8> {
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

fn nonce(text: &str) -> Nonce {
    let nonce = Nonce(
        b64u_decode(text)
            .try_into()
            .unwrap_or_else(|_| panic!("nonce {text} is not {NONCE_BYTES} bytes")),
    );
    assert_eq!(b64u_encode(&nonce.0), text);
    nonce
}

/// U of the binding for a fixture final URL (manifest-patch section 2.2):
/// scheme and host in lower case, no default port, no fragment. A helper for
/// the URL forms of the delta fixtures only; the test fails on any other
/// form. The URL canonicalization of the product is WP-08.
fn canonical_url(url: &str) -> String {
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

/// Origin and path of a final URL that is already canonical (scheme and
/// host in lower case, no default port, no fragment, a path); the test fails
/// on any other URL. The URL canonicalization of the product is WP-08.
fn origin_and_path(url: &str) -> (String, String) {
    let (scheme, rest) = url
        .split_once("://")
        .unwrap_or_else(|| panic!("{url}: not an absolute URL"));
    assert!(scheme == "https" || scheme == "http", "{url}: scheme");
    assert!(!url.contains('#'), "{url}: fragment");
    let end = rest.find(['/', '?']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    assert!(
        !authority.is_empty()
            && authority.bytes().all(|b| b.is_ascii_lowercase()
                || b.is_ascii_digit()
                || b == b'.'
                || b == b'-'
                || b == b':'),
        "{url}: authority is not canonical"
    );
    let default_port = if scheme == "https" { ":443" } else { ":80" };
    assert!(!authority.ends_with(default_port), "{url}: default port");
    assert!(tail.starts_with('/'), "{url}: no path");
    assert!(
        tail.bytes().all(|b| b.is_ascii_graphic() && b != b'\\')
            && !tail.contains("/./")
            && !tail.contains("/../"),
        "{url}: path is not canonical"
    );
    (format!("{scheme}://{authority}"), tail.to_owned())
}

/// The block descriptors of a split, as text for a comparison.
fn describe(document: &RqpDocument) -> Vec<(String, &'static str, String)> {
    document
        .blocks
        .iter()
        .map(|b| {
            (
                b.descriptor.id.to_string(),
                match b.descriptor.format {
                    BlockFormat::Html => "html",
                    BlockFormat::Json => "json",
                },
                b.descriptor.sha256.to_b64u(),
            )
        })
        .collect()
}

fn str_of<'a>(v: &'a Value, key: &str, id: &str) -> &'a str {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{id}: expected member {key}"))
}

/// Compare a document, its revisions and its blocks with
/// `expected.result.snapshot`.
fn assert_snapshot(
    id: &str,
    want: &Value,
    bytes: &[u8],
    document: &RqpDocument,
    page_revision: &PageRevision,
) {
    assert_eq!(
        document.template_revision.to_string(),
        str_of(want, "template_revision", id),
        "{id}: template revision"
    );
    assert_eq!(
        page_revision.to_string(),
        str_of(want, "page_revision", id),
        "{id}: page revision"
    );
    assert_eq!(
        sha256(bytes).to_b64u(),
        str_of(want, "document_sha256", id),
        "{id}: document sha256"
    );
    let blocks = want
        .get("blocks")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{id}: expected blocks"));
    assert_eq!(blocks.len(), document.blocks.len(), "{id}: block count");
    let got = describe(document);
    for ((block, (gid, gformat, ghash)), span) in blocks.iter().zip(&got).zip(&document.blocks) {
        assert_eq!(gid, str_of(block, "id", id), "{id}: block id");
        assert_eq!(*gformat, str_of(block, "format", id), "{id}: {gid} format");
        assert_eq!(ghash, str_of(block, "sha256", id), "{id}: {gid} sha256");
        // `content` is absent for a block that the oracle gives by its hash only.
        if let Some(want_content) = block.get("content").and_then(Value::as_str) {
            assert_eq!(
                &bytes[span.start..span.end],
                want_content.as_bytes(),
                "{id}: {gid} content"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Branches

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Branch {
    FullStored,
    FullRejected,
    DeltaPatched,
    DeltaRejected,
}

fn run_full(fixture: &Fixture, reason: &str) -> Branch {
    let id = &fixture.id;
    let ctx = fixture.client_context_data().unwrap();
    let result = fixture.client_result().unwrap();
    let body = fixture
        .response_body
        .as_deref()
        .unwrap_or_else(|| panic!("{id}: no response body"));
    let limits = TemplateLimits::default();
    match result.outcome.as_str() {
        "stored" => {
            let want = result
                .snapshot
                .as_ref()
                .unwrap_or_else(|| panic!("{id}: stored without snapshot"));
            let (origin, path) = origin_and_path(str_of(want, "url", id));
            let full = read_full_document(body, &origin, &path, &limits)
                .unwrap_or_else(|e| panic!("{id}: expected stored, got error: {e}"));
            assert_snapshot(
                id,
                want,
                full.original(),
                full.document(),
                full.page_revision(),
            );
            Branch::FullStored
        }
        "rejected" => {
            assert_eq!(result.error.as_deref(), Some("MANIFEST_INVALID"), "{id}");
            let (origin, path) = origin_and_path(&ctx.final_url);
            let error = read_full_document(body, &origin, &path, &limits)
                .err()
                .unwrap_or_else(|| panic!("{id}: expected MANIFEST_INVALID, got a document"));
            assert_eq!(error.code().as_str(), "MANIFEST_INVALID", "{id}");
            if reason == "locator" {
                assert!(
                    matches!(error, FullDocumentError::Manifest(_)),
                    "{id}: locator reason, got {error:?}"
                );
            }
            Branch::FullRejected
        }
        other => panic!("{id}: first-visit outcome {other} is not decided by the document part"),
    }
}

fn run_delta(fixture: &Fixture, reason: &str) -> Branch {
    let id = &fixture.id;
    let ctx = fixture.client_context_data().unwrap();
    let result = fixture.client_result().unwrap();
    let snapshot = ctx
        .snapshot
        .as_ref()
        .unwrap_or_else(|| panic!("{id}: delta without snapshot"));
    let page = snapshot
        .page
        .as_deref()
        .unwrap_or_else(|| panic!("{id}: snapshot without page"));
    let original = std::fs::read(
        rivqen_testkit::fixtures_root()
            .join("rqp/protocol/pages")
            .join(format!("{page}.html")),
    )
    .unwrap_or_else(|e| panic!("{id}: page {page}: {e}"));
    let limits = TemplateLimits::default();
    let stored = split_rqp(&original, &limits)
        .unwrap_or_else(|e| panic!("{id}: stored page {page} does not split: {e}"));
    let meta = snapshot_meta(snapshot);
    assert_eq!(
        stored.template_revision.to_string(),
        snapshot.template_revision,
        "{id}: template revision of the stored page"
    );
    let want_blocks: Vec<(String, &str, String)> = snapshot
        .blocks
        .iter()
        .map(|b| {
            (
                b.id.clone(),
                if b.format == "html" { "html" } else { "json" },
                b.sha256.clone(),
            )
        })
        .collect();
    assert_eq!(
        describe(&stored),
        want_blocks,
        "{id}: blocks of the stored page"
    );
    let url = canonical_url(&ctx.final_url);
    let n = nonce(
        ctx.nonce
            .as_deref()
            .unwrap_or_else(|| panic!("{id}: no nonce")),
    );
    let body = fixture
        .response_body
        .as_deref()
        .unwrap_or_else(|| panic!("{id}: no response body"));
    let delta = verify_delta(
        body,
        MAX_PATCH_BYTES,
        &meta,
        Some(&url),
        Some(&n),
        MAX_BLOCK_BYTES,
    )
    .unwrap_or_else(|e| panic!("{id}: verify_delta rejects: {e}"));
    let outcome = check_patch(&original, &stored, &meta, &delta, &limits);
    match result.outcome.as_str() {
        "patched" => {
            let want = result
                .snapshot
                .as_ref()
                .unwrap_or_else(|| panic!("{id}: patched without snapshot"));
            let patched = outcome.unwrap_or_else(|e| panic!("{id}: expected patched, got {e:?}"));
            assert_snapshot(
                id,
                want,
                patched.document(),
                patched.analysis(),
                patched.page_revision(),
            );
            Branch::DeltaPatched
        }
        "rejected" => {
            assert_eq!(
                result.error.as_deref(),
                Some("PATCH_CONTENT_REJECTED"),
                "{id}"
            );
            let error = outcome
                .err()
                .unwrap_or_else(|| panic!("{id}: expected PATCH_CONTENT_REJECTED, got a document"));
            assert_eq!(
                error.code().as_str(),
                "PATCH_CONTENT_REJECTED",
                "{id}: {error:?}"
            );
            if reason == "content" {
                assert!(
                    matches!(error, PatchError::Content(_)),
                    "{id}: content reason, got {error:?}"
                );
            }
            Branch::DeltaRejected
        }
        other => panic!("{id}: delta outcome {other} is not decided by the document part"),
    }
}

/// Run the document part of every fixture that WP-06 owns.
fn run_wp06() -> BTreeMap<String, Branch> {
    let mut ran = BTreeMap::new();
    for entry in ledger().owned_by(Owner::Wp06) {
        let fixture = client_fixture(&entry.id);
        let ctx = fixture.client_context_data().unwrap();
        let branch = match ctx.navigation.as_str() {
            "first-visit" => run_full(fixture, &entry.reason),
            "delta" => run_delta(fixture, &entry.reason),
            other => panic!("{}: navigation {other} has no document part", entry.id),
        };
        ran.insert(entry.id.clone(), branch);
    }
    ran
}

fn count(ran: &BTreeMap<String, Branch>, branch: Branch) -> usize {
    ran.values().filter(|b| **b == branch).count()
}

// ---------------------------------------------------------------------------
// Tests

#[test]
fn ledger_wp06_counts() {
    let entries: Vec<_> = ledger().owned_by(Owner::Wp06).collect();
    assert_eq!(entries.len(), WP06);
    let by = |reason: &str| entries.iter().filter(|e| e.reason == reason).count();
    assert_eq!(by("markup"), MARKUP);
    assert_eq!(by("content"), CONTENT);
    assert_eq!(by("locator"), LOCATOR);
}

#[test]
fn ledger_wp06_full_stored() {
    let ran = run_wp06();
    assert_eq!(count(&ran, Branch::FullStored), FULL_STORED);
}

#[test]
fn ledger_wp06_full_rejected() {
    let ran = run_wp06();
    assert_eq!(count(&ran, Branch::FullRejected), FULL_REJECTED);
}

#[test]
fn ledger_wp06_delta_patched() {
    let ran = run_wp06();
    assert_eq!(count(&ran, Branch::DeltaPatched), DELTA_PATCHED);
}

#[test]
fn ledger_wp06_delta_rejected() {
    let ran = run_wp06();
    assert_eq!(count(&ran, Branch::DeltaRejected), DELTA_REJECTED);
}

#[test]
fn ledger_wp06_fully_covered() {
    let ran = run_wp06();
    assert_eq!(ran.len(), WP06);
    let total = FULL_STORED + FULL_REJECTED + DELTA_PATCHED + DELTA_REJECTED;
    assert_eq!(total, WP06);
    for (branch, want) in [
        (Branch::FullStored, FULL_STORED),
        (Branch::FullRejected, FULL_REJECTED),
        (Branch::DeltaPatched, DELTA_PATCHED),
        (Branch::DeltaRejected, DELTA_REJECTED),
    ] {
        assert_eq!(count(&ran, branch), want, "{branch:?}");
    }
}
