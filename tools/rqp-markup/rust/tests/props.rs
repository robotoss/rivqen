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
    clippy::format_collect,
    clippy::format_push_string
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

/// Shapes at the top level (the token stack is empty before and after each
/// one) with the largest D that they add while they run, and the units of D
/// that they keep after they end (`markup.md` M-26 items 2 to 6).
const D_SHAPES: &[(&str, usize, usize)] = &[
    // Exact pops give everything back (item 4).
    ("<b></b>", 1, 0),
    ("<svg><g></g></svg>", 2, 0),
    // The tags of `noscript` content count in the pass of M-11 item 8
    // (H-71), and D is restored after the end tag (DL-044 c).
    ("<noscript><b><i></i></b></noscript>", 3, 0),
    ("<div><noscript><svg><path></svg></noscript></div>", 4, 0),
    ("<br><img src=\"x\">", 0, 0),
    // Implied parts count once per F and come back with F (item 3).
    ("<table><tr><td>x</td></tr></table>", 4, 0),
    ("<table><td>a</td><td>b</td></table>", 4, 0),
    ("<table><col><col></table>", 2, 0),
    // A crossing keeps all units (item 5); so does a dirty element.
    ("<span><div></span>", 2, 2),
    ("<span><span><div></span></span>", 3, 3),
    ("<div><span><b></span></div>", 3, 3),
    ("<p><b>x</p>", 2, 2),
    ("<li><ul></li>", 2, 2),
    ("<svg><g></svg>", 2, 2),
    ("<select><option>a<option>b</select>", 3, 3),
    ("<table><tr><td>a</tr></table>", 4, 4),
    // `body` and `html` never give their unit back.
    ("<body></body>", 1, 1),
    ("<html></html>", 1, 1),
];

/// The expected result of [`D_SHAPES`] in this order, then a block: valid
/// when D never goes above 256.
fn d_model(shapes: &[usize]) -> bool {
    let mut d = 0;
    for &index in shapes {
        let (_, peak, kept) = D_SHAPES[index];
        if d + peak > MAX_OPEN_ELEMENTS {
            return false;
        }
        d += kept;
    }
    d < MAX_OPEN_ELEMENTS
}

/// A well-nested sequence of elements from [`HTML_NAMES`] with void
/// elements between them: `ops` opens an element (`Some`) or closes the
/// current one (`None`). Returns the content and the offsets of its end tags.
fn balanced_content(ops: &[Option<usize>]) -> (String, Vec<(usize, usize)>) {
    let mut out = String::new();
    let mut open = Vec::new();
    let mut ends = Vec::new();
    let mut close = |out: &mut String, name: &str| {
        let start = out.len();
        out.push_str("</");
        out.push_str(name);
        out.push('>');
        ends.push((start, out.len()));
    };
    for op in ops {
        match op {
            Some(name) => {
                let name = HTML_NAMES[name % HTML_NAMES.len()];
                out.push('<');
                out.push_str(name);
                out.push_str("><br>");
                open.push(name);
            }
            None => {
                if let Some(name) = open.pop() {
                    close(&mut out, name);
                }
            }
        }
    }
    while let Some(name) = open.pop() {
        close(&mut out, name);
    }
    (out, ends)
}

