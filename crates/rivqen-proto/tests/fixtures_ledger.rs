// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The WP-05 checks of the fixture ledger (DL-145) on the RQP protocol
//! client fixtures, as data through `rivqen-testkit` (DL-146):
//!
//! - `request-fields`: the request field lines, byte for byte.
//! - owner WP-05: the outcome and the error code; the delta fixtures run
//!   through `delta::verify_delta`, the charset fixtures through the
//!   response field functions (negotiation section 6.3, item 3).
//! - `binding`: `verify_delta` accepts the delta (a positive control).
//!
//! The ledger selects the fixtures; `expected.json` (oracle:
//! `tools/rqp-protocol`) gives the expected values. No list of fixture ids is
//! in this file. A fixture of the ledger that no check runs fails
//! `fixtures_ledger_fully_covered`.
//!
//! Checks 1 to 3 of diff-engine section 3 and the request routing are
//! session decisions (WP-08). For the fixtures that this file runs, their
//! inputs hold; each runner asserts that, so a wrong ledger line fails.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use std::collections::{BTreeMap, BTreeSet};

use rivqen_model::error::HasErrorCode;
use rivqen_model::http::FieldLine;
use rivqen_model::limits::ResourceLimits;
use rivqen_proto::delta::{DeltaError, VerifiedDelta, verify_delta};
use rivqen_proto::fields::{
    DELTA_MEDIA_TYPE, HTML_MEDIA_TYPE, RequestBasis, ResponseHead, request_fields,
};
use rivqen_proto::types::{MAX_BLOCK_BYTES, MAX_PATCH_BYTES};
use rivqen_testkit::fixtures::{ClientContext, Fixture};
use rivqen_testkit::ledger::{Check, Owner};

use common::{canonical_url, client_fixture, ledger, nonce, snapshot_meta};

/// The DL-145 counts of the ledger.
const REQUEST_FIELDS: usize = 150;
const WP05_OUTCOMES: usize = 25;
const BINDINGS: usize = 26;

// ---------------------------------------------------------------------------
// Request fields

/// Which input of `request_fields` the caller gives for a fixture context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Basis {
    FirstVisit,
    Fallback,
    /// A delta navigation whose snapshot is not usable: a `public` snapshot
    /// with the setting off is no snapshot (H-81).
    UnusableSnapshot,
    Snapshot,
}

fn usable(ctx: &ClientContext) -> bool {
    ctx.snapshot
        .as_ref()
        .is_some_and(|s| s.partition.as_deref() != Some("public") || ctx.public_enabled)
}

fn basis_of(ctx: &ClientContext) -> Basis {
    match ctx.navigation.as_str() {
        "delta" if usable(ctx) => Basis::Snapshot,
        "delta" if ctx.snapshot.is_some() => Basis::UnusableSnapshot,
        "fallback" => Basis::Fallback,
        "first-visit" | "delta" => Basis::FirstVisit,
        other => panic!("navigation {other}"),
    }
}

fn pairs(lines: &[FieldLine]) -> Vec<(&str, &str)> {
    lines
        .iter()
        .map(|f| (f.name.as_str(), f.value.as_str()))
        .collect()
}

/// Run `request_fields` on every fixture with the ledger check
/// `request-fields`; returns the basis of each fixture.
fn run_request_fields() -> BTreeMap<String, Basis> {
    let mut ran = BTreeMap::new();
    for entry in ledger().with_check(Check::RequestFields) {
        let fixture = client_fixture(&entry.id);
        let ctx = fixture.client_context_data().unwrap();
        let want = fixture.expected_request().unwrap();
        let basis = basis_of(&ctx);
        let snapshot = ctx.snapshot.as_ref().map(snapshot_meta);
        let n = ctx.nonce.as_deref().map(nonce);
        let input = match basis {
            Basis::FirstVisit | Basis::UnusableSnapshot => RequestBasis::FirstVisit,
            Basis::Fallback => RequestBasis::FallbackLoad,
            Basis::Snapshot => RequestBasis::Snapshot {
                snapshot: snapshot.as_ref().unwrap(),
                nonce: n.as_ref(),
            },
        };
        let got = request_fields(input).unwrap_or_else(|e| panic!("{}: {e}", entry.id));
        assert_eq!(pairs(&got), pairs(&want.headers), "{}", entry.id);
        ran.insert(entry.id.clone(), basis);
    }
    ran
}

