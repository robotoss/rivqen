// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Properties of the legacy split and rebuild (template-engine.md section 4):
//! P-TPL-1 (round trip), P-TPL-2 (any bytes), P-TPL-3 (equal to the
//! expressions of C-06 and C-07, DL-162).
//!
//! Fixed seeds; `PROPTEST_CASES` raises the case count.

#![cfg(feature = "legacy")]
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

mod legacy_oracle;

use legacy_oracle::{BLOCK_EXPRESSION, Regex, TITLE_EXPRESSION};
use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use rivqen_template::legacy::{
    DataBlock, Diagnostic, RenderError, SplitOutcome, render_v1, split_v1,
};
use rivqen_template::limits::TemplateLimits;

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

/// Tokens that build marker-rich text: every part of both expressions in
/// several cases, the bytes that end or break a match, and key text.
const TOKENS: &[&str] = &[
    "<title",
    "<TiTlE",
    "</title>",
    "</TITLE>",
    ">",
    "<!--sonicdiff",
    "<!--SonicDiff",
    "-",
    "--",
    "-->",
    "-end-->",
    "-END-->",
    "a",
    "b",
    "B",
    "_",
    "7",
    "title",
    "end",
    "\n",
    "\r",
    " ",
    "x",
    "<",
    "{",
    "}",
    "{a}",
    "{b}",
    "{title}",
    "é",
    "\u{1F600}",
    // Whole markers, so that full matches are frequent.
    "<!--sonicdiff-a-->",
    "<!--SONICDIFF-a-end-->",
    "<!--sonicdiff-b-->",
    "<!--sonicdiff-B-end-->",
    "<!--sonicdiff-->",
    "<!--sonicdiff-end-->",
    "<!--sonicdiff-end-end-->",
    "<!--sonicdiff--a-->",
    "<!--sonicdiff-title-->",
    "<!--sonicdiff-Title-->",
    "<!--sonicdiff-abcd-->",
    "<title>",
];

fn marker_rich() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(prop::sample::select(TOKENS), 0..48)
        .prop_map(|tokens| tokens.concat().into_bytes())
}

fn any_input() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        3 => marker_rich(),
        1 => prop::collection::vec(any::<u8>(), 0..96),
        1 => (marker_rich(), any::<u8>(), any::<prop::sample::Index>()).prop_map(
            |(mut bytes, byte, at)| {
                let at = at.index(bytes.len() + 1);
                bytes.insert(at, byte);
                bytes
            }
        ),
    ]
}

fn limits() -> impl Strategy<Value = TemplateLimits> {
    let small = (
        prop::sample::select(vec![0_usize, 40, 160, 5 * 1024 * 1024]),
        prop::sample::select(vec![0_usize, 1, 2, 256]),
        prop::sample::select(vec![0_usize, 36, 60, 1024 * 1024]),
        prop::sample::select(vec![0_usize, 1, 3, 5, 128]),
    )
        .prop_map(|(document, blocks, block, name)| TemplateLimits {
            max_document_bytes: document,
            max_blocks: blocks,
            max_block_bytes: block,
            max_name_bytes: name,
            ..TemplateLimits::default()
        });
    prop_oneof![3 => Just(TemplateLimits::default()), 2 => small]
}

