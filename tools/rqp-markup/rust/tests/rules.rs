// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! One or more tests for each rule M-01…M-27 of `markup.md` section 2 and
//! for each error code of `CONTRACT.md` section 5. Each invalid case checks
//! the code and the rule that fails.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::format_collect
)]

use rqp_markup::{
    Block, Document, ErrorCode, Format, MAX_BLOCK_BYTES, MAX_INPUT_BYTES, MAX_OPEN_ELEMENTS,
    MarkupError, analyze,
};

const HEAD: &str = "<!DOCTYPE html>\n<html><head><title>T</title></head><body>\n";
const TAIL: &str = "\n</body></html>\n";

fn page(body: &str) -> String {
    format!("{HEAD}{body}{TAIL}")
}

/// The blocks of a valid page body as `(id, format, content)`.
fn blocks(body: &str) -> Vec<(String, Format, String)> {
    blocks_of(page(body).as_bytes())
}

fn blocks_of(input: &[u8]) -> Vec<(String, Format, String)> {
    match analyze(input) {
        Ok(doc) => doc
            .blocks
            .iter()
            .map(|b| {
                let content = String::from_utf8(input[b.start..b.end].to_vec()).unwrap();
                (b.id.clone(), b.format, content)
            })
            .collect(),
        Err(e) => panic!("expected a valid document, got {e}"),
    }
}

fn html(id: &str, content: &str) -> (String, Format, String) {
    (id.to_owned(), Format::Html, content.to_owned())
}

fn error(body: &str) -> (ErrorCode, &'static str) {
    error_of(page(body).as_bytes())
}

fn error_of(input: &[u8]) -> (ErrorCode, &'static str) {
    match analyze(input) {
        Ok(_) => panic!("expected an invalid document"),
        Err(MarkupError { code, rule }) => (code, rule),
    }
}

const STRUCTURE: ErrorCode = ErrorCode::Structure;

fn assert_valid(body: &str) {
    if let Err(e) = analyze(page(body).as_bytes()) {
        panic!("{body:?}: expected valid, got {e}");
    }
}

fn assert_invalid(body: &str, code: ErrorCode, rule: &str) {
    assert_eq!(error(body), (code, rule), "{body:?}");
}

// ---- 2.2 Input ------------------------------------------------------------

#[test]
fn m01_input_size_limit_is_checked_first() {
    let mut input = vec![b'a'; MAX_INPUT_BYTES];
    assert_eq!(analyze(&input).unwrap().blocks, Vec::<Block>::new());
    input.push(b'a');
    assert_eq!(error_of(&input), (ErrorCode::Limit, "M-01"));
    // Before M-02: a large input with invalid UTF-8 reports the limit.
    input[0] = 0xFF;
    assert_eq!(error_of(&input), (ErrorCode::Limit, "M-01"));
}

#[test]
fn m02_encoding() {
    for bad in [
        &b"\xC3\x28"[..],
        b"\xC0\xAF",
        b"\xE0\x80\xAF",
        b"\xED\xA0\x80",
        b"\xF4\x90\x80\x80",
        b"\xE2\x82",
        b"\x80",
        b"\xFF",
    ] {
        let mut input = page("<p data-rq-block=\"a\">x</p>").into_bytes();
        input.extend_from_slice(bad);
        assert_eq!(error_of(&input), (ErrorCode::Encoding, "M-02"), "{bad:?}");
    }
    // U+0000, control characters and U+FEFF inside the document are valid.
    assert_eq!(
        blocks("<p data-rq-block=\"a\">\0\u{1}\u{feff}\u{10FFFF}</p>"),
        [html("a", "\0\u{1}\u{feff}\u{10FFFF}")]
    );
}

#[test]
fn m02_bom_counts_in_offsets_but_is_not_a_token() {
    let input = "\u{feff}<p data-rq-block=\"a\">x</p>";
    let doc = analyze(input.as_bytes()).unwrap();
    assert_eq!((doc.blocks[0].start, doc.blocks[0].end), (24, 25));
    // A BOM is part of the template.
    let without = analyze(b"<p data-rq-block=\"a\">x</p>").unwrap();
    assert_ne!(doc.template_revision, without.template_revision);
    // The BOM is skipped only at offset 0: a document that starts with the
    // BOM and then `</p>` crossing nothing is still valid.
    assert!(analyze("\u{feff}\u{feff}<p>".as_bytes()).is_ok());
}

// ---- 2.3 Tokenization and token stack --------------------------------------

#[test]
fn m03_tag_and_comment_boundaries() {
    assert_eq!(
        blocks(
            "<a title=\"x > <p data-rq-block='f'>\" href=\"#\">l</a><p data-rq-block=\"r\">v</p>"
        ),
        [html("r", "v")]
    );
    assert_eq!(
        blocks(
            "<!-- <p data-rq-block=\"f\"> --><!--><p data-rq-block=\"a\">A</p><!-- x --!><p data-rq-block=\"b\">B</p>"
        ),
        [html("a", "A"), html("b", "B")]
    );
    assert_eq!(
        blocks(
            "<!x <p data-rq-block=\"f\">\n</ <p data-rq-block=\"g\">\n<?pi <p data-rq-block=\"h\">?><p data-rq-block=\"r\">r</p>"
        ),
        [html("r", "r")]
    );
    // A DOCTYPE ends at the first `>`.
    assert_eq!(
        blocks("<!DOCTYPE x \"<p data-rq-block='f'>\"><p data-rq-block=\"r\">r</p>"),
        [html("r", "r")]
    );
    // End tags may have attributes and a self-closing flag; no effect.
    assert_eq!(
        blocks("<div data-rq-block=\"d\">x</div class=\"a>b\" />"),
        [html("d", "x")]
    );
}

#[test]
fn m03_crlf_and_case() {
    let input = "<P\r\nDATA-RQ-BLOCK=\"a\"\r\n>x\r\n</p\r\n>";
    assert_eq!(blocks_of(input.as_bytes()), [html("a", "x\r\n")]);
    assert_eq!(
        blocks("<SCRIPT TYPE=\"application/json\" Data-Rq-Block=\"j\">[1]</ScRiPt>"),
        [("j".to_owned(), Format::Json, "[1]".to_owned())]
    );
}

#[test]
fn m04_raw_text_states() {
    for (open, close) in [
        ("<title>", "</title>"),
        ("<textarea>", "</textarea>"),
        ("<style>", "</style>"),
        ("<xmp>", "</xmp>"),
        ("<iframe>", "</iframe>"),
        ("<noembed>", "</noembed>"),
        ("<noframes>", "</noframes>"),
        ("<script>", "</script>"),
        ("<title/>", "</title>"),
        ("<script/>", "</script>"),
    ] {
        let body =
            format!("{open}<p data-rq-block=\"fake\">x</p>{close}<p data-rq-block=\"r\">r</p>");
        assert_eq!(blocks(&body), [html("r", "r")], "{open}");
    }
    // Script data with the double-escaped state.
    assert_eq!(
        blocks(
            "<script><!--<script>a</script><p data-rq-block=\"f\">--></script><p data-rq-block=\"r\">r</p>"
        ),
        [html("r", "r")]
    );
    // PLAINTEXT never ends.
    assert_eq!(
        blocks("<plaintext></plaintext><p data-rq-block=\"f\">x</p>"),
        Vec::<(String, Format, String)>::new()
    );
    // A raw text element that is never closed hides the rest.
    assert_eq!(
        blocks_of(b"<style><p data-rq-block=\"f\">x</p>"),
        Vec::<(String, Format, String)>::new()
    );
}

#[test]
fn m04_no_state_change_in_a_foreign_region() {
    // `svg style` is not RAWTEXT: the `<g>` inside is a tag (M-07.4 fails).
    assert_invalid("<svg><style>i{}/*<g>*/</style></svg>", STRUCTURE, "M-07");
    assert_valid("<svg><style>.a{fill:red}</style><title>Icon</title></svg>");
}

#[test]
fn m05_token_stack() {
    // A stray end tag outside a block is ignored.
    assert_eq!(
        blocks("</span></div><p data-rq-block=\"a\">x</p>"),
        [html("a", "x")]
    );
    // A self-closing flag on a non-void element is ignored outside blocks.
    assert_eq!(
        blocks("<div/><span data-rq-block=\"a\">x</span></div>"),
        [html("a", "x")]
    );
    // Void elements are not pushed: `</br>` outside a block matches nothing.
    assert_valid("<br><img><hr></br><p data-rq-block=\"a\">x</p>");
    // A self-closing svg opens no foreign region.
    assert_eq!(
        blocks("<p><svg/><math/><span data-rq-block=\"s\">x</span></p>"),
        [html("s", "x")]
    );
    // An end tag pops to the nearest element with its name.
    assert_valid("<div><span><i>a</div><p data-rq-block=\"a\">x</p>");
}

#[test]
fn m05_svg_region_ends_with_its_end_tag() {
    assert_eq!(
        blocks("<svg><g><path d=\"M0 0\"></g></svg><p data-rq-block=\"a\">x</p>"),
        [html("a", "x")]
    );
    assert_eq!(
        blocks("<math><mi>x</mi></math><p data-rq-block=\"a\">x</p>"),
        [html("a", "x")]
    );
    // A nested svg does not end the region.
    assert_invalid("<svg><svg></svg><p>x</p></svg>", STRUCTURE, "M-07");
}

// ---- 2.4 Document rules -----------------------------------------------------

#[test]
fn m06_crossing() {
    for body in [
        "<div><button>a</div>",
        "<table><tr><td><a href=\"#\">a</td></tr></table>",
        "<div><select><option>a</option></div>",
        "<div><object>a</div>",
        "<div><marquee>a</div>",
        "<div><applet>a</div>",
        "<div><template>a</div>",
        "<div><nobr>a</div>",
        "<div><table>a</div>",
    ] {
        let (code, rule) = error(body);
        assert_eq!(code, STRUCTURE, "{body}");
        assert!(rule == "M-06" || rule == "M-08", "{body}: {rule}");
    }
    assert_invalid("<div><button>a</div>", STRUCTURE, "M-06");
    // Allowed crossings.
    assert_valid("<div><span>a</div><ul><li>a</ul>");
    assert_valid("<table><tr><td>a</tr></table>");
    assert_valid("<table><tbody><tr><th>a</table>");
    assert_valid("<table><caption>a</table>");
    // A table-family end tag cannot cross a guarded non-table element.
    assert_invalid("<table><tr><td><button>a</tr></table>", STRUCTURE, "M-06");
    // A non-table end tag cannot cross a table part.
    assert_invalid("<div><table><tr><td>a</div>", STRUCTURE, "M-06");
}

const FORMATTING: [&str; 14] = [
    "a", "b", "big", "code", "em", "font", "i", "nobr", "s", "small", "strike", "strong", "tt", "u",
];

/// The block of the review repros: a later `<a>` or `<nobr>` in it runs the
/// adoption agency again in the browser.
const CUT_BLOCK: &str = "<span data-rq-block=\"x\">y<a>z</a>w</span>";

/// M-06 item 2 (H-26): an end tag with a name in `FORMATTING` must not
/// cross a `SPECIAL` element pushed in HTML context, or any element pushed
/// in a foreign region (FX-RQ-MARKUP-FORMATTING-CROSS-*).
#[test]
fn m06_2_formatting_end_tags() {
    // The review repros: 8 special elements stop the adoption agency.
    let divs = "<div>".repeat(8);
    assert_invalid(&format!("<a>{divs}x</a>{CUT_BLOCK}"), STRUCTURE, "M-06");
    assert_invalid(
        &format!("<nobr>{divs}x</nobr><span data-rq-block=\"x\">y<nobr>z</nobr>w</span>"),
        STRUCTURE,
        "M-06",
    );
    // One special element is enough, for every formatting name.
    for name in FORMATTING {
        assert_invalid(
            &format!("<{name}><div>x</{name}>y</div>"),
            STRUCTURE,
            "M-06",
        );
        assert_invalid(&format!("<{name}><p>x</{name}>"), STRUCTURE, "M-06");
        // Valid neighbors: a phrasing element is not special, and the
        // special element closed first is not crossed.
        assert_valid(&format!("<{name}><span><q>x</{name}>"));
        assert_valid(&format!("<{name}><div>x</div></{name}>"));
    }
    // Any special name, with the same `b` end tag.
    for special in [
        "address",
        "article",
        "blockquote",
        "center",
        "dd",
        "dir",
        "dl",
        "dt",
        "fieldset",
        "figure",
        "footer",
        "form",
        "h1",
        "h6",
        "header",
        "hgroup",
        "li",
        "listing",
        "main",
        "menu",
        "nav",
        "ol",
        "pre",
        "search",
        "section",
        "summary",
        "ul",
    ] {
        assert_invalid(&format!("<b><{special}>x</b>"), STRUCTURE, "M-06");
    }
    // Foreign elements: every element pushed in a foreign region counts.
    assert_invalid(
        "<svg><a href=\"#\"><text>t</a></svg><p data-rq-block=\"a\">z</p>",
        STRUCTURE,
        "M-06",
    );
    assert_invalid("<svg><a><g><g></a></svg>", STRUCTURE, "M-06");
    assert_valid("<svg><a href=\"#\"><text>t</text></a></svg>");
    // A non-formatting end tag may cross them (M-06 item 1 permitting).
    assert_valid("<svg><g><a><text>t</g></svg>");
    assert_valid("<p><b>a</p><div><span><i>x</div><span><div>x</span>");
    // The control document of the study (FX-RQ-MARKUP-FORMATTING-CROSS-PHRASING).
    assert_valid(
        "<a href=\"#\"><span>x</a><b><i>y</b><p><b>z</p>w <svg><a href=\"#\"><text>t</text></a></svg>",
    );
}

/// M-06 item 3 (H-30): in HTML context, a `tbody`, `thead`, `tfoot`, `tr`
/// or `table` end tag must match an element of the token stack
/// (FX-RQ-MARKUP-TABLE-STRAY-*).
#[test]
fn m06_3_stray_table_end_tags() {
    for name in ["tbody", "thead", "tfoot", "tr", "table", "TBODY", "Tr"] {
        // The review repros: the cell closes and the block moves before the
        // table.
        assert_invalid(
            &format!("<table><td></{name}><div data-rq-block=\"x\">y</div></td></table>"),
            STRUCTURE,
            "M-06",
        );
        assert_invalid(&format!("<p>a</{name}>b</p>"), STRUCTURE, "M-06");
        assert_invalid(&format!("</{name}>"), STRUCTURE, "M-06");
    }
    assert_invalid(
        "<table><tbody><tr><td>x</td></tr></tbody></thead></table>",
        STRUCTURE,
        "M-06",
    );
    // An implied `tbody` or `tr` stays allowed, and so do matching end tags.
    assert_valid("<table><td>a</td></table><table><tr><td>a</td></tr></table>");
    assert_valid(
        "<table><thead><tr><th>h</th></tr></thead><tfoot><tr><td>f</td></tr></tfoot></table>",
    );
    // Other table-family end tags without a match are ignored
    // (FX-RQ-MARKUP-TABLE-IMPLIED-PARTS).
    assert_valid(
        "</td></th></caption></colgroup></col><table><caption>c</caption></caption><col></colgroup><tr><td>a</td></td></th></col></tr></table>",
    );
    // In a foreign region M-07 item 2 applies (any stray end tag).
    assert_invalid("<svg></tbody></svg>", STRUCTURE, "M-07");
    assert_valid("<svg><tbody></tbody></svg>");
    // In block content M-23 applies.
    assert_invalid("<div data-rq-block=\"a\"></tr></div>", STRUCTURE, "M-23");
}

