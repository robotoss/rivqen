// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Golden tests of the legacy functions against the committed fixtures
//! (WP-06 S1, T-03; P-TPL-4). The fixtures are data; a fixture that does not
//! pass is reported with its id, and no fixture file changes.
//!
//! Which fixtures: the canonical server fixtures (no `--JAVA`, `--NODE`,
//! `--PHP` suffix) of `fixtures/legacy` and the cases of
//! `fixtures/malformed/legacy` that have a `split` member, and the canonical
//! `split` function fixtures. The server profile fixtures and the Android and
//! iOS results are out of scope (DL-159); the function fixtures for `diff`
//! and `etag` belong to WP-08 (DL-160).
//!
//! Fixtures with `split.applied = false` fall in two classes:
//!
//! - Scope rule (legacy wire C-01, C-03): the request method is not `GET`,
//!   the application status is not 200, or the application sends not exactly
//!   one `text/html; charset=utf-8` content type. The server does not call the
//!   split. `split_v1` still applies on the body; the test checks that the
//!   scope condition is in the data of the fixture and that `split_v1`
//!   applies.
//! - Body (C-03, C-07): the body is not UTF-8 (`InvalidUtf8`), a block name
//!   repeats (`DuplicateName`) or a block is named `title`
//!   (`BlockNamedTitle`). `split_v1` must give that reason.

#![cfg(feature = "legacy")]
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use rivqen_template::legacy::{
    DataBlock, NotApplied, SplitOutcome, etag_v1, render_v1, split_v1, template_tag_v1,
};
use rivqen_template::limits::TemplateLimits;
use rivqen_testkit::fixtures::{Fixture, load_legacy, load_malformed};
use rivqen_testkit::functions;
use serde_json::Value;

/// Why a fixture is not applied, as the test derives it from the data.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Class {
    Applied,
    Scope,
    Body(NotApplied),
}

/// The scope condition of C-01 and C-03 in the case data.
fn scope_condition(f: &Fixture) -> bool {
    let method = f
        .case
        .pointer("/request/method")
        .and_then(Value::as_str)
        .unwrap_or("GET");
    let status = f
        .case
        .pointer("/app/status")
        .and_then(Value::as_u64)
        .unwrap_or(200);
    let types: Vec<&str> = match f.case.pointer("/app/headers").and_then(Value::as_array) {
        None => vec!["text/html; charset=utf-8"],
        Some(lines) => lines
            .iter()
            .filter_map(|l| l.as_array())
            .filter(|l| {
                l[0].as_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case("content-type"))
            })
            .filter_map(|l| l[1].as_str())
            .collect(),
    };
    method != "GET" || status != 200 || types != ["text/html; charset=utf-8"]
}

/// The reason of a body class, from the rules of the case and the id.
fn body_reason(f: &Fixture) -> NotApplied {
    let rules: Vec<&str> = f
        .case
        .get("rules")
        .and_then(Value::as_array)
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if f.id.contains("DUPLICATE-BLOCKS") {
        NotApplied::DuplicateName
    } else if f.id.contains("BLOCK-NAMED-TITLE") {
        NotApplied::BlockNamedTitle
    } else {
        assert!(rules.contains(&"C-03"), "{}: rules {rules:?}", f.id);
        NotApplied::InvalidUtf8
    }
}

fn classify(f: &Fixture) -> Class {
    let applied = f
        .expected
        .pointer("/split/applied")
        .and_then(Value::as_bool)
        .unwrap();
    if applied {
        Class::Applied
    } else if scope_condition(f) {
        Class::Scope
    } else {
        Class::Body(body_reason(f))
    }
}

