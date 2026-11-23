// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Golden test of `diff_v1` against the canonical function fixtures `diff`
//! (`fixtures/legacy/functions`, CL-42, DL-160).
//!
//! The JSON objects of a case are data lists in text order. `serde_json`
//! here keeps object members sorted by key, so the test takes the order of
//! each object from the text of the file (`text_order`) and fails when it
//! cannot find a key. The token `value-not-string` comes from the conversion
//! of a JSON object to a data list: a `DataBlock` value is always a string
//! (module text of `rivqen_diff::legacy`).

#![cfg(feature = "legacy")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::fs;

use rivqen_diff::legacy::diff_v1;
use rivqen_template::legacy::DataBlock;
use rivqen_testkit::fixtures_root;
use rivqen_testkit::functions::{self, FunctionFixture};
use serde_json::{Map, Value};

/// The CL-42 token of a value that is not a string.
const VALUE_NOT_STRING: &str = "value-not-string";

/// The keys of `object` in the order of their first occurrence in `text`
/// after `anchor`.
fn text_order(text: &str, anchor: &str, object: &Map<String, Value>) -> Vec<String> {
    let start = text
        .find(anchor)
        .unwrap_or_else(|| panic!("anchor {anchor} not found"));
    let rest = &text[start..];
    let mut keyed: Vec<(usize, String)> = object
        .keys()
        .map(|key| {
            let quoted = serde_json::to_string(key).unwrap();
            let at = rest
                .find(&quoted)
                .unwrap_or_else(|| panic!("key {quoted} not found after {anchor}"));
            (at, key.clone())
        })
        .collect();
    keyed.sort();
    keyed.into_iter().map(|(_, key)| key).collect()
}

/// A JSON object as a data list in text order, or the CL-42 token.
fn data_list(object: &Map<String, Value>, order: &[String]) -> Result<Vec<DataBlock>, String> {
    order
        .iter()
        .map(|key| match &object[key] {
            Value::String(value) => Ok(DataBlock {
                key: key.clone(),
                value: value.clone(),
            }),
            _ => Err(VALUE_NOT_STRING.to_owned()),
        })
        .collect()
}

/// The text of one case of `cases.json`: one line per case.
fn case_line<'t>(cases_text: &'t str, id: &str) -> &'t str {
    let needle = format!("{{\"id\":\"{id}\",");
    let lines: Vec<&str> = cases_text
        .lines()
        .filter(|line| line.contains(&needle))
        .collect();
    assert_eq!(lines.len(), 1, "{id}: one line in cases.json");
    lines[0]
}

/// The text of one result of `legacy-canonical.json`: from its id to the
/// next id.
fn result_text<'t>(results_text: &'t str, id: &str) -> &'t str {
    let needle = format!("\"id\": \"{id}\"");
    let start = results_text
        .find(&needle)
        .unwrap_or_else(|| panic!("{id}: not in legacy-canonical.json"));
    let after = start + needle.len();
    let end = results_text[after..]
        .find("\"id\":")
        .map_or(results_text.len(), |at| after + at);
    &results_text[start..end]
}

/// The diff of one case as the fixture result form: the keys and values in
/// order, or the error token.
fn run(f: &FunctionFixture, line: &str) -> Result<Vec<(String, String)>, String> {
    let local = match &f.input["local"] {
        Value::Null => None,
        Value::Object(object) => {
            let order = text_order(line, "\"local\":", object);
            Some(data_list(object, &order)?)
        }
        other => panic!("{}: local is {other}", f.id),
    };
    let server_object = f.input["server"].as_object().unwrap();
    let server = data_list(
        server_object,
        &text_order(line, "\"server\":", server_object),
    )?;
    let diff = diff_v1(local.as_deref(), &server).map_err(|e| format!("{e:?}"))?;
    Ok(diff
        .into_iter()
        .map(|b| (b.key.clone(), b.value.clone()))
        .collect())
}

/// The expected value of one fixture in the same form.
fn expected(f: &FunctionFixture, text: &str) -> Result<Vec<(String, String)>, String> {
    if let Some(token) = &f.error {
        return Err(token.clone());
    }
    let diff = f.result.as_ref().unwrap()["diff"].as_object().unwrap();
    Ok(text_order(text, "\"diff\":", diff)
        .into_iter()
        .map(|key| (key.clone(), diff[&key].as_str().unwrap().to_owned()))
        .collect())
}

#[test]
fn legacy_fn_diff() {
    let dir = fixtures_root().join("legacy").join("functions");
    let cases_text = fs::read_to_string(dir.join("cases.json")).unwrap();
    let results_text = fs::read_to_string(dir.join("legacy-canonical.json")).unwrap();
    let mut failures = Vec::new();
    let (mut count, mut tokens, mut ordered) = (0, 0, 0);
    for f in functions::load("canonical").unwrap() {
        if f.function != "diff" {
            continue;
        }
        count += 1;
        let line = case_line(&cases_text, &f.id);
        // `local` comes before `server` in each case, so the anchor of
        // `server` does not see the keys of `local`.
        assert!(
            line.find("\"local\":") < line.find("\"server\":"),
            "{}",
            f.id
        );
        let want = expected(&f, result_text(&results_text, &f.id));
        let got = run(&f, line);
        if want.is_err() {
            tokens += 1;
        }
        if want.as_ref().is_ok_and(|diff| diff.len() > 1) {
            ordered += 1;
        }
        if got != want {
            failures.push(format!("{}: got {got:?}, want {want:?}", f.id));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(count, 24);
    // FN-DIFF-07 and FN-DIFF-08.
    assert_eq!(tokens, 2);
    // FN-DIFF-05, FN-DIFF-NEW-02, FN-DIFF-NEW-03: more than one entry, so
    // the order is compared too.
    assert_eq!(ordered, 3);
}

/// The order of the result is the server order, not the key order: a server
/// list in reverse key order gives a diff in reverse key order (D-25).
#[test]
fn legacy_fn_diff_keeps_server_order() {
    let block = |key: &str, value: &str| DataBlock {
        key: key.to_owned(),
        value: value.to_owned(),
    };
    let local = [block("{a}", "x"), block("{b}", "y")];
    let server = [block("{c}", "new"), block("{b}", "z"), block("{a}", "x2")];
    let diff = diff_v1(Some(&local), &server).unwrap();
    let keys: Vec<&str> = diff.iter().map(|b| b.key.as_str()).collect();
    assert_eq!(keys, ["{c}", "{b}", "{a}"]);
}