/// M-06 item 4 (H-32): a `FORMATTING` end tag must not pop an element with
/// the special-crossed mark (FX-RQ-MARKUP-FORMATTING-MARKED-*).
#[test]
fn m06_4_special_crossed_mark() {
    // The review repros: the tree builder ignores `</span>` (special `div`)
    // and `</li>` (no `li` in list item scope), keeps the elements, and the
    // later formatting end tag finds 8 furthest blocks.
    let divs = "<div>".repeat(8);
    assert_invalid(
        &format!("<a><span>{divs}x</span></a>{CUT_BLOCK}"),
        STRUCTURE,
        "M-06",
    );
    assert_invalid(
        &format!(
            "<nobr><span>{divs}x</span></nobr><span data-rq-block=\"x\">y<nobr>z</nobr>w</span>"
        ),
        STRUCTURE,
        "M-06",
    );
    let lists = "<li><ul>".repeat(4);
    assert_invalid(
        &format!("<a>{lists}{}x</a>{CUT_BLOCK}", "</li>".repeat(4)),
        STRUCTURE,
        "M-06",
    );
    // The minimal shapes of markup.md M-06.
    assert_invalid("<a><span><div>a</span></a>", STRUCTURE, "M-06");
    assert_invalid("<a><li><ul></li></a>", STRUCTURE, "M-06");
    // The mark is on the new current node; a formatting end tag that pops
    // it as a crossed element fails too.
    assert_invalid("<a><b><span><div>x</span></a>", STRUCTURE, "M-06");
    // Item (a) with a foreign crossed element: the matching element is not
    // the region root (H-75).
    assert_invalid(
        "<a href=\"#\"><svg><g><path d=\"M0\"></g></svg></a>",
        STRUCTURE,
        "M-06",
    );
    assert_invalid(
        "<a href=\"#\"><svg><svg><path d=\"M0\"></svg></svg></a>",
        STRUCTURE,
        "M-06",
    );
    // Item (b): an exact pop of a marked element passes the mark down.
    assert_invalid("<b><p><span><div>x</span></p></b>", STRUCTURE, "M-06");
    assert_invalid(
        "<b><p><span><div>x</span></p><i>y</i></b>",
        STRUCTURE,
        "M-06",
    );
    // An end tag that matches nothing, and a start tag, do not change it.
    assert_invalid("<b><span><div>x</span></q><i></i></b>", STRUCTURE, "M-06");
    // A second crossing keeps the mark (the mark is never removed).
    assert_invalid(
        "<a><span><div>x</span><span><div>y</span></a>",
        STRUCTURE,
        "M-06",
    );
    assert_invalid("<a><li><ul></li><li><ul></li></a>", STRUCTURE, "M-06");
    // Valid neighbors.
    for body in [
        // `</span>` crosses only `i` (not special): nothing gets the mark.
        "<a href=\"#\"><b><span><i>x</span></b></a>",
        // Nothing is crossed.
        "<a><span><div>a</div></span></a>",
        "<a><li><ul></ul></li></a>",
        "<b><svg><g></g></svg></b>",
        // H-75: the end tag of the region root crosses its region and sets
        // no mark, so the formatting end tag below can pop `a` and `b`.
        "<b><svg><g></svg></b>",
        "<a href=\"#\"><svg><path d=\"M0\"></svg></a>",
        "<a href=\"#\"><math><mrow><mi>x</mi></math></a>",
        // A non-formatting end tag may pop a marked element.
        "<div><span><div>x</span></div>",
        // The mark goes away with its element: a new `b` is not marked.
        "<b><span><div>x</span></b-x><b>y</b>",
        "<div><span><div>x</span></div><b>y</b>",
        // A formatting element below the marked one is not popped.
        "<b>x</b><p><span><div>x</span></p><b>y</b>",
    ] {
        assert_valid(body);
    }
    // FX-RQ-MARKUP-FORMATTING-MARKED-PHRASING.
    assert_eq!(
        blocks(&format!(
            "<a href=\"#\"><b><span><i>x</span></b></a>{}",
            "<span data-rq-block=\"x\">y<a href=\"#\">z</a>w</span>"
        )),
        [html("x", "y<a href=\"#\">z</a>w")]
    );
}

#[test]
fn m07_foreign_breakout() {
    assert_invalid("<svg><p>a</p></svg>", STRUCTURE, "M-07");
    assert_invalid("<math><div>a</div></math>", STRUCTURE, "M-07");
    assert_invalid("<svg><font>a</font></svg>", STRUCTURE, "M-07");
    assert_invalid("<svg><h3>a</h3></svg>", STRUCTURE, "M-07");
    assert_valid("<svg><g><text>a</text></g></svg>");
}

#[test]
fn m07_foreign_end_tags() {
    // A stray end tag in the region.
    assert_invalid("<svg></p></svg>", STRUCTURE, "M-07");
    assert_invalid("<svg></unknown-name></svg>", STRUCTURE, "M-07");
    // An end tag that matches an element outside the region.
    assert_invalid("<div><svg></div>", STRUCTURE, "M-07");
    // Crossing elements of the region is allowed, also a foreign `a`.
    assert_valid("<svg><a><g><text>t</text></svg>");
    assert_valid("<div><svg><g></svg></div>");
}

#[test]
fn m07_integration_points_have_text_only() {
    assert_invalid(
        "<svg><foreignObject><div>a</div></foreignObject></svg>",
        STRUCTURE,
        "M-07",
    );
    assert_invalid("<svg><desc><!-- c --></desc></svg>", STRUCTURE, "M-07");
    assert_invalid("<math><mi><!DOCTYPE x></mi></math>", STRUCTURE, "M-07");
    assert_invalid("<math><mi>x</mo></math>", STRUCTURE, "M-07");
    assert_invalid(
        "<math><annotation-xml><b>x</b></annotation-xml></math>",
        STRUCTURE,
        "M-07",
    );
    assert_valid("<svg><desc>a &amp; b</desc><foreignObject/></svg>");
    assert_valid("<math><mi>x</mi><mo>=</mo><mn>2</mn><ms>s</ms><mtext>t</mtext></math>");
}

/// E5 (H-21): a CDATA section that satisfies M-12 is allowed in foreign
/// TEXT content and in integration points. M-12 still applies there.
#[test]
fn m07_e5_cdata_in_foreign_text_content() {
    for body in [
        "<svg><style><![CDATA[.a{fill:red}]]></style></svg>",
        "<svg><script><![CDATA[if(a<b){}]]></script></svg>",
        "<svg><desc><![CDATA[a<b]]></desc></svg>",
        "<svg><title>t<![CDATA[x]]>u</title></svg>",
        "<math><mi><![CDATA[x]]></mi></math>",
        "<svg><foreignObject><![CDATA[]]></foreignObject></svg>",
        "<svg><desc><![CDATA[a]]><![CDATA[b]]></desc></svg>",
    ] {
        assert_eq!(
            blocks(&format!("{body}<p data-rq-block=\"a\">x</p>")),
            [html("a", "x")],
            "{body}"
        );
    }
    // M-12 is not relaxed: the first `>` must end a `]]>`.
    assert_invalid(
        "<svg><style><![CDATA[a>b]]></style></svg>",
        STRUCTURE,
        "M-12",
    );
    assert_invalid("<svg><desc><![CDATA[a]>]]></desc></svg>", STRUCTURE, "M-12");
    // Only CDATA is new: other non-character tokens stay invalid.
    assert_invalid("<svg><style><!--c--></style></svg>", STRUCTURE, "M-07");
    assert_invalid(
        "<svg><desc><![CDATA[x]]><b>y</b></desc></svg>",
        STRUCTURE,
        "M-07",
    );
    // The CDATA section does not end the text-only state.
    assert_invalid(
        "<svg><desc><![CDATA[x]]></g></desc></svg>",
        STRUCTURE,
        "M-07",
    );
}

#[test]
fn m07_text_elements_in_a_region() {
    assert_invalid("<svg><style/></svg>", STRUCTURE, "M-07");
    assert_invalid("<svg><title/></svg>", STRUCTURE, "M-07");
    assert_invalid("<svg><script/></svg>", STRUCTURE, "M-07");
    assert_invalid("<svg><plaintext>x</plaintext></svg>", STRUCTURE, "M-07");
    assert_invalid("<svg><script><g></g></script></svg>", STRUCTURE, "M-07");
    assert_valid("<svg><script>var a = 1;</script><textarea>t</textarea></svg>");
}

#[test]
fn m08_select() {
    assert_valid(
        "<select name=\"s\"><option value=\"1\" selected>One</option><optgroup label=\"g\"><option>Two</option></optgroup><hr><!-- c --><![CDATA[x]]></select>",
    );
    assert_valid("<select><option>a</select>");
    assert_valid("<select></option></optgroup></select>");
    assert_invalid(
        "<select><option><b>a</b></option></select>",
        STRUCTURE,
        "M-08",
    );
    assert_invalid("<select><option>a</b></select>", STRUCTURE, "M-08");
    assert_invalid("<select><!DOCTYPE x></select>", STRUCTURE, "M-08");
    assert_invalid("<select><select></select>", STRUCTURE, "M-08");
    assert_invalid("<select><option>a</div></select>", STRUCTURE, "M-08");
    // No explicit end tag.
    assert_eq!(error_of(b"<select><option>a"), (STRUCTURE, "M-08"));
}

/// M-08 applies to a `select` pushed in HTML context only (defect found by
/// T-07: a foreign `select` was checked by M-08).
#[test]
fn m08_does_not_apply_to_a_foreign_select() {
    for body in [
        "<math><select><th></th></select></math>",
        "<svg><select><style></style></select></svg>",
        "<svg><select><g><text>t</text></g></select></svg>",
        "<svg><select><!DOCTYPE x></select></svg>",
    ] {
        assert_eq!(
            blocks(&format!("{body}<p data-rq-block=\"a\">x</p>")),
            [html("a", "x")],
            "{body}"
        );
    }
    // E2: the input can end in a foreign region, also with a foreign select open.
    assert!(analyze(b"<p data-rq-block=\"a\">x</p><svg><select>").is_ok());
    // M-07 still applies inside the foreign select: breakout names and
    // stray end tags (M-08 would allow `</option>`).
    assert_invalid("<svg><select><p>a</p></select></svg>", STRUCTURE, "M-07");
    assert_invalid("<svg><select></g-x></select></svg>", STRUCTURE, "M-07");
    assert_invalid("<math><select></option></select></math>", STRUCTURE, "M-07");
    // An HTML select after a foreign one is checked by M-08.
    assert_invalid(
        "<svg><select></select></svg><select><b>a</b></select>",
        STRUCTURE,
        "M-08",
    );
    assert_invalid(
        "<svg><select></select></svg><select><option>a</div></select>",
        STRUCTURE,
        "M-08",
    );
    assert_invalid(
        "<svg><select></svg><select><!DOCTYPE x></select>",
        STRUCTURE,
        "M-08",
    );
    assert_eq!(
        error_of(b"<svg><select></select></svg><select><option>a"),
        (STRUCTURE, "M-08")
    );
}

#[test]
fn m09_table_parts() {
    assert_valid(
        "<table><caption>c</caption><colgroup><col></colgroup><col><thead><tr><th>h</th></tr></thead><tbody><tr><td>a</td><td>b</td></tr></tbody><tfoot></tfoot><tr><td>x</td></tr></table>",
    );
    // H-85a: the second `td` closes the first (implied closure); `col` in a
    // cell closes the cell and the row, so `</td>` is ignored and `</tr>`
    // matches nothing (M-06 item 3).
    assert_valid("<table><tr><td>a<td>b</td></tr></table>");
    assert_invalid("<table><tr><td><col></td></tr></table>", STRUCTURE, "M-06");
    // H-85a: the `caption` closes the row, so `</tr>` matches nothing.
    assert_invalid(
        "<table><tr><caption>c</caption></tr></table>",
        STRUCTURE,
        "M-06",
    );
    // H-85a: each of these start tags closes F, so the end tag of F that
    // follows matches nothing (M-06 item 3), or the closure is refused.
    assert_invalid(
        "<table><tbody><tbody></tbody></tbody></table>",
        STRUCTURE,
        "M-06",
    );
    assert_invalid("<table><tr><tr></tr></tr></table>", STRUCTURE, "M-06");
    // A stray `</colgroup>` or `</caption>` is ignored (M-06 item 3 names
    // only the table end tags), so these are valid.
    assert_valid("<table><colgroup><tr></tr></colgroup></table>");
    assert_valid("<table><caption><td></td></caption></table>");
    assert_invalid("<table><thead><col></thead></table>", STRUCTURE, "M-06");
    // The refused closure: the row is not the current node.
    assert_invalid("<table><tr><b>x<tr></tr></table>", STRUCTURE, "M-09");
    // No table-family element open: allowed by M-09.
    assert_valid("<td>a</td><tr></tr><col>");
    // The search stops at template.
    assert_valid("<table><tr><td><template><td>a</td></template></td></tr></table>");
}

/// M-09 condition 1 (H-35): in a template, `col` sends the tree builder to
/// "in column group", which ignores every later start tag, also raw text
/// ones (FX-RQ-MARKUP-TEMPLATE-COL-TITLE, -TEMPLATE-COL-TEXTAREA,
/// -TEMPLATE-TABLE-PARTS).
#[test]
fn m09_1_col_in_a_template() {
    for body in [
        // The review repro: the title text is markup in the browser.
        "<p data-rq-block=\"z\">real</p><template><col><title></template><p data-rq-block=\"y\">fake</p></title></template>",
        "<template><col></template>",
        "<template><div><col></div></template>",
        "<TEMPLATE><COL></TEMPLATE>",
        // A table that is closed again is not the nearest element.
        "<template><table></table><col></template>",
        // A template in a table or a cell: the template is nearer.
        "<table><template><col></template></table>",
        "<table><tr><td><template><b><col></b></template></td></tr></table>",
    ] {
        assert_invalid(body, STRUCTURE, "M-09");
    }
    assert_eq!(
        error_of(b"<head><template><col><textarea></template><script type=\"application/rivqen-manifest+json\">{}</script></textarea></template></head>"),
        (STRUCTURE, "M-09")
    );
    for body in [
        "<template><tr><td>x</td></tr></template>",
        "<template><table><col></table></template>",
        "<template><colgroup><col></colgroup></template>",
        // The tree builder inserts the `title` in these paths.
        "<template><colgroup><title>t</title></colgroup></template>",
        // In a foreign region `col` is a foreign element.
        "<template><svg><col/></svg></template>",
        // No template open: M-09 only.
        "<col><table><col></table>",
        "<template></template><col>",
    ] {
        assert_eq!(
            blocks(&format!("{body}<p data-rq-block=\"a\">x</p>")),
            [html("a", "x")],
            "{body}"
        );
    }
}

/// M-09 condition 2 (H-37): a table part, `col` or `table` start tag while
/// F is a table, a row group, a row or a column group must have F as the
/// current node. Else "clear the stack back to a table context" pops
/// foster-parented elements without their end tags
/// (FX-RQ-MARKUP-TABLE-FOSTER-MARKER, -TABLE-FOSTER-MARKER-DEPTH,
/// -TABLE-CURRENT).
#[test]
fn m09_2_f_is_the_current_node() {
    for body in [
        // The review repro: the `marquee` marker cuts the block.
        "<object><a href=\"1\"><table><marquee><table></table></marquee></table>x</a></object><span data-rq-block=\"k\">y<a href=\"2\">z</a>w</span>",
        "<table><tr><object><td></td></object></tr></table>",
        "<table><form><tr></tr></form></table>",
        "<table><div><caption></caption></div></table>",
        "<table><b><colgroup></colgroup></b></table>",
        "<table><span><col></span></table>",
        "<table><tbody><span><tr></tr></span></tbody></table>",
        "<table><thead><i><tr></tr></i></thead></table>",
        "<table><tfoot><i><td></td></i></tfoot></table>",
        "<table><tr><b><th></th></b></tr></table>",
        "<table><colgroup><div><col></div></colgroup></table>",
        "<table><colgroup><div><table></table></div></colgroup></table>",
        "<table><tr><div><table></table></div></tr></table>",
        // An element that is not pushed does not count; one that is does.
        "<table><br><p><tr></tr></p></table>",
    ] {
        assert_invalid(body, STRUCTURE, "M-09");
    }
    for body in [
        // Foster-parented elements closed before the next part.
        "<table><div></div><tr><td>x</td></tr><b>y</b><tr><td>z</td></tr></table>",
        "<table><colgroup><col><col></colgroup><tbody><tr><td>x</td></tr></tbody></table>",
        // F is a cell or a caption: a nested table is allowed.
        "<table><tr><td><b><table><tr><td>n</td></tr></table></b></td></tr></table>",
        "<table><caption><div><table></table></div></caption></table>",
        // Not pushed elements and other tokens above F.
        "<table><br><input><!-- c --><tr><td>x</td></tr></table>",
        // No F: a table in a formatting element.
        "<b><table><tr><td>x</td></tr></table></b>",
        // The search for F stops at a template.
        "<table><tr><td>x</td></tr><template><div><tr><td>y</td></tr></div></template></table>",
    ] {
        assert_eq!(
            blocks(&format!("{body}<p data-rq-block=\"a\">x</p>")),
            [html("a", "x")],
            "{body}"
        );
    }
    // The check comes before the implied parts of M-26: at D = 256 the
    // `td` fails M-09, not the limit of its implied row group and row.
    let prefix = "<div>".repeat(MAX_OPEN_ELEMENTS - 2);
    assert_eq!(
        error_of(format!("{prefix}<table><b><td>").as_bytes()),
        (STRUCTURE, "M-09")
    );
    assert_eq!(
        error_of(format!("{prefix}<table><td>").as_bytes()),
        (ErrorCode::Limit, "M-26")
    );
}

