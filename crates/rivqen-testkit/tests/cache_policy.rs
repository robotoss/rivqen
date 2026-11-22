// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! Tests of the cache policy matrix loader and of the committed matrix files
//! (WP-07 S1, T-04; DL-182).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use rivqen_cache::partition::Credentials;
use rivqen_model::http::FieldLine;
use rivqen_model::time::UnixTime;
use rivqen_testkit::cache_policy::{
    Case, ExpectedSkip, ExpectedUse, FailureKind, Kind, LegacyExpect, LookupExpect, ManifestPolicy,
    StoreExpect, Times, load, load_at,
};
use rivqen_testkit::fixtures::FixtureError;
use rivqen_testkit::fixtures_root;
use serde_json::Value;

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    fs::remove_dir_all(&dir).ok();
    fs::create_dir_all(&dir).unwrap();
    dir
}

const LOOKUP: &str = r#"{"id": "POL-LOOKUP-001", "kind": "lookup", "source": ["RFC 9111 §4.2.1"], "note": "n",
  "response_fields": [["date", "Tue, 14 Nov 2023 22:13:20 GMT"], ["cache-control", "max-age=600"]],
  "request_time": 1699999996, "response_time": 1700000000, "now": 1699999999,
  "manifest": {"offline": true}, "expect": {"lifetime": 600, "age": null, "use": "revalidate"}}"#;
const FAILURE: &str = r#"{"id": "POL-FAILURE-001", "kind": "failure", "source": ["RFC 5861 §4"], "note": "n",
  "response_fields": [], "request_time": 1, "response_time": 2, "now": 3,
  "manifest": null, "failure": "tls", "sensitive": true, "expect": {"serve": false}}"#;
const STORE: &str = r#"{"id": "POL-STORE-001", "kind": "store", "source": ["cache policy §2 rule 1"], "note": "n",
  "response_fields": [["Cache-Control", "no-store"]], "request_fields": [["cache-control", "no-cache"]],
  "status": 200, "credentials": "unknown", "authenticated_cache": true,
  "expect": {"store": false, "reason": "no-store"}}"#;
const LEGACY: &str = r#"{"id": "POL-LEGACY-001", "kind": "legacy", "source": ["CL-19"], "note": "n",
  "response_fields": [], "request_fields": [], "status": 503, "credentials": "absent",
  "authenticated_cache": false, "cache_offline": null, "unavailable_time_ms": 60000,
  "expect": {"commit": false, "remove_entry": true, "disable_key_ms": null}}"#;

/// A directory with the four files, each with the given rows.
fn matrix(
    name: &str,
    lookup: &[&str],
    failure: &[&str],
    store: &[&str],
    legacy: &[&str],
) -> PathBuf {
    let root = scratch(name);
    for (file, rows) in [
        ("lookup.json", lookup),
        ("failure.json", failure),
        ("store.json", store),
        ("legacy.json", legacy),
    ] {
        fs::write(root.join(file), format!("[{}]\n", rows.join(",\n"))).unwrap();
    }
    root
}

fn valid(name: &str) -> PathBuf {
    matrix(name, &[LOOKUP], &[FAILURE], &[STORE], &[LEGACY])
}

/// Expects a `Shape` error for `file` of `root` whose text holds `needle`.
fn assert_shape(result: Result<impl std::fmt::Debug, FixtureError>, path: &Path, needle: &str) {
    match result.unwrap_err() {
        FixtureError::Shape { path: got, what } => {
            assert_eq!(got, path, "{what}");
            assert!(
                what.contains(needle),
                "`{what}` does not contain `{needle}`"
            );
        }
        other => panic!("{other}"),
    }
}

