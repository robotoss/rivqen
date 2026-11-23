// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Properties P-DIFF-1 to P-DIFF-4 of diff engine section 5 through
//! `rqp::validate`, on generated snapshots and the deltas that a server
//! builds for them.
//!
//! Fixed seeds; `PROPTEST_CASES` raises the case count.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

mod rqp_common;

use std::collections::BTreeMap;

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use rivqen_diff::rqp::{ValidateError, ValidatedPatch, validate};
use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_proto::types::{BlockFormat, Limits, Mode, Policy};

use rqp_common::{
    NONCE, OTHER_NONCE, OTHER_URL, Snap, UNBOUND, URL, bound_delta, changed, envelope, limits,
    page_of, request, run, server_delta,
};

fn config(cases: u32, seed: u64) -> ProptestConfig {
    ProptestConfig {
        cases: std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(cases),
        rng_seed: RngSeed::Fixed(seed),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

// ---------------------------------------------------------------------------
// Generators

/// Pieces of `html` block content that are valid RQP markup and pass the
/// patch content rule.
const HTML_PIECES: [&str; 9] = [
    "a",
    "120.00",
    "<b>x</b>",
    "&amp;",
    "<span class=\"p\">1</span>",
    "\u{e9}",
    " ",
    "<i>y</i><br>",
    "<img src=\"/i.png\" alt=\"\">",
];

/// `json` block contents (valid JSON texts, M-25).
const JSON_TEXTS: [&str; 6] = [
    "{}",
    "[]",
    "1",
    "\"s\"",
    "{\"items\":3}",
    "[1,2,{\"a\":null}]",
];

const MARKERS: [&str; 2] = ["Static header", "Other header"];

fn content(format: BlockFormat) -> BoxedStrategy<String> {
    match format {
        BlockFormat::Html => prop::collection::vec(prop::sample::select(&HTML_PIECES[..]), 0..4)
            .prop_map(|pieces| pieces.concat())
            .boxed(),
        BlockFormat::Json => prop::sample::select(&JSON_TEXTS[..])
            .prop_map(str::to_owned)
            .boxed(),
    }
}

fn contents(formats: &[BlockFormat]) -> Vec<BoxedStrategy<(BlockFormat, String)>> {
    formats
        .iter()
        .map(|&format| content(format).prop_map(move |c| (format, c)).boxed())
        .collect()
}

fn members() -> impl Strategy<Value = (Option<Policy>, Option<Limits>)> {
    prop_oneof![
        Just((None, None)),
        Just((
            Some(Policy {
                mode: Some(Mode::Public),
                ttl_seconds: Some(60),
                offline: None,
            }),
            None
        )),
        Just((
            None,
            Some(Limits {
                max_patch_bytes: Some(65_536)
            })
        )),
    ]
}

/// Three documents of one template (same marker, same blocks) with their
/// contents, and the manifest members of the snapshot.
#[derive(Debug, Clone)]
struct Case {
    marker: &'static str,
    a: Vec<(BlockFormat, String)>,
    b: Vec<(BlockFormat, String)>,
    c: Vec<(BlockFormat, String)>,
    policy: Option<Policy>,
    limits: Option<Limits>,
}

impl Case {
    fn snap(&self, blocks: &[(BlockFormat, String)]) -> Snap {
        Snap::with_members(&page_of(self.marker, blocks), self.policy, self.limits)
    }
}

fn case() -> impl Strategy<Value = Case> {
    (
        prop::collection::vec(
            prop_oneof![Just(BlockFormat::Html), Just(BlockFormat::Json)],
            1..=5,
        ),
        prop::sample::select(&MARKERS[..]),
        members(),
    )
        .prop_flat_map(|(formats, marker, (policy, limits))| {
            (
                contents(&formats),
                contents(&formats),
                contents(&formats),
                Just(marker),
                Just(policy),
                Just(limits),
            )
        })
        .prop_map(|(a, b, c, marker, policy, limits)| Case {
            marker,
            a,
            b,
            c,
            policy,
            limits,
        })
}

/// The codes that `validate` can give (diff engine section 3).
fn assert_allowed(error: &ValidateError) {
    assert!(
        matches!(
            error.code(),
            ErrorCode::PatchTooLarge
                | ErrorCode::PatchInvalid
                | ErrorCode::IntegrityError
                | ErrorCode::PatchContentRejected
        ),
        "{error}"
    );
    assert!(
        [4, 5, 6, 9, 10, 11, 12, 13].contains(&error.check()),
        "{error}"
    );
}

/// A validated patch is the snapshot `next`: bytes, analysis and values.
fn assert_is(patch: &ValidatedPatch, next: &Snap) {
    assert_eq!(patch.document(), next.original.as_slice());
    assert_eq!(patch.analysis(), &next.analysis);
    assert_eq!(patch.snapshot_meta(), &next.meta);
}

// ---------------------------------------------------------------------------
// P-DIFF-1

proptest! {
    #![proptest_config(config(256, 0x5150_0001))]

    /// P-DIFF-1: `apply(rev_n, patch(n → n+1)) == rev_(n+1)`, with the
    /// changed blocks or with every block (H-7).
    #[test]
    fn rqp_prop_apply(case in case(), every in any::<bool>()) {
        let base = case.snap(&case.a);
        let next = case.snap(&case.b);
        let indices = if every {
            (0..case.a.len()).collect()
        } else {
            changed(&base, &next)
        };
        let body = server_delta(&base, &next, &indices, URL, &NONCE);
        let patch = run(&body, &base).unwrap();
        assert_is(&patch, &next);
        prop_assert_eq!(patch.base_revision(), &base.revision());
        let got: Vec<(usize, &str)> = patch
            .replacements()
            .iter()
            .map(|r| (r.block(), r.content()))
            .collect();
        let want: Vec<(usize, &str)> = indices.iter().map(|&i| (i, next.content(i))).collect();
        prop_assert_eq!(got, want);
    }
}

// ---------------------------------------------------------------------------
// P-DIFF-2

#[derive(Debug, Clone, Copy)]
enum Other {
    /// A snapshot with another base revision of the same template.
    Base,
    /// The snapshot that the patch targets (a stale replay on its target).
    OwnTarget,
    /// Another template: the same blocks, another static text.
    Template,
    /// Another final URL.
    Url,
    /// Another request on the same snapshot (another nonce: A → B → A, or a
    /// replay to another user with the same `public` base).
    Request,
}

proptest! {
    #![proptest_config(config(256, 0x5150_0002))]

    /// P-DIFF-2: a patch for another base, template, URL or request fails
    /// check 11 and changes nothing, also when its operations cover every
    /// block.
    #[test]
    fn rqp_prop_other_base(
        case in case(),
        other in prop_oneof![
            Just(Other::Base),
            Just(Other::OwnTarget),
            Just(Other::Template),
            Just(Other::Url),
            Just(Other::Request),
        ],
    ) {
        let built_for = case.snap(&case.a);
        let next = case.snap(&case.b);
        let every: Vec<usize> = (0..case.a.len()).collect();
        let body = server_delta(&built_for, &next, &every, URL, &NONCE);
        let (snap, url, nonce) = match other {
            Other::Base => (case.snap(&case.c), URL, NONCE),
            Other::OwnTarget => (next.clone(), URL, NONCE),
            Other::Template => {
                let marker = if case.marker == MARKERS[0] { MARKERS[1] } else { MARKERS[0] };
                let page = page_of(marker, &case.a);
                (Snap::with_members(&page, case.policy, case.limits), URL, NONCE)
            }
            Other::Url => (built_for.clone(), OTHER_URL, NONCE),
            Other::Request => (built_for.clone(), URL, OTHER_NONCE),
        };
        prop_assume!(
            !matches!(other, Other::Base | Other::OwnTarget)
                || snap.revision() != built_for.revision()
        );
        let before = snap.clone();
        let error = validate(&body, &snap.stored(), &request(url, &nonce), &limits())
            .unwrap_err();
        prop_assert_eq!((error.check(), error.code()), (11, ErrorCode::IntegrityError));
        prop_assert_eq!(snap, before);
    }
}

// ---------------------------------------------------------------------------
// P-DIFF-3

proptest! {
    #![proptest_config(config(256, 0x5150_0003))]

    /// P-DIFF-3: applying the same patch twice gives the result of one
    /// apply: the second apply is on n+1 with B = n, so check 11 rejects it;
    /// when n+1 is n, it gives the same document again.
    #[test]
    fn rqp_prop_twice(case in case()) {
        let base = case.snap(&case.a);
        let next = case.snap(&case.b);
        let body = server_delta(&base, &next, &changed(&base, &next), URL, &NONCE);
        let once = run(&body, &base).unwrap();
        let stored = Snap::from_patch(&once);
        let twice = run(&body, &stored);
        if base.revision() == next.revision() {
            let twice = twice.unwrap();
            prop_assert_eq!(twice.document(), once.document());
            prop_assert_eq!(twice.snapshot_meta(), once.snapshot_meta());
        } else {
            let error = twice.unwrap_err();
            prop_assert_eq!((error.check(), error.code()), (11, ErrorCode::IntegrityError));
        }
    }
}

// ---------------------------------------------------------------------------
// P-DIFF-4

/// Contents of operations, also ones that checks 12 and 13 reject.
const ANY_CONTENT: [&str; 10] = [
    "1",
    "<b>x</b>",
    "",
    "{}",
    "<script>x()</script>",
    "<b onclick=\"x()\">1</b>",
    "</div><div>",
    "<a href=\"javascript:x\">1</a>",
    "<svg><a href=\"/x\">1</a></svg>",
    "{\"items\":",
];

proptest! {
    #![proptest_config(config(512, 0x5150_0004))]

    /// P-DIFF-4: any body gives an error with an allowed code and check
    /// number, never a panic.
    #[test]
    fn rqp_prop_malformed_any_body(
        case in case(),
        body in prop::collection::vec(any::<u8>(), 0..400),
    ) {
        let base = case.snap(&case.a);
        let error = run(&body, &base).unwrap_err();
        assert_allowed(&error);
    }

    /// P-DIFF-4: one changed, removed or inserted byte of a valid body gives
    /// an error with an allowed code, or the page of the server; never
    /// another page.
    #[test]
    fn rqp_prop_malformed_mutated_body(
        case in case(),
        at in any::<prop::sample::Index>(),
        byte in any::<u8>(),
        kind in 0_u8..3,
    ) {
        let base = case.snap(&case.a);
        let next = case.snap(&case.b);
        let mut body = server_delta(&base, &next, &changed(&base, &next), URL, &NONCE);
        let i = at.index(body.len());
        match kind {
            0 => body[i] = byte,
            1 => { body.remove(i); }
            _ => body.insert(i, byte),
        }
        match run(&body, &base) {
            Ok(patch) => assert_is(&patch, &next),
            Err(error) => assert_allowed(&error),
        }
    }

    /// P-DIFF-4: a stored original that is not the analysed document (a
    /// changed byte) gives an error or the page of the server; never another
    /// page.
    #[test]
    fn rqp_prop_malformed_stored_original(
        case in case(),
        at in any::<prop::sample::Index>(),
        byte in any::<u8>(),
    ) {
        let base = case.snap(&case.a);
        let next = case.snap(&case.b);
        let body = server_delta(&base, &next, &changed(&base, &next), URL, &NONCE);
        let mut corrupted = base.clone();
        let i = at.index(corrupted.original.len());
        corrupted.original[i] = byte;
        match run(&body, &corrupted) {
            Ok(patch) => assert_is(&patch, &next),
            Err(error) => assert_allowed(&error),
        }
    }

    /// P-DIFF-4 and R-CORE-05: operations with any ids, order and content
    /// (most of them in document order on known blocks), with a binding that
    /// fits them (or not). A validated patch is exactly
    /// the base with the operation contents in their blocks, and no patched
    /// `html` block has content that checks 12 and 13 reject.
    #[test]
    fn rqp_prop_malformed_operations(
        case in case(),
        ops in prop::collection::vec(
            (0_usize..7, prop::sample::select(&ANY_CONTENT[..])),
            0..6,
        ),
        ordered in prop::bool::weighted(0.8),
        bound in prop::bool::weighted(0.8),
    ) {
        let base = case.snap(&case.a);
        let mut ops = ops;
        if ordered {
            // Known blocks, at most one operation each, in document order.
            ops.retain(|(i, _)| *i < case.a.len());
            ops.sort_by_key(|(i, _)| *i);
            ops.dedup_by_key(|(i, _)| *i);
        }
        let operations: Vec<(String, String)> = ops
            .iter()
            .map(|(i, c)| (format!("b{i}"), (*c).to_owned()))
            .collect();
        let body = if bound && !operations.is_empty() {
            bound_delta(&base, &operations)
        } else {
            envelope(UNBOUND, &operations)
        };
        match run(&body, &base) {
            Ok(patch) => {
                prop_assert!(bound);
                let new: BTreeMap<&str, &str> = operations
                    .iter()
                    .map(|(id, c)| (id.as_str(), c.as_str()))
                    .collect();
                let blocks: Vec<(BlockFormat, String)> = case
                    .a
                    .iter()
                    .enumerate()
                    .map(|(i, (format, old))| {
                        let id = format!("b{i}");
                        (*format, new.get(id.as_str()).map_or_else(|| old.clone(), |c| (*c).to_owned()))
                    })
                    .collect();
                let expected = case.snap(&blocks);
                assert_is(&patch, &expected);
                for replacement in patch.replacements() {
                    let text = replacement.content();
                    prop_assert!(!text.contains("<script") && !text.contains("onclick"));
                    prop_assert!(!text.contains("javascript:") && !text.contains("<svg><a"));
                }
            }
            Err(error) => assert_allowed(&error),
        }
    }
}