/// The P-TPL-2 bounds of an applied split of `html` under `limits`.
fn assert_within_limits(html: &[u8], outcome: &SplitOutcome, limits: &TemplateLimits) {
    let SplitOutcome::Applied(split) = outcome else {
        return;
    };
    assert!(html.len() <= limits.max_document_bytes);
    assert!(split.template.len() <= html.len());
    assert_eq!(split.data[0].key, "{title}");
    assert!(split.data.len() <= limits.max_blocks + 1);
    for block in &split.data[1..] {
        let name = &block.key[1..block.key.len() - 1];
        assert!(block.key.starts_with('{') && block.key.ends_with('}'));
        assert!(!name.is_empty() && name.len() <= limits.max_name_bytes);
        assert!(name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
        assert!(block.value.len() <= limits.max_block_bytes);
    }
    for diagnostic in &split.diagnostics {
        let Diagnostic::MismatchedEndName { index } = diagnostic else {
            continue;
        };
        assert!((1..split.data.len()).contains(index));
    }
}

/// `true` when every placeholder of the split of `html` is the first
/// occurrence of its key in the template, and the title placeholder is in
/// the template (the reference positions of the oracle).
fn unambiguous(html: &[u8], template: &str, limits: &TemplateLimits) -> bool {
    let places = legacy_oracle::placeholders(html, limits).unwrap();
    let title_matched = Regex::new(TITLE_EXPRESSION).find_at(html, 0).is_some();
    let title_placed = places.iter().any(|(_, key)| key == "{title}");
    title_placed == title_matched
        && places
            .iter()
            .all(|(at, key)| template.find(key.as_str()) == Some(*at))
}

/// A page from structured parts with unique block names, the title outside
/// the blocks and no `{`: every split of it is unambiguous.
fn structured_page() -> impl Strategy<Value = Vec<u8>> {
    let filler = prop::collection::vec(
        prop::sample::select(
            &[
                "x", " ", "\n", "\r", "<", ">", "-", "é", "<!--", "</title", "}",
            ][..],
        ),
        0..6,
    )
    .prop_map(|parts| parts.concat());
    let block = (
        prop::sample::select(&["-", ""][..]),
        prop::sample::select(&["<!--sonicdiff", "<!--SONICDIFF"][..]),
        filler.clone().prop_filter("content", |c| !c.is_empty()),
        prop::sample::select(&["", "a", "zz"][..]),
    );
    (
        prop::option::of(prop::sample::select(
            &["<title>Shop</title>", "<TITLE a=1>\rT</title>"][..],
        )),
        prop::collection::vec((filler.clone(), block), 0..6),
        filler,
    )
        .prop_map(|(title, blocks, tail)| {
            let mut page = String::new();
            if let Some(title) = title {
                page.push_str(title);
            }
            for (index, (gap, (dash, mark, content, end_name))) in blocks.into_iter().enumerate() {
                page.push_str(&gap);
                // The filler has no `sonicdiff`, so only these markers exist.
                page.push_str(mark);
                page.push_str(dash);
                page.push('n');
                page.push_str(&index.to_string());
                page.push_str("-->");
                page.push_str(&content);
                page.push_str(mark);
                page.push('-');
                page.push_str(end_name);
                page.push_str("-end-->");
            }
            page.push_str(&tail);
            page.into_bytes()
        })
}

#[test]
fn legacy_oracle_follows_backtracking_priority() {
    let block = Regex::new(BLOCK_EXPRESSION);
    // Lazy content: the first end marker ends the match.
    let caps = block
        .find_at(
            b"<!--sonicdiff-a-->1<!--sonicdiff-a-end-->2<!--sonicdiff-a-end-->",
            0,
        )
        .unwrap();
    assert_eq!(caps[0], Some((0, 41)));
    // Greedy `-?`: `-end-end-->` ends with the name `end`.
    let caps = block
        .find_at(b"<!--sonicdiff-a-->1<!--sonicdiff-end-end-->", 0)
        .unwrap();
    assert_eq!(caps[0], Some((0, 43)));
    assert_eq!(caps[3], Some((33, 36)));
    // At least one byte of content.
    assert!(
        block
            .find_at(b"<!--sonicdiff-a--><!--sonicdiff-a-end-->", 0)
            .is_none()
    );
    let title = Regex::new(TITLE_EXPRESSION);
    assert_eq!(
        title.find_at(b"<TITLE>a\r</title>", 0).unwrap()[0],
        Some((0, 17))
    );
    assert!(title.find_at(b"<title>a\n</title>", 0).is_none());
}

proptest! {
    #![proptest_config(config(2048, 0x0006_7e31_2026_1112))]

    /// P-TPL-3: the scanner gives exactly the split of the expressions.
    #[test]
    fn legacy_prop_equals_expressions(html in any_input(), limits in limits()) {
        let expected = legacy_oracle::split(&html, &limits);
        prop_assert_eq!(split_v1(&html, &limits), expected);
    }
}

proptest! {
    #![proptest_config(config(512, 0x0006_7e32_2026_1112))]

    /// P-TPL-1: a page without key text before its placeholders rebuilds
    /// byte for byte.
    #[test]
    fn legacy_prop_round_trip(html in structured_page()) {
        let limits = TemplateLimits::default();
        let SplitOutcome::Applied(split) = split_v1(&html, &limits).unwrap() else {
            return Err(TestCaseError::fail("a structured page always splits"));
        };
        let page = render_v1(&split.template, &split.data, &limits).unwrap();
        prop_assert_eq!(page.as_bytes(), &html[..]);
    }

    /// P-TPL-1, exact domain: the rebuild gives the page back exactly when
    /// each placeholder is the first occurrence of its key in the template
    /// and the title placeholder is in the template.
    #[test]
    fn legacy_prop_round_trip_iff_first_occurrence(html in marker_rich(), limits in limits()) {
        if let Ok(SplitOutcome::Applied(split)) = split_v1(&html, &limits) {
            let rebuilt = render_v1(&split.template, &split.data, &limits);
            let same = rebuilt.as_ref().is_ok_and(|page| page.as_bytes() == html.as_slice());
            prop_assert_eq!(same, unambiguous(&html, &split.template, &limits));
            if let Err(error) = rebuilt {
                prop_assert_eq!(error, RenderError::Mismatch);
            }
        }
    }

    /// P-TPL-2: any bytes never panic and the result stays within the limits.
    #[test]
    fn legacy_prop_any_bytes(html in prop::collection::vec(any::<u8>(), 0..2048), rich in any_input(), limits in limits()) {
        for input in [&html, &rich] {
            let outcome = split_v1(input, &limits);
            if let Ok(outcome) = &outcome {
                assert_within_limits(input, outcome, &limits);
                if let SplitOutcome::Applied(split) = outcome
                    && let Ok(page) = render_v1(&split.template, &split.data, &limits)
                {
                    prop_assert!(page.len() <= limits.max_document_bytes);
                }
            }
        }
    }

    /// P-TPL-2 for the rebuild: any template and data never panic, and an
    /// output is within `max_document_bytes`.
    #[test]
    fn legacy_prop_render_any_input(
        template in "[{}a-c_ \u{e9}]{0,64}",
        data in prop::collection::vec(("\\{?[a-c_]{0,3}\\}?|\\{title\\}", "[{}a-c<>]{0,40}"), 0..6),
        limits in limits(),
    ) {
        let data: Vec<DataBlock> = data
            .into_iter()
            .map(|(key, value)| DataBlock { key, value })
            .collect();
        if let Ok(page) = render_v1(&template, &data, &limits) {
            prop_assert!(page.len() <= limits.max_document_bytes);
        }
    }
}