#[test]
fn cache_policy_loader_reads_typed_rows() {
    let root = valid("policy_typed");
    fs::write(root.join("README.md"), "not a matrix file").unwrap();
    let files = load_at(&root).unwrap();
    let kinds: Vec<Kind> = files.iter().map(|f| f.kind).collect();
    assert_eq!(kinds, Kind::ALL);
    for file in &files {
        assert_eq!(file.path, root.join(file.kind.file_name()));
        assert_eq!(file.rows.len(), 1);
        assert_eq!(file.rows[0].case.kind(), file.kind);
        assert_eq!(file.rows[0].note, "n");
    }

    let Case::Lookup(lookup) = &files[0].rows[0].case else {
        panic!()
    };
    assert_eq!(files[0].rows[0].id, "POL-LOOKUP-001");
    assert_eq!(files[0].rows[0].source, ["RFC 9111 §4.2.1"]);
    assert_eq!(
        lookup.response_fields,
        [
            FieldLine::new("date", "Tue, 14 Nov 2023 22:13:20 GMT"),
            FieldLine::new("cache-control", "max-age=600"),
        ]
    );
    assert_eq!(
        lookup.times,
        Times {
            request_time: UnixTime(1_699_999_996),
            response_time: UnixTime(1_700_000_000),
            now: UnixTime(1_699_999_999),
        }
    );
    assert_eq!(
        lookup.manifest,
        Some(ManifestPolicy {
            ttl_seconds: None,
            offline: Some(true),
        })
    );
    assert_eq!(
        lookup.expect,
        LookupExpect {
            lifetime: 600,
            age: None,
            entry_use: ExpectedUse::Revalidate,
        }
    );

    let Case::Failure(failure) = &files[1].rows[0].case else {
        panic!()
    };
    assert_eq!(failure.response_fields, []);
    assert_eq!(failure.manifest, None);
    assert_eq!(failure.failure, FailureKind::Tls);
    assert!(failure.sensitive);
    assert!(!failure.serve);

    let Case::Store(store) = &files[2].rows[0].case else {
        panic!()
    };
    assert_eq!(
        store.response_fields,
        [FieldLine::new("Cache-Control", "no-store")]
    );
    assert_eq!(
        store.request_fields,
        [FieldLine::new("cache-control", "no-cache")]
    );
    assert_eq!(store.status, 200);
    assert_eq!(store.credentials, Credentials::Unknown);
    assert!(store.authenticated_cache);
    assert_eq!(store.expect, StoreExpect::Skip(ExpectedSkip::NoStore));

    let Case::Legacy(legacy) = &files[3].rows[0].case else {
        panic!()
    };
    assert_eq!(legacy.status, 503);
    assert_eq!(legacy.credentials, Credentials::Absent);
    assert!(!legacy.authenticated_cache);
    assert_eq!(legacy.cache_offline, None);
    assert_eq!(legacy.unavailable_time_ms, 60_000);
    assert_eq!(
        legacy.expect,
        LegacyExpect {
            commit: false,
            remove_entry: true,
            disable_key_ms: None,
        }
    );
}

