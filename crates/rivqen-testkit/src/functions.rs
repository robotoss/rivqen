// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Loader of the legacy function fixtures `fixtures/legacy/functions`
//! (WP-06 S1, T-03): `cases.json` and the result file of one profile, as
//! data (DL-146).
//!
//! `cases.json` is a list of cases (`id`, `fn`, `input`, `divergences`). The
//! result file of a profile (`legacy-<profile>.json`) has a `results` list
//! (`id`, `fn`, `evidence`, and `result` or `error`). [`load`] joins them by
//! `id`, in the order of `cases.json`. A case without a result and a result
//! without a case are errors, so a fixture cannot drop out of a test
//! unseen. Every failure names the path of the file.
//!
//! `serde_json` here keeps object members sorted by key. The members of
//! `result.data` of a `split` case come in the order of the profile, and that
//! order is part of the result (CL-41). [`FunctionFixture::data_keys`] holds
//! it, read from the text of the file.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::fixtures::{FixtureError, load_json};
use crate::fixtures_root;

/// One function case joined with the result of one profile.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionFixture {
    /// The case id, for example `FN-SPLIT-01`.
    pub id: String,
    /// The function name (`fn` of the case), for example `split`.
    pub function: String,
    /// `input` of the case.
    pub input: Value,
    /// `divergences` of the case (empty when absent).
    pub divergences: Vec<String>,
    /// `evidence` of the result: `trace`, `rule` or `none`.
    pub evidence: String,
    /// `result` of the profile; `None` when absent or `null`.
    pub result: Option<Value>,
    /// `error` of the profile: the token of the failure; `None` when absent.
    pub error: Option<String>,
    /// The member names of `result.data` in file order, when the result has
    /// an object `data`.
    pub data_keys: Option<Vec<String>>,
}

/// The fixtures of one profile (`canonical`, `android`, `ios`) in the order
/// of `fixtures/legacy/functions/cases.json`.
///
/// # Errors
/// [`FixtureError`] with the path of the file that failed. See [`load_at`].
pub fn load(profile: &str) -> Result<Vec<FunctionFixture>, FixtureError> {
    load_at(&fixtures_root().join("legacy").join("functions"), profile)
}