#[test]
fn m10_frameset() {
    assert_invalid(
        "<frameset><frame src=\"a.html\"></frameset>",
        STRUCTURE,
        "M-10",
    );
    assert_invalid("<svg><frameset></frameset></svg>", STRUCTURE, "M-10");
    assert_valid("<!-- <frameset> --><script>\"<frameset>\"</script>");
}

/// M-10 item 2 (H-36): no `form` start tag in HTML context while an HTML
/// `form` is on the token stack. The tree builder ignores the nested start
/// tag (form element pointer) and can keep the outer form open
/// (FX-RQ-MARKUP-FORM-NESTED, -FORM-NESTED-DEPTH, -FORM-SEQUENTIAL).
#[test]
fn m10_2_nested_forms() {
    let unit = "<form><object><form></form></object></form>";
    for body in [
        format!("<a href=\"1\">{unit}x</a><span data-rq-block=\"k\">y<a href=\"2\">z</a>w</span>"),
        "<form><div><form></form></div></form>".to_owned(),
        "<FORM><Form></form></FORM>".to_owned(),
        // A closed foreign region does not hide the outer form.
        "<form><svg></svg><form></form></form>".to_owned(),
        // In block content too.
        "<div data-rq-block=\"a\"><form><p><form></form></p></form></div>".to_owned(),
    ] {
        assert_invalid(&body, STRUCTURE, "M-10");
    }
    // 600 units: the first nested start tag fails.
    assert_eq!(error_of(unit.repeat(600).as_bytes()), (STRUCTURE, "M-10"));
    for body in [
        "<form><input name=\"q\"></form><form><input name=\"r\"></form>",
        // A crossing pops the form: the count goes down.
        "<div><form></div><form>x</form>",
        "<div><span><form></span></div><form></form>",
        // A `form` in a foreign region is a foreign element; it is never
        // counted, also when a crossing pops it.
        "<form><svg><form></form></svg></form>",
        "<svg><form></form></svg><form></form>",
        "<svg><g><form></svg><form></form>",
        // A form that holds a block.
        "<form><div data-rq-block=\"b\">y</div></form><form></form>",
    ] {
        let found = blocks(&format!("{body}<p data-rq-block=\"a\">x</p>"));
        assert_eq!(found.last(), Some(&html("a", "x")), "{body}");
    }
}

/// M-10 item 3 (H-43): no `for` attribute on a `template` start tag in HTML
/// context. WHATWG content patching (`<template for>`) replaces or removes
/// the nodes between `<?start>` and `<?end>`, which the token model reads as
/// comments.
#[test]
fn m10_3_template_for() {
    let block = "<div data-rq-block=\"a\">x</div>";
    for tag in [
        "<template for=\"p\"><b>n</b></template>",
        "<template FOR=\"\"></template>",
        "<template for></template>",
        "<template for=p></template>",
        "<TEMPLATE id=\"t\" For='p'></TEMPLATE>",
        // Any occurrence, also a duplicate.
        "<template for=\"p\" for=\"q\"></template>",
        "<template id=\"t\" id=\"u\" fOr></template>",
        "<template for/></template>",
        // Inside other elements and in a table.
        "<div><template for=\"p\"></template></div>",
        "<table><template for=\"p\"></template></table>",
    ] {
        assert_invalid(&format!("{block}{tag}"), STRUCTURE, "M-10");
        assert_invalid(&format!("{tag}{block}"), STRUCTURE, "M-10");
    }
    for tag in [
        "<template id=\"t\"><b>n</b></template>",
        "<template></template>",
        // Another name that contains `for`; another tag with `for`.
        "<template data-for=\"p\" form=\"q\" format></template>",
        "<label for=\"p\">l</label>",
        "<output for=\"p\"></output>",
        // A value is not a name.
        "<template id=\"for\" title=for></template>",
        // A foreign `template` is not affected.
        "<svg><template for=\"p\"></template></svg>",
        "<math><template for></template></math>",
    ] {
        let found = blocks(&format!("{tag}{block}"));
        assert_eq!(found.last(), Some(&html("a", "x")), "{tag}");
    }
    // After a closed foreign region the rule applies again.
    assert_invalid(
        &format!("<svg></svg><template for></template>{block}"),
        STRUCTURE,
        "M-10",
    );
    // C already forbids `template` (M-11), with or without `for`.
    assert_invalid(
        "<noscript><template for=\"p\"></template></noscript>",
        STRUCTURE,
        "M-11",
    );
}

#[test]
fn m11_noscript() {
    assert_valid(
        "<noscript><iframe src=\"https://www.example.com/ns.html\"></iframe></noscript><noscript><img src=\"p.gif\" alt=\"\"></noscript><noscript></noscript><noscript><style>a{}</style><!-- c --></noscript>",
    );
    assert_invalid(
        "<noscript><p data-rq-block=\"ns\">x</p></noscript>",
        STRUCTURE,
        "M-11",
    );
    assert_invalid(
        "<noscript><!-- </noscript><p data-rq-block=\"real\">r</p><!-- --></noscript>",
        STRUCTURE,
        "M-11",
    );
    for name in [
        "frameset",
        "math",
        "noscript",
        "plaintext",
        "select",
        "svg",
        "template",
    ] {
        assert_invalid(&format!("<noscript><{name}></noscript>"), STRUCTURE, "M-11");
    }
    // The content ends inside a tag, a raw text section or a DOCTYPE.
    assert_invalid(
        "<noscript><img src=\"</noscript>\"></noscript>",
        STRUCTURE,
        "M-11",
    );
    assert_invalid("<noscript><style></noscript>", STRUCTURE, "M-11");
    assert_invalid("<noscript><!DOCTYPE </noscript>", STRUCTURE, "M-11");
    // Self-closing noscript is still RAWTEXT.
    assert_invalid(
        "<noscript/><p data-rq-block=\"ns\">x</p></noscript>",
        STRUCTURE,
        "M-11",
    );
    // E3: without an end tag, C is the rest of the input.
    assert!(analyze(b"<noscript><img src=x>").is_ok());
    assert_eq!(
        error_of(b"<noscript><p data-rq-block=\"a\">"),
        (STRUCTURE, "M-11")
    );
    // In a foreign region noscript is not RAWTEXT; M-11 does not apply, M-07.4 does.
    assert_valid("<svg><noscript>x &lt;</noscript></svg>");
    assert_invalid("<svg><noscript><g></g></noscript></svg>", STRUCTURE, "M-07");
}

/// E8 (H-22): M-13 also applies to the content C of `noscript` (M-11). A
/// reader with scripting disabled parses C as markup.
#[test]
fn m11_m13_reserved_manifest_in_noscript() {
    let reserved = ErrorCode::Reserved;
    for script in [
        "<script type=\"application/rivqen-manifest+json\">{}</script>",
        "<SCRIPT TYPE=\"X-Rivqen-Manifest\"></SCRIPT>",
        "<script type=application/rivqen-manifest&#43;json></script>",
        "<script type='&amp;'></script>",
        "<img src=x><script type=\"rivqen-manifest\"></script>",
    ] {
        assert_invalid(&format!("<noscript>{script}</noscript>"), reserved, "M-13");
    }
    // In head, and at EOF (E3).
    assert_eq!(
        error_of(
            b"<html><head><noscript><script type=\"application/rivqen-manifest+json\">{}</script></noscript></head></html>"
        ),
        (reserved, "M-13")
    );
    assert_eq!(
        error_of(b"<noscript><script type=\"rivqen-manifest\"></script>"),
        (reserved, "M-13")
    );
    // Not a script start tag with a reserved first `type`: valid.
    for content in [
        "<script>var a = 1;</script>",
        "<script type=\"text/javascript\" type=\"rivqen-manifest\"></script>",
        "<div type=\"rivqen-manifest\"></div>",
        "rivqen-manifest &amp;",
        "<!-- <script type=\"rivqen-manifest\"> -->",
        "<style><script type=\"rivqen-manifest\"></style>",
        "<script>\"<script type='rivqen-manifest'>\"</script>",
        // An end tag with attributes is not a start tag. (A stray end tag in
        // C is invalid by M-11 item 5, so the end tag closes a script.)
        "<script></script type=\"rivqen-manifest\">",
    ] {
        assert_eq!(
            blocks(&format!(
                "<noscript>{content}</noscript><p data-rq-block=\"a\">x</p>"
            )),
            [html("a", "x")],
            "{content}"
        );
    }
}

/// M-11 item 5 (H-27): the noscript content C is balanced, checked with a
/// separate stack. With scripting disabled `</noscript>` is ignored when C
/// leaves a special element open (FX-RQ-MARKUP-NOSCRIPT-UNBALANCED, -TABLE,
/// -CROSSING, -STRAY-END, -EOF-UNBALANCED, -BALANCED).
#[test]
fn m11_5_noscript_content_is_balanced() {
    let block = "<div data-rq-block=\"a\">x<button>y</button>z</div>";
    for content in [
        // The review repro, and the other cases.
        "<button>",
        "<table>",
        "<div><p>x</div>",
        "<a href=\"#\"><span>x</a></span>",
        "</div><img src=\"p.gif\" alt=\"\">",
        "<b>x</i>",
        "</br>",
        "<div></div></div>",
        // A self-closing flag on a non-void name has no effect (M-05 item 1).
        "<span/>",
        // An end tag in raw text of C is text: `style` stays open.
        "<p><style></p></style>",
    ] {
        assert_invalid(
            &format!("<noscript>{content}</noscript>{block}"),
            STRUCTURE,
            "M-11",
        );
    }
    // E3: without an end tag, C is the rest of the input.
    assert_eq!(
        error_of(b"<p data-rq-block=\"real\">r</p><noscript><div>x"),
        (STRUCTURE, "M-11")
    );
    assert_eq!(
        blocks_of(b"<p data-rq-block=\"real\">r</p><noscript><div>x</div>"),
        vec![html("real", "r")]
    );
    // Balanced content: nested elements, void elements (not pushed), raw
    // text elements with their end tags, comments, ASCII case and U+0000 in
    // names (the tokenizer lowercases and maps U+0000 to U+FFFD).
    for content in [
        "",
        "text",
        "<div><a href=\"#\"><img src=\"p.gif\" alt=\"\"></a><table><tr><td>t</td></tr></table><p>text<br></p></div>",
        "<iframe src=\"x\"></iframe><style>a{}</style><title>t</title><textarea>x</textarea><script>s</script>",
        "<DIV><P>x</p></Div>",
        "<a\0>x</A\u{FFFD}>",
        "<span/></span>",
        "<hr/><br/><input><wbr>",
        "<!-- <div> --><!DOCTYPE x>",
    ] {
        assert_eq!(
            blocks(&format!("<noscript>{content}</noscript>{block}")),
            [html("a", "x<button>y</button>z")],
            "{content:?}"
        );
    }
    // Names that differ only in non-ASCII case are different names.
    assert_invalid(
        "<noscript><x\u{E4}></x\u{C4}></noscript>",
        STRUCTURE,
        "M-11",
    );
    // In head too.
    assert_eq!(
        error_of(b"<html><head><noscript><div></noscript></head></html>"),
        (STRUCTURE, "M-11")
    );
}

/// M-11 item 6 (H-38): a table part or `col` in the noscript content C
/// needs a `table` of C that is still open. With scripting disabled the
/// table part closes the cell or caption around `noscript`
/// (FX-RQ-MARKUP-NOSCRIPT-TABLE-PART, -CAPTION, -COL, -TH,
/// -NOSCRIPT-TABLE-IN-CELL).
#[test]
fn m11_6_table_parts_in_noscript() {
    let block = "<div data-rq-block=\"x\">a<table></table>b</div>";
    for content in [
        "<td></td>",
        "<th></th>",
        "<tr></tr>",
        "<tbody></tbody>",
        "<thead></thead>",
        "<tfoot></tfoot>",
        "<caption></caption>",
        "<colgroup></colgroup>",
        "<col>",
        "<TD></TD>",
        // The table of C is closed.
        "<table></table><tr></tr>",
        "<table></table><table></table><td></td>",
        // A name that only starts with `table` is not a table.
        "<tablex><td></td></tablex>",
    ] {
        for context in [
            "<table><tr><td>{}</td></tr></table>",
            "<table><caption>{}</caption></table>",
        ] {
            let body = context.replace("{}", &format!("<noscript>{content}</noscript>{block}"));
            assert_invalid(&body, STRUCTURE, "M-11");
        }
    }
    // Outside a cell too, and at EOF (E3).
    assert_invalid("<noscript><td></td></noscript>", STRUCTURE, "M-11");
    assert_eq!(error_of(b"<noscript><col>"), (STRUCTURE, "M-11"));
    for content in [
        "<table><tr><td>x</td></tr></table>",
        "<table><caption>c</caption><colgroup><col></colgroup><tbody><tr><th>h</th></tr></tbody></table>",
        "<TABLE><tr></tr></TABLE>",
        // The outer table is still open after the inner one closes.
        "<table><table></table><td></td></table>",
        "<table><tr><td><table></table></td><td>y</td></tr></table>",
        // Names that are not table parts.
        "<tbody-x></tbody-x><colx></colx><div>t</div>",
    ] {
        let body =
            format!("<table><tr><td><noscript>{content}</noscript>{block}</td></tr></table>");
        assert_eq!(blocks(&body), [html("x", "a<table></table>b")], "{content}");
    }
}

#[test]
fn m12_cdata() {
    assert_invalid("<p><![CDATA[ a > b ]]></p>", STRUCTURE, "M-12");
    assert_invalid("<svg><![CDATA[ a ]> ]]></svg>", STRUCTURE, "M-12");
    assert_valid("<svg><![CDATA[ a < b ]]><text>t</text></svg><p><![CDATA[x]]></p>");
}

#[test]
fn m13_reserved_manifest() {
    for ty in [
        "application/rivqen-manifest+json",
        " Application/Rivqen-Manifest+JSON ",
        "x-RIVQEN-MANIFEST",
        "application/rivqen&#45;manifest+json",
        "text/x&amp;y",
    ] {
        let body = format!("<script type=\"{ty}\">{{}}</script>");
        assert_invalid(&body, ErrorCode::Reserved, "M-13");
    }
    assert_invalid(
        "<svg><script type=\"rivqen-manifest\"></script></svg>",
        ErrorCode::Reserved,
        "M-13",
    );
    // Only the first `type` attribute counts; other elements do not count.
    assert_valid("<script type=\"text/javascript\" type=\"rivqen-manifest\"></script>");
    assert_valid(
        "<div type=\"rivqen-manifest\"></div><script>var t = \"<script type='rivqen-manifest'>\";</script>",
    );
    assert_valid("<script type=\"rivqen-manifes\"></script><script></script>");
}