#[test]
fn cache_policy_loader_reads_every_enum_value() {
    let mut lookup = Vec::new();
    for (n, entry_use) in ["fresh", "stale-while-revalidate", "revalidate"]
        .iter()
        .enumerate()
    {
        lookup.push(
            LOOKUP
                .replace("POL-LOOKUP-001", &format!("POL-LOOKUP-00{}", n + 1))
                .replace("\"revalidate\"", &format!("\"{entry_use}\""))
                .replace("\"age\": null", "\"age\": 7")
                .replace("{\"offline\": true}", "{\"ttl_seconds\": 31536000}"),
        );
    }
    let failure = FAILURE
        .replace("\"tls\"", "\"network\"")
        .replace("false}", "true}");
    let mut store = Vec::new();
    for (n, reason) in ["vary-any", "credentialed", "status"].iter().enumerate() {
        store.push(
            STORE
                .replace("POL-STORE-001", &format!("POL-STORE-00{}", n + 1))
                .replace("\"no-store\"}", &format!("\"{reason}\"}}"))
                .replace(
                    "\"unknown\"",
                    ["\"absent\"", "\"present\"", "\"unknown\""][n],
                ),
        );
    }
    store.push(STORE.replace("POL-STORE-001", "POL-STORE-004").replace(
        r#"{"store": false, "reason": "no-store"}"#,
        r#"{"store": true}"#,
    ));
    let legacy = LEGACY
        .replace("\"cache_offline\": null", "\"cache_offline\": \"\"")
        .replace("\"disable_key_ms\": null", "\"disable_key_ms\": 60000");
    let lookup: Vec<&str> = lookup.iter().map(String::as_str).collect();
    let store: Vec<&str> = store.iter().map(String::as_str).collect();
    let root = matrix("policy_enums", &lookup, &[&failure], &store, &[&legacy]);
    let files = load_at(&root).unwrap();

    let uses: Vec<(ExpectedUse, Option<u64>, Option<u64>)> = files[0]
        .rows
        .iter()
        .map(|row| match &row.case {
            Case::Lookup(c) => (
                c.expect.entry_use,
                c.expect.age,
                c.manifest.unwrap().ttl_seconds,
            ),
            _ => panic!(),
        })
        .collect();
    assert_eq!(
        uses,
        [
            (ExpectedUse::Fresh, Some(7), Some(31_536_000)),
            (ExpectedUse::StaleWhileRevalidate, Some(7), Some(31_536_000)),
            (ExpectedUse::Revalidate, Some(7), Some(31_536_000)),
        ]
    );
    let Case::Failure(f) = &files[1].rows[0].case else {
        panic!()
    };
    assert_eq!((f.failure, f.serve), (FailureKind::Network, true));
    let stores: Vec<(StoreExpect, Credentials)> = files[2]
        .rows
        .iter()
        .map(|row| match &row.case {
            Case::Store(c) => (c.expect, c.credentials),
            _ => panic!(),
        })
        .collect();
    assert_eq!(
        stores,
        [
            (
                StoreExpect::Skip(ExpectedSkip::VaryAny),
                Credentials::Absent
            ),
            (
                StoreExpect::Skip(ExpectedSkip::Credentialed),
                Credentials::Present
            ),
            (
                StoreExpect::Skip(ExpectedSkip::Status),
                Credentials::Unknown
            ),
            (StoreExpect::Store, Credentials::Unknown),
        ]
    );
    let Case::Legacy(l) = &files[3].rows[0].case else {
        panic!()
    };
    assert_eq!(l.cache_offline.as_deref(), Some(""));
    assert_eq!(l.expect.disable_key_ms, Some(60_000));
}