#[test]
fn fixtures_request_fields_match_ledger() {
    let ran = run_request_fields();
    assert_eq!(ran.len(), REQUEST_FIELDS);
    let count = |basis: Basis| ran.values().filter(|b| **b == basis).count();
    // Every basis of request_fields is used by the fixtures.
    assert_eq!(count(Basis::FirstVisit), 55);
    assert_eq!(count(Basis::Snapshot), 91);
    assert_eq!(count(Basis::Fallback), 2);
    assert_eq!(count(Basis::UnusableSnapshot), 2);
    // One fixture sends another nonce than the KAT nonce 00…0F.
    let other_nonce: Vec<&str> = ran
        .iter()
        .filter(|(_, b)| **b == Basis::Snapshot)
        .map(|(id, _)| id.as_str())
        .filter(|id| {
            client_fixture(id)
                .client_context_data()
                .unwrap()
                .nonce
                .as_deref()
                != Some("AAECAwQFBgcICQoLDA0ODw")
        })
        .collect();
    assert_eq!(other_nonce.len(), 1);
}

// ---------------------------------------------------------------------------
// Delta fixtures

/// `verify_delta` on the delta response of a fixture, with the inputs that
/// the session gives after checks 1 to 3: the client limits, the snapshot,
/// U of the final URL and the nonce of the request.
fn run_delta(fixture: &Fixture) -> Result<VerifiedDelta, DeltaError> {
    let id = &fixture.id;
    let ctx = fixture.client_context_data().unwrap();
    let response = fixture.client_response().unwrap();
    // Checks 1 and 2 hold: a 200 delta for a delta request with a usable
    // snapshot and a nonce.
    assert_eq!(response.status, 200, "{id}");
    assert_eq!(
        head(&response.headers).media_type().as_deref(),
        Some(DELTA_MEDIA_TYPE),
        "{id}"
    );
    assert_eq!(basis_of(&ctx), Basis::Snapshot, "{id}");
    let snapshot = snapshot_meta(ctx.snapshot.as_ref().unwrap());
    let n = nonce(ctx.nonce.as_deref().unwrap());
    let url = canonical_url(&ctx.final_url);
    let body = fixture.response_body.as_deref().unwrap();
    verify_delta(
        body,
        MAX_PATCH_BYTES,
        &snapshot,
        Some(&url),
        Some(&n),
        MAX_BLOCK_BYTES,
    )
}

/// Negotiation section 6.3, item 3, up to the UTF-8 decoding check: the
/// code when the client rejects the full document there, else `None` (the
/// manifest checks that follow need the markup analysis, WP-06).
fn run_full_charset(fixture: &Fixture) -> Option<&'static str> {
    let id = &fixture.id;
    let ctx = fixture.client_context_data().unwrap();
    let response = fixture.client_response().unwrap();
    // The final request carried RQP fields (allowlist, https, no user
    // information): the ledger gives these fixtures the check
    // request-fields, and the final URL is the request URL.
    assert_eq!(ctx.final_url, ctx.url, "{id}");
    assert!(
        ledger()
            .get(id)
            .unwrap()
            .checks
            .contains(&Check::RequestFields),
        "{id}"
    );
    assert_eq!(response.status, 200, "{id}");
    assert_eq!(
        head(&response.headers).media_type().as_deref(),
        Some(HTML_MEDIA_TYPE),
        "{id}"
    );
    let head = head(&response.headers);
    assert!(head.rq_version_is_1(), "{id}");
    let body = fixture.response_body.as_deref().unwrap_or_default();
    (!head.decoded_as_utf8(body)).then_some("MANIFEST_INVALID")
}

/// The response head of a fixture, checked against the default
/// `maxHeaderBytes` limits (DL-153); every fixture head is within them.
fn head(lines: &[FieldLine]) -> ResponseHead<'_> {
    let limits = ResourceLimits::default();
    let line = usize::try_from(limits.max_header_bytes).unwrap();
    let total = usize::try_from(limits.max_headers_total_bytes).unwrap();
    ResponseHead::check(lines, line, total).unwrap()
}

/// The code that the ledger reason of a WP-05 fixture stands for.
fn code_of_reason(reason: &str) -> &'static str {
    match reason {
        "integrity" => "INTEGRITY_ERROR",
        "patch" => "PATCH_INVALID",
        "size" => "PATCH_TOO_LARGE",
        "charset" => "MANIFEST_INVALID",
        other => panic!("ledger reason {other} has no WP-05 check"),
    }
}