/// M-13 item 2 (H-84, DL-048 a): a `&` is reserved when a letter follows, or
/// when a numeric reference has a value that can make a manifest type.
#[test]
fn m13_2_character_references_in_script_types() {
    let script = |ty: &str| format!("<script type=\"{ty}\">{{}}</script>");
    for ty in [
        "text/x-tmpl & b",
        "text/x&#58;y",
        "text/x&#x7E;",
        "text/x&#;",
        "text/x&#x;",
        "text/x&#X;",
        "text/x&",
        "text/x&#",
        "text/x&&#58;",
        "&#33;",
        "&#x21;",
        "&#x7f;",
        "&#127;",
        "&#00000000000000000000058;",
        "text/x&#58",
        "text/x&#xg;",
        "& ",
        "&1",
        "&;",
    ] {
        assert_valid(&script(ty));
        assert_valid(&format!("<noscript>{}</noscript>", script(ty)));
    }
    for ty in [
        "text/x&amp;y",
        "text/a&b",
        "&A",
        "&z",
        "application&#x2F;x",
        "application&#47;x",
        "text/&#0000065pp",
        "text/&#x41;",
        "text/&#x61;",
        "text/plain&#X9",
        "text/plain&#9;",
        "text/plain&#32;",
        "text/plain&#x20;",
        "text/plain&#0;",
        "text/plain&#x0",
        "x&#43;y",
        "x&#45;y",
        "x&#46;y",
        "x&#48;",
        "x&#57;",
        "x&#128;",
        "x&#x80;",
        "x&#305;",
        "x&#1114111;",
        "x&#4294967296;",
        "x&#xFFFFFFFFFFFFFFFFFFFF;",
        "x&#99999999999999999999999999",
        "text/x&#58;&amp;",
        "text/x&#58;&#65;",
        "x&#x5A;",
        "x&#x7A",
    ] {
        assert_invalid(&script(ty), ErrorCode::Reserved, "M-13");
        assert_invalid(
            &format!("<noscript>{}</noscript>", script(ty)),
            ErrorCode::Reserved,
            "M-13",
        );
    }
    // The boundaries of the rejected set.
    for (value, reserved) in [
        (0x20, true),
        (0x21, false),
        (0x2A, false),
        (0x2B, true),
        (0x2C, false),
        (0x2D, true),
        (0x2E, true),
        (0x2F, true),
        (0x30, true),
        (0x39, true),
        (0x3A, false),
        (0x40, false),
        (0x41, true),
        (0x5A, true),
        (0x5B, false),
        (0x60, false),
        (0x61, true),
        (0x7A, true),
        (0x7B, false),
        (0x7F, false),
        (0x80, true),
    ] {
        let decimal = script(&format!("x&#{value};"));
        let hex = script(&format!("x&#x{value:x};"));
        for text in [decimal, hex] {
            if reserved {
                assert_invalid(&text, ErrorCode::Reserved, "M-13");
            } else {
                assert_valid(&text);
            }
        }
    }
}

/// M-13 item 3 (H-28): a byte >= 0x80 in the first `type` raw value of a
/// script start tag is reserved: Unicode look-alikes of the manifest type
/// (dotless i, long s) and harmless values alike
/// (FX-RQ-MARKUP-RESERVED-NON-ASCII, -LONG-S, -OTHER, -NOSCRIPT-...).
#[test]
fn m13_3_non_ascii_script_types_are_reserved() {
    for ty in [
        "application/r\u{131}vqen-manifest+json",
        "application/rivqen-manife\u{17F}t+json",
        "text/j\u{E4}vascript",
        "\u{80}",
        "\u{1F600}",
        "text/javascript\u{A0}",
    ] {
        let script = format!("<script type=\"{ty}\">{{}}</script>");
        assert_invalid(&script, ErrorCode::Reserved, "M-13");
        // In a foreign region, in an unquoted value, and in noscript content
        // (M-11 item 4).
        assert_invalid(&format!("<svg>{script}</svg>"), ErrorCode::Reserved, "M-13");
        assert_invalid(
            &format!("<script type={ty}></script>"),
            ErrorCode::Reserved,
            "M-13",
        );
        assert_invalid(
            &format!("<noscript>{script}</noscript>"),
            ErrorCode::Reserved,
            "M-13",
        );
    }
    // E3: noscript content to the end of the input.
    assert_eq!(
        error_of("<noscript><script type=\"\u{E4}\"></script>".as_bytes()),
        (ErrorCode::Reserved, "M-13")
    );
    // Non-ASCII outside the first script `type` value is allowed.
    assert_valid(
        "<script type=\"text/javascript\" type=\"\u{E4}\" data-x=\"\u{E4}\">var \u{E4} = 1;</script>",
    );
    assert_valid("<div type=\"\u{131}\"></div><script data-type=\"\u{17F}\"></script>");
    assert_valid(
        "<noscript><img alt=\"\u{E4}\"><script type=\"text/javascript\"></script></noscript>",
    );
    // A JSON block needs the exact ASCII type anyway (M-16 comes after M-13).
    assert_invalid(
        "<script type=\"application/js\u{F6}n\" data-rq-block=\"j\">{}</script>",
        ErrorCode::Reserved,
        "M-13",
    );
}

// ---- 2.5 Blocks -------------------------------------------------------------

#[test]
fn m14_block_start_tags() {
    assert_eq!(
        blocks(
            "<p>a</p data-rq-block=\"x\"><p data-rq-block-x=\"y\">b</p><p data-rq-blocks=\"z\">c</p><p data-rq-block=\"real\">r</p>"
        ),
        [html("real", "r")]
    );
    assert_eq!(
        blocks(
            "<p data-rq-block=\"first\" data-rq-block=\"second\">x</p><span DATA-RQ-BLOCK=\"a1\" data-rq-block=\"BAD ID\">y</span>"
        ),
        [html("first", "x"), html("a1", "y")]
    );
    assert_eq!(
        blocks("<p data-rq-block=\"text\">&lt;p data-rq-block=&quot;x&quot;&gt;</p>"),
        [html("text", "&lt;p data-rq-block=&quot;x&quot;&gt;")]
    );
    assert_eq!(
        blocks("<textarea><p data-rq-block=\"f\">x</p></textarea><p data-rq-block=\"r\">r</p>"),
        [html("r", "r")]
    );
}

#[test]
fn m15_block_id() {
    assert_eq!(
        blocks(&format!(
            "<p data-rq-block=\"0\">a</p><p data-rq-block=\"a_b-c9\">b</p><p data-rq-block=\"{}\">c</p>",
            "x".repeat(64)
        ))
        .len(),
        3
    );
    for id in [
        String::new(),
        "-a".to_owned(),
        "_a".to_owned(),
        "Price".to_owned(),
        "a.b".to_owned(),
        " a".to_owned(),
        "a ".to_owned(),
        "\u{0446}".to_owned(),
        "&#112;rice".to_owned(),
        "x".repeat(65),
        "a/b".to_owned(),
        "a:b".to_owned(),
        "a{".to_owned(),
        "`a".to_owned(),
    ] {
        let body = format!("<p data-rq-block=\"{id}\">x</p>");
        assert_invalid(&body, ErrorCode::InvalidId, "M-15");
    }
    assert_invalid("<p data-rq-block>x</p>", ErrorCode::InvalidId, "M-15");
}

#[test]
fn m16_block_element() {
    for body in [
        "<ul><li data-rq-block=\"item\">a</li></ul>",
        "<style data-rq-block=\"css\">p{}</style>",
        "<textarea data-rq-block=\"t\">t</textarea>",
        "<script data-rq-block=\"code\">1</script>",
        "<script type=\"application/ld+JSON\" data-rq-block=\"ld\">{}</script>",
        "<script type=\"application/ld+json;charset=utf-8\" data-rq-block=\"ld\">{}</script>",
        "<script type=\"application/ld+json \" data-rq-block=\"ld\">{}</script>",
        "<script type=\"application/ld-json\" data-rq-block=\"ld\">{}</script>",
        "<script type=\"Application/JSON\" data-rq-block=\"d\">{}</script>",
        "<script type=\" application/json\" data-rq-block=\"d\">{}</script>",
        "<script type=\"application/json;charset=utf-8\" data-rq-block=\"d\">{}</script>",
        "<svg><title data-rq-block=\"icon\">Icon</title></svg>",
        "<math><mi data-rq-block=\"m\">x</mi></math>",
        "<template data-rq-block=\"tpl\"><p>a</p></template>",
        "<img data-rq-block=\"logo\" src=\"logo.png\" alt=\"\">",
        "<b data-rq-block=\"b\">x</b>",
        "<html data-rq-block=\"h\">",
    ] {
        assert_invalid(body, ErrorCode::ForbiddenElement, "M-16");
    }
    let names = [
        "article", "aside", "div", "footer", "h1", "h2", "h3", "h4", "h5", "h6", "header", "main",
        "nav", "p", "section", "span",
    ];
    for name in names {
        let body = format!("<{name} data-rq-block=\"b\">x</{name}>");
        assert_eq!(blocks(&body), [html("b", "x")], "{name}");
    }
    let doc = blocks_of(b"<title data-rq-block=\"t\">a &amp; <b></title><script type=\"application/json\" data-rq-block=\"j\">{\"a\":\"</p>\"}</script>");
    assert_eq!(
        doc,
        [
            html("t", "a &amp; <b>"),
            ("j".to_owned(), Format::Json, "{\"a\":\"</p>\"}".to_owned())
        ]
    );
    // H-86: `application/ld+json` is a `json` type; M-25 applies to it.
    assert_eq!(
        blocks(
            "<script type=\"application/ld+json\" data-rq-block=\"ld\">{\"@context\":\"x\"}</script>"
        ),
        [(
            "ld".to_owned(),
            Format::Json,
            "{\"@context\":\"x\"}".to_owned()
        )]
    );
    assert_invalid(
        "<script type=\"application/ld+json\" data-rq-block=\"ld\">{\"a\":}</script>",
        ErrorCode::BadJson,
        "M-25",
    );
    assert_invalid(
        "<script type=\"application/ld+json\" data-rq-block=\"ld\"></script>",
        ErrorCode::BadJson,
        "M-25",
    );
}

#[test]
fn m17_nested() {
    assert_invalid(
        "<div data-rq-block=\"outer\"><p data-rq-block=\"inner\">x</p></div>",
        ErrorCode::Nested,
        "M-17",
    );
    // Checked before M-15 and M-16 for the inner start tag.
    assert_invalid(
        "<div data-rq-block=\"outer\"><li data-rq-block=\"BAD\">x</li></div>",
        ErrorCode::Nested,
        "M-17",
    );
    // A block after the end of another block is not nested.
    assert_eq!(
        blocks("<div data-rq-block=\"a\">x</div><div data-rq-block=\"b\">y</div>").len(),
        2
    );
}

#[test]
fn m18_start_tag_form() {
    assert_invalid("<div data-rq-block=\"box\"/>a</div>", STRUCTURE, "M-18");
    assert_invalid("<title data-rq-block=\"t\"/>a</title>", STRUCTURE, "M-18");
}

#[test]
fn m19_block_context() {
    assert_invalid(
        "<template><p data-rq-block=\"row\">a</p></template>",
        STRUCTURE,
        "M-19",
    );
    assert_invalid(
        "<table><template><td><p data-rq-block=\"r\">a</p></td></template></table>",
        STRUCTURE,
        "M-19",
    );
    for wrapper in [
        "<table>",
        "<table><tbody>",
        "<table><tr>",
        "<table><colgroup>",
        "<table><thead>",
        "<table><tfoot>",
    ] {
        let body = format!("{wrapper}<div data-rq-block=\"box\">b</div>");
        assert_invalid(&body, STRUCTURE, "M-19");
    }
    assert_eq!(
        blocks("<table><caption><span data-rq-block=\"cap\">P</span></caption><tr><td><span data-rq-block=\"cell\">1</span></td><th><div data-rq-block=\"head\">H</div></th></tr></table>").len(),
        3
    );
    // After `</template>` and `</table>` the context is free again.
    assert_valid(
        "<template><p>x</p></template><table><tr><td>a</td></tr></table><p data-rq-block=\"a\">x</p>",
    );
    // A block after `</html>` and after an open `p` is valid.
    assert_valid("<p>Intro<div data-rq-block=\"box\">x</div>");
    assert!(analyze(b"<html><body></body></html><p data-rq-block=\"late\">x</p>").is_ok());
}

#[test]
fn m19_select_context() {
    // M-08 fails first for a div in a select; both are structure errors.
    let (code, _) = error("<select><div data-rq-block=\"a\">x</div></select>");
    assert_eq!(code, STRUCTURE);
}

#[test]
fn m20_explicit_end_tag() {
    assert_eq!(
        error_of(b"<div data-rq-block=\"box\">abc"),
        (STRUCTURE, "M-20")
    );
    assert_eq!(
        error_of(b"<div data-rq-block=\"box\"><b>abc</b>"),
        (STRUCTURE, "M-20")
    );
    assert_eq!(
        error_of(b"<script type=\"application/json\" data-rq-block=\"d\">{}"),
        (STRUCTURE, "M-20")
    );
    assert_eq!(
        error_of(b"<title data-rq-block=\"t\">Shop</title"),
        (STRUCTURE, "M-20")
    );
    // Implied end: M-24 a.
    assert_invalid("<p data-rq-block=\"lead\">one<p>two</p>", STRUCTURE, "M-24");
}

#[test]
fn m21_unique_id() {
    assert_invalid(
        "<p data-rq-block=\"price\">1</p><span data-rq-block=\"price\">2</span>",
        ErrorCode::Duplicate,
        "M-21",
    );
    assert_eq!(
        blocks("<p data-rq-block=\"a\">1</p><p data-rq-block=\"b\">2</p>").len(),
        2
    );
}

// ---- 2.6 Content of html blocks --------------------------------------------

#[test]
fn m22_allowed_tokens() {
    assert_invalid(
        "<div data-rq-block=\"b\"><!DOCTYPE html>a</div>",
        STRUCTURE,
        "M-22",
    );
    for name in [
        "body",
        "frame",
        "head",
        "html",
        "noembed",
        "noframes",
        "optgroup",
        "option",
        "plaintext",
        "select",
        "textarea",
        "title",
        "xmp",
    ] {
        let body = format!("<div data-rq-block=\"b\"><{name}>");
        assert_invalid(&body, STRUCTURE, "M-22");
    }
    assert_invalid(
        "<div data-rq-block=\"b\"><span/></span></div>",
        STRUCTURE,
        "M-22",
    );
    assert_invalid(
        "<div data-rq-block=\"b\">a<br></br>b</div>",
        STRUCTURE,
        "M-22",
    );
    assert_invalid("<div data-rq-block=\"b\"></img></div>", STRUCTURE, "M-22");
    // Item 3 for `script` and item 4 for the void names that moved (H-78).
    assert_invalid(
        "<div data-rq-block=\"b\"><script/></div>",
        STRUCTURE,
        "M-22",
    );
    assert_invalid("<div data-rq-block=\"b\"><style/></div>", STRUCTURE, "M-22");
    assert_invalid(
        "<div data-rq-block=\"b\"><iframe/></div>",
        STRUCTURE,
        "M-22",
    );
    for name in ["embed", "link", "meta", "base", "basefont", "bgsound"] {
        let body = format!("<div data-rq-block=\"b\"><{name}></{name}></div>");
        assert_invalid(&body, STRUCTURE, "M-22");
    }
    // Comments, character references, void elements with `/>`, other elements.
    assert_eq!(
        blocks(
            "<div data-rq-block=\"b\"><!-- c --><?pi?>&amp;<br/><img src=\"a\"/><custom-el>x</custom-el></div>"
        ),
        [html(
            "b",
            "<!-- c --><?pi?>&amp;<br/><img src=\"a\"/><custom-el>x</custom-el>"
        )]
    );
}