#[test]
#[allow(clippy::too_many_lines)] // one table of cases; rustfmt writes each case on five lines
fn cache_policy_loader_names_the_file_and_the_row_of_each_error() {
    // (file, the broken row, a text the error must hold). Each case breaks one
    // rule of the README in an otherwise valid matrix.
    let lookup = |from: &str, to: &str| LOOKUP.replacen(from, to, 1);
    let failure = |from: &str, to: &str| FAILURE.replacen(from, to, 1);
    let store = |from: &str, to: &str| STORE.replacen(from, to, 1);
    let legacy = |from: &str, to: &str| LEGACY.replacen(from, to, 1);
    let cases: Vec<(Kind, String, &str)> = vec![
        (
            Kind::Lookup,
            lookup("\"note\"", "\"extra\": 1, \"note\""),
            "unknown member `extra`",
        ),
        (
            Kind::Lookup,
            lookup("\"now\": 1699999999,", ""),
            "`now` is missing",
        ),
        (
            Kind::Lookup,
            lookup("\"now\": 1699999999", "\"now\": -1"),
            "`now` is not",
        ),
        (
            Kind::Lookup,
            lookup("\"kind\": \"lookup\"", "\"kind\": \"store\""),
            "`kind` is `store`",
        ),
        (
            Kind::Lookup,
            lookup("POL-LOOKUP-001", "POL-STORE-001"),
            "`id` is not POL-LOOKUP-<NNN>",
        ),
        (
            Kind::Lookup,
            lookup("POL-LOOKUP-001", "POL-LOOKUP-1"),
            "`id` is not",
        ),
        (
            Kind::Lookup,
            lookup("[\"RFC 9111 §4.2.1\"]", "[]"),
            "`source`",
        ),
        (
            Kind::Lookup,
            lookup("[\"RFC 9111 §4.2.1\"]", "\"RFC 9111 §4.2.1\""),
            "`source`",
        ),
        (
            Kind::Lookup,
            lookup("[\"RFC 9111 §4.2.1\"]", "[\" \"]"),
            "`source`",
        ),
        (
            Kind::Lookup,
            lookup("\"note\": \"n\"", "\"note\": \"\""),
            "`note` is empty",
        ),
        (
            Kind::Lookup,
            lookup("\"use\": \"revalidate\"", "\"use\": \"stale\""),
            "`use` is not",
        ),
        (
            Kind::Lookup,
            lookup("\"age\": null", "\"age\": \"1\""),
            "`age` is not",
        ),
        (
            Kind::Lookup,
            lookup("\"lifetime\": 600, ", ""),
            "`lifetime` is missing",
        ),
        (
            Kind::Lookup,
            lookup(
                "\"use\": \"revalidate\"",
                "\"use\": \"revalidate\", \"x\": 1",
            ),
            "`expect` has an unknown member `x`",
        ),
        (
            Kind::Lookup,
            lookup("{\"offline\": true}", "{}"),
            "`manifest` has no member",
        ),
        (
            Kind::Lookup,
            lookup("{\"offline\": true}", "{\"mode\": \"public\"}"),
            "unknown member `mode`",
        ),
        (
            Kind::Lookup,
            lookup("{\"offline\": true}", "{\"ttl_seconds\": 31536001}"),
            "`manifest.ttl_seconds`",
        ),
        (
            Kind::Lookup,
            lookup("{\"offline\": true}", "{\"offline\": 1}"),
            "`manifest.offline`",
        ),
        (
            Kind::Lookup,
            lookup("{\"offline\": true}", "false"),
            "`manifest` is not null or an object",
        ),
        (
            Kind::Lookup,
            lookup(
                "[\"cache-control\", \"max-age=600\"]",
                "[\"cache-control\"]",
            ),
            "`response_fields` has an entry",
        ),
        (
            Kind::Lookup,
            lookup("[\"cache-control\", \"max-age=600\"]", "[\"\", \"x\"]"),
            "`response_fields` has an entry",
        ),
        (
            Kind::Failure,
            failure("\"tls\"", "\"timeout\""),
            "`failure` is not",
        ),
        (
            Kind::Failure,
            failure("\"sensitive\": true", "\"sensitive\": \"yes\""),
            "`sensitive` is not a boolean",
        ),
        (
            Kind::Failure,
            failure("{\"serve\": false}", "[false]"),
            "`expect` is not an object",
        ),
        (
            Kind::Store,
            store("\"unknown\"", "\"maybe\""),
            "`credentials` is not",
        ),
        (
            Kind::Store,
            store("\"status\": 200", "\"status\": 99"),
            "`status` is not a status code",
        ),
        (
            Kind::Store,
            store("\"status\": 200", "\"status\": 600"),
            "`status` is not a status code",
        ),
        (
            Kind::Store,
            store("\"reason\": \"no-store\"", "\"reason\": \"private\""),
            "`reason` is not",
        ),
        (
            Kind::Store,
            store(", \"reason\": \"no-store\"", ""),
            "`reason` is missing",
        ),
        (
            Kind::Store,
            store("\"store\": false", "\"store\": true"),
            "`reason` with `store: true`",
        ),
        (
            Kind::Store,
            store(
                "\"request_fields\": [[\"cache-control\", \"no-cache\"]]",
                "\"request_fields\": {}",
            ),
            "`request_fields` is not an array",
        ),
        (
            Kind::Store,
            store("\"note\"", "\"now\": 1, \"note\""),
            "unknown member `now` for kind store",
        ),
        (
            Kind::Legacy,
            legacy("\"cache_offline\": null", "\"cache_offline\": true"),
            "`cache_offline` is not",
        ),
        (
            Kind::Legacy,
            legacy("\"unavailable_time_ms\": 60000,", ""),
            "`unavailable_time_ms` is missing",
        ),
        (
            Kind::Legacy,
            legacy("\"disable_key_ms\": null", "\"disable_key_ms\": -5"),
            "`disable_key_ms` is not",
        ),
        (
            Kind::Legacy,
            legacy("\"remove_entry\": true, ", ""),
            "`remove_entry` is missing",
        ),
        (
            Kind::Legacy,
            legacy("\"note\"", "\"manifest\": null, \"note\""),
            "unknown member `manifest` for kind legacy",
        ),
    ];
    for (n, (kind, row, needle)) in cases.iter().enumerate() {
        let rows = |k: Kind, default: &'static str| -> Vec<&str> {
            vec![if k == *kind { row.as_str() } else { default }]
        };
        let root = matrix(
            &format!("policy_error_{n}"),
            &rows(Kind::Lookup, LOOKUP),
            &rows(Kind::Failure, FAILURE),
            &rows(Kind::Store, STORE),
            &rows(Kind::Legacy, LEGACY),
        );
        let path = root.join(kind.file_name());
        let error = load_at(&root).unwrap_err();
        let FixtureError::Shape { path: got, what } = &error else {
            panic!("case {n}: {error}")
        };
        assert_eq!(*got, path, "case {n}: {what}");
        assert!(
            what.contains(needle),
            "case {n}: `{what}` does not contain `{needle}`"
        );
        // The row is named by its id; a broken id is named as it is in the file.
        let id = serde_json::from_str::<Value>(row).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(
            what.contains(&format!("row {id}")),
            "case {n}: `{what}` does not name the row"
        );
    }
}

