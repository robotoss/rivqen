// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Legacy data diff and checked rebuild (WP-08 S1, T-02): `diff_v1` and
//! `Rebuilt::new` on two data lists and a template built from the bytes.
//!
//! Input: the bytes are read as lossy UTF-8 and split at U+0003 into three
//! parts: the template, the stored list and the server list. A list part is
//! split at U+0001 into entries, `key U+0002 value`; an entry without U+0002
//! is a value with a generated key (`{title}` for the first, `{bN}` after).
//! The lists can repeat a key. The first byte selects the limits.
//!
//! Invariants (a failed one panics):
//!
//! 1. No panic in `diff_v1` or `Rebuilt::new`.
//! 2. `diff_v1(local, server)` is an error exactly when a key repeats in
//!    `local` (`DuplicateLocalKey`) or else in `server`
//!    (`DuplicateServerKey`). Else the diff is a subsequence of `server`; an
//!    entry is in it exactly when `local` has no equal value for its key; the
//!    diff applied to `local` gives the server value for each server key.
//!    `diff_v1(server, server)` is empty, and `diff_v1(None, server)` is the
//!    whole server list.
//! 3. `Rebuilt::new` with the hash of the rebuild of the template and the
//!    server list gives that rebuild, at most `max_document_bytes`, or a
//!    `Diff` error for a repeated stored key. With a hash of another text it
//!    gives no page.
//! 4. The template as a page: when its split applies and `Rebuilt::new` with
//!    its own split and hash gives a page, the page is the template
//!    (P-DIFF-5: never another page).
#![no_main]

use std::collections::{BTreeMap, BTreeSet};

use libfuzzer_sys::fuzz_target;
use rivqen_diff::legacy::{DiffError, RebuildError, RebuildInput, Rebuilt, diff_v1};
use rivqen_template::legacy::{DataBlock, SplitOutcome, etag_v1, render_v1, split_v1};
use rivqen_template::limits::{MAX_LEGACY_NAME_BYTES, MAX_OPEN_ELEMENTS, TemplateLimits};

fn list(part: &str) -> Vec<DataBlock> {
    if part.is_empty() {
        return Vec::new();
    }
    part.split('\u{1}')
        .enumerate()
        .map(|(index, entry)| match entry.split_once('\u{2}') {
            Some((key, value)) => DataBlock {
                key: key.to_owned(),
                value: value.to_owned(),
            },
            None => DataBlock {
                key: if index == 0 {
                    "{title}".to_owned()
                } else {
                    format!("{{b{index}}}")
                },
                value: entry.to_owned(),
            },
        })
        .collect()
}

fn repeats(list: &[DataBlock]) -> bool {
    let keys: BTreeSet<&str> = list.iter().map(|b| b.key.as_str()).collect();
    keys.len() != list.len()
}

fn check_diff(local: Option<&[DataBlock]>, server: &[DataBlock]) {
    let result = diff_v1(local, server);
    if local.is_some_and(repeats) {
        assert_eq!(result, Err(DiffError::DuplicateLocalKey));
        return;
    }
    if repeats(server) {
        assert_eq!(result, Err(DiffError::DuplicateServerKey));
        return;
    }
    let diff = result.unwrap();
    let stored: BTreeMap<&str, &str> = local
        .unwrap_or_default()
        .iter()
        .map(|b| (b.key.as_str(), b.value.as_str()))
        .collect();
    let mut rest = diff.iter().peekable();
    for entry in server {
        let changed = stored.get(entry.key.as_str()) != Some(&entry.value.as_str());
        let listed = rest.peek().is_some_and(|d| std::ptr::eq(**d, entry));
        assert_eq!(listed, changed);
        if listed {
            rest.next();
        }
    }
    assert!(rest.next().is_none());
    let mut merged = stored;
    for entry in &diff {
        merged.insert(&entry.key, &entry.value);
    }
    for entry in server {
        assert_eq!(merged.get(entry.key.as_str()), Some(&entry.value.as_str()));
    }
    assert!(diff_v1(Some(server), server).unwrap().is_empty());
    assert_eq!(diff_v1(None, server).unwrap().len(), server.len());
}

fn rebuild(
    template: &str,
    stored: &[DataBlock],
    server: Vec<DataBlock>,
    hash: &str,
    limits: &TemplateLimits,
) -> Result<Rebuilt, RebuildError> {
    let input = RebuildInput {
        template,
        stored,
        server,
        html_sha1: hash,
        etag: hash,
        response_template_tag: "tag",
        stored_template_tag: "tag",
    };
    Rebuilt::new(input, limits)
}

fuzz_target!(|data: &[u8]| {
    let limits = match data.first() {
        Some(byte) if byte % 2 == 1 => {
            TemplateLimits::new(256, 4, 64, MAX_LEGACY_NAME_BYTES, MAX_OPEN_ELEMENTS)
        }
        _ => TemplateLimits::default(),
    };
    let text = String::from_utf8_lossy(data);
    let mut parts = text.splitn(3, '\u{3}');
    let template = parts.next().unwrap_or_default();
    let stored = list(parts.next().unwrap_or_default());
    let server = list(parts.next().unwrap_or_default());

    // Invariants 1 and 2.
    check_diff(Some(&stored), &server);
    check_diff(None, &server);

    // Invariant 3.
    let rendered = render_v1(template, &server, &limits);
    match &rendered {
        Ok(page) => {
            let hash = etag_v1(page.as_bytes()).to_string();
            match rebuild(template, &stored, server.clone(), &hash, &limits) {
                Ok(rebuilt) => {
                    assert_eq!(rebuilt.page(), page);
                    assert!(rebuilt.page().len() <= limits.max_document_bytes());
                    assert_eq!(rebuilt.data(), server.as_slice());
                }
                Err(error) => {
                    assert_eq!(error, RebuildError::Diff(DiffError::DuplicateLocalKey));
                    assert!(repeats(&stored));
                }
            }
            let other = etag_v1(template.as_bytes()).to_string();
            if other != hash {
                assert!(rebuild(template, &stored, server.clone(), &other, &limits).is_err());
            }
        }
        Err(error) => {
            let got = rebuild(template, &stored, server.clone(), "", &limits);
            assert_eq!(got.unwrap_err(), RebuildError::Render(*error));
        }
    }

    // Invariant 4.
    if let Ok(SplitOutcome::Applied(split)) = split_v1(template.as_bytes(), &limits) {
        let hash = etag_v1(template.as_bytes()).to_string();
        if let Ok(rebuilt) = rebuild(
            &split.template,
            &split.data,
            split.data.clone(),
            &hash,
            &limits,
        ) {
            assert_eq!(rebuilt.page(), template);
            assert!(rebuilt.diff().is_empty());
        }
    }
});
