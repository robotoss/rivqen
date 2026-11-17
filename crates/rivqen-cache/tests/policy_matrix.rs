// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The policy matrix `fixtures/cache/policy` (WP-07 S1 T-03; DL-182): every
//! row of the four files gives its expected value.
//!
//! The test builds the inputs of a row only with the parsers of
//! `rivqen_cache::header` and the meanings of the README section "Meaning of
//! the values"; it has no policy rule of its own. A failure names the row
//! id. The match on the kind is exhaustive, and the test fails when a row is
//! not run (the rows of `legacy.json` are run only with the feature
//! `legacy`; without it the test checks that they are the only rows that do
//! not run).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rivqen_cache::header::{parse_age, parse_cache_control, parse_http_date, parse_vary};
use rivqen_cache::policy::{
    EntryPolicy, Expires, Failure, SkipReason, StoreDecision, StoreInput, Use, current_age,
    freshness_lifetime, lookup_use, serve_on_failure, store_decision,
};
use rivqen_model::http::FieldLine;
use rivqen_testkit::cache_policy::{
    Case, ExpectedSkip, ExpectedUse, FailureKind, Kind, LegacyCase, ManifestPolicy, PolicyRow,
    StoreCase, StoreExpect, Times, load,
};

/// The value of the first field line `name`, when there is one.
fn first_value<'a>(fields: &'a [FieldLine], name: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|line| line.is(name))
        .map(|line| line.value.as_str())
}

/// `Expires` of a stored response: absent, a valid date or not valid.
fn expires(fields: &[FieldLine]) -> Expires {
    match first_value(fields, "expires") {
        None => Expires::Absent,
        Some(text) => parse_http_date(text).map_or(Expires::Invalid, Expires::At),
    }
}

/// The `EntryPolicy` of a stored response (README: `manifest: null` and an
/// absent member have no effect; DL-198 item i).
fn entry_policy(
    fields: &[FieldLine],
    times: Times,
    manifest: Option<ManifestPolicy>,
) -> EntryPolicy {
    let ttl_seconds = manifest
        .and_then(|m| m.ttl_seconds)
        .map(|ttl| u32::try_from(ttl).expect("the loader bounds ttl_seconds to one year"));
    let offline = manifest.and_then(|m| m.offline).unwrap_or(false);
    EntryPolicy {
        cache_control: parse_cache_control(fields),
        expires: expires(fields),
        date: first_value(fields, "date").and_then(parse_http_date),
        age: parse_age(fields),
        request_time: times.request_time,
        response_time: times.response_time,
        ttl_seconds,
        offline,
    }
}

/// The response has a `Set-Cookie` or `Set-Cookie2` line (H-62); the value
/// does not matter.
fn has_set_cookie(fields: &[FieldLine]) -> bool {
    fields
        .iter()
        .any(|line| line.is("set-cookie") || line.is("set-cookie2"))
}

fn expected_use(entry_use: ExpectedUse) -> Use {
    match entry_use {
        ExpectedUse::Fresh => Use::Fresh,
        ExpectedUse::StaleWhileRevalidate => Use::StaleWhileRevalidate,
        ExpectedUse::Revalidate => Use::Revalidate,
    }
}

/// The README reason of a decision. The reason `status` covers both status
/// reasons: [`SkipReason::TransientStatus`] is the subset of the status
/// skips after which the legacy rules keep the entry (CL-18 item 1); the
/// legacy rows `POL-LEGACY-029` to `-032` tell the two apart.
fn decision_expect(decision: StoreDecision) -> StoreExpect {
    match decision {
        StoreDecision::Store => StoreExpect::Store,
        StoreDecision::Skip(SkipReason::NoStore) => StoreExpect::Skip(ExpectedSkip::NoStore),
        StoreDecision::Skip(SkipReason::VaryAny) => StoreExpect::Skip(ExpectedSkip::VaryAny),
        StoreDecision::Skip(SkipReason::Credentialed) => {
            StoreExpect::Skip(ExpectedSkip::Credentialed)
        }
        StoreDecision::Skip(SkipReason::Status | SkipReason::TransientStatus) => {
            StoreExpect::Skip(ExpectedSkip::Status)
        }
        StoreDecision::Skip(other) => {
            panic!("a skip reason that the README has no name for: {other:?}")
        }
    }
}

/// The parts of a store or legacy row that `StoreInput` borrows.
struct StoreParts {
    request: rivqen_cache::header::CacheControl,
    response: rivqen_cache::header::CacheControl,
    vary: rivqen_cache::header::Vary,
    set_cookie: bool,
}

impl StoreParts {
    fn new(request_fields: &[FieldLine], response_fields: &[FieldLine]) -> Self {
        Self {
            request: parse_cache_control(request_fields),
            response: parse_cache_control(response_fields),
            vary: parse_vary(response_fields),
            set_cookie: has_set_cookie(response_fields),
        }
    }
}