#[test]
fn cache_policy_loader_names_a_row_without_an_id_by_its_index() {
    let second = LOOKUP.replace("\"id\": \"POL-LOOKUP-001\", ", "");
    let root = matrix(
        "policy_no_id",
        &[LOOKUP, &second],
        &[FAILURE],
        &[STORE],
        &[LEGACY],
    );
    assert_shape(
        load_at(&root),
        &root.join("lookup.json"),
        "row 1 (by index): `id` is missing",
    );

    let root = matrix(
        "policy_not_object",
        &[LOOKUP],
        &[FAILURE, "7"],
        &[STORE],
        &[LEGACY],
    );
    assert_shape(
        load_at(&root),
        &root.join("failure.json"),
        "row 1 (by index): not an object",
    );
}

#[test]
fn cache_policy_loader_requires_unique_sorted_ids() {
    let first = LEGACY.replace("POL-LEGACY-001", "POL-LEGACY-002");
    let root = matrix(
        "policy_unsorted",
        &[LOOKUP],
        &[FAILURE],
        &[STORE],
        &[&first, LEGACY],
    );
    assert_shape(
        load_at(&root),
        &root.join("legacy.json"),
        "row POL-LEGACY-001: the ids are not unique or not sorted (it follows POL-LEGACY-002)",
    );

    let root = matrix(
        "policy_duplicate",
        &[LOOKUP],
        &[FAILURE],
        &[STORE, STORE],
        &[LEGACY],
    );
    assert_shape(
        load_at(&root),
        &root.join("store.json"),
        "row POL-STORE-001: the ids are not unique",
    );

    let second = STORE.replace("POL-STORE-001", "POL-STORE-002");
    let root = matrix(
        "policy_sorted",
        &[LOOKUP],
        &[FAILURE],
        &[STORE, &second],
        &[LEGACY],
    );
    assert_eq!(load_at(&root).unwrap()[2].rows.len(), 2);
}

#[test]
fn cache_policy_loader_rejects_a_json_file_that_no_kind_runs() {
    let root = valid("policy_extra_file");
    fs::write(root.join("vary.json"), "[]").unwrap();
    assert_shape(
        load_at(&root),
        &root.join("vary.json"),
        "not a file of the policy matrix",
    );

    // The extension compares ASCII case-insensitively. (A name that differs
    // from a matrix file only in case is not used: the file system can fold
    // case.)
    let root = valid("policy_extra_upper");
    fs::write(root.join("Vary.JSON"), "[]").unwrap();
    assert_shape(
        load_at(&root),
        &root.join("Vary.JSON"),
        "not a file of the policy matrix",
    );

    // A file that is not JSON is not a matrix file.
    let root = valid("policy_other_file");
    fs::write(root.join("notes.txt"), "x").unwrap();
    assert_eq!(load_at(&root).unwrap().len(), 4);
}

#[test]
fn cache_policy_loader_names_the_path_of_a_missing_or_broken_file() {
    let root = valid("policy_missing");
    fs::remove_file(root.join("store.json")).unwrap();
    assert!(matches!(
        load_at(&root).unwrap_err(),
        FixtureError::Io { path, .. } if path == root.join("store.json")
    ));

    let root = valid("policy_bad_json");
    fs::write(root.join("failure.json"), "[{").unwrap();
    assert!(matches!(
        load_at(&root).unwrap_err(),
        FixtureError::Json { path, .. } if path == root.join("failure.json")
    ));

    let root = valid("policy_not_array");
    fs::write(root.join("legacy.json"), "{}").unwrap();
    assert_shape(
        load_at(&root),
        &root.join("legacy.json"),
        "not a JSON array of rows",
    );

    let missing = scratch("policy_no_root").join("absent");
    assert!(matches!(
        load_at(&missing).unwrap_err(),
        FixtureError::Io { path, .. } if path == missing
    ));
}

