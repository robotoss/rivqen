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
use rqp_markup::{Document, ErrorCode, MAX_OPEN_ELEMENTS, MarkupError, analyze, result_json};
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

/// Names that M-06…M-24 accept at any depth, nested in any order, in HTML
/// context and in the content of a `div` block.
const HTML_NAMES: &[&str] = &["div", "span", "b", "i", "em", "section", "article"];
/// Names for the content of an `svg` region.
const SVG_NAMES: &[&str] = &["g", "a", "symbol", "defs", "mask"];
/// Tokens that M-05 does not push: in HTML context outside blocks, in the
/// content of an `html` block (M-22 forbids `<svg/>` there), and in a
/// foreign region.
const NOT_PUSHED: [&[&str]; 3] = [
    &[
        "<br>",
        "<img src=\"x\">",
        "<wbr>",
        "<svg/>",
        "<math/>",
        "<!-- c -->",
        "t",
    ],
    &[
        "<br>",
        "<img src=\"x\">",
        "<wbr>",
        "<hr/>",
        "<!-- c -->",
        "t",
    ],
    &["<g/>", "<rect/>", "<!-- c -->", "t"],
];

/// A well-nested input: an optional block element (HTML context only; the
/// content of an `html` block cannot contain `svg`, M-22), an optional `svg`
/// element, one element per entry of `nested`, each followed by an optional
/// token that is not pushed, then the end tags in reverse order. Returns the
/// input and the largest number of elements on the token stack.
fn nested_input(
    nested: &[(usize, Option<usize>)],
    foreign: bool,
    in_block: bool,
) -> (String, usize) {
    let in_block = in_block && !foreign;
    let context = if foreign { 2 } else { usize::from(in_block) };
    let names = if foreign { SVG_NAMES } else { HTML_NAMES };
    let mut open = Vec::new();
    if in_block {
        open.push(("<div data-rq-block=\"p\">".to_owned(), "div"));
    }
    if foreign {
        open.push(("<svg>".to_owned(), "svg"));
    }
    for &(name, _) in nested {
        let name = names[name % names.len()];
        open.push((format!("<{name}>"), name));
    }
    let mut input = String::new();
    for (index, (start_tag, _)) in open.iter().enumerate() {
        input.push_str(start_tag);
        let fill = index
            .checked_sub(open.len() - nested.len())
            .and_then(|i| nested[i].1);
        if let Some(fill) = fill {
            let tokens = NOT_PUSHED[context];
            input.push_str(tokens[fill % tokens.len()]);
        }
    }
    for (_, name) in open.iter().rev() {
        input.push_str("</");
        input.push_str(name);
        input.push('>');
    }
    (input, open.len())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    /// M-26 depth (H-25): a well-nested document is valid when its deepest
    /// point has at most 256 elements on the token stack, and is
    /// `RQP_MARKUP_LIMIT` (M-26) otherwise. Tokens that M-05 does not push
    /// never count.
    #[test]
    fn depth_limit_holds_for_nested_documents(
        nested in prop::collection::vec((any::<usize>(), prop::option::of(any::<usize>())), 240..270),
        foreign in any::<bool>(),
        in_block in any::<bool>(),
    ) {
        let (input, depth) = nested_input(&nested, foreign, in_block);
        let result = analyze(input.as_bytes());
        if depth <= MAX_OPEN_ELEMENTS {
            prop_assert!(result.is_ok(), "depth {depth}: {result:?}");
            prop_assert_eq!(result.unwrap().blocks.len(), usize::from(in_block && !foreign));
        } else {
            prop_assert_eq!(
                result.map(|_| ()),
                Err(MarkupError { code: ErrorCode::Limit, rule: "M-26" })
            );
        }
    }

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