fn decide_store(case: &StoreCase) -> StoreDecision {
    let parts = StoreParts::new(&case.request_fields, &case.response_fields);
    store_decision(&StoreInput {
        status: case.status,
        request: &parts.request,
        response: &parts.response,
        vary: &parts.vary,
        credentials: case.credentials,
        set_cookie: parts.set_cookie,
        authenticated_cache: case.authenticated_cache,
    })
}

#[cfg(feature = "legacy")]
fn run_legacy(case: &LegacyCase) -> Result<(), String> {
    use rivqen_cache::header::parse_cache_offline;
    use rivqen_cache::policy::legacy::{LegacyCacheOutcome, legacy_cache_outcome};

    let parts = StoreParts::new(&case.request_fields, &case.response_fields);
    let store = store_decision(&StoreInput {
        status: case.status,
        request: &parts.request,
        response: &parts.response,
        vary: &parts.vary,
        credentials: case.credentials,
        set_cookie: parts.set_cookie,
        authenticated_cache: case.authenticated_cache,
    });
    let offline = parse_cache_offline(case.cache_offline.as_deref());
    let got = legacy_cache_outcome(offline, store, case.unavailable_time_ms);
    let want = LegacyCacheOutcome {
        commit: case.expect.commit,
        remove_entry: case.expect.remove_entry,
        disable_key_ms: case.expect.disable_key_ms,
    };
    if got == want {
        Ok(())
    } else {
        Err(format!(
            "legacy outcome {got:?}, expected {want:?} (store {store:?})"
        ))
    }
}

/// What happened to one row.
enum Ran {
    /// The row ran and gave its expected value.
    Passed,
    /// The row did not run: its decision is not built (feature `legacy`).
    #[cfg_attr(feature = "legacy", allow(dead_code))]
    NotBuilt,
}

fn check<T: Copy + PartialEq + std::fmt::Debug>(what: &str, got: T, want: T) -> Result<(), String> {
    if got == want {
        Ok(())
    } else {
        Err(format!("{what} {got:?}, expected {want:?}"))
    }
}

fn run(row: &PolicyRow) -> Result<Ran, String> {
    match &row.case {
        Case::Lookup(case) => {
            let policy = entry_policy(&case.response_fields, case.times, case.manifest);
            let now = case.times.now;
            check(
                "lifetime",
                freshness_lifetime(&policy),
                case.expect.lifetime,
            )?;
            check("age", current_age(&policy, now), case.expect.age)?;
            check(
                "use",
                lookup_use(&policy, now, false),
                expected_use(case.expect.entry_use),
            )?;
            Ok(Ran::Passed)
        }
        Case::Failure(case) => {
            let policy = entry_policy(&case.response_fields, case.times, case.manifest);
            let failure = match case.failure {
                FailureKind::Network => Failure::Network,
                FailureKind::Tls => Failure::Tls,
            };
            check(
                "serve",
                serve_on_failure(&policy, case.times.now, failure, case.sensitive),
                case.serve,
            )?;
            Ok(Ran::Passed)
        }
        Case::Store(case) => {
            check("store", decision_expect(decide_store(case)), case.expect)?;
            Ok(Ran::Passed)
        }
        Case::Legacy(case) => run_legacy_row(case),
    }
}

#[cfg(feature = "legacy")]
fn run_legacy_row(case: &LegacyCase) -> Result<Ran, String> {
    run_legacy(case).map(|()| Ran::Passed)
}

#[cfg(not(feature = "legacy"))]
#[allow(clippy::unnecessary_wraps)] // the same signature as with the feature
fn run_legacy_row(_case: &LegacyCase) -> Result<Ran, String> {
    Ok(Ran::NotBuilt)
}

#[test]
fn policy_matrix() {
    let files = load().expect("the policy matrix loads");
    let kinds: Vec<Kind> = files.iter().map(|file| file.kind).collect();
    assert_eq!(kinds, Kind::ALL, "one file for each kind, in order");
    let mut failures = Vec::new();
    for file in &files {
        assert!(!file.rows.is_empty(), "{} has no row", file.path.display());
        let mut passed = 0_usize;
        let mut not_built = 0_usize;
        let mut failed = 0_usize;
        for row in &file.rows {
            assert_eq!(row.case.kind(), file.kind, "{}: kind of the file", row.id);
            match run(row) {
                Ok(Ran::Passed) => passed += 1,
                Ok(Ran::NotBuilt) => not_built += 1,
                Err(message) => {
                    failed += 1;
                    failures.push(format!("{}: {message}", row.id));
                }
            }
        }
        let skipped_ok = file.kind == Kind::Legacy && cfg!(not(feature = "legacy"));
        if skipped_ok {
            assert_eq!(
                not_built,
                file.rows.len(),
                "legacy rows without the feature"
            );
        } else {
            assert_eq!(
                not_built,
                0,
                "{}: rows of a kind that the test does not run",
                file.path.display()
            );
            assert_eq!(
                passed + failed,
                file.rows.len(),
                "{}: every row ran",
                file.path.display()
            );
        }
    }
    assert!(
        failures.is_empty(),
        "rows that fail:\n{}",
        failures.join("\n")
    );
}