// ---------------------------------------------------------------- the committed files

/// The seconds of an IMF-fixdate (RFC 9110 §5.6.7), or `None`. Test code: it
/// checks that the dates of the matrix are real dates with the right day name.
fn imf_fixdate(text: &str) -> Option<i64> {
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let parts: Vec<&str> = text.split(' ').collect();
    let [day_name, day, month, year, clock, "GMT"] = parts.as_slice() else {
        return None;
    };
    let day_name = day_name.strip_suffix(',')?;
    let day: i64 = day.parse().ok().filter(|_| day.len() == 2)?;
    let month = i64::try_from(MONTHS.iter().position(|m| m == month)?).ok()? + 1;
    let year: i64 = year.parse().ok().filter(|_| year.len() == 4)?;
    let hms: Vec<i64> = clock
        .split(':')
        .map(|p| p.parse().ok().filter(|_| p.len() == 2))
        .collect::<Option<_>>()?;
    let [h, m, s] = hms.as_slice() else {
        return None;
    };
    // Days from civil (proleptic Gregorian).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let seconds = days * 86_400 + h * 3600 + m * 60 + s;
    // Round trip: the day name and the ranges agree.
    let back_days = seconds.div_euclid(86_400);
    let ok = DAYS[usize::try_from(back_days.rem_euclid(7)).ok()?] == day_name
        && (1..=31).contains(&day)
        && *h < 24
        && *m < 60
        && *s < 60;
    ok.then_some(seconds)
}

#[test]
fn imf_fixdate_helper_reads_known_dates() {
    // RFC 9110 §5.6.7 example and the epoch.
    assert_eq!(
        imf_fixdate("Sun, 06 Nov 1994 08:49:37 GMT"),
        Some(784_111_777)
    );
    assert_eq!(imf_fixdate("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
    assert_eq!(imf_fixdate("Mon, 06 Nov 1994 08:49:37 GMT"), None);
    assert_eq!(imf_fixdate("0"), None);
}

fn field<'a>(fields: &'a [FieldLine], name: &str) -> Vec<&'a str> {
    fields
        .iter()
        .filter(|f| f.is(name))
        .map(|f| f.value.as_str())
        .collect()
}

/// A source names a section of a document that README rule 1 (or its file
/// table) names, or a rule id.
fn names_a_section(source: &str) -> bool {
    const DOCUMENTS: [&str; 10] = [
        "RFC 9111 §",
        "RFC 9110 §",
        "RFC 5861 §",
        "cache policy §",
        "negotiation §",
        "manifest-patch §",
        "legacy-wire §",
        "legacy-client §",
        "cache identity §",
        "cache index §",
    ];
    let rule = |prefix: &str, digits: usize| {
        source.strip_prefix(prefix).is_some_and(|rest| {
            rest.len() >= digits && rest.bytes().take(digits).all(|b| b.is_ascii_digit())
        })
    };
    DOCUMENTS.iter().any(|d| {
        source
            .strip_prefix(d)
            .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
    }) || rule("CL-", 2)
        || rule("DL-", 3)
}