/// A well-nested sequence of elements from `names` (as
/// [`balanced_content`], without void elements): `ops` opens an element
/// (`Some`) or closes the current one (`None`). Returns the markup and
/// whether `rule` held at each start tag: `rule(name, open)` sees the names
/// that are open before the start tag.
fn nested_names(
    ops: &[Option<usize>],
    names: &[&'static str],
    rule: impl Fn(&str, &[&str]) -> bool,
) -> (String, bool) {
    let mut out = String::new();
    let mut open: Vec<&str> = Vec::new();
    let mut holds = true;
    for op in ops {
        match op {
            Some(index) => {
                let name = names[index % names.len()];
                holds &= rule(name, &open);
                out.push_str(&format!("<{name}>"));
                open.push(name);
            }
            None => {
                if let Some(name) = open.pop() {
                    out.push_str(&format!("</{name}>"));
                }
            }
        }
    }
    while let Some(name) = open.pop() {
        out.push_str(&format!("</{name}>"));
    }
    (out, holds)
}

/// The ops of [`nested_names`] for `names` names.
fn nested_ops(names: usize) -> impl Strategy<Value = Vec<Option<usize>>> {
    prop::collection::vec(prop::option::weighted(0.6, 0..names), 1..40)
}

/// Elements that the tree builder foster-parents in a table, and table
/// parts that a table can hold (M-09 condition 2).
const FOSTERED: &[&str] = &["div", "b", "span", "i", "object", "marquee"];
const TABLE_CONTENT: &[&str] = &[
    "<tr><td>x</td></tr>",
    "<tbody><tr><th>h</th></tr></tbody>",
    "<colgroup><col></colgroup>",
    "<col>",
    "<caption>c</caption>",
    "<table></table>",
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    /// M-10 item 2 (H-36): a well-nested document is valid exactly when no
    /// `form` start tag is inside a `form`; else `RQP_MARKUP_STRUCTURE`
    /// (M-10).
    #[test]
    fn a_form_start_tag_inside_a_form_is_invalid(
        ops in nested_ops(4),
    ) {
        let (markup, holds) =
            nested_names(&ops, &["form", "div", "b", "span"], |name, open| {
                name != "form" || !open.contains(&"form")
            });
        let result = analyze(format!("{markup}<p data-rq-block=\"z\">x</p>").as_bytes())
            .map(|doc| doc.blocks.len());
        if holds {
            prop_assert_eq!(result, Ok(1));
        } else {
            prop_assert_eq!(result, Err(MarkupError { code: ErrorCode::Structure, rule: "M-10" }));
        }
    }

    /// M-11 item 6 (H-38): a table part in `noscript` content in a cell
    /// needs a `table` of the content open. Without one the result is
    /// `RQP_MARKUP_STRUCTURE` (M-11). With one, the main pass applies M-09
    /// condition 2 to the part (H-71): valid when the table is the current
    /// node, else M-09.
    #[test]
    fn table_parts_in_noscript_need_an_open_table(
        wrappers in prop::collection::vec(prop::bool::ANY, 0..6),
        table_at in prop::option::of(0usize..7),
        part in 0..4usize,
    ) {
        let mut content = String::new();
        let mut open = Vec::new();
        for depth in 0..=wrappers.len() {
            if table_at == Some(depth) {
                content.push_str("<table>");
                open.push("table");
            }
            if let Some(&div) = wrappers.get(depth) {
                let name = if div { "div" } else { "span" };
                content.push_str(&format!("<{name}>"));
                open.push(name);
            }
        }
        content.push_str(["<td>x</td>", "<tr></tr>", "<caption>x</caption>", "<col>"][part]);
        for name in open.iter().rev() {
            content.push_str(&format!("</{name}>"));
        }
        let input = format!(
            "<table><tr><td><noscript>{content}</noscript><div data-rq-block=\"z\">x</div></td></tr></table>"
        );
        let result = analyze(input.as_bytes()).map(|doc| doc.blocks.len());
        let table_open = open.contains(&"table");
        if !table_open {
            prop_assert_eq!(result, Err(MarkupError { code: ErrorCode::Structure, rule: "M-11" }));
        } else if open.last() == Some(&"table") {
            prop_assert_eq!(result, Ok(1));
        } else {
            prop_assert_eq!(result, Err(MarkupError { code: ErrorCode::Structure, rule: "M-09" }));
        }
    }

    /// M-09 condition 2 (H-37): in a table, elements that the tree builder
    /// foster-parents are allowed when they are closed before the next table
    /// part. A table part while one of them is open is
    /// `RQP_MARKUP_STRUCTURE` (M-09).
    #[test]
    fn a_table_part_needs_its_table_as_the_current_node(
        wrappers in prop::collection::vec((0..FOSTERED.len(), any::<bool>()), 0..12),
        part in 0..TABLE_CONTENT.len(),
    ) {
        let mut input = String::from("<table>");
        let mut open = Vec::new();
        for &(index, left_open) in &wrappers {
            let name = FOSTERED[index];
            input.push_str(&format!("<{name}>"));
            if left_open {
                open.push(name);
            } else {
                input.push_str(&format!("</{name}>"));
            }
        }
        input.push_str(TABLE_CONTENT[part]);
        for name in open.iter().rev() {
            input.push_str(&format!("</{name}>"));
        }
        input.push_str("</table><p data-rq-block=\"z\">x</p>");
        let result = analyze(input.as_bytes()).map(|doc| doc.blocks.len());
        if open.is_empty() {
            prop_assert_eq!(result, Ok(1));
        } else {
            prop_assert_eq!(result, Err(MarkupError { code: ErrorCode::Structure, rule: "M-09" }));
        }
    }

    /// M-26 (H-31): for shapes with known costs, the document is valid
    /// exactly when D stays at most 256, else `RQP_MARKUP_LIMIT` (M-26).
    #[test]
    fn depth_counter_matches_the_model_of_known_shapes(
        shapes in prop::collection::vec(0..D_SHAPES.len(), 120..280),
    ) {
        let input: String = shapes
            .iter()
            .map(|&i| D_SHAPES[i].0)
            .chain(["<p data-rq-block=\"z\">x</p>"])
            .collect();
        let result = analyze(input.as_bytes()).map(|doc| doc.blocks.len());
        if d_model(&shapes) {
            prop_assert_eq!(result, Ok(1));
        } else {
            prop_assert_eq!(
                result,
                Err(MarkupError { code: ErrorCode::Limit, rule: "M-26" })
            );
        }
    }

    /// M-11 item 5 (H-27): balanced `noscript` content of any depth is
    /// valid (C does not count in D); without one of its end tags it is
    /// `RQP_MARKUP_STRUCTURE` (M-11).
    #[test]
    fn noscript_content_must_be_balanced(
        ops in prop::collection::vec(prop::option::weighted(0.6, 0..HTML_NAMES.len()), 1..400),
        drop in any::<prop::sample::Index>(),
    ) {
        let (content, ends) = balanced_content(&ops);
        let page = |c: &str| format!("<noscript>{c}</noscript><p data-rq-block=\"z\">x</p>");
        let valid = analyze(page(&content).as_bytes()).map(|doc| doc.blocks.len());
        prop_assert_eq!(valid, Ok(1));
        if !ends.is_empty() {
            let (start, end) = ends[drop.index(ends.len())];
            let broken = format!("{}{}", &content[..start], &content[end..]);
            prop_assert_eq!(
                analyze(page(&broken).as_bytes()).map(|doc| doc.blocks.len()),
                Err(MarkupError { code: ErrorCode::Structure, rule: "M-11" })
            );
        }
    }

    /// M-26 depth counter D: in a well-nested document every end tag pops
    /// exactly the current node, so D is the number of open elements. The
    /// document is valid when its deepest point has at most 256 elements on
    /// the token stack, and is `RQP_MARKUP_LIMIT` (M-26) otherwise. Tokens
    /// that M-05 does not push never count.
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
fn depth_model_reaches_both_results() {
    // Guard against a shape generator that never reaches the limit, or
    // always does.
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    let strategy = prop::collection::vec(0..D_SHAPES.len(), 120..280);
    let (mut valid, mut limit) = (0, 0);
    for _ in 0..500 {
        if d_model(&strategy.new_tree(&mut runner).unwrap().current()) {
            valid += 1;
        } else {
            limit += 1;
        }
    }
    assert!(valid > 50 && limit > 50, "{valid} valid, {limit} limit");
}

#[test]
fn nested_name_models_reach_both_results() {
    // Guard against an op generator for the M-10 and M-11 properties that
    // never breaks the rule, or always does.
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    let (mut forms, mut parts) = ([0, 0], [0, 0]);
    for _ in 0..500 {
        let ops = nested_ops(4).new_tree(&mut runner).unwrap().current();
        let (_, holds) = nested_names(&ops, &["form", "div", "b", "span"], |name, open| {
            name != "form" || !open.contains(&"form")
        });
        forms[usize::from(holds)] += 1;
        let ops = nested_ops(6).new_tree(&mut runner).unwrap().current();
        let names = ["div", "table", "td", "tr", "caption", "span"];
        let (_, holds) = nested_names(&ops, &names, |name, open| {
            !["td", "tr", "caption"].contains(&name) || open.contains(&"table")
        });
        parts[usize::from(holds)] += 1;
    }
    assert!(forms.iter().all(|&n| n > 50), "forms {forms:?}");
    assert!(parts.iter().all(|&n| n > 50), "parts {parts:?}");
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
