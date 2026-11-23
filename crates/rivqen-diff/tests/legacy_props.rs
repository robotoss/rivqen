// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Properties of the legacy data diff and of the checked rebuild (diff
//! engine section 2 and P-DIFF-5; CL-27, CL-29, CL-42).
//!
//! Fixed seeds; `PROPTEST_CASES` raises the case count.

#![cfg(feature = "legacy")]
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use rivqen_diff::legacy::{DiffError, RebuildError, RebuildInput, Rebuilt, diff_v1};
use rivqen_template::legacy::{DataBlock, SplitOutcome, etag_v1, render_v1, split_v1};
use rivqen_template::limits::{MAX_OPEN_ELEMENTS, TemplateLimits};

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

/// Keys and values from small sets, so that equal keys and equal values are
/// frequent.
fn entry() -> impl Strategy<Value = DataBlock> {
    (
        prop::sample::select(&["{title}", "{a}", "{b}", "{c}", "{d}", "{A}", "{e_1}"][..]),
        prop::sample::select(&["", "x", "y", "X", "x ", "\u{e9}", "<b>x</b>"][..]),
    )
        .prop_map(|(key, value)| DataBlock {
            key: key.to_owned(),
            value: value.to_owned(),
        })
}

/// A data list that may repeat a key.
fn any_list() -> impl Strategy<Value = Vec<DataBlock>> {
    prop::collection::vec(entry(), 0..10)
}

/// A data list without a repeated key, in any key order.
fn unique_list() -> impl Strategy<Value = Vec<DataBlock>> {
    any_list().prop_map(|list| {
        let mut seen = BTreeSet::new();
        list.into_iter()
            .filter(|b| seen.insert(b.key.clone()))
            .collect()
    })
}

fn repeats(list: &[DataBlock]) -> bool {
    let keys: BTreeSet<&str> = list.iter().map(|b| b.key.as_str()).collect();
    keys.len() != list.len()
}

/// Filler text without `{` and without a marker.
fn filler() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::sample::select(
            &[
                "x", " ", "\n", "\r", "<", ">", "-", "\u{e9}", "<!--", "</title", "}",
            ][..],
        ),
        0..6,
    )
    .prop_map(|parts| parts.concat())
}

/// Block content: filler, not empty (a block has at least one byte).
fn content() -> impl Strategy<Value = String> {
    filler().prop_filter("content", |c| !c.is_empty())
}

/// Two pages from one skeleton: the same title, gaps and block names, and
/// two contents for each block. Unique block names, the title outside the
/// blocks and no `{`: the split of each page rebuilds to the page (P-TPL-1),
/// and the two templates are equal.
fn page_pair() -> impl Strategy<Value = (String, String)> {
    (
        prop::option::of(prop::sample::select(
            &["<title>Shop</title>", "<TITLE a=1>\rT</title>"][..],
        )),
        prop::collection::vec(
            (
                filler(),
                prop::sample::select(&["<!--sonicdiff", "<!--SONICDIFF"][..]),
                content(),
                content(),
            ),
            0..6,
        ),
        filler(),
    )
        .prop_map(|(title, blocks, tail)| {
            let mut pages = [String::new(), String::new()];
            for page in &mut pages {
                page.push_str(title.unwrap_or_default());
            }
            for (index, (gap, mark, first, second)) in blocks.into_iter().enumerate() {
                for (page, content) in pages.iter_mut().zip([&first, &second]) {
                    page.push_str(&gap);
                    write!(page, "{mark}-n{index}-->{content}{mark}-n{index}-end-->").unwrap();
                }
            }
            let [first, second] = pages;
            (first + &tail, second + &tail)
        })
}

/// Tokens of marker-rich text: markers, key text and the bytes that break
/// a match.
const TOKENS: &[&str] = &[
    "<title>",
    "</title>",
    "<TITLE>",
    "x",
    "-",
    ">",
    "{",
    "}",
    "{a}",
    "{title}",
    "\n",
    "<!--sonicdiff-a-->",
    "<!--sonicdiff-a-end-->",
    "<!--SONICDIFF-b-->",
    "<!--sonicdiff-b-end-->",
    "<!--sonicdiff-title-->",
    "<!--sonicdiff-end-end-->",
];

fn marker_rich() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(TOKENS), 0..40).prop_map(|tokens| tokens.concat())
}

fn split(page: &str) -> (String, Vec<DataBlock>) {
    match split_v1(page.as_bytes(), &TemplateLimits::default()).unwrap() {
        SplitOutcome::Applied(split) => (split.template, split.data),
        SplitOutcome::NotApplied(reason) => panic!("{reason:?}"),
    }
}

