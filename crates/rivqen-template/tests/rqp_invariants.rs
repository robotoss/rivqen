// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Invariants of `split_rqp` (WP-06 S1, T-02): the template and the block
//! contents give the input back, any bytes stay within the limits, and an
//! error holds no document bytes.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::format_collect,
    clippy::format_push_string
)]

use proptest::prelude::*;
use rivqen_integrity::{TemplateRevision, sha256, sha256_parts};
use rivqen_proto::types::{MAX_BLOCK_BYTES, MAX_BLOCKS};
use rivqen_template::limits::{MAX_MARKUP_INPUT_BYTES, TemplateLimits};
use rivqen_template::rqp::{MarkupCode, RqpDocument, split_rqp};

/// Markup outside blocks.
const OUTSIDE: &[&str] = &[
    "",
    "x",
    " a < b ",
    "<p>t</p>",
    "<br>",
    "<!-- c -->",
    "<svg><g></g></svg>",
    "<style>p{}</style>",
    "<script>var a = '<p>';</script>",
    "\u{e9}\u{1F600}",
    "\r\n",
];

/// Content of an `html` block.
const CONTENT: &[&str] = &[
    "",
    "x",
    "120.00",
    "<b>b</b>",
    "<span>s</span>",
    "<svg><path/></svg>",
    "<!-- c -->",
    "&amp; &lt;",
    "\u{e9}\u{1F600}",
    "<i>a</i><em>b</em>",
];

/// Content of a `json` block.
const JSON: &[&str] = &[
    "[]",
    "{}",
    "[1,2]",
    "{\"a\":\"<p>\"}",
    " null ",
    "\"\u{e9}\"",
];

#[derive(Clone, Debug)]
enum Piece {
    Outside(&'static str),
    Html(&'static str, &'static str),
    Json(&'static str),
}

fn piece() -> impl Strategy<Value = Piece> {
    prop_oneof![
        prop::sample::select(OUTSIDE).prop_map(Piece::Outside),
        (
            prop::sample::select(&["div", "p", "span", "section"][..]),
            prop::sample::select(CONTENT)
        )
            .prop_map(|(name, content)| Piece::Html(name, content)),
        prop::sample::select(JSON).prop_map(Piece::Json),
    ]
}

/// A document of pieces, each block with a fresh id, maybe after a BOM.
fn document() -> impl Strategy<Value = Vec<u8>> {
    (any::<bool>(), prop::collection::vec(piece(), 0..24)).prop_map(|(bom, pieces)| {
        let mut out = String::new();
        if bom {
            out.push('\u{feff}');
        }
        for (n, piece) in pieces.iter().enumerate() {
            match piece {
                Piece::Outside(text) => out.push_str(text),
                Piece::Html(name, content) => {
                    out.push_str(&format!(
                        "<{name} data-rq-block=\"b{n}\">{content}</{name}>"
                    ));
                }
                Piece::Json(content) => out.push_str(&format!(
                    "<script type=\"application/json\" data-rq-block=\"j{n}\">{content}</script>"
                )),
            }
        }
        out.into_bytes()
    })
}

/// The template: the input with the content `[start, end)` of every block
/// removed (manifest and patch section 1.3).
fn template(input: &[u8], doc: &RqpDocument) -> Vec<u8> {
    let mut out = Vec::new();
    let mut kept_from = 0;
    for block in &doc.blocks {
        out.extend_from_slice(&input[kept_from..block.start]);
        kept_from = block.end;
    }
    out.extend_from_slice(&input[kept_from..]);
    out
}

/// The template with the block contents put back at their places.
fn rebuild(template: &[u8], doc: &RqpDocument, contents: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut removed = 0;
    let mut copied = 0;
    for (block, content) in doc.blocks.iter().zip(contents) {
        let at = block.start - removed;
        out.extend_from_slice(&template[copied..at]);
        out.extend_from_slice(content);
        copied = at;
        removed += block.end - block.start;
    }
    out.extend_from_slice(&template[copied..]);
    out
}

/// Markup pieces that reach many rules, also invalid ones.
const SOUP: &[&str] = &[
    "<div data-rq-block=\"a\">",
    "<p data-rq-block=\"b\">",
    "<script type=\"application/json\" data-rq-block=\"j\">",
    "<title data-rq-block=\"t\">",
    "</div>",
    "</p>",
    "</script>",
    "</title>",
    "<div>",
    "<b>",
    "</b>",
    "<table>",
    "<tr>",
    "<td>",
    "</table>",
    "<noscript>",
    "</noscript>",
    "<svg>",
    "</svg>",
    "<math>",
    "<template>",
    "</template>",
    "<select>",
    "<!-- c -->",
    "<![CDATA[x]]>",
    "[1]",
    "x",
    "<",
    "\u{feff}",
];

fn soup() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(prop::sample::select(SOUP), 0..48)
        .prop_map(|pieces| pieces.concat().into_bytes())
}

fn any_input() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        prop::collection::vec(any::<u8>(), 0..1024),
        soup(),
        document(),
    ]
}