/// Run every fixture that WP-05 owns; returns the code of each.
fn run_wp05_outcomes() -> BTreeMap<String, &'static str> {
    let mut ran = BTreeMap::new();
    for entry in ledger().owned_by(Owner::Wp05) {
        let id = &entry.id;
        let fixture = client_fixture(id);
        let result = fixture.client_result().unwrap();
        let want = code_of_reason(&entry.reason);
        assert_eq!(result.outcome, "rejected", "{id}");
        assert_eq!(result.error.as_deref(), Some(want), "{id}");
        let got = if entry.reason == "charset" {
            run_full_charset(fixture)
        } else {
            let error = run_delta(fixture).err();
            // An integrity fixture fails at the binding (check 11).
            if entry.reason == "integrity" {
                assert_eq!(error.map(|e| e.check()), Some(11), "{id}");
            }
            error.map(|e| e.code().as_str())
        };
        assert_eq!(got, Some(want), "{id}");
        ran.insert(id.clone(), want);
    }
    ran
}

#[test]
fn fixtures_wp05_outcomes() {
    let ran = run_wp05_outcomes();
    assert_eq!(ran.len(), WP05_OUTCOMES);
    let count = |code: &str| ran.values().filter(|c| **c == code).count();
    assert_eq!(count("INTEGRITY_ERROR"), 13);
    assert_eq!(count("PATCH_INVALID"), 8);
    assert_eq!(count("PATCH_TOO_LARGE"), 1);
    assert_eq!(count("MANIFEST_INVALID"), 3);
}

/// Run `verify_delta` on every fixture with the ledger check `binding`;
/// each must pass, with the page revision and the blocks of the expected
/// snapshot.
fn run_bindings() -> BTreeSet<String> {
    let mut ran = BTreeSet::new();
    for entry in ledger().with_check(Check::Binding) {
        let id = &entry.id;
        let fixture = client_fixture(id);
        let result = fixture.client_result().unwrap();
        assert_eq!(result.outcome, "patched", "{id}");
        let verified = run_delta(fixture).unwrap_or_else(|e| panic!("{id}: {e}"));
        // R′ and L′ are the page revision and the blocks of the new snapshot
        // (checks 12 and 13 do not change them).
        let expected = result.snapshot.as_ref().unwrap();
        assert_eq!(
            verified.next_revision().to_string(),
            expected["page_revision"].as_str().unwrap(),
            "{id}"
        );
        let blocks: Vec<(String, &str, String)> = verified
            .next_blocks()
            .iter()
            .map(|b| (b.id.to_string(), b.format.as_str(), b.sha256.to_b64u()))
            .collect();
        let want: Vec<(String, &str, String)> = expected["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| {
                (
                    b["id"].as_str().unwrap().to_owned(),
                    b["format"].as_str().unwrap(),
                    b["sha256"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        assert_eq!(blocks, want, "{id}");
        ran.insert(id.clone());
    }
    ran
}

#[test]
fn fixtures_bindings_accept() {
    assert_eq!(run_bindings().len(), BINDINGS);
}

// ---------------------------------------------------------------------------
// Coverage

#[test]
fn fixtures_ledger_fully_covered() {
    let ledger = ledger();
    // Every client fixture has a line (common::ledger verifies it).
    assert_eq!(ledger.entries().count(), common::protocol().client.len());
    let ids = |it: Vec<&String>| it.into_iter().cloned().collect::<BTreeSet<String>>();
    let request_fields: BTreeSet<String> = run_request_fields().into_keys().collect();
    let outcomes: BTreeSet<String> = run_wp05_outcomes().into_keys().collect();
    let bindings = run_bindings();
    assert_eq!(
        request_fields,
        ids(ledger
            .with_check(Check::RequestFields)
            .map(|e| &e.id)
            .collect())
    );
    assert_eq!(
        outcomes,
        ids(ledger.owned_by(Owner::Wp05).map(|e| &e.id).collect())
    );
    assert_eq!(
        bindings,
        ids(ledger.with_check(Check::Binding).map(|e| &e.id).collect())
    );
    // The DL-145 counts.
    assert_eq!(request_fields.len(), REQUEST_FIELDS);
    assert_eq!(outcomes.len(), WP05_OUTCOMES);
    assert_eq!(bindings.len(), BINDINGS);
    // Every fixture that WP-05 owns has a WP-05 check; the others belong to
    // WP-06 and WP-08 (their outcomes are not run here).
    for entry in ledger.entries() {
        let run = request_fields.contains(&entry.id)
            || outcomes.contains(&entry.id)
            || bindings.contains(&entry.id);
        assert!(run || entry.owner != Owner::Wp05, "{}", entry.id);
    }
}