#[test]
fn cache_policy_files_follow_the_readme() {
    let files = load().unwrap();
    let root = fixtures_root().join("cache").join("policy");

    // The four files, in the README order; each row has the kind of its file.
    assert_eq!(files.iter().map(|f| f.kind).collect::<Vec<_>>(), Kind::ALL);
    let counts: Vec<usize> = files.iter().map(|f| f.rows.len()).collect();
    assert_eq!(counts, [64, 24, 30, 33], "update the counts with the rows");
    assert!(counts.iter().sum::<usize>() >= 60);

    let mut ids = BTreeSet::new();
    for file in &files {
        assert_eq!(file.path, root.join(file.kind.file_name()));
        // Text: UTF-8, LF line ends, two spaces of indentation.
        let text = fs::read_to_string(&file.path).unwrap();
        assert!(text.ends_with("]\n"), "{}", file.path.display());
        for (n, line) in text.lines().enumerate() {
            let at = format!("{}:{}", file.path.display(), n + 1);
            assert!(!line.contains('\r') && !line.contains('\t'), "{at}");
            assert_eq!(line, line.trim_end(), "{at}: trailing white space");
            let indent = line.len() - line.trim_start_matches(' ').len();
            assert_eq!(indent % 2, 0, "{at}: indentation");
        }
        for row in &file.rows {
            assert_eq!(row.case.kind(), file.kind, "{}", row.id);
            // Ids are unique in all files; the loader checks the order.
            assert!(ids.insert(row.id.clone()), "{}", row.id);
            assert!(!row.source.is_empty(), "{}", row.id);
            for source in &row.source {
                assert!(
                    names_a_section(source),
                    "{}: `{source}` names no section",
                    row.id
                );
            }
            assert!(row.note.ends_with('.'), "{}: one sentence", row.id);
            check_dates(&row.id, &row.case);
        }
    }
}

/// Rule 4: the dates in field values are real IMF-fixdates; in a lookup or a
/// failure row the `Date` field is not before `request_time` minus a day and
/// not after `now` plus a day, so it belongs to the numbers of the row. An
/// `Expires` value that is not an IMF-fixdate is an invalid date on purpose
/// (RFC 9111 §5.3) and must not look like one.
fn check_dates(id: &str, case: &Case) {
    let (fields, times) = match case {
        Case::Lookup(c) => (&c.response_fields, Some(c.times)),
        Case::Failure(c) => (&c.response_fields, Some(c.times)),
        Case::Store(c) => (&c.response_fields, None),
        Case::Legacy(c) => {
            // The member `cache_offline` gives the value; no field line repeats it.
            assert!(
                field(&c.response_fields, "cache-offline").is_empty(),
                "{id}"
            );
            (&c.response_fields, None)
        }
    };
    for date in field(fields, "date") {
        let seconds = imf_fixdate(date).unwrap_or_else(|| panic!("{id}: Date `{date}`"));
        if let Some(t) = times {
            let low = i64::try_from(t.request_time.0).unwrap() - 86_400;
            let high = i64::try_from(t.now.0.max(t.response_time.0)).unwrap() + 86_400;
            assert!((low..=high).contains(&seconds), "{id}: Date `{date}`");
        }
    }
    for expires in field(fields, "expires") {
        if imf_fixdate(expires).is_none() {
            assert!(
                !expires.ends_with(" GMT"),
                "{id}: Expires `{expires}` is a broken date"
            );
        }
    }
    if times.is_some() {
        // DL-198 item b: the row that tests an absent `Date` has none.
        let dates = usize::from(!NO_DATE_ROWS.contains(&id));
        assert_eq!(field(fields, "date").len(), dates, "{id}: Date fields");
    }
}

/// The rows that test an `Expires` field without a `Date` field (DL-198 item b).
const NO_DATE_ROWS: [&str; 1] = ["POL-LOOKUP-050"];

