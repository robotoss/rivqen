// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The delta fixtures of the RQP client ledger through `rqp::validate`
//! (checks 4 to 13 of diff engine section 3 in one call).
//!
//! The fixtures are every client fixture whose response is a `200` with the
//! delta media type. `expected.json` (oracle: `tools/rqp-protocol`) gives
//! the outcome and the code; the ledger reason gives the check:
//!
//! | Owner | Reason | Expected |
//! |---|---|---|
//! | WP-05 | `size` | check 4 or 10, `PATCH_TOO_LARGE` |
//! | WP-05 | `patch` | check 5, 6 or 9, `PATCH_INVALID` |
//! | WP-05 | `integrity` | check 11, `INTEGRITY_ERROR` |
//! | WP-06 | `markup` | patched, or check 12 |
//! | WP-06 | `content` | check 13, `PATCH_CONTENT_REJECTED` |
//! | WP-08 | `patch` | checks 1 to 3 of the session reject it (S3); `validate` does not run |
//!
//! The helpers that read a fixture are private copies of the helpers of
//! `crates/rivqen-template/tests/rqp_ledger_fixtures.rs` until T-06 moves
//! them to `rivqen_testkit::rqp_client`. U comes from the fixture text with
//! the test helper `canonical_url`, not from the product URL parser.

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

use rivqen_diff::rqp::{DeltaRequest, PatchLimits, StoredSnapshot, ValidatedPatch, validate};
use rivqen_integrity::{PageRevision, Sha256Digest, TemplateRevision, b64u_encode, sha256};
use rivqen_model::error::HasErrorCode;
use rivqen_model::limits::ResourceLimits;
use rivqen_proto::types::{
    BlockDescriptor, BlockFormat, BlockId, Limits, Mode, NONCE_BYTES, Nonce, Policy, SnapshotMeta,
};
use rivqen_template::limits::TemplateLimits;
use rivqen_template::rqp::split_rqp;
use rivqen_testkit::fixtures::{
    BlockData, ClientContext, Fixture, Members, RqpProtocol, SnapshotData, load_rqp_protocol,
};
use rivqen_testkit::ledger::{Ledger, Owner};
use serde_json::Value;

/// The branches (measured from `expected.json` and the ledger).
const DELTA_FIXTURES: usize = 87;
const PATCHED: usize = 26;
const REJECTED: usize = 56;
const SESSION: usize = 5;
/// The rejected fixtures by the check that rejects them.
const BY_CHECK: [(u8, usize); 7] = [(4, 1), (5, 1), (6, 4), (9, 3), (11, 13), (12, 6), (13, 28)];

// ---------------------------------------------------------------------------
// Fixture data (private copies until T-06)

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
/// form. The product parser is `rivqen_url::parse`.
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

fn str_of<'a>(v: &'a Value, key: &str, id: &str) -> &'a str {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{id}: expected member {key}"))
}

// ---------------------------------------------------------------------------
// Selection

/// The response is a `200` with the delta media type (negotiation section
/// 6.3, item 2: ASCII case-insensitive, parameters ignored).
fn is_delta_response(fixture: &Fixture) -> bool {
    let response = fixture.client_response().unwrap();
    response.status == 200
        && response.headers.iter().any(|line| {
            line.name.eq_ignore_ascii_case("content-type")
                && line
                    .value
                    .split(';')
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .eq_ignore_ascii_case("application/vnd.rivqen.delta+json")
        })
}

/// A delta request with a usable snapshot whose final URL is the request URL:
/// checks 1 and 2 of the session pass.
fn session_checks_pass(ctx: &ClientContext) -> bool {
    let usable = ctx
        .snapshot
        .as_ref()
        .is_some_and(|s| s.partition.as_deref() != Some("public") || ctx.public_enabled);
    // A final URL with user information carries no RQP fields (H-68); the
    // test helper does not read that form.
    let user_info = ctx.final_url.contains('@');
    ctx.navigation == "delta"
        && usable
        && ctx.nonce.is_some()
        && !user_info
        && canonical_url(&ctx.final_url) == canonical_url(&ctx.url)
}

// ---------------------------------------------------------------------------
// Run

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Branch {
    Patched,
    Rejected(u8),
    Session,
}

/// Compare a validated patch with `expected.result.snapshot`.
fn assert_snapshot(id: &str, want: &Value, patch: &ValidatedPatch) {
    let meta = patch.snapshot_meta();
    assert_eq!(
        meta.template_revision.to_string(),
        str_of(want, "template_revision", id),
        "{id}: template revision"
    );
    assert_eq!(
        patch.page_revision().to_string(),
        str_of(want, "page_revision", id),
        "{id}: page revision"
    );
    assert_eq!(
        sha256(patch.document()).to_b64u(),
        str_of(want, "document_sha256", id),
        "{id}: document sha256"
    );
    let blocks = want
        .get("blocks")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{id}: expected blocks"));
    assert_eq!(blocks.len(), meta.blocks.len(), "{id}: block count");
    let analysis = patch.analysis();
    for ((block, got), span) in blocks.iter().zip(&meta.blocks).zip(&analysis.blocks) {
        assert_eq!(got.id.as_str(), str_of(block, "id", id), "{id}: block id");
        assert_eq!(
            got.format.as_str(),
            str_of(block, "format", id),
            "{id}: format"
        );
        assert_eq!(
            got.sha256.to_b64u(),
            str_of(block, "sha256", id),
            "{id}: sha256"
        );
        assert_eq!(&span.descriptor, got, "{id}: analysis and snapshot values");
        if let Some(content) = block.get("content").and_then(Value::as_str) {
            assert_eq!(
                &patch.document()[span.start..span.end],
                content.as_bytes(),
                "{id}: content"
            );
        }
    }
    // The replacements are the new contents, in document order.
    let mut previous = None;
    for replacement in patch.replacements() {
        assert!(previous < Some(replacement.block()), "{id}: order");
        previous = Some(replacement.block());
        let span = &analysis.blocks[replacement.block()];
        assert_eq!(
            &patch.document()[span.start..span.end],
            replacement.content().as_bytes(),
            "{id}: replacement content"
        );
    }
}