proptest! {
    #![proptest_config(config(1024, 0x0008_d1ff_2026_1123))]

    /// `diff_v1(x, x)` is empty (CL-28: no message).
    #[test]
    fn legacy_prop_diff_of_itself_is_empty(list in unique_list()) {
        prop_assert!(diff_v1(Some(&list), &list).unwrap().is_empty());
    }

    /// The diff applied to the stored data gives the server value for each
    /// server key; it is a subsequence of the server list; an entry is in it
    /// exactly when its value differs from the stored value or the stored
    /// data does not have its key.
    #[test]
    fn legacy_prop_diff_applied_gives_the_server_values(
        local in unique_list(),
        server in unique_list(),
    ) {
        let diff = diff_v1(Some(&local), &server).unwrap();
        let mut merged: BTreeMap<&str, &str> = local
            .iter()
            .map(|b| (b.key.as_str(), b.value.as_str()))
            .collect();
        for entry in &diff {
            merged.insert(&entry.key, &entry.value);
        }
        for entry in &server {
            prop_assert_eq!(merged.get(entry.key.as_str()).copied(), Some(entry.value.as_str()));
        }
        // Subsequence, in server order, and the exact membership rule.
        let mut rest = diff.iter().peekable();
        for entry in &server {
            let stored = local.iter().find(|b| b.key == entry.key);
            let changed = stored.is_none_or(|b| b.value != entry.value);
            let listed = rest.peek().is_some_and(|d| std::ptr::eq(**d, entry));
            prop_assert_eq!(listed, changed);
            if listed {
                rest.next();
            }
        }
        prop_assert!(rest.next().is_none());
        // No stored data: the full server data.
        let full: Vec<&DataBlock> = server.iter().collect();
        prop_assert_eq!(diff_v1(None, &server).unwrap(), full);
    }

    /// Any lists: no panic; a repeated stored key, then a repeated server
    /// key, is the error.
    #[test]
    fn legacy_prop_diff_any_lists(local in any_list(), server in any_list(), absent in any::<bool>()) {
        let local = (!absent).then_some(local.as_slice());
        let result = diff_v1(local, &server);
        if local.is_some_and(repeats) {
            prop_assert_eq!(result, Err(DiffError::DuplicateLocalKey));
        } else if repeats(&server) {
            prop_assert_eq!(result, Err(DiffError::DuplicateServerKey));
        } else {
            prop_assert!(result.is_ok());
        }
    }
}