#[test]
fn cache_policy_rows_cover_the_brief() {
    // Each acceptance item of T-04 has at least one row; the ids name them.
    let files = load().unwrap();
    let all: Vec<&Case> = files
        .iter()
        .flat_map(|f| f.rows.iter().map(|r| &r.case))
        .collect();
    let lookups = || {
        all.iter().filter_map(|c| match c {
            Case::Lookup(l) => Some(l),
            _ => None,
        })
    };
    // now before response_time: age null, revalidate.
    assert!(
        lookups().any(|l| l.expect.age.is_none() && l.expect.entry_use == ExpectedUse::Revalidate)
    );
    // Each use occurs.
    for entry_use in [
        ExpectedUse::Fresh,
        ExpectedUse::StaleWhileRevalidate,
        ExpectedUse::Revalidate,
    ] {
        assert!(
            lookups().any(|l| l.expect.entry_use == entry_use),
            "{entry_use:?}"
        );
    }
    // ttl_seconds 0.
    assert!(lookups().any(|l| l.manifest.and_then(|m| m.ttl_seconds) == Some(0)));
    // Failure: both kinds, both sensitive values, both answers.
    let failures: Vec<_> = all
        .iter()
        .filter_map(|c| match c {
            Case::Failure(f) => Some((f.failure, f.sensitive, f.serve)),
            _ => None,
        })
        .collect();
    for wanted in [
        (FailureKind::Tls, true, false),
        (FailureKind::Tls, false, true),
        (FailureKind::Network, false, true),
        (FailureKind::Network, false, false),
    ] {
        assert!(failures.contains(&wanted), "{wanted:?}");
    }
    // Store: every reason, and stores.
    let stores: BTreeSet<String> = all
        .iter()
        .filter_map(|c| match c {
            Case::Store(s) => Some(format!("{:?}", s.expect)),
            _ => None,
        })
        .collect();
    assert_eq!(stores.len(), 5, "{stores:?}");
    // Legacy: every cache-offline value with and without no-store.
    for value in [
        Some("true"),
        Some("false"),
        Some("store"),
        Some("http"),
        None,
        Some("yes"),
        Some("TRUE"),
    ] {
        for no_store in [false, true] {
            assert!(
                all.iter().any(|c| matches!(c, Case::Legacy(l)
                    if l.cache_offline.as_deref() == value
                        && field(&l.response_fields, "cache-control").contains(&"no-store") == no_store)),
                "{value:?} no-store={no_store}"
            );
        }
    }
}

#[test]
fn cache_policy_rows_cover_the_decisions_of_dl_198() {
    let files = load().unwrap();
    let all: Vec<&Case> = files
        .iter()
        .flat_map(|f| f.rows.iter().map(|r| &r.case))
        .collect();
    let lookups = || {
        all.iter().filter_map(|c| match c {
            Case::Lookup(l) => Some(l),
            _ => None,
        })
    };
    // DL-198: the rows of T-04b, by the case that each one decides.
    let lookup_with = |text: &str, expected: (u64, ExpectedUse)| {
        lookups().any(|l| {
            l.expect.lifetime == expected.0
                && l.expect.entry_use == expected.1
                && field(&l.response_fields, "cache-control")
                    .iter()
                    .any(|v| v.contains(text))
        })
    };
    // Item d: the window follows the bounded lifetime.
    assert!(lookup_with(
        "stale-while-revalidate=30",
        (100, ExpectedUse::StaleWhileRevalidate)
    ));
    // Item f: max-age=0 is explicit; no max-age gives no window.
    assert!(lookup_with(
        "max-age=0, stale-while-revalidate=60",
        (0, ExpectedUse::StaleWhileRevalidate)
    ));
    assert!(lookup_with(
        "stale-while-revalidate=60",
        (0, ExpectedUse::Revalidate)
    ));
    // Items b and c: Expires with no Date, and Expires before Date, give 0.
    assert!(
        lookups().any(|l| field(&l.response_fields, "date").is_empty()
            && !field(&l.response_fields, "expires").is_empty()
            && l.expect.lifetime == 0)
    );
    assert!(lookups().any(|l| {
        l.expect.lifetime == 0
            && field(&l.response_fields, "expires")
                .iter()
                .any(|e| imf_fixdate(e).is_some_and(|x| x < 1_700_000_000))
    }));
    // DL-197 item 4: the largest max-age, and two different values.
    assert!(lookups().any(|l| l.expect.lifetime == 2_147_483_648));
    assert!(lookup_with(
        "max-age=600, max-age=300",
        (0, ExpectedUse::Revalidate)
    ));
    // Items e, f, h: failure rows that do not serve, and the row before response_time.
    let failures_with = |text: &str, serve: bool| {
        all.iter().any(|c| {
            matches!(c, Case::Failure(f)
            if f.serve == serve
                && field(&f.response_fields, "cache-control").iter().any(|v| v.contains(text)))
        })
    };
    assert!(failures_with(
        "no-cache, max-age=600, stale-if-error",
        false
    ));
    assert!(failures_with("stale-if-error=1200", false));
    assert!(failures_with("max-age=0, stale-if-error=60", true));
    assert!(all.iter().any(|c| matches!(c, Case::Failure(f)
        if !f.serve && f.times.now.0 < f.times.response_time.0)));
}
