// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Golden tests of the legacy client functions against the canonical
//! function fixtures `fixtures/legacy/functions` (profile
//! `legacy-canonical`; legacy client CL-14, CL-15, CL-16, CL-24). Each test
//! asserts the number of its cases, so a fixture cannot drop out unseen. A
//! case that fails is reported with its id; no fixture file changes.

#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use rivqen_model::http::FieldLine;
use rivqen_proto_legacy::message::{MessageInput, ResultCode, message};
use rivqen_proto_legacy::response::{bare_etag, charset, is_legacy_response, is_template_change};
use rivqen_template::legacy::DataBlock;
use rivqen_testkit::functions::{self, FunctionFixture};
use serde_json::Value;

/// The canonical cases of one function.
fn cases(function: &str) -> Vec<FunctionFixture> {
    functions::load("canonical")
        .unwrap()
        .into_iter()
        .filter(|f| f.function == function)
        .collect()
}

/// The `value` member of the expected result.
fn expected_value(f: &FunctionFixture) -> &Value {
    assert_eq!(f.evidence, "rule", "{}", f.id);
    assert!(f.error.is_none(), "{}: an error is not expected", f.id);
    f.result
        .as_ref()
        .and_then(|r| r.get("value"))
        .unwrap_or_else(|| panic!("{}: no result value", f.id))
}

/// Field lines of a case: a list of `[name, value]` pairs (as `pairsOf` of
/// the generator; an object form is not used by the canonical cases).
fn field_lines(f: &FunctionFixture, value: &Value) -> Vec<FieldLine> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("{}: fields is not a list", f.id))
        .iter()
        .map(|pair| {
            let pair = pair.as_array().unwrap();
            assert_eq!(pair.len(), 2, "{}", f.id);
            FieldLine::new(pair[0].as_str().unwrap(), pair[1].as_str().unwrap())
        })
        .collect()
}

fn input_str<'a>(f: &'a FunctionFixture, member: &str) -> &'a str {
    f.input
        .get(member)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{}: no string {member}", f.id))
}

#[test]
fn legacy_fn_is_legacy_response() {
    let cases = cases("is_legacy_response");
    assert_eq!(cases.len(), 15);
    for f in &cases {
        let fields = field_lines(f, f.input.get("fields").unwrap());
        let expected = expected_value(f).as_bool().unwrap();
        assert_eq!(is_legacy_response(&fields), expected, "{}", f.id);
    }
}

#[test]
fn legacy_fn_etag() {
    let cases = cases("etag");
    assert_eq!(cases.len(), 37);
    for f in &cases {
        let expected = expected_value(f).as_str().unwrap();
        assert_eq!(bare_etag(input_str(f, "value")), expected, "{}", f.id);
    }
}

#[test]
fn legacy_fn_template_change() {
    let cases = cases("template_change");
    assert_eq!(cases.len(), 25);
    for f in &cases {
        let expected = expected_value(f).as_bool().unwrap();
        assert_eq!(
            is_template_change(input_str(f, "value")),
            expected,
            "{}",
            f.id
        );
    }
}

#[test]
fn legacy_fn_charset() {
    let cases = cases("charset");
    assert_eq!(cases.len(), 25);
    for f in &cases {
        let expected = expected_value(f).as_str().unwrap();
        assert_eq!(charset(input_str(f, "content_type")), expected, "{}", f.id);
    }
}

fn code(f: &FunctionFixture, member: &str) -> ResultCode {
    let value = f.input.get(member).and_then(Value::as_i64).unwrap();
    ResultCode::from_value(value).unwrap_or_else(|| panic!("{}: unknown code {value}", f.id))
}

#[test]
fn legacy_fn_message() {
    let cases = cases("message");
    assert_eq!(cases.len(), 25);
    let mut none = 0;
    for f in &cases {
        let diff_object = f.input.get("diff").and_then(Value::as_object).unwrap();
        // serde_json sorts object members by name; the cases have at most
        // one member, so the order of the text is kept.
        assert!(
            diff_object.len() <= 1,
            "{}: the diff order would be lost",
            f.id
        );
        let diff: Vec<DataBlock> = diff_object
            .iter()
            .map(|(key, value)| DataBlock {
                key: key.clone(),
                value: value.as_str().unwrap().to_owned(),
            })
            .collect();
        let fields = field_lines(f, f.input.get("fields").unwrap());
        let input = MessageInput {
            src: code(f, "src_code"),
            final_code: code(f, "final_code"),
            diff: &diff,
            fields: &fields,
            reload: false,
        };
        assert_eq!(f.evidence, "rule", "{}", f.id);
        let expected = f
            .result
            .as_ref()
            .and_then(|r| r.get("json"))
            .unwrap_or_else(|| panic!("{}: no json member", f.id));
        let got = message(&input);
        match expected {
            Value::Null => {
                none += 1;
                assert_eq!(got, None, "{}", f.id);
            }
            Value::String(text) => assert_eq!(got.as_deref(), Some(text.as_str()), "{}", f.id),
            other => panic!("{}: json is {other:?}", f.id),
        }
    }
    // FN-MSG-06 (final 200, empty diff) and FN-MSG-11 (final -1).
    assert_eq!(none, 2);
}