/// Small limits and the default ones.
fn any_limits() -> impl Strategy<Value = TemplateLimits> {
    prop_oneof![
        Just(TemplateLimits::default()),
        (0..2048_usize, 0..5_usize, 0..64_usize, 0..10_usize).prop_map(
            |(document, blocks, block_bytes, open)| TemplateLimits {
                max_document_bytes: document,
                max_blocks: blocks,
                max_block_bytes: block_bytes,
                max_name_bytes: 128,
                max_open_elements: open,
            }
        ),
        Just(TemplateLimits {
            max_document_bytes: usize::MAX,
            max_blocks: usize::MAX,
            max_block_bytes: usize::MAX,
            max_name_bytes: usize::MAX,
            max_open_elements: usize::MAX,
        }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    /// The template (the input without every `[start, end)`) plus the block
    /// contents put back gives the input byte for byte; the template
    /// revision is the hash of that template and each block hash the hash of
    /// its content.
    #[test]
    fn rqp_prop_round_trip(input in document()) {
        let doc = split_rqp(&input, &TemplateLimits::default()).unwrap();
        let template = template(&input, &doc);
        prop_assert_eq!(
            doc.template_revision,
            TemplateRevision::from_digest(sha256_parts(&[b"rqp-t1\n", &template]))
        );
        let contents: Vec<&[u8]> = doc
            .blocks
            .iter()
            .map(|block| &input[block.start..block.end])
            .collect();
        for (block, content) in doc.blocks.iter().zip(&contents) {
            prop_assert_eq!(block.descriptor.sha256, sha256(content));
        }
        prop_assert_eq!(rebuild(&template, &doc, &contents), input);
    }

    /// Any bytes with any limits: no panic, the same result twice, no
    /// internal defect, and a valid result within the limits.
    #[test]
    fn rqp_prop_any_bytes(input in any_input(), limits in any_limits()) {
        let result = split_rqp(&input, &limits);
        prop_assert_eq!(&result, &split_rqp(&input, &limits));
        let max_input = limits.max_document_bytes.min(MAX_MARKUP_INPUT_BYTES);
        match result {
            Ok(doc) => {
                prop_assert!(input.len() <= max_input);
                prop_assert!(doc.blocks.len() <= limits.max_blocks.min(MAX_BLOCKS));
                let mut previous_end = 0;
                for block in &doc.blocks {
                    prop_assert!(previous_end <= block.start);
                    prop_assert!(block.start <= block.end && block.end <= input.len());
                    let size = block.end - block.start;
                    prop_assert!(size <= limits.max_block_bytes.min(MAX_BLOCK_BYTES));
                    previous_end = block.end;
                }
            }
            Err(error) => {
                prop_assert_ne!(error.rule(), "internal");
                if input.len() > max_input {
                    prop_assert_eq!((error.code(), error.rule()), (MarkupCode::Limit, "M-01"));
                }
            }
        }
    }

    /// The text of an error is the code and the rule only: no byte of the
    /// document, also when the failing id, content or type holds a marker.
    #[test]
    fn rqp_error_holds_no_input(marker in "[a-z][a-z0-9]{11,23}", shape in 0..8_usize) {
        let upper = marker.to_ascii_uppercase();
        let input = match shape {
            0 => format!("<p data-rq-block=\"{upper}\">x</p>"),
            1 => format!("<p data-rq-block=\"{marker}\">x</p><p data-rq-block=\"{marker}\">y</p>"),
            2 => format!("<p data-rq-block=\"a\">{marker}<div>x</div></p>"),
            3 => format!(
                "<script type=\"application/json\" data-rq-block=\"j\">{marker}</script>"
            ),
            4 => format!("<{marker} data-rq-block=\"a\">x</{marker}>"),
            5 => format!("<script type=\"rivqen-manifest{marker}\"></script>"),
            // Invalid UTF-8 after the marker (below).
            6 => format!("<p data-rq-block=\"a\">{marker}</p>"),
            _ => format!("<div data-rq-block=\"a\">{marker}<p data-rq-block=\"b\"></p></div>"),
        };
        let mut bytes = input.into_bytes();
        if shape == 6 {
            bytes.push(0xFF);
        }
        let error = split_rqp(&bytes, &TemplateLimits::default()).unwrap_err();
        let display = error.to_string();
        let debug = format!("{error:?}");
        prop_assert_eq!(&display, &format!("{} ({})", error.code().as_str(), error.rule()));
        prop_assert_eq!(
            &debug,
            &format!("MarkupError {{ code: {:?}, rule: {:?} }}", error.code(), error.rule())
        );
        for text in [&display, &debug] {
            prop_assert!(!text.contains(&marker) && !text.contains(&upper), "{}", text);
        }
    }
}

/// Guard against a generator that never reaches the cases that matter:
/// the round trip needs valid documents with several blocks.
#[test]
fn rqp_prop_generator_reaches_documents_with_blocks() {
    use proptest::strategy::ValueTree as _;
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    let mut with_blocks = 0;
    for _ in 0..500 {
        let input = document().new_tree(&mut runner).unwrap().current();
        let doc = split_rqp(&input, &TemplateLimits::default()).unwrap();
        if doc.blocks.len() >= 3 {
            with_blocks += 1;
        }
    }
    assert!(
        with_blocks > 250,
        "only {with_blocks} documents with 3 blocks"
    );
}
