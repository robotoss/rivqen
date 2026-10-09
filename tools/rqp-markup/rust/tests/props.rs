// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Property tests: no panic on any input, and the invariants of a valid
//! result (offsets ascending and within the input, content bytes and their
//! hashes, block tags around the content).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::format_collect
)]

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use proptest::prelude::*;
use proptest::strategy::ValueTree as _;
use rqp_markup::{Document, analyze, result_json};
use sha2::{Digest, Sha256};

/// Markup pieces that reach many rules: block tags, raw text elements,
/// comments, foreign content, tables and stray end tags.
const PIECES: &[&str] = &[
    "<div data-rq-block=\"a\">",
    "<p data-rq-block=\"b\">",
    "<span data-rq-block=\"c\">",
    "<title data-rq-block=\"t\">",
    "<script type=\"application/json\" data-rq-block=\"j\">",
    "<h2 data-rq-block=\"h\">",
    "</div>",
    "</p>",
    "</span>",
    "</title>",
    "</script>",
    "</h2>",
    "<div>",
    "<p>",
    "<b>",
    "</b>",
    "<br>",
    "<br/>",
    "<img src=\"x\">",
    "<ul>",
    "<li>",
    "</li>",
    "</ul>",
    "<table>",
    "<tr>",
    "<td>",
    "</td>",
    "</tr>",
    "</table>",
    "<svg>",
    "</svg>",
    "<g>",
    "</g>",
    "<title>",
    "<style>",
    "</style>",
    "<script>",
    "<!--",
    "-->",
    "<!-- c -->",
    "<![CDATA[",
    "]]>",
    "<noscript>",
    "</noscript>",
    "<select>",
    "<option>",
    "</select>",
    "<template>",
    "</template>",
    "<a href=\"#\">",
    "</a>",
    "<button>",
    "</button>",
    "<ruby>",
    "<rt>",
    "</ruby>",
    "<!DOCTYPE html>",
    "<?pi?>",
    "</ x>",
    "{\"k\":[1,2]}",
    "[1]",
    "text ",
    "\u{e9}\u{1F600}",
    "\r\n",
    "<",
    ">",
    "\"",
    "&amp;",
    "\u{feff}",
    "\0",
];

fn soup() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(prop::sample::select(PIECES), 0..60)
        .prop_map(|pieces| pieces.concat().into_bytes())
}

fn b64u_sha256(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(bytes))
}

/// The invariants of a valid result for `input`.
fn check_valid(input: &[u8], doc: &Document) {
    let mut previous_end = 0;
    for block in &doc.blocks {
        assert!(block.start <= block.end, "start after end");
        assert!(block.end <= input.len(), "end outside the input");
        assert!(
            previous_end <= block.start,
            "blocks overlap or are out of order"
        );
        // The start tag ends just before the content; the end tag starts at `end`.
        assert_eq!(input[block.start - 1], b'>');
        assert_eq!(&input[block.end..block.end + 2], b"</");
        assert_eq!(block.sha256, b64u_sha256(&input[block.start..block.end]));
        // The id is in the start tag.
        let tag_text = String::from_utf8_lossy(&input[..block.start]);
        let tag_start = tag_text.rfind('<').unwrap();
        assert!(tag_text[tag_start..].contains(&block.id));
        previous_end = block.end;
    }
    assert_eq!(doc.template_revision.len(), 46);
    assert_eq!(doc.page_revision.len(), 46);
}

fn check(input: &[u8]) {
    let first = analyze(input);
    // Deterministic.
    assert_eq!(first, analyze(input));
    if let Ok(doc) = &first {
        check_valid(input, doc);
    }
    let line = result_json(&first, Some("f"));
    let value: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value["valid"], first.is_ok());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn arbitrary_bytes_never_panic(input in prop::collection::vec(any::<u8>(), 0..512)) {
        check(&input);
    }

    #[test]
    fn arbitrary_text_never_panics(input in ".{0,300}") {
        check(input.as_bytes());
    }

    #[test]
    fn markup_soup_keeps_the_result_invariants(input in soup()) {
        check(&input);
    }

    #[test]
    fn a_valid_block_content_can_change_without_changing_the_template(
        content in "[a-z0-9 .,]{0,40}",
        other in "[a-z0-9 .,]{0,40}",
    ) {
        let page = |c: &str| format!("<main><p data-rq-block=\"price\">{c}</p></main>");
        let a = analyze(page(&content).as_bytes()).unwrap();
        let b = analyze(page(&other).as_bytes()).unwrap();
        prop_assert_eq!(&a.template_revision, &b.template_revision);
        prop_assert_eq!(a.page_revision == b.page_revision, content == other);
    }
}

#[test]
fn soup_reaches_valid_documents_with_blocks() {
    // Guard against a generator that never produces a block.
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    let mut with_blocks = 0;
    for _ in 0..2000 {
        let input = soup().new_tree(&mut runner).unwrap().current();
        if analyze(&input).is_ok_and(|doc| !doc.blocks.is_empty()) {
            with_blocks += 1;
        }
    }
    assert!(with_blocks > 20, "only {with_blocks} documents with blocks");
}