proptest! {
    #![proptest_config(config(512, 0x0008_d200_2026_1123))]

    /// P-DIFF-5 through `Rebuilt`: the split of a page rebuilds to the page,
    /// and the check passes with the page hash.
    #[test]
    fn legacy_prop_rebuilt_round_trip((page, _) in page_pair()) {
        let (template, data) = split(&page);
        let sha1 = etag_v1(page.as_bytes()).to_string();
        let input = RebuildInput {
            template: &template,
            stored: &data,
            server: data.clone(),
            html_sha1: &sha1,
            etag: &sha1,
            response_template_tag: "tag",
            stored_template_tag: "tag",
        };
        let rebuilt = Rebuilt::new(input, &TemplateLimits::default()).unwrap();
        prop_assert_eq!(rebuilt.page(), page.as_str());
        prop_assert!(rebuilt.diff().is_empty());
    }

    /// A data update: the stored entry of the first page and the data of the
    /// second page give the second page, the second data and the entries
    /// that changed.
    #[test]
    fn legacy_prop_rebuilt_data_update((old, new) in page_pair()) {
        let (template, stored) = split(&old);
        let (new_template, server) = split(&new);
        prop_assert_eq!(&template, &new_template);
        let sha1 = etag_v1(new.as_bytes()).to_string();
        let input = RebuildInput {
            template: &template,
            stored: &stored,
            server: server.clone(),
            html_sha1: &sha1,
            etag: &sha1,
            response_template_tag: "tag",
            stored_template_tag: "tag",
        };
        let rebuilt = Rebuilt::new(input, &TemplateLimits::default()).unwrap();
        prop_assert_eq!(rebuilt.page(), new.as_str());
        prop_assert_eq!(rebuilt.data(), server.as_slice());
        let changed: Vec<DataBlock> = server
            .iter()
            .zip(&stored)
            .filter(|(s, o)| s.value != o.value)
            .map(|(s, _)| s.clone())
            .collect();
        prop_assert_eq!(rebuilt.diff(), changed.as_slice());
        // The hash of the old page does not pass for the new data, unless
        // the pages are equal.
        if old != new {
            let old_sha1 = etag_v1(old.as_bytes()).to_string();
            let input = RebuildInput {
                template: &template,
                stored: &stored,
                server,
                html_sha1: &old_sha1,
                etag: &old_sha1,
                response_template_tag: "tag",
                stored_template_tag: "tag",
            };
            prop_assert_eq!(
                Rebuilt::new(input, &TemplateLimits::default()).unwrap_err(),
                RebuildError::HtmlSha1Mismatch
            );
        }
    }

    /// Never a wrong page: for any page H, its split as the stored entry and
    /// as the server data, and the hash of H, `Rebuilt` gives H or an error.
    /// With other server data (the values of another split) it also gives H
    /// or an error.
    #[test]
    fn legacy_prop_rebuilt_never_gives_another_page(
        page in marker_rich(),
        other in marker_rich(),
        small in any::<bool>(),
    ) {
        let limits = if small {
            TemplateLimits::new(64, 2, 32, 8, MAX_OPEN_ELEMENTS)
        } else {
            TemplateLimits::default()
        };
        let Ok(SplitOutcome::Applied(own)) = split_v1(page.as_bytes(), &limits) else {
            return Ok(());
        };
        let sha1 = etag_v1(page.as_bytes()).to_string();
        let mut candidates = vec![own.data.clone()];
        if let Ok(SplitOutcome::Applied(foreign)) = split_v1(other.as_bytes(), &limits) {
            candidates.push(foreign.data);
        }
        for server in candidates {
            let input = RebuildInput {
                template: &own.template,
                stored: &own.data,
                server: server.clone(),
                html_sha1: &sha1,
                etag: &sha1,
                response_template_tag: "tag",
                stored_template_tag: "tag",
            };
            if let Ok(rebuilt) = Rebuilt::new(input, &limits) {
                prop_assert_eq!(rebuilt.page(), page.as_str());
                prop_assert!(rebuilt.page().len() <= limits.max_document_bytes());
            } else {
                // An error only when the rebuild is not the page.
                let same = render_v1(&own.template, &server, &limits).is_ok_and(|p| p == page);
                prop_assert!(!same);
            }
        }
    }

    /// Any template, data and hashes: no panic; a page passes only with its
    /// own hash in both values.
    #[test]
    fn legacy_prop_rebuilt_any_input(
        template in prop::collection::vec(
            prop::sample::select(&["{title}", "{a}", "{b}", "{c}", "{A}", "{e_1}", "x", "<b>", "{", "}", " "][..]),
            0..12,
        ).prop_map(|t| t.concat()),
        stored in any_list(),
        server in any_list(),
        fit in any::<bool>(),
        use_own_hash in any::<bool>(),
    ) {
        // Half of the cases keep only the server entries that can fit the
        // template, so that the rebuild often succeeds.
        let server: Vec<DataBlock> = if fit {
            let mut seen = BTreeSet::new();
            let mut list: Vec<DataBlock> = server
                .into_iter()
                .filter(|b| template.contains(&b.key) && seen.insert(b.key.clone()))
                .collect();
            if !seen.contains("{title}") {
                list.insert(0, DataBlock { key: "{title}".to_owned(), value: String::new() });
            }
            list
        } else {
            server
        };
        let limits = TemplateLimits::default();
        let rendered = render_v1(&template, &server, &limits);
        let hash = match (&rendered, use_own_hash) {
            (Ok(page), true) => etag_v1(page.as_bytes()).to_string(),
            _ => "0".repeat(40),
        };
        let input = RebuildInput {
            template: &template,
            stored: &stored,
            server,
            html_sha1: &hash,
            etag: &hash,
            response_template_tag: "tag",
            stored_template_tag: "tag",
        };
        match Rebuilt::new(input, &limits) {
            Ok(rebuilt) => {
                prop_assert_eq!(Ok(rebuilt.page()), rendered.as_deref());
                prop_assert_eq!(rebuilt.etag().to_string(), hash);
                prop_assert!(!repeats(&stored));
            }
            Err(RebuildError::Render(error)) => prop_assert_eq!(Err(error), rendered),
            Err(RebuildError::HtmlSha1Mismatch) => {
                prop_assert!(rendered.is_ok_and(|p| etag_v1(p.as_bytes()).as_str() != hash));
            }
            Err(RebuildError::Diff(error)) => {
                prop_assert_eq!(error, DiffError::DuplicateLocalKey);
                prop_assert!(repeats(&stored));
            }
            Err(error) => prop_assert!(false, "unexpected {error:?}"),
        }
    }
}