/// The fixtures of the directory `root` for `legacy-<profile>.json`.
///
/// # Errors
/// [`FixtureError::Io`] when a file cannot be read, [`FixtureError::Json`]
/// when it is not JSON, [`FixtureError::Shape`] when `profile` is not made of
/// `a-z`, `0-9` and `-`, when a member is missing or mistyped, when an id
/// repeats, when a case has no result or a result has no case.
#[allow(clippy::too_many_lines)] // one linear join; splitting it would only move the error closures
pub fn load_at(root: &Path, profile: &str) -> Result<Vec<FunctionFixture>, FixtureError> {
    let cases_path = root.join("cases.json");
    let results_path = root.join(format!("legacy-{profile}.json"));
    if profile.is_empty()
        || !profile
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(FixtureError::Shape {
            path: results_path,
            what: "the profile name must be made of a-z, 0-9 and `-`".to_owned(),
        });
    }
    let cases = load_json(&cases_path)?;
    let results_json = load_json(&results_path)?;
    let results_text = fs::read(&results_path).map_err(|source| FixtureError::Io {
        path: results_path.clone(),
        source,
    })?;
    let shape_cases = |what: String| FixtureError::Shape {
        path: cases_path.clone(),
        what,
    };
    let shape_results = |what: String| FixtureError::Shape {
        path: results_path.clone(),
        what,
    };

    let key_orders = data_key_orders(&results_text).map_err(shape_results)?;
    let result_items = results_json
        .get("results")
        .and_then(Value::as_array)
        .ok_or_else(|| shape_results("`results` is not a list".to_owned()))?;
    if key_orders.len() != result_items.len() {
        return Err(shape_results(
            "the text scan and the parser disagree on `results`".to_owned(),
        ));
    }
    let mut by_id: HashMap<&str, (&Value, Option<Vec<String>>)> = HashMap::new();
    for (item, keys) in result_items.iter().zip(key_orders) {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| shape_results("a result has no string `id`".to_owned()))?;
        if by_id.insert(id, (item, keys)).is_some() {
            return Err(shape_results(format!("result `{id}` appears twice")));
        }
    }

    let case_items = cases
        .as_array()
        .ok_or_else(|| shape_cases("the file is not a list".to_owned()))?;
    let mut seen: Vec<&str> = Vec::new();
    let mut out = Vec::with_capacity(case_items.len());
    for case in case_items {
        let id = case
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| shape_cases("a case has no string `id`".to_owned()))?;
        if seen.contains(&id) {
            return Err(shape_cases(format!("case `{id}` appears twice")));
        }
        seen.push(id);
        let function = case
            .get("fn")
            .and_then(Value::as_str)
            .ok_or_else(|| shape_cases(format!("case `{id}` has no string `fn`")))?;
        let divergences = match case.get("divergences") {
            None => Vec::new(),
            Some(list) => list
                .as_array()
                .and_then(|a| a.iter().map(|d| d.as_str().map(str::to_owned)).collect())
                .ok_or_else(|| {
                    shape_cases(format!("case `{id}`: `divergences` is not a list of text"))
                })?,
        };
        let (item, data_keys) = by_id
            .remove(id)
            .ok_or_else(|| shape_results(format!("case `{id}` has no result")))?;
        if item.get("fn").and_then(Value::as_str) != Some(function) {
            return Err(shape_results(format!(
                "result `{id}`: `fn` differs from the case (`{function}`)"
            )));
        }
        let evidence = item
            .get("evidence")
            .and_then(Value::as_str)
            .ok_or_else(|| shape_results(format!("result `{id}` has no string `evidence`")))?;
        let error = match item.get("error") {
            None => None,
            Some(Value::String(token)) => Some(token.clone()),
            Some(_) => return Err(shape_results(format!("result `{id}`: `error` is not text"))),
        };
        out.push(FunctionFixture {
            id: id.to_owned(),
            function: function.to_owned(),
            input: case.get("input").cloned().unwrap_or(Value::Null),
            divergences,
            evidence: evidence.to_owned(),
            result: item.get("result").filter(|r| !r.is_null()).cloned(),
            error,
            data_keys,
        });
    }
    // What is left in `by_id` has no case.
    let mut left: Vec<&str> = by_id.keys().copied().collect();
    left.sort_unstable();
    if let Some(extra) = left.first() {
        return Err(shape_results(format!("result `{extra}` has no case")));
    }
    Ok(out)
}

// A scanner for the one thing `serde_json::Value` does not keep: the order of
// the members of `results[].result.data`. The text is already valid JSON (it
// went through `load_json`), so the scanner only has to walk it.

type Scan<T> = Result<T, String>;

fn skip_ws(b: &[u8], mut p: usize) -> usize {
    while matches!(b.get(p), Some(b' ' | b'\t' | b'\n' | b'\r')) {
        p = p.saturating_add(1);
    }
    p
}

fn expect(b: &[u8], p: usize, want: u8) -> Scan<usize> {
    if b.get(p) == Some(&want) {
        Ok(p.saturating_add(1))
    } else {
        Err(format!("expected `{}` at byte {p}", char::from(want)))
    }
}

/// The decoded string that starts at `p` and the byte after it.
fn scan_string(b: &[u8], p: usize) -> Scan<(String, usize)> {
    let mut q = expect(b, p, b'"')?;
    while let Some(&c) = b.get(q) {
        match c {
            b'\\' => q = q.saturating_add(2),
            b'"' => {
                let raw = b
                    .get(p..=q)
                    .and_then(|s| core::str::from_utf8(s).ok())
                    .ok_or_else(|| format!("bad string at byte {p}"))?;
                let text = serde_json::from_str::<String>(raw).map_err(|e| e.to_string())?;
                return Ok((text, q.saturating_add(1)));
            }
            _ => q = q.saturating_add(1),
        }
    }
    Err(format!("unterminated string at byte {p}"))
}