/// M-22 item 6 (H-94, H-97, DL-056): an HTML `template` start tag in the
/// content with an attribute named `shadowrootmode` or `shadowroot`.
#[test]
fn m22_6_declarative_shadow_root_attributes() {
    for attrs in [
        "shadowrootmode=\"open\"",
        "SHADOWROOTMODE",
        "shadowroot=\"open\"",
        "shadowroot",
        "SHADOWROOT=closed",
        "id=\"a\" ShadowRoot=\"\" class=\"b\" shadowroot=\"open\"",
        "id=\"t\" data-x=\"1\" shadowrootclonable shadowroot=\"open\"",
    ] {
        let body = format!("<div data-rq-block=\"b\"><template {attrs}>x</template></div>");
        assert_invalid(&body, STRUCTURE, "M-22");
    }
    // In a template of the content (H-94: the template contents are content).
    assert_invalid(
        "<div data-rq-block=\"b\"><template><template shadowroot>x</template></template></div>",
        STRUCTURE,
        "M-22",
    );
    assert_invalid(
        "<div data-rq-block=\"b\"><template><b><template SHADOWROOT=open>x</template></b></template></div>",
        STRUCTURE,
        "M-22",
    );
    // Other names, values, foreign context, other elements and outside the content.
    for content in [
        "<template shadowrootx=\"open\" shadowroots>t</template>",
        "<template data-shadowroot=\"open\" title=\"shadowroot\">t</template>",
        "<template shadowrootclonable shadowrootdelegatesfocus>t</template>",
        "<svg><template shadowroot=\"open\"></template></svg>",
        "<div shadowroot=\"open\">t</div>",
    ] {
        let body = format!("<div data-rq-block=\"b\">{content}</div>");
        assert_eq!(blocks(&body), [html("b", content)], "{content:?}");
    }
    assert_valid(
        "<div><template shadowroot=\"open\">x</template><p data-rq-block=\"b\">y</p></div>",
    );
}

/// H-78: the patch-safety elements are allowed in the content (M-22).
#[test]
fn m22_patch_safety_elements_are_allowed_in_the_content() {
    for content in [
        "<base href=\"/\">",
        "<basefont>",
        "<bgsound>",
        "<embed src=\"a\">",
        "<link rel=\"stylesheet\" href=\"a.css\">",
        "<meta name=\"a\" content=\"b\">",
        "<object data=\"a\">x</object>",
        "<script>f()</script>",
        "<style>p{}</style>",
        "<iframe src=\"/v\"></iframe>",
        "<link/><meta/><embed/>",
    ] {
        assert_eq!(
            blocks(&format!("<div data-rq-block=\"b\">{content}</div>")),
            [html("b", content)],
            "{content}"
        );
    }
    // An `object` is pushed: its own end tag is needed (M-23).
    assert_invalid(
        "<div data-rq-block=\"b\"><object>x</div>",
        STRUCTURE,
        "M-23",
    );
    // M-13 still applies to a script in the content.
    assert_invalid(
        "<div data-rq-block=\"b\"><script type=\"rivqen-manifest\"></script></div>",
        ErrorCode::Reserved,
        "M-13",
    );
}

/// M-04 in the content (H-78): raw text up to the appropriate end tag.
#[test]
fn m04_raw_text_in_the_content() {
    for content in [
        // The end tag of the block is only text in script data.
        "x<script>document.write(\"</div>\")</script>y",
        "<script>a</scripty></script>",
        "<script>a</script foo>b",
        "<SCRIPT>a</SCRIPT >b",
        "<script>a</script/>b",
        "<script>a<!--<script>b</script>c--></script>d",
        "<script><!-- </script> -->",
        "<script><div data-rq-block=\"x\"><!-- <p></script>",
        "<style>a</div>b</style>",
        "<style></styles></style>",
        "<iframe></div><p></iframe>",
        "<iframe><!--</iframe>",
    ] {
        assert_eq!(
            blocks(&format!("<div data-rq-block=\"b\">{content}</div>")),
            [html("b", content)],
            "{content}"
        );
    }
    // Raw text that does not end before the end of the input: M-20.
    for body in [
        "<div data-rq-block=\"b\"><script>a</div>",
        "<div data-rq-block=\"b\"><script><!--<script>a</script></div>",
        "<div data-rq-block=\"b\"><style>a</div>",
        "<div data-rq-block=\"b\"><iframe>a</div>",
        "<div data-rq-block=\"b\"><script>a</scripty></div>",
    ] {
        assert_eq!(error_of(body.as_bytes()), (STRUCTURE, "M-20"), "{body}");
    }
    // The raw text element is the current node: the end tag pops it, and the
    // elements around it still match (M-23).
    assert_eq!(
        blocks("<div data-rq-block=\"b\"><p><style>a</style></p></div>"),
        [html("b", "<p><style>a</style></p>")]
    );
    assert_invalid(
        "<div data-rq-block=\"b\"><span><style>a</style></div></span>",
        STRUCTURE,
        "M-23",
    );
    // In a foreign region a start tag does not change the state: the
    // element holds text only (M-07 item 4).
    assert_eq!(
        blocks("<div data-rq-block=\"b\"><svg><script>a &lt; b</script></svg></div>").len(),
        1
    );
    assert_invalid(
        "<div data-rq-block=\"b\"><svg><script>a<b></b></script></svg></div>",
        STRUCTURE,
        "M-07",
    );
}

/// Raw text in the content is read once (linear work): large inputs of
/// scripts, styles and unfinished escaped states finish.
#[test]
fn m04_raw_text_in_the_content_is_linear() {
    let n = 20_000;
    let many = format!(
        "<div data-rq-block=\"b\">{}</div>",
        "<script>a</script>".repeat(n)
    );
    assert_eq!(blocks(&many).len(), 1);
    for unit in [
        "<script><!--<script>",
        "<style><",
        "<iframe></ifram",
        "<script><!--",
    ] {
        let body = format!("<div data-rq-block=\"b\">{}", unit.repeat(n));
        assert_eq!(
            error_of(page(&body).as_bytes()),
            (STRUCTURE, "M-20"),
            "{unit}"
        );
    }
    let amps = format!("<script type=\"x{}\"></script>", "&#0".repeat(50_000));
    assert_valid(&amps.replace("&#0", "&#58;"));
    assert_invalid(&amps, ErrorCode::Reserved, "M-13");
}

#[test]
fn m23_balance() {
    assert_invalid(
        "<div data-rq-block=\"b\"><span>a</div></span>",
        STRUCTURE,
        "M-23",
    );
    assert_invalid(
        "<div data-rq-block=\"b\">a</span>b</div>",
        STRUCTURE,
        "M-23",
    );
    assert_invalid(
        "<div><div data-rq-block=\"b\">a</div></div x><span data-rq-block=\"c\">x</div>",
        STRUCTURE,
        "M-23",
    );
    assert_invalid(
        "<ul><li><div data-rq-block=\"b\">a</li></div></ul>",
        STRUCTURE,
        "M-23",
    );
    assert_invalid("<div data-rq-block=\"b\"><p>a</div>", STRUCTURE, "M-23");
    assert_eq!(
        blocks("<div data-rq-block=\"b\"><div><p>a</p><div></div></div></div>"),
        [html("b", "<div><p>a</p><div></div></div>")]
    );
}

#[test]
fn m24_implied_end_tags() {
    let invalid = [
        // a
        "<p data-rq-block=\"b\"><ul><li>a</li></ul></p>",
        "<p>a <span data-rq-block=\"b\"><div>b</div></span></p>",
        "<span data-rq-block=\"b\"><p>x</p></span>",
        "<span data-rq-block=\"b\"><h2>x</h2></span>",
        // b
        "<h2 data-rq-block=\"b\"><h3>a</h3></h2>",
        "<div data-rq-block=\"b\"><h2>a<h3>b</h3></h2></div>",
        // c
        "<dl><dd><div data-rq-block=\"b\"><dt>a</dt></div></dd></dl>",
        "<ul><li><div data-rq-block=\"b\"><li>a</li></div></li></ul>",
        "<div data-rq-block=\"b\"><dd>a</dd></div>",
        // d, e, f
        "<button><span data-rq-block=\"b\"><button>a</button></span></button>",
        "<a href=\"#\"><span data-rq-block=\"b\"><a href=\"#\">b</a></span></a>",
        "<nobr><span data-rq-block=\"b\"><nobr>b</nobr></span></nobr>",
        "<div data-rq-block=\"b\"><a><a>x</a></a></div>",
        // g
        "<div data-rq-block=\"b\"><tr><td>a</td></tr></div>",
        "<div data-rq-block=\"b\"><td>a</td></div>",
        "<div data-rq-block=\"b\"><col></div>",
        "<div data-rq-block=\"b\"><caption>c</caption></div>",
        "<table><tr><td><div data-rq-block=\"b\"><td>a</td></div></td></tr></table>",
        // h
        "<ruby>a<p data-rq-block=\"b\"><rb>b</rb></p></ruby>",
        "<div data-rq-block=\"b\"><ruby>a<rt>b<rb>c</rb></rt></ruby></div>",
        "<div data-rq-block=\"b\"><ruby><rtc>a<rtc>c</rtc></rtc></ruby></div>",
        "<div data-rq-block=\"b\"><ruby><rp>a<rtc>c</rtc></rp></ruby></div>",
        "<div data-rq-block=\"b\"><ruby><rb>a<rb>c</rb></rb></ruby></div>",
        // i
        "<ruby>a<p data-rq-block=\"b\"><rt>b</rt></p></ruby>",
        "<div data-rq-block=\"b\"><ruby><rt>a<rp>b</rp></rt></ruby></div>",
        "<div data-rq-block=\"b\"><ruby><rb>a<rt>b</rt></rb></ruby></div>",
        "<div data-rq-block=\"b\"><ruby><rp>a<rp>b</rp></rp></ruby></div>",
        "<div data-rq-block=\"b\"><rp>a</rp></div>",
    ];
    for body in invalid {
        assert_invalid(body, STRUCTURE, "M-24");
    }
    let valid = [
        "<p>Price: <span data-rq-block=\"b\">120</span> EUR</p>",
        "<div data-rq-block=\"b\"><p>a</p><ul><li>x</li></ul></div>",
        "<div data-rq-block=\"b\"><h2>a</h2><h3>b</h3></div>",
        "<h1><div data-rq-block=\"b\">x</div></h1>",
        "<div data-rq-block=\"b\"><dl><dt>a</dt><dd>b</dd></dl><menu><li>m</li></menu><ol><li>o</li></ol></div>",
        "<div data-rq-block=\"b\"><button>a</button><a>b</a><nobr>c</nobr></div>",
        "<div data-rq-block=\"b\"><table><caption>c</caption><colgroup><col></colgroup><tr><td>a</td></tr></table></div>",
        "<div data-rq-block=\"b\"><ruby>a<rb>b</rb><rt>c</rt><rp>(</rp><rtc>d</rtc></ruby></div>",
        "<p data-rq-block=\"b\"><b>a</b><i>b</i></p>",
        "<span data-rq-block=\"b\"><a href=\"#\">a</a></span>",
    ];
    for body in valid {
        assert_eq!(blocks(body).len(), 1, "{body}");
    }
}

/// M-24 item j (H-18): an `li`, `dd` or `dt` opened in the content is above
/// the nearest list opened in the content.
#[test]
fn m24_j_list_item_above_the_nearest_list() {
    let invalid = [
        "<ul><li><div><li>a</li></div></li></ul>",
        "<ul><li>a<li>b</li></li></ul>",
        "<ol><li><span><li>a</li></span></li></ol>",
        "<menu><li><p><li>a</li></p></li></menu>",
        "<dl><dd><dt>a</dt></dd></dl>",
        "<dl><dt><span><dd>a</dd></span></dt></dl>",
        "<dl><dd><dd>a</dd></dd></dl>",
        "<ol><li><dd>a</dd></li></ol>",
        "<dl><dt><li>a</li></dt></dl>",
        "<ul><li><ul><li>a</li></ul><div><li>b</li></div></li></ul>",
    ];
    for content in invalid {
        let body = format!("<div data-rq-block=\"b\">{content}</div>");
        assert_invalid(&body, STRUCTURE, "M-24");
    }
    let valid = [
        "<ul><li>a</li><li>b</li></ul>",
        "<ul><li><ul><li>a</li></ul></li><li>b</li></ul>",
        "<ul><li><dl><dt>a</dt><dd>b</dd></dl></li></ul>",
        "<dl><dd><ol><li>a</li></ol></dd></dl>",
        "<menu><li>a</li></menu><dl><dt>b</dt></dl>",
    ];
    for content in valid {
        let body = format!("<div data-rq-block=\"b\">{content}</div>");
        assert_eq!(blocks(&body), [html("b", content)]);
    }
    // An item outside the content is below the list of the content.
    assert_eq!(
        blocks("<ul><li><div data-rq-block=\"b\"><ul><li>a</li></ul></div></li></ul>").len(),
        1
    );
    // Outside a block, j does not apply (a content rule).
    assert_valid("<ul><li><div><li>a</li></div></li></ul>");
}

/// M-24 item k (H-18): in table context, only table parts and `col` start
/// tags are allowed in the content. Text and comments stay allowed.
#[test]
fn m24_k_table_context() {
    let invalid = [
        "<table><div>a</div></table>",
        "<table><table></table></table>",
        "<table><br></table>",
        "<table><tbody><span>a</span></tbody></table>",
        "<table><thead><p>a</p></thead></table>",
        "<table><tfoot><b>a</b></tfoot></table>",
        "<table><tr><img src=x></tr></table>",
        "<table><colgroup><span></span></colgroup></table>",
        "<table><caption>c</caption><div>a</div></table>",
        "<table><tr><td>a</td><x-y></x-y></tr></table>",
    ];
    for content in invalid {
        let body = format!("<div data-rq-block=\"b\">{content}</div>");
        assert_invalid(&body, STRUCTURE, "M-24");
    }
    let valid = [
        "<table>Total<!-- c --><![CDATA[x]]><tbody><tr><td><div>a</div><p>b</p></td></tr></tbody></table>",
        "<table><caption><p>c</p></caption><colgroup><col></colgroup><col></table>",
        "<table><thead><tr><th><span>h</span></th></tr></thead><tfoot></tfoot></table>",
        "<table><tr><td><table><tr><td>x</td></tr></table></td></tr></table>",
        "<table></table><div>a</div>",
    ];
    for content in valid {
        let body = format!("<div data-rq-block=\"b\">{content}</div>");
        assert_eq!(blocks(&body), [html("b", content)]);
    }
    // A table part in the wrong table context is M-09, not k.
    assert_invalid(
        "<div data-rq-block=\"b\"><table><tbody><col></tbody></table></div>",
        STRUCTURE,
        "M-09",
    );
    // A block in a cell: k looks at F, and F is the cell.
    assert_eq!(
        blocks("<table><tr><td><div data-rq-block=\"b\"><b>a</b></div></td></tr></table>"),
        [html("b", "<b>a</b>")]
    );
    // Outside a block, k does not apply (a content rule; M-19 keeps blocks
    // out of table context).
    assert_valid("<table><div>a</div></table>");
}

// ---- 2.7 JSON, 2.8 Limits, 2.9 Result -------------------------------------

#[test]
fn m25_json() {
    for content in ["{\"a\":1,}", "", "[\"\\ud800\"]", "\u{a0}[1]", "[1] x"] {
        let body =
            format!("<script type=\"application/json\" data-rq-block=\"data\">{content}</script>");
        assert_invalid(&body, ErrorCode::BadJson, "M-25");
    }
    let deep = |n: usize| {
        format!(
            "<script type=\"application/json\" data-rq-block=\"d\">{}{}</script>",
            "[".repeat(n),
            "]".repeat(n)
        )
    };
    assert_eq!(blocks(&deep(64)).len(), 1);
    assert_invalid(&deep(65), ErrorCode::BadJson, "M-25");
    assert_eq!(
        blocks("<script type=\"application/json\" data-rq-block=\"e\">\r\n {\"n\": 1e400, \"a\": 1, \"a\": 2, \"s\": \"\\ud83d\\ude00\", \"e\": \"<\\/script>\"}\n</script>")
            .len(),
        1
    );
    // HTML blocks are never checked as JSON.
    assert_eq!(blocks("<div data-rq-block=\"h\">{</div>").len(), 1);
}

#[test]
fn m26_block_count() {
    let many = |n: usize| -> String {
        (0..n)
            .map(|i| format!("<p data-rq-block=\"b{i}\">{i}</p>"))
            .collect()
    };
    assert_eq!(blocks(&many(256)).len(), 256);
    assert_invalid(&many(257), ErrorCode::Limit, "M-26");
}

#[test]
fn m26_block_size() {
    let sized = |n: usize| format!("<div data-rq-block=\"big\">{}</div>", "a".repeat(n));
    assert_eq!(blocks(&sized(MAX_BLOCK_BYTES)).len(), 1);
    assert_invalid(&sized(MAX_BLOCK_BYTES + 1), ErrorCode::Limit, "M-26");
    // The size limit is checked before the JSON check.
    let json = format!(
        "<script type=\"application/json\" data-rq-block=\"j\">[{}</script>",
        " ".repeat(MAX_BLOCK_BYTES)
    );
    assert_invalid(&json, ErrorCode::Limit, "M-26");
}