/// The checks that the ledger reason allows.
fn checks_of_reason(owner: Owner, reason: &str) -> &'static [u8] {
    match (owner, reason) {
        (Owner::Wp05, "size") => &[4, 10],
        (Owner::Wp05, "patch") => &[5, 6, 9],
        (Owner::Wp05, "integrity") => &[11],
        (Owner::Wp06, "markup") => &[12],
        (Owner::Wp06, "content") => &[13],
        other => panic!("ledger {other:?} has no delta check"),
    }
}

fn run_fixture(fixture: &Fixture) -> Branch {
    let id = fixture.id.as_str();
    let entry = ledger().get(id).unwrap();
    let ctx = fixture.client_context_data().unwrap();
    let result = fixture.client_result().unwrap();
    if entry.owner == Owner::Wp08 {
        // Checks 1 to 3 of the session reject it before `validate` (S3).
        assert_eq!(entry.reason, "patch", "{id}");
        assert_eq!(result.outcome, "rejected", "{id}");
        assert_eq!(result.error.as_deref(), Some("PATCH_INVALID"), "{id}");
        assert!(!session_checks_pass(&ctx), "{id}: the session checks pass");
        return Branch::Session;
    }
    assert!(session_checks_pass(&ctx), "{id}: a session check fails");
    let snapshot = ctx.snapshot.as_ref().unwrap();
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
    let analysis = split_rqp(&original, &TemplateLimits::default())
        .unwrap_or_else(|e| panic!("{id}: stored page {page} does not split: {e}"));
    let meta = snapshot_meta(snapshot);
    let url = canonical_url(&ctx.final_url);
    let n = nonce(ctx.nonce.as_deref().unwrap());
    let body = fixture
        .response_body
        .as_deref()
        .unwrap_or_else(|| panic!("{id}: no response body"));
    let limits = PatchLimits::try_from(&ResourceLimits::default()).unwrap();
    let stored = StoredSnapshot {
        original: &original,
        analysis: &analysis,
        meta: &meta,
    };
    let request = DeltaRequest {
        final_url: Some(&url),
        nonce: Some(&n),
    };
    let outcome = validate(body, &stored, &request, &limits);
    match result.outcome.as_str() {
        "patched" => {
            let want = result
                .snapshot
                .as_ref()
                .unwrap_or_else(|| panic!("{id}: patched without snapshot"));
            let patch = outcome.unwrap_or_else(|e| panic!("{id}: expected patched, got {e}"));
            assert_eq!(patch.base_revision(), &meta.page_revision, "{id}");
            assert_snapshot(id, want, &patch);
            Branch::Patched
        }
        "rejected" => {
            let error = outcome
                .err()
                .unwrap_or_else(|| panic!("{id}: expected rejected, got a patch"));
            assert_eq!(
                Some(error.code().as_str()),
                result.error.as_deref(),
                "{id}: {error}"
            );
            assert!(
                checks_of_reason(entry.owner, &entry.reason).contains(&error.check()),
                "{id}: reason {} and {error}",
                entry.reason
            );
            Branch::Rejected(error.check())
        }
        other => panic!("{id}: delta outcome {other}"),
    }
}

fn run_all() -> BTreeMap<String, Branch> {
    protocol()
        .client
        .iter()
        .filter(|f| is_delta_response(f))
        .map(|f| (f.id.clone(), run_fixture(f)))
        .collect()
}

#[test]
fn rqp_ledger_delta() {
    let ran = run_all();
    assert_eq!(ran.len(), DELTA_FIXTURES);
    let count = |want: fn(&Branch) -> bool| ran.values().filter(|b| want(b)).count();
    assert_eq!(count(|b| *b == Branch::Patched), PATCHED);
    assert_eq!(count(|b| matches!(b, Branch::Rejected(_))), REJECTED);
    assert_eq!(count(|b| *b == Branch::Session), SESSION);
    let mut by_check: BTreeMap<u8, usize> = BTreeMap::new();
    for branch in ran.values() {
        if let Branch::Rejected(check) = branch {
            *by_check.entry(*check).or_default() += 1;
        }
    }
    assert_eq!(by_check.into_iter().collect::<Vec<_>>(), BY_CHECK);
    // Every delta fixture of WP-05 and WP-06 runs here: the outcomes of the
    // ledger that a delta body decides.
    let owned = |owner: Owner| {
        ledger()
            .owned_by(owner)
            .filter(|e| ran.contains_key(&e.id))
            .count()
    };
    assert_eq!(owned(Owner::Wp05) + owned(Owner::Wp06), PATCHED + REJECTED);
    assert_eq!(owned(Owner::Wp08), SESSION);
}