fn str_list(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

#[derive(Default)]
struct Tally {
    checked: usize,
    applied: usize,
    scope: Vec<String>,
    body: Vec<String>,
    field_checks: usize,
    data_checks: usize,
    failures: Vec<String>,
}

impl Tally {
    fn fail(&mut self, id: &str, what: impl std::fmt::Display) {
        self.failures.push(format!("{id}: {what}"));
    }

    fn check(&mut self, f: &Fixture) {
        self.checked += 1;
        let html = f.page.as_deref().unwrap();
        let limits = TemplateLimits::default();
        let outcome = match split_v1(html, &limits) {
            Ok(outcome) => outcome,
            Err(e) => return self.fail(&f.id, format!("split_v1 failed: {e:?}")),
        };
        let class = classify(f);
        match (class, outcome) {
            (Class::Applied, SplitOutcome::Applied(split)) => {
                self.applied += 1;
                self.applied_fixture(f, html, &split.template, &split.data);
            }
            (Class::Scope, SplitOutcome::Applied(_)) => self.scope.push(f.id.clone()),
            (Class::Body(want), SplitOutcome::NotApplied(got)) if want == got => {
                self.body.push(f.id.clone());
            }
            (class, outcome) => self.fail(&f.id, format!("{class:?} but {outcome:?}")),
        }
        if class != Class::Applied {
            // `null` members, nothing else to compare.
            let split = &f.expected["split"];
            let empty = split["template_sha1"].is_null()
                && split["rebuild_sha1"].is_null()
                && split["keys"].as_array().is_some_and(Vec::is_empty);
            if !empty {
                self.fail(&f.id, "a not applied split has template, keys or rebuild");
            }
        }
    }

    fn applied_fixture(&mut self, f: &Fixture, html: &[u8], template: &str, data: &[DataBlock]) {
        let id = f.id.as_str();
        let split = &f.expected["split"];
        let template_tag = template_tag_v1(template).to_string();
        if split["template_sha1"].as_str() != Some(template_tag.as_str()) {
            self.fail(id, format!("template_sha1 differs from {template_tag}"));
        }
        let keys: Vec<String> = data.iter().map(|b| b.key.clone()).collect();
        if str_list(&split["keys"]) != keys {
            self.fail(id, format!("keys differ from {keys:?}"));
        }
        let rebuilt = match render_v1(template, data, &TemplateLimits::default()) {
            Ok(page) => page,
            Err(e) => return self.fail(id, format!("render_v1 failed: {e:?}")),
        };
        let rebuild_sha1 = etag_v1(rebuilt.as_bytes()).to_string();
        if split["rebuild_sha1"].as_str() != Some(rebuild_sha1.as_str()) {
            self.fail(id, format!("rebuild_sha1 differs from {rebuild_sha1}"));
        }

        // Response fields: present only on a 200 response with a body.
        let etag = etag_v1(html).to_string();
        for (name, want) in [("etag", &etag), ("template-tag", &template_tag)] {
            let values = str_list(&f.expected["fields"][name]);
            if !values.is_empty() {
                self.field_checks += 1;
                if values != [want.as_str()] {
                    self.fail(id, format!("field {name} is {values:?}, not {want}"));
                }
            }
        }

        let body = &f.expected["body"];
        if body["kind"] == "data" {
            self.data_checks += 1;
            let expected = body["data"].as_object().unwrap();
            let same = expected.len() == data.len()
                && data.iter().all(|b| {
                    expected.get(&b.key).and_then(Value::as_str) == Some(b.value.as_str())
                });
            if !same {
                self.fail(id, "body.data differs from the data of the split");
            }
            if body["template_tag"].as_str() != Some(template_tag.as_str()) {
                self.fail(id, "body.template_tag differs");
            }
            if body["html_sha1"].as_str() != Some(etag.as_str()) {
                self.fail(id, "body.html_sha1 differs");
            }
        }
    }

    fn done(&self) {
        assert!(
            self.failures.is_empty(),
            "{} fixtures failed:\n{}",
            self.failures.len(),
            self.failures.join("\n")
        );
    }
}

#[test]
fn legacy_fixtures_pages() {
    let mut tally = Tally::default();
    for f in load_legacy().unwrap() {
        let profile_fixture = f.id.contains("--");
        if !profile_fixture && f.expected.get("split").is_some() {
            tally.check(&f);
        }
    }
    tally.done();
    assert_eq!(tally.checked, 59);
    assert_eq!(tally.applied, 50);
    // Scope rule C-01: HEAD, POST, status 404 and 302, a JSON content type
    // (PASS-NON-HTML), charset iso-8859-1 (CHARSET-OTHER).
    assert_eq!(
        tally.scope,
        [
            "FX-LEG-CHARSET-OTHER",
            "FX-LEG-PASS-HEAD",
            "FX-LEG-PASS-NON-HTML",
            "FX-LEG-PASS-NOT-FOUND",
            "FX-LEG-PASS-POST",
            "FX-LEG-REDIRECT",
        ]
    );
    // Body rules C-03 and C-07.
    assert_eq!(
        tally.body,
        [
            "FX-LEG-DUPLICATE-BLOCKS-DATA",
            "FX-LEG-DUPLICATE-BLOCKS-FIRST",
            "FX-LEG-INVALID-UTF8",
        ]
    );
    // Fields and bodies were really compared.
    assert!(tally.field_checks >= 2 * 30, "{}", tally.field_checks);
    assert!(tally.data_checks >= 10, "{}", tally.data_checks);
}

#[test]
fn legacy_fixtures_malformed() {
    let mut tally = Tally::default();
    for f in load_malformed().unwrap().cases {
        if f.expected.get("split").is_some() {
            tally.check(&f);
        }
    }
    tally.done();
    assert_eq!(tally.checked, 22);
    assert_eq!(tally.applied, 17);
    assert_eq!(tally.scope, ["FX-LEG-MAL-CONTENT-TYPE-TWO"]);
    assert_eq!(
        tally.body,
        [
            "FX-LEG-MAL-BLOCK-NAMED-TITLE",
            "FX-LEG-MAL-OVERLONG",
            "FX-LEG-MAL-SURROGATE",
            "FX-LEG-MAL-TRUNCATED-UTF8",
        ]
    );
}

#[test]
fn legacy_fixtures_functions() {
    let limits = TemplateLimits::default();
    let mut failures = Vec::new();
    let (mut results, mut errors) = (0, 0);
    for f in functions::load("canonical").unwrap() {
        if f.function != "split" {
            continue;
        }
        let html = f.input["html"].as_str().unwrap();
        let outcome = split_v1(html.as_bytes(), &limits).unwrap();
        match (&f.result, f.error.as_deref(), outcome) {
            (Some(want), None, SplitOutcome::Applied(split)) => {
                results += 1;
                let keys: Vec<String> = split.data.iter().map(|b| b.key.clone()).collect();
                if want["template"].as_str() != Some(split.template.as_str()) {
                    failures.push(format!("{}: template differs", f.id));
                }
                if f.data_keys.as_ref() != Some(&keys) {
                    failures.push(format!("{}: keys {keys:?} differ", f.id));
                }
                let data = want["data"].as_object().unwrap();
                let same = data.len() == split.data.len()
                    && split.data.iter().all(|b| {
                        data.get(&b.key).and_then(Value::as_str) == Some(b.value.as_str())
                    });
                if !same {
                    failures.push(format!("{}: data differs", f.id));
                }
            }
            (None, Some("duplicate-block-name"), SplitOutcome::NotApplied(got)) => {
                errors += 1;
                if f.id != "FN-SPLIT-09" || got != NotApplied::DuplicateName {
                    failures.push(format!("{}: got {got:?}", f.id));
                }
            }
            (want, error, got) => {
                failures.push(format!("{}: {want:?} {error:?} but {got:?}", f.id));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!((results, errors), (34, 1));
}