// M-26 depth counter D (H-25, H-31). These inputs do not use `page()`: its
// `html` and `body` elements hold units of D and would change the counts.

const LIMIT: (ErrorCode, &str) = (ErrorCode::Limit, "M-26");

fn depth_valid(input: &str) {
    if let Err(e) = analyze(input.as_bytes()) {
        panic!("{} bytes: expected valid, got {e}", input.len());
    }
}

fn depth_limit(input: &str) {
    assert_eq!(error_of(input.as_bytes()), LIMIT, "{} bytes", input.len());
}

#[test]
fn m26_depth_in_html_context_outside_blocks() {
    // A push that makes the stack larger than 256 elements fails.
    depth_valid(&"<div>".repeat(256));
    depth_limit(&"<div>".repeat(257));
    // Every name counts, also elements that end tags close later and raw
    // text elements (M-04).
    depth_limit(&format!("{}{}", "<b>".repeat(257), "</b>".repeat(257)));
    depth_valid(&format!("{}<title>x</title>", "<b>".repeat(255)));
    depth_limit(&format!("{}<title>x</title>", "<b>".repeat(256)));
    depth_limit(&format!("{}<noscript></noscript>", "<b>".repeat(256)));
    // A self-closing flag on a name that is not void has no effect: the
    // element is pushed (M-05 item 1).
    depth_valid(&format!("{}<span/>", "<div>".repeat(255)));
    depth_limit(&format!("{}<span/>", "<div>".repeat(256)));
}

#[test]
fn m26_depth_of_a_block_element() {
    // FX-RQ-MARKUP-LIMIT-DEPTH-256 and -257: the block element is the 256th
    // or the 257th element.
    let block = |n: usize| {
        format!(
            "{}<div data-rq-block=\"deep\">x</div>{}",
            "<div>".repeat(n),
            "</div>".repeat(n)
        )
    };
    let input = block(255);
    assert_eq!(blocks_of(input.as_bytes()), vec![html("deep", "x")]);
    depth_limit(&block(256));
}

#[test]
fn m26_depth_in_block_content() {
    // The block element and 255 elements in the content: 256 elements.
    let content = |open: &str, close: &str, n: usize| {
        format!(
            "<div data-rq-block=\"c\">{}{}</div>",
            open.repeat(n),
            close.repeat(n)
        )
    };
    let input = content("<b>", "</b>", 255);
    assert_eq!(blocks_of(input.as_bytes()).len(), 1);
    depth_limit(&content("<b>", "</b>", 256));
    // Elements around the block count too.
    let around = format!(
        "{}{}{}",
        "<section>".repeat(100),
        content("<b>", "</b>", 155),
        "</section>".repeat(100)
    );
    assert_eq!(blocks_of(around.as_bytes()).len(), 1);
    let around = format!("{}{}", "<section>".repeat(100), content("<b>", "</b>", 156));
    depth_limit(&around);
    // Lists in the content: the block element and 127 `ul` + `li` pairs are
    // 255 elements; the `li` of the 128th pair is the 257th element.
    let input = content("<ul><li>", "</li></ul>", 127);
    assert_eq!(blocks_of(input.as_bytes()).len(), 1);
    depth_limit(&format!(
        "<div data-rq-block=\"c\">{}",
        "<ul><li>".repeat(128)
    ));
}

#[test]
fn m26_depth_in_a_foreign_region() {
    // FX-RQ-MARKUP-LIMIT-DEPTH-FOREIGN: `svg` and 255 or 256 `g` elements.
    depth_valid(&format!("<svg>{}", "<g>".repeat(255)));
    depth_limit(&format!("<svg>{}", "<g>".repeat(256)));
    depth_limit(&format!("<math>{}", "<mrow>".repeat(256)));
    // HTML elements before the region count.
    depth_valid(&format!("{}<svg>{}", "<div>".repeat(200), "<g>".repeat(55)));
    depth_limit(&format!("{}<svg>{}", "<div>".repeat(200), "<g>".repeat(56)));
    // The `svg` element itself is the 257th element.
    depth_limit(&format!("{}<svg>", "<div>".repeat(256)));
}

#[test]
fn m26_elements_that_are_not_pushed_do_not_count() {
    // FX-RQ-MARKUP-LIMIT-DEPTH-VOID: at 256 elements, void elements and
    // self-closing `svg` and `math` are not pushed (M-05 item 1).
    let full = "<div>".repeat(256);
    depth_valid(&format!(
        "{full}<br><img src=\"x\"><input><hr/><wbr><svg/><math/><svg/>"
    ));
    // In a foreign region, a self-closing element is not pushed (M-05 item 2).
    depth_valid(&format!(
        "<svg>{}<g/><path d=\"M0\"/><rect/>",
        "<g>".repeat(255)
    ));
    // In block content, void elements are not pushed.
    let content = format!(
        "{}<br><img src=\"x\">{}",
        "<b>".repeat(255),
        "</b>".repeat(255)
    );
    let input = format!("<div data-rq-block=\"v\">{content}</div>");
    assert_eq!(blocks_of(input.as_bytes()), vec![html("v", &content)]);
    // Characters, comments and DOCTYPE tokens do not change the stack.
    depth_valid(&format!("{full}text<!-- c --><!DOCTYPE html><?pi?>"));
}

#[test]
fn m26_depth_recovers_after_exact_pops() {
    // M-26 item 4: 256 elements, all closed by exact pops, then 256
    // elements again.
    let fill = "<div>".repeat(256);
    let close = "</div>".repeat(256);
    depth_valid(&format!("{fill}{close}{fill}"));
    depth_limit(&format!("{fill}{close}{fill}<div>"));
    // A foreign region closed element by element.
    let region = format!("<svg>{}{}</svg>", "<g>".repeat(255), "</g>".repeat(255));
    depth_valid(&format!("{region}{region}{fill}"));
    // In block content each end tag pops the current node (M-23); after the
    // block end tag the stack is empty again.
    let block = |id: &str| {
        format!(
            "<div data-rq-block=\"{id}\">{}{}</div>",
            "<b>".repeat(255),
            "</b>".repeat(255)
        )
    };
    let input = format!("{}{}{fill}", block("a"), block("b"));
    assert_eq!(blocks_of(input.as_bytes()).len(), 2);
}

/// `n` copies of `unit`, then a block. The block element adds 1 to D.
fn units_then_block(unit: &str, n: usize) -> String {
    format!("{}<p data-rq-block=\"z\">x</p>", unit.repeat(n))
}

/// M-26 item 5 (H-31): an end tag that crosses elements gives no units
/// back. The review repro: 300 × `<span><div></span>` before a block gives a
/// WHATWG stack of 603 elements while the token stack is empty
/// (FX-RQ-MARKUP-LIMIT-DEPTH-IGNORED-END, -127).
#[test]
fn m26_d_crossings_keep_their_units() {
    let unit = "<span><div></span>";
    depth_limit(&units_then_block(unit, 300));
    // Each unit keeps 2: 127 units and the block are 255, 128 units are 256
    // and the block element is the 257th unit.
    assert_eq!(
        blocks_of(units_then_block(unit, 127).as_bytes()),
        vec![html("z", "x")]
    );
    depth_limit(&units_then_block(unit, 128));
    // The same element closed explicitly gives everything back.
    depth_valid(&units_then_block("<span><div></div></span>", 1000));
    // The shapes where the tree builder ignores an end tag that the token
    // stack uses (FX-RQ-MARKUP-LIMIT-DEPTH-MIXED): each keeps 2 or 3.
    for (unit, kept) in [
        ("<h1><span><h2>x</h1>", 3),
        ("<li><ul></li>", 2),
        ("<form><div></form>", 2),
        ("<p><b>x</p>y", 2),
    ] {
        let n = 255 / kept;
        depth_valid(&units_then_block(unit, n));
        depth_limit(&units_then_block(unit, n + 1));
    }
    // A crossing in a foreign region keeps its units too.
    depth_valid(&units_then_block("<svg><g></svg>", 127));
    depth_limit(&units_then_block("<svg><g></svg>", 128));
}

/// M-26 item 5: after a crossing, the new current node is dirty; its own
/// exact end tag then gives nothing back (FX-RQ-MARKUP-LIMIT-DEPTH-DIRTY,
/// -DIRTY-CLEAN).
#[test]
fn m26_d_dirty_elements_keep_their_units() {
    // `</span>` crosses the `div`: the middle `span` is dirty, its pop makes
    // the outer `span` dirty. Each unit keeps 4.
    let unit = "<span><span><span><div></span></span></span>";
    depth_valid(&units_then_block(unit, 63));
    depth_limit(&units_then_block(unit, 64));
    // Two levels: each unit keeps 3 (markup.md M-26 example).
    let unit = "<span><span><div></span></span>";
    depth_valid(&units_then_block(unit, 85));
    depth_limit(&units_then_block(unit, 86));
    // A second crossing keeps the dirty mark: each unit keeps 5.
    let unit = "<div><span><b></span><span><b></span></div>";
    depth_valid(&units_then_block(unit, 51));
    depth_limit(&units_then_block(unit, 52));
    // Without the crossing nothing is dirty.
    depth_valid(&units_then_block(
        "<span><span><span><div></div></span></span></span>",
        1000,
    ));
    // The dirty mark goes away with its element: a new element with the
    // same name is clean.
    depth_valid(&format!(
        "<div><span><b></span></div>{}",
        "<div></div>".repeat(1000)
    ));
}

/// M-26 items 4 and 5: the pop of `html` or `body` gives nothing back, also
/// as an exact pop, and makes the new current node dirty.
#[test]
fn m26_d_html_and_body_keep_their_units() {
    for unit in ["<body></body>", "<html></html>"] {
        depth_valid(&units_then_block(unit, 255));
        depth_limit(&units_then_block(unit, 256));
    }
    let unit = "<div><body></body></div>";
    depth_valid(&units_then_block(unit, 127));
    depth_limit(&units_then_block(unit, 128));
    // Other names pop exactly.
    depth_valid(&units_then_block("<head></head><main></main>", 1000));
}

/// M-26 item 3: implied table parts (row group, row, column group) count
/// once per F element and come back with the exact pop of F
/// (FX-RQ-MARKUP-LIMIT-DEPTH-TABLE, -TABLE-63).
#[test]
fn m26_d_implied_table_parts() {
    let divs = |n: usize| "<div>".repeat(n);
    // `table`, row group, row and `td`: 4.
    for table in [
        "<table><td>a</td></table>",
        "<table><tr><td>a</td></tr></table>",
        "<table><tbody><td>a</td></tbody></table>",
        "<table><tbody><tr><td>a</td></tr></tbody></table>",
        "<table><thead><th>a</th></thead></table>",
    ] {
        depth_valid(&format!("{}{table}", divs(252)));
        depth_limit(&format!("{}{table}", divs(253)));
        // Each part comes back with its F.
        depth_valid(&format!("{}{}", table.repeat(1000), divs(256)));
    }
    // Each part counts once per F: many cells and rows.
    let cells = format!("<table>{}</table>", "<td>a</td>".repeat(500));
    depth_valid(&format!("{}{cells}", divs(252)));
    let rows = format!("<table>{}</table>", "<tr><td>a</td></tr>".repeat(500));
    depth_valid(&format!("{}{rows}", divs(252)));
    // `col` is not pushed, but its implied column group counts.
    depth_valid(&format!("{}<table><col><col>", divs(254)));
    depth_limit(&format!("{}<table><col>", divs(255)));
    depth_valid(&format!("{}<table><colgroup><col>", divs(254)));
    // Nested tables: each `<table><td>` adds 4 (in a `td` there is no
    // implied part), so 64 of them give D = 256.
    depth_valid(&"<table><td>".repeat(64));
    depth_limit(
        &"<table><td>"
            .repeat(64)
            .replacen("<table>", "<div><table>", 1),
    );
    // A table that a crossing closes keeps all its units: `</tr>` crosses the
    // `td` and makes the `table` dirty.
    let unit = "<table><tr><td>a</tr></table>";
    depth_valid(&units_then_block(unit, 63));
    depth_limit(&units_then_block(unit, 64));
    // Table parts in a foreign region have no implied parts (`table` itself
    // is a breakout name there, M-07).
    depth_valid(&format!("<svg>{}", "<tr><td>".repeat(127)));
    depth_limit(&format!("<svg>{}", "<tr><td>".repeat(128)));
}

/// The place of the depth check among the other checks of a start tag. A
/// fixture never depends on it (a fixture has one error); these tests pin
/// the choice of this candidate (README "How the rules are read").
#[test]
fn m26_depth_is_checked_at_the_push() {
    let full = "<div>".repeat(256);
    // Block start tag: M-15…M-21 and the block count come first, then the
    // push of the block element.
    assert_eq!(
        error_of(format!("{full}<p data-rq-block=\"A\">x</p>").as_bytes()),
        (ErrorCode::InvalidId, "M-15")
    );
    assert_eq!(
        error_of(format!("{full}<p data-rq-block=\"a\"/>").as_bytes()),
        (STRUCTURE, "M-18")
    );
    depth_limit(&format!("{full}<p data-rq-block=\"a\">x</p>"));
    // A foreign region: M-07 comes first.
    let region = format!("<svg>{}", "<g>".repeat(255));
    assert_eq!(
        error_of(format!("{region}<p>").as_bytes()),
        (STRUCTURE, "M-07")
    );
    // Block content: M-22 and M-24 come first.
    let content = format!("<div data-rq-block=\"c\">{}", "<b>".repeat(255));
    assert_eq!(
        error_of(format!("{content}<option>").as_bytes()),
        (STRUCTURE, "M-22")
    );
    assert_eq!(
        error_of(format!("{content}<li>").as_bytes()),
        (STRUCTURE, "M-24")
    );
    depth_limit(&format!("{content}<i>"));
    // HTML context: M-09 comes first. 252 `div`, `table`, the implied row
    // group, `tr` and `td` give D = 256; the second `tr` closes the cell
    // and the row (H-85a, exact pops), so it fits. A `b` that is not the
    // current node of a row fails M-09 before the push.
    depth_valid(&format!("{}<table><tr><td><tr>", "<div>".repeat(252)));
    assert_eq!(
        error_of(format!("{}<table><tr><b><tr>", "<div>".repeat(252)).as_bytes()),
        (STRUCTURE, "M-09")
    );
    // The implied parts of a start tag count before its push.
    depth_limit(&format!("{}<table><td>", "<div>".repeat(253)));
}

/// M-26 item 7: the tags of the `noscript` content C (M-11) are not part of
/// the main pass and do not change D. The `noscript` element itself is
/// pushed and counts (FX-RQ-MARKUP-LIMIT-DEPTH-NOSCRIPT and -ELEMENT).
#[test]
fn m26_depth_counts_noscript_content_and_restores_it() {
    // M-26 item 7 (H-71): C is balanced and counts in the pass of M-11
    // item 8. The `noscript` element is one unit, so 255 elements of C fit.
    let content = |n: usize| format!("{}{}", "<b>".repeat(n), "</b>".repeat(n));
    depth_valid(&format!("<noscript>{}</noscript>", content(255)));
    depth_limit(&format!("<noscript>{}</noscript>", content(256)));
    depth_valid(&format!(
        "{}<noscript>{}</noscript>",
        "<div>".repeat(200),
        content(55)
    ));
    depth_limit(&format!(
        "{}<noscript>{}</noscript>",
        "<div>".repeat(200),
        content(56)
    ));
    // After the end tag `noscript`, D is the value before C: 256 elements
    // before the `noscript` element and a block are fine again.
    depth_limit(&format!(
        "{}<noscript><b></b></noscript>",
        "<div>".repeat(256)
    ));
    depth_valid(&format!(
        "{}<noscript><b></b></noscript></div>",
        "<div>".repeat(254)
    ));
    // The crossing of a region root makes the new current node dirty in the
    // pass of item 8 (M-26 item 5), and D is restored with the dirty mark: the
    // `noscript` end tag gives its unit back.
    let svg = "<noscript><svg><path d=\"M0\"></svg></noscript>";
    depth_valid(&format!(
        "{}{svg}{}",
        "<div>".repeat(253),
        "</div>".repeat(253)
    ));
    depth_valid(&format!(
        "{}{}",
        svg.repeat(300),
        "<p data-rq-block=\"z\">x</p>"
    ));
}