/// The members of the object at `p` in file order (name and the byte where
/// the value starts), and the byte after the object.
fn scan_members(b: &[u8], p: usize) -> Scan<(Vec<(String, usize)>, usize)> {
    let mut q = skip_ws(b, expect(b, p, b'{')?);
    let mut members = Vec::new();
    if b.get(q) == Some(&b'}') {
        return Ok((members, q.saturating_add(1)));
    }
    loop {
        let (name, after) = scan_string(b, skip_ws(b, q))?;
        let value = skip_ws(b, expect(b, skip_ws(b, after), b':')?);
        members.push((name, value));
        q = skip_ws(b, skip_value(b, value)?);
        match b.get(q) {
            Some(b',') => q = q.saturating_add(1),
            Some(b'}') => return Ok((members, q.saturating_add(1))),
            _ => return Err(format!("bad object at byte {q}")),
        }
    }
}

/// The byte where each element of the array at `p` starts, and the byte
/// after the array.
fn scan_elements(b: &[u8], p: usize) -> Scan<(Vec<usize>, usize)> {
    let mut q = skip_ws(b, expect(b, p, b'[')?);
    let mut starts = Vec::new();
    if b.get(q) == Some(&b']') {
        return Ok((starts, q.saturating_add(1)));
    }
    loop {
        q = skip_ws(b, q);
        starts.push(q);
        q = skip_ws(b, skip_value(b, q)?);
        match b.get(q) {
            Some(b',') => q = q.saturating_add(1),
            Some(b']') => return Ok((starts, q.saturating_add(1))),
            _ => return Err(format!("bad array at byte {q}")),
        }
    }
}

/// The byte after the value that starts at `p`.
fn skip_value(b: &[u8], p: usize) -> Scan<usize> {
    match b.get(p) {
        Some(b'"') => scan_string(b, p).map(|(_, end)| end),
        Some(b'{') => scan_members(b, p).map(|(_, end)| end),
        Some(b'[') => scan_elements(b, p).map(|(_, end)| end),
        Some(_) => {
            // A number, `true`, `false` or `null`.
            let mut q = p;
            while b
                .get(q)
                .is_some_and(|c| !matches!(c, b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r'))
            {
                q = q.saturating_add(1);
            }
            if q == p {
                Err(format!("bad value at byte {p}"))
            } else {
                Ok(q)
            }
        }
        None => Err("the text ends inside a value".to_owned()),
    }
}

fn member(members: &[(String, usize)], name: &str) -> Option<usize> {
    members.iter().find(|(n, _)| n == name).map(|(_, v)| *v)
}

/// The member names of the object at `p`, or `None` when it is not an object.
fn object_keys(b: &[u8], p: usize) -> Scan<Option<Vec<String>>> {
    if b.get(p) != Some(&b'{') {
        return Ok(None);
    }
    let (members, _) = scan_members(b, p)?;
    Ok(Some(members.into_iter().map(|(name, _)| name).collect()))
}

/// For each element of `results`: the member names of `result.data`.
fn data_key_orders(text: &[u8]) -> Scan<Vec<Option<Vec<String>>>> {
    let (top, _) = scan_members(text, skip_ws(text, 0))?;
    let results = member(&top, "results").ok_or("`results` is missing")?;
    let (starts, _) = scan_elements(text, results)?;
    starts
        .into_iter()
        .map(|start| {
            let (item, _) = scan_members(text, start)?;
            let Some(result) = member(&item, "result") else {
                return Ok(None);
            };
            if text.get(result) != Some(&b'{') {
                return Ok(None);
            }
            let (fields, _) = scan_members(text, result)?;
            match member(&fields, "data") {
                Some(data) => object_keys(text, data),
                None => Ok(None),
            }
        })
        .collect()
}