#[test]
fn m27_result_offsets_hashes_and_revisions() {
    let input = b"<p data-rq-block=\"price\">120.00</p>";
    let doc: Document = analyze(input).unwrap();
    assert_eq!(doc.blocks.len(), 1);
    let block = &doc.blocks[0];
    assert_eq!(
        (block.id.as_str(), block.format, block.start, block.end),
        ("price", Format::Html, 25, 31)
    );
    // SHA-256("120.00"), base64url without padding.
    assert_eq!(block.sha256.len(), 43);
    assert_eq!(
        block.sha256,
        analyze(b"<p data-rq-block=\"price\">120.00</p>")
            .unwrap()
            .blocks[0]
            .sha256
    );
    assert!(doc.template_revision.starts_with("t1.") && doc.template_revision.len() == 46);
    assert!(doc.page_revision.starts_with("r1.") && doc.page_revision.len() == 46);

    // The template keeps the tags and drops the content.
    let other = analyze(b"<p data-rq-block=\"price\">99</p>").unwrap();
    assert_eq!(other.template_revision, doc.template_revision);
    assert_ne!(other.page_revision, doc.page_revision);
    assert_ne!(other.blocks[0].sha256, block.sha256);
    // The id and the format are part of the page revision.
    let renamed = analyze(b"<p data-rq-block=\"cost\">120.00</p>").unwrap();
    assert_ne!(renamed.template_revision, doc.template_revision);
    // Blocks are in document order.
    let two = analyze(b"<p data-rq-block=\"b\">1</p><p data-rq-block=\"a\">2</p>").unwrap();
    assert_eq!(two.blocks[0].id, "b");
    assert!(two.blocks[0].end < two.blocks[1].start);
    // A document without blocks is valid.
    assert_eq!(analyze(b"").unwrap().blocks, Vec::<Block>::new());
}

/// The readings of README section "How the rules are read". The rules
/// leave room here; these tests pin the choice of this candidate.
#[test]
fn readings_where_the_rules_leave_room() {
    // M-07.3: a `<` that is a character token is allowed in an
    // integration point; `</>` emits no token.
    assert_valid("<svg><desc>a < b </> c</desc></svg>");
    // M-07: EOF in a foreign text-only element and in a foreign region.
    assert!(analyze(b"<p data-rq-block=\"a\">x</p><svg><title>x").is_ok());
    assert!(analyze(b"<p data-rq-block=\"a\">x</p><math><mi>").is_ok());
    // M-10 and M-07.1 apply in a foreign region without exceptions.
    assert_invalid("<svg><font>x</font></svg>", STRUCTURE, "M-07");
    // M-12: no `>` after `<![CDATA[` (E4), also in a foreign integration
    // point (E4 with E5).
    assert!(analyze(b"<p data-rq-block=\"a\">x</p><![CDATA[ x").is_ok());
    assert!(analyze(b"<p data-rq-block=\"a\">x</p><svg><desc><![CDATA[ x").is_ok());
    // E6: `<?` to EOF is a comment, so it is invalid in an integration point.
    assert!(analyze(b"<p data-rq-block=\"a\">x</p><?php echo 1;").is_ok());
    assert_eq!(
        error_of(b"<p data-rq-block=\"a\">x</p><svg><desc><?x"),
        (STRUCTURE, "M-07")
    );
    // M-11: a noscript without an end tag is checked up to EOF.
    assert!(analyze(b"<noscript><img src=x>").is_ok());
}

#[test]
fn error_codes_have_wire_names() {
    let codes = [
        (ErrorCode::Encoding, "RQP_MARKUP_ENCODING"),
        (ErrorCode::Limit, "RQP_MARKUP_LIMIT"),
        (ErrorCode::InvalidId, "RQP_MARKUP_INVALID_ID"),
        (ErrorCode::ForbiddenElement, "RQP_MARKUP_FORBIDDEN_ELEMENT"),
        (ErrorCode::Duplicate, "RQP_MARKUP_DUPLICATE"),
        (ErrorCode::Nested, "RQP_MARKUP_NESTED"),
        (ErrorCode::Structure, "RQP_MARKUP_STRUCTURE"),
        (ErrorCode::BadJson, "RQP_MARKUP_BAD_JSON"),
        (ErrorCode::Reserved, "RQP_MARKUP_RESERVED"),
    ];
    for (code, name) in codes {
        assert_eq!(code.as_str(), name);
    }
    assert_eq!(Format::Html.as_str(), "html");
    assert_eq!(Format::Json.as_str(), "json");
}

/// `prefix`, then `unit` as many times as the total stays at most `max`
/// bytes, then `suffix`.
fn fill(prefix: &[u8], unit: &[u8], suffix: &[u8], max: usize) -> Vec<u8> {
    let count = (max - prefix.len() - suffix.len()) / unit.len();
    [prefix, &unit.repeat(count), suffix].concat()
}

fn near_limit(input: &[u8]) -> bool {
    input.len() > 5_000_000 && input.len() <= MAX_INPUT_BYTES
}

#[test]
fn large_adversarial_inputs_finish() {
    // Each input is close to the size limit and would take minutes with
    // quadratic work.
    let n = 1_000_000;
    // Many failed end tag searches (`</i>` has no open element). Before the
    // depth limit (M-26) the stack had one million elements; now the
    // 257th `<b>` stops the pass, and the same search runs on a full stack.
    let deep: Vec<u8> = [b"<b>".repeat(n), b"</i>".repeat(n / 2)].concat();
    assert_eq!(error_of(&deep), (ErrorCode::Limit, "M-26"));
    let full = fill(
        &b"<b>".repeat(MAX_OPEN_ELEMENTS),
        b"</i>",
        b"",
        MAX_INPUT_BYTES,
    );
    assert!(near_limit(&full));
    assert!(analyze(&full).is_ok());
    // Many different names (the name table grows with the input). Nested
    // without end tags, the 257th name stops the pass.
    let names: Vec<u8> = (0..300_000u32)
        .flat_map(|i| format!("<x{i:x}>").into_bytes())
        .collect();
    assert_eq!(error_of(&names), (ErrorCode::Limit, "M-26"));
    // Groups of 256 nested names, each closed by its end tags in reverse
    // order (exact pops, M-26 item 4), keep D at the limit and intern more
    // than 250 000 names. (A group closed by the end tag of its first name
    // crosses 255 elements and keeps their units, M-26 item 5.)
    let mut groups: Vec<u8> = Vec::new();
    let mut next = 0u32;
    loop {
        let group: Vec<u8> = (next..next + 256)
            .map(|i| format!("<x{i:x}>"))
            .chain((next..next + 256).rev().map(|i| format!("</x{i:x}>")))
            .flat_map(String::into_bytes)
            .collect();
        if groups.len() + group.len() > MAX_INPUT_BYTES {
            break;
        }
        groups.extend_from_slice(&group);
        next += 256;
    }
    assert!(near_limit(&groups) && next > 250_000, "{next} names");
    assert!(analyze(&groups).is_ok());
    // More than 500 000 names on a full stack: in a foreign region a
    // self-closing element is not pushed (M-05 item 2), but its name is new.
    let mut foreign = [b"<div>".repeat(MAX_OPEN_ELEMENTS - 1), b"<svg>".to_vec()].concat();
    let mut count = 0u32;
    loop {
        let tag = format!("<x{count:x}/>");
        if foreign.len() + tag.len() > MAX_INPUT_BYTES {
            break;
        }
        foreign.extend_from_slice(tag.as_bytes());
        count += 1;
    }
    assert!(near_limit(&foreign) && count > 500_000, "{count} names");
    assert!(analyze(&foreign).is_ok());
    let raw: Vec<u8> = [&b"<title>"[..], &b"</titl</titlex".repeat(n / 3)].concat();
    assert!(analyze(&raw).is_ok());
    let script: Vec<u8> = [&b"<script><!--"[..], &b"<script</scrip".repeat(n / 3)].concat();
    assert!(analyze(&script).is_ok());
    let comments = b"<!--<!-".repeat(n / 2);
    assert!(analyze(&comments).is_ok());
    let attrs: Vec<u8> = [&b"<p "[..], &b"a=b ".repeat(n), b">"].concat();
    assert!(analyze(&attrs).is_ok());
    let json: Vec<u8> = [
        &b"<script type=\"application/json\" data-rq-block=\"j\">["[..],
        &b"[1,{\"a\":\"\\u0041\"}],".repeat(40_000),
        b"0]</script>",
    ]
    .concat();
    assert!(analyze(&json).is_ok());
    // M-24 j and k and the M-13 check of noscript content (T-08).
    // Five blocks, each just under the 1 MiB content limit (M-26).
    let five_blocks = |content: &[u8]| -> Vec<u8> {
        (0..5)
            .flat_map(|i| {
                [
                    format!("<div data-rq-block=\"b{i}\">").into_bytes(),
                    content.to_vec(),
                    b"</div>".to_vec(),
                ]
                .concat()
            })
            .collect()
    };
    // One deep list or table in each block: the depth limit stops it.
    let deep_lists =
        five_blocks(&[b"<ul><li>".repeat(58_000), b"</li></ul>".repeat(58_000)].concat());
    assert_eq!(error_of(&deep_lists), (ErrorCode::Limit, "M-26"));
    let deep_tables = five_blocks(
        &[
            b"<table><tr><td>".repeat(31_000),
            b"</td></tr></table>".repeat(31_000),
        ]
        .concat(),
    );
    assert_eq!(error_of(&deep_tables), (ErrorCode::Limit, "M-26"));
    // Many lists and tables at the maximum depth: the block element and
    // 127 `ul` + `li` pairs (D = 255), or 63 `table` + `tr` + `td` triples
    // with the implied row group of each table (D = 1 + 63 × 4 = 253),
    // repeated to fill each block.
    let unit = [b"<ul><li>".repeat(127), b"</li></ul>".repeat(127)].concat();
    let lists = five_blocks(&fill(b"", &unit, b"", MAX_BLOCK_BYTES));
    assert!(near_limit(&lists), "{} bytes", lists.len());
    assert_eq!(analyze(&lists).unwrap().blocks.len(), 5);
    let unit = [
        b"<table><tr><td>".repeat(63),
        b"</td></tr></table>".repeat(63),
    ]
    .concat();
    let tables = five_blocks(&fill(b"", &unit, b"", MAX_BLOCK_BYTES));
    assert!(near_limit(&tables), "{} bytes", tables.len());
    assert_eq!(analyze(&tables).unwrap().blocks.len(), 5);
    let scripts: Vec<u8> = [
        &b"<noscript>"[..],
        &b"<script type=\"a\"></script>".repeat(190_000),
        b"</noscript>",
    ]
    .concat();
    assert!(analyze(&scripts).is_ok());
}

#[test]
#[allow(clippy::too_many_lines)] // One list of shapes; each has its comment.
fn large_nested_inputs_closed_by_end_tags_finish() {
    // A full token stack that end tags close again, many times. Each input
    // is close to the size limit (M-01) and would take minutes if a pop cost
    // grew with the number of earlier pushes and pops (T-09). The depth
    // limit (M-26) stops each deep shape at its 257th element, so each shape
    // is repeated at the maximum depth instead.
    // A block after the closed elements: its start tag fails M-16 or M-19
    // when a pop leaves a name pointer, a table mark or the foreign root
    // behind.
    let tail = b"<p data-rq-block=\"end\">x</p>";
    let only_tail = |input: &[u8]| {
        assert!(near_limit(input), "{} bytes", input.len());
        assert_eq!(blocks_of(input), vec![html("end", "x")]);
    };
    let limit = |input: &[u8]| assert_eq!(error_of(input), (ErrorCode::Limit, "M-26"));
    let max = MAX_OPEN_ELEMENTS;
    // Each end tag pops one element (M-05 step 3).
    let n = 740_000;
    limit(&[b"<b>".repeat(n), b"</b>".repeat(n), tail.to_vec()].concat());
    let unit = [b"<b>".repeat(max), b"</b>".repeat(max)].concat();
    only_tail(&fill(b"", &unit, tail, MAX_INPUT_BYTES));
    // Two names: the pointer of each name goes back to the previous element
    // with the same name.
    let n = 370_000;
    limit(&[b"<b><i>".repeat(n), b"</i></b>".repeat(n), tail.to_vec()].concat());
    let unit = [b"<b><i>".repeat(max / 2), b"</i></b>".repeat(max / 2)].concat();
    only_tail(&fill(b"", &unit, tail, MAX_INPUT_BYTES));
    // Many short elements on top of a full stack: the pop of `i` restores
    // "no open `i`" without a search through the stack.
    let n = 370_000;
    limit(
        &[
            b"<b>".repeat(n),
            b"<i></i>".repeat(n),
            b"</b>".repeat(n),
            tail.to_vec(),
        ]
        .concat(),
    );
    let suffix = [b"</div>".repeat(max - 1), tail.to_vec()].concat();
    only_tail(&fill(
        &b"<div>".repeat(max - 1),
        b"<b></b>",
        &suffix,
        MAX_INPUT_BYTES,
    ));
    // One end tag pops all elements above its target. A crossing keeps its
    // units of D (M-26 item 5), so all crossings of one document pop at most
    // 256 elements together: the same crossing again is a limit error.
    let n = 1_740_000;
    limit(&[&b"<div>"[..], &b"<b>".repeat(n), b"</div>", tail].concat());
    let unit = [&b"<div>"[..], &b"<b>".repeat(max - 2), b"</div>"].concat();
    limit(&[unit.as_slice(), &unit, tail].concat());
    // After one crossing of 254 elements (D = 255), many short elements
    // pop exactly up to the size limit.
    only_tail(&fill(&unit, b"<i></i>", tail, MAX_INPUT_BYTES));
    // Table marks (M-09, M-19): `template` is pushed and popped with a mark.
    let n = 247_000;
    limit(
        &[
            b"<template>".repeat(n),
            b"</template>".repeat(n),
            tail.to_vec(),
        ]
        .concat(),
    );
    let unit = [b"<template>".repeat(max), b"</template>".repeat(max)].concat();
    only_tail(&fill(b"", &unit, tail, MAX_INPUT_BYTES));
    // The checks of H-35 to H-38 cost O(1) per tag: many forms at the
    // maximum depth (M-10 item 2), many rows of one table (M-09 condition 2),
    // many `col` in one template table (condition 1), and many table parts
    // in one noscript table (M-11 item 6).
    let suffix = [b"</div>".repeat(max - 1), tail.to_vec()].concat();
    only_tail(&fill(
        &b"<div>".repeat(max - 1),
        b"<form></form>",
        &suffix,
        MAX_INPUT_BYTES,
    ));
    only_tail(&fill(
        b"<table>",
        b"<tr><td>x</td></tr>",
        &[&b"</table>"[..], tail].concat(),
        MAX_INPUT_BYTES,
    ));
    only_tail(&fill(
        b"<template><table>",
        b"<col>",
        &[&b"</table></template>"[..], tail].concat(),
        MAX_INPUT_BYTES,
    ));
    only_tail(&fill(
        b"<noscript><table>",
        b"<tr><td></td></tr>",
        &[&b"</table></noscript>"[..], tail].concat(),
        MAX_INPUT_BYTES,
    ));
    // The foreign root (M-07) is reset when `svg` is popped.
    let n = 740_000;
    limit(
        &[
            &b"<svg>"[..],
            &b"<g>".repeat(n),
            &b"</g>".repeat(n),
            b"</svg>",
            tail,
        ]
        .concat(),
    );
    let unit = [b"<g>".repeat(max - 1), b"</g>".repeat(max - 1)].concat();
    only_tail(&fill(
        b"<svg>",
        &unit,
        &[&b"</svg>"[..], tail].concat(),
        MAX_INPUT_BYTES,
    ));
    // Many regions, each at the maximum depth.
    let unit = [
        &b"<svg>"[..],
        &b"<g>".repeat(max - 1),
        &b"</g>".repeat(max - 1),
        b"</svg>",
    ]
    .concat();
    only_tail(&fill(b"", &unit, tail, MAX_INPUT_BYTES));
    // In block content each end tag pops the current node (M-23). Five
    // blocks, each just under the 1 MiB content limit (M-26).
    let five = |content: &[u8]| -> Vec<u8> {
        (0..5)
            .flat_map(|i| {
                [
                    format!("<div data-rq-block=\"b{i}\">").into_bytes(),
                    content.to_vec(),
                    b"</div>".to_vec(),
                ]
                .concat()
            })
            .collect()
    };
    let n = 149_000;
    limit(&five(&[b"<b>".repeat(n), b"</b>".repeat(n)].concat()));
    // The block element and 255 elements in the content.
    let unit = [b"<b>".repeat(max - 1), b"</b>".repeat(max - 1)].concat();
    let content = fill(b"", &unit, b"", MAX_BLOCK_BYTES);
    let nested = five(&content);
    assert!(near_limit(&nested), "{} bytes", nested.len());
    let found = blocks_of(&nested);
    assert_eq!(found.len(), 5);
    for (i, (id, format, text)) in found.iter().enumerate() {
        assert_eq!(
            (id.as_str(), *format),
            (format!("b{i}").as_str(), Format::Html)
        );
        assert_eq!(text.as_bytes(), content.as_slice());
    }
}

// ---- WP-17 S3: H-71, H-73, H-74, H-75 ----------------------------------------

/// M-05 item 4 (H-73, H-78): `svg` and `math` in the content open a region;
/// M-07 and M-23 apply there, and M-22 does not.
#[test]
fn m22_5_foreign_regions_in_html_block_content() {
    let icon = "<svg viewBox=\"0 0 16 16\"><title>Close</title><path d=\"M0\"/></svg> Close";
    assert_eq!(
        blocks(&format!("<span data-rq-block=\"i\">{icon}</span>")),
        [html("i", icon)]
    );
    let math = "<math><mi>x</mi><mo>+</mo><mn>1</mn></math>";
    assert_eq!(
        blocks(&format!("<p data-rq-block=\"f\">{math}</p>")),
        [html("f", math)]
    );
    // A self-closing region root opens no region (M-22 item 3 exempts it).
    assert_eq!(
        blocks("<p data-rq-block=\"s\"><svg/><math/>x</p>"),
        [html("s", "<svg/><math/>x")]
    );
    // Since H-78 every element that M-07 allows is allowed: there is no
    // list of names for a region of the content.
    for name in [
        "a",
        "animate",
        "set",
        "foreignObject",
        "image",
        "feImage",
        "textPath",
        "unknown-name",
    ] {
        let content = format!("<svg><{name}></{name}></svg>");
        assert_eq!(
            blocks(&format!("<p data-rq-block=\"b\">{content}</p>")),
            [html("b", &content)],
            "{name}"
        );
    }
    for content in [
        "<math><maction></maction></math>",
        "<svg><set attributeName=\"x\"/></svg>",
        "<svg><script>a &lt; b</script><style>p{}</style></svg>",
        "<svg><annotation-xml>t</annotation-xml></svg>",
    ] {
        assert_eq!(
            blocks(&format!("<p data-rq-block=\"b\">{content}</p>")),
            [html("b", content)],
            "{content}"
        );
    }
    // Items 2 to 4 are for the HTML context only: `title` is forbidden in
    // HTML content, and allowed in the region.
    assert_invalid(
        "<p data-rq-block=\"b\"><title>x</title></p>",
        STRUCTURE,
        "M-22",
    );
    // M-07 applies in the region of the content.
    assert_invalid(
        "<p data-rq-block=\"b\"><svg><desc><g></g></desc></svg></p>",
        STRUCTURE,
        "M-07",
    );
    assert_invalid(
        "<p data-rq-block=\"b\"><svg><p>x</p></svg></p>",
        STRUCTURE,
        "M-07",
    );
    assert_invalid(
        "<p data-rq-block=\"b\"><svg><title>a<b></b></title></svg></p>",
        STRUCTURE,
        "M-07",
    );
    // M-17 comes first for a block start tag in the region.
    assert_eq!(
        error("<p data-rq-block=\"b\"><svg><g data-rq-block=\"c\"></g></svg></p>"),
        (ErrorCode::Nested, "M-17")
    );
    // M-24 item k applies to the region root start tag in table context.
    assert_invalid(
        "<td><p data-rq-block=\"b\"><table><svg></svg></table></p></td>",
        STRUCTURE,
        "M-24",
    );
}

/// M-23 (H-73, H-75): balance in a region of the content.
#[test]
fn m23_balance_in_a_foreign_region_of_the_content() {
    // The end tag of the region root closes the elements of its region.
    let content = "<svg><g><path d=\"M0\"></svg>";
    assert_eq!(
        blocks(&format!("<p data-rq-block=\"b\">{content}</p>")),
        [html("b", content)]
    );
    // Every other end tag matches the current node.
    assert_invalid(
        "<p data-rq-block=\"b\"><svg><g><path d=\"M0\"></g></svg></p>",
        STRUCTURE,
        "M-23",
    );
    assert_invalid(
        "<p data-rq-block=\"b\"><svg></g></svg></p>",
        STRUCTURE,
        "M-23",
    );
    // An inner `svg` is not the region root.
    assert_invalid(
        "<p data-rq-block=\"b\"><svg><svg><path d=\"M0\"></svg></svg></p>",
        STRUCTURE,
        "M-23",
    );
    assert_valid("<p data-rq-block=\"b\"><svg><svg></svg></svg></p>");
    // The block end tag can not close an open region.
    assert_invalid("<p data-rq-block=\"b\"><svg><g></p>", STRUCTURE, "M-23");
    // M-22 item 4 does not apply in the region: `</br>` is a stray end tag.
    assert_invalid(
        "<p data-rq-block=\"b\"><svg></br></svg></p>",
        STRUCTURE,
        "M-23",
    );
    // M-07 item 6 with M-20: the input ends in a region of the content.
    assert_eq!(
        error_of(b"<p data-rq-block=\"b\"><svg><path d=\"M0\">"),
        (STRUCTURE, "M-20")
    );
}

/// M-26 in a region of the content (H-73): pushes count.
#[test]
fn m26_pushes_in_a_foreign_region_of_the_content_count() {
    let block = |n: usize| {
        format!(
            "<p data-rq-block=\"b\"><svg>{}{}</svg></p>",
            "<g>".repeat(n),
            "</g>".repeat(n)
        )
    };
    // The block element, the region root and 254 elements.
    depth_valid(&block(254));
    depth_limit(&block(255));
}

/// M-06 item 4 and M-26 (H-75): the crossing sets no mark but D stays.
#[test]
fn m26_region_root_crossing_keeps_d() {
    let icons = |n: usize| {
        format!(
            "{}<p data-rq-block=\"z\">x</p>",
            "<span><svg><path d=\"M0\"></svg></span>".repeat(n)
        )
    };
    depth_valid(&icons(85));
    depth_limit(&icons(86));
}

/// M-11 item 9 (H-74) and item 5 (H-75): a region in `noscript` content.
#[test]
fn m11_9_foreign_region_in_noscript_content() {
    for body in [
        "<noscript><img src=\"p.gif\"><svg width=\"1\" height=\"1\"><path d=\"M0\"></path></svg></noscript>",
        "<noscript><a href=\"/\"><svg><path d=\"M0\"></svg>Home</a></noscript>",
        "<noscript><svg><g><path d=\"M0\"></svg></noscript>",
        "<noscript><svg><svg></svg></svg></noscript>",
        "<noscript><svg/></noscript>",
        "<noscript><svg><style><![CDATA[.a{fill:red}]]></style></svg></noscript>",
        "<noscript><svg><desc>a < b</desc></svg></noscript>",
    ] {
        assert_valid(body);
    }
    // E3: no end tag.
    assert!(analyze(b"<noscript><svg><path d=\"M0\"></svg>").is_ok());
    for (body, rule) in [
        // Item 5: only the end tag of the region root can cross.
        (
            "<noscript><svg><g><path d=\"M0\"></g></svg></noscript>",
            "M-11",
        ),
        (
            "<noscript><svg><svg><path d=\"M0\"></svg></svg></noscript>",
            "M-11",
        ),
        ("<noscript><svg><g></noscript>", "M-11"),
        ("<noscript><svg></g></svg></noscript>", "M-11"),
        // Items 2, 3, 6, 7 in the region.
        (
            "<noscript><svg><g data-rq-block=\"a\"></g></svg></noscript>",
            "M-11",
        ),
        (
            "<noscript><svg><template></template></svg></noscript>",
            "M-11",
        ),
        ("<noscript><svg><select></select></svg></noscript>", "M-11"),
        ("<noscript><svg><tr></tr></svg></noscript>", "M-11"),
        ("<noscript><svg><body></body></svg></noscript>", "M-11"),
        ("<noscript><svg><html></html></svg></noscript>", "M-11"),
        // Item 3: `math` stays forbidden.
        ("<noscript><math><mi>x</mi></math></noscript>", "M-11"),
        ("<noscript><svg><math></math></svg></noscript>", "M-11"),
        // Item 8: the rules of the main pass in the region.
        ("<noscript><svg><p>x</p></svg></noscript>", "M-07"),
        (
            "<noscript><svg><title><g></g></title></svg></noscript>",
            "M-07",
        ),
        (
            "<noscript><svg><textarea><b></b></textarea></svg></noscript>",
            "M-07",
        ),
        (
            "<noscript><svg><style><![CDATA[a>b{}]]></style></svg></noscript>",
            "M-12",
        ),
    ] {
        assert_invalid(body, STRUCTURE, rule);
    }
    assert_invalid(
        "<noscript><svg><script type=\"rivqen-manifest\"></script></svg></noscript>",
        ErrorCode::Reserved,
        "M-13",
    );
}

/// M-11 item 7 (H-71): `html`, `body` and `head` tags anywhere in C.
#[test]
fn m11_7_document_tags_in_noscript_content() {
    for content in [
        "<html>",
        "<body>",
        "<head>",
        "</html>",
        "</body>",
        "</head>",
        "<BODY class=\"x\">",
        "<div></body></div>",
    ] {
        assert_invalid(
            &format!("<noscript>{content}</noscript>"),
            STRUCTURE,
            "M-11",
        );
    }
    assert_valid("<noscript><header></header><bodyx></bodyx></noscript>");
}

/// M-11 item 8 (H-71, DL-044): C goes through the rules of the main pass.
#[test]
fn m11_8_main_pass_rules_in_noscript_content() {
    // M-10 item 2: a form is open around the `noscript`.
    assert_invalid(
        "<form><noscript><form>x</form></noscript></form>",
        STRUCTURE,
        "M-10",
    );
    assert_invalid(
        "<form><div><noscript><form></form></noscript></div></form>",
        STRUCTURE,
        "M-10",
    );
    assert_valid("<form></form><noscript><form></form></noscript><form></form>");
    // M-09 condition 2: F is the outer table.
    assert_invalid(
        "<table><noscript><table></table></noscript></table>",
        STRUCTURE,
        "M-09",
    );
    // M-09 condition 2 inside C (the security review repro T-16).
    assert_invalid(
        "<object><a href=\"1\"><noscript><table><marquee><table></table></marquee></table></noscript>x</a></object>",
        STRUCTURE,
        "M-09",
    );
    assert_invalid("<noscript><![CDATA[a>b]]></noscript>", STRUCTURE, "M-12");
    // After the end tag, the main pass has its own state: the `form` of C is
    // gone, and the dirty mark of the region crossing is restored.
    assert_valid("<noscript><form></form></noscript><form></form>");
    assert_valid("<b><noscript><svg><path d=\"M0\"></svg></noscript></b>");
}

/// The state of the main pass after the end tag `noscript` (DL-044 c): the
/// dirty mark that the region crossing sets in the pass of item 8 is gone, so
/// the exact pops give their units back and D is 0 after each repeat.
#[test]
fn m11_8_state_after_noscript_is_the_state_before_it() {
    let icon = "<noscript><svg><path d=\"M0\"></svg></noscript>";
    depth_valid(&format!(
        "{}<p data-rq-block=\"z\">x</p>",
        format!("<div>{icon}</div>").repeat(300)
    ));
}

// ---- M-09 implied closure (H-85a) ------------------------------------------

#[test]
fn m09_closure_closes_cell_row_and_group() {
    for body in [
        "<td>a<td>b</td>",
        "<td>a<th>b</th>",
        "<table><tr><td>a<tr><td>b</td></tr></table>",
        "<table><thead><tr><th>h<tbody><tr><td>x</td></tr></table>",
        "<table><thead><tr><th>h<tfoot><tr><td>x</td></tr></table>",
        "<table><caption>c<tr><td>x</td></tr></table>",
        "<table><caption>c<colgroup><col></colgroup></table>",
        "<table><colgroup><col><tr><td>x</td></tr></table>",
        "<table><colgroup><col><col><tbody><tr><td>x</td></tr></table>",
        "<table><tr><td><table><tr><td>a<td>b</td></tr></table>x</td></tr></table>",
        // The closure crosses a special element as `</td>` does.
        "<table><tr><td><div>a<td>b</td></tr></table>",
    ] {
        assert_valid(body);
    }
}

#[test]
fn m09_closure_never_closes_a_table() {
    // The inner cells close; the outer cell, row and table stay open.
    assert_valid("<table><tr><td><table><tr><td>a<td>b</td></tr></table>c</td></tr></table>");
    // A `table` start tag is not a table part: no closure, condition 2 only.
    assert_invalid("<table><b>x<table>", STRUCTURE, "M-09");
}

#[test]
fn m09_closure_needs_the_row_group_or_row_as_current_node() {
    // Step 2: the `b` is above the row.
    assert_invalid("<table><tr><td>a</td><b>x<tr>", STRUCTURE, "M-09");
    assert_invalid("<table><tbody><b>x<tbody>", STRUCTURE, "M-09");
    assert_invalid("<table><colgroup><b>x<tr>", STRUCTURE, "M-09");
}

#[test]
fn m09_closure_applies_m06_item_1_to_the_crossing() {
    assert_invalid("<td><a href=\"#\">a<td>", STRUCTURE, "M-06");
    assert_invalid("<td><object>a<td>", STRUCTURE, "M-06");
}

#[test]
fn m09_closure_passes_the_special_crossed_mark() {
    // DL-050 (2): the cell crossed a `div`, so the row has the mark, and the
    // `</a>` of the formatting element cannot pop it.
    assert_invalid(
        "<a href=\"#\"><table><tr><td><div>x<td>y</td></tr></table></a>",
        STRUCTURE,
        "M-06",
    );
    assert_valid("<a href=\"#\"><table><tr><td>x<td>y</td></tr></table></a>");
}

#[test]
fn m09_closure_runs_before_condition_1() {
    // DL-050 (1): after the closure the `template` is the nearest element.
    assert_invalid("<template><tr><td>a<col>", STRUCTURE, "M-09");
    // The same shape with a `table` first has an F that allows `col`.
    assert_valid("<table><tr><td>a<col></table>");
}

#[test]
fn m09_closure_is_off_in_block_content_and_in_noscript_content() {
    assert_invalid(
        "<div data-rq-block=\"t\"><table><tr><td>a<td>b</td></tr></table></div>",
        STRUCTURE,
        "M-09",
    );
    assert_invalid(
        "<noscript><table><tr><td>a<td>b</table></noscript>",
        STRUCTURE,
        "M-09",
    );
    assert_invalid(
        "<table><tr><td><noscript><td>x</noscript>",
        STRUCTURE,
        "M-11",
    );
}

#[test]
fn m09_closure_gives_units_of_d_back_for_exact_pops() {
    let cells = "<tr><td>a<td>b".repeat(300);
    depth_valid(&format!("<table>{cells}</table>"));
}

#[test]
fn m09_closure_crossing_counts_toward_the_limit() {
    depth_valid(&format!("<table><tr>{}", "<td><span>a".repeat(126)));
    depth_limit(&format!("<table><tr>{}", "<td><span>a".repeat(127)));
    // 251 `div`, a table, row group, row and cell, then a span that the
    // closure crosses: the new `tr` makes D = 257.
    depth_limit(&format!("{}<table><tr><td><span><tr>", "<div>".repeat(251)));
}
