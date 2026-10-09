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
    Document, ErrorCode, Format, MAX_BLOCK_BYTES, MAX_INPUT_BYTES, MarkupError, analyze,
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
    assert!(analyze(&input).unwrap().blocks.is_empty());
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
    assert!(blocks("<plaintext></plaintext><p data-rq-block=\"f\">x</p>").is_empty());
    // A raw text element that is never closed hides the rest.
    assert!(blocks_of(b"<style><p data-rq-block=\"f\">x</p>").is_empty());
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
    assert_invalid("<svg><desc><![CDATA[x]]></desc></svg>", STRUCTURE, "M-07");
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

#[test]
fn m09_table_parts() {
    assert_valid(
        "<table><caption>c</caption><colgroup><col></colgroup><col><thead><tr><th>h</th></tr></thead><tbody><tr><td>a</td><td>b</td></tr></tbody><tfoot></tfoot><tr><td>x</td></tr></table>",
    );
    assert_invalid("<table><tr><td>a<td>b</td></tr></table>", STRUCTURE, "M-09");
    assert_invalid("<table><tr><td><col></td></tr></table>", STRUCTURE, "M-09");
    assert_invalid(
        "<table><tr><caption>c</caption></tr></table>",
        STRUCTURE,
        "M-09",
    );
    assert_invalid(
        "<table><tbody><tbody></tbody></tbody></table>",
        STRUCTURE,
        "M-09",
    );
    assert_invalid("<table><tr><tr></tr></tr></table>", STRUCTURE, "M-09");
    assert_invalid(
        "<table><colgroup><tr></tr></colgroup></table>",
        STRUCTURE,
        "M-09",
    );
    assert_invalid(
        "<table><caption><td></td></caption></table>",
        STRUCTURE,
        "M-09",
    );
    assert_invalid("<table><thead><col></thead></table>", STRUCTURE, "M-09");
    // No table-family element open: allowed by M-09.
    assert_valid("<td>a</td><tr></tr><col>");
    // The search stops at template.
    assert_valid("<table><tr><td><template><td>a</td></template></td></tr></table>");
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
fn m10_frameset() {
    assert_invalid(
        "<frameset><frame src=\"a.html\"></frameset>",
        STRUCTURE,
        "M-10",
    );
    assert_invalid("<svg><frameset></frameset></svg>", STRUCTURE, "M-10");
    assert_valid("<!-- <frameset> --><script>\"<frameset>\"</script>");
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
    // At EOF the content is checked too.
    assert!(analyze(b"<noscript><img src=x>").is_ok());
    assert_eq!(
        error_of(b"<noscript><p data-rq-block=\"a\">"),
        (STRUCTURE, "M-11")
    );
    // In a foreign region noscript is not RAWTEXT; M-11 does not apply, M-07.4 does.
    assert_valid("<svg><noscript>x &lt;</noscript></svg>");
    assert_invalid("<svg><noscript><g></g></noscript></svg>", STRUCTURE, "M-07");
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
        "&",
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
        "<script type=\"application/ld+json\" data-rq-block=\"ld\">{}</script>",
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
        "base",
        "basefont",
        "bgsound",
        "body",
        "embed",
        "frame",
        "head",
        "html",
        "iframe",
        "link",
        "math",
        "meta",
        "noembed",
        "noframes",
        "noscript",
        "object",
        "optgroup",
        "option",
        "plaintext",
        "script",
        "select",
        "style",
        "svg",
        "template",
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
    assert!(analyze(b"").unwrap().blocks.is_empty());
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
    // M-12: no `>` after `<![CDATA[`.
    assert!(analyze(b"<p data-rq-block=\"a\">x</p><![CDATA[ x").is_ok());
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

#[test]
fn large_adversarial_inputs_finish() {
    // Each input is close to the size limit and would take minutes with
    // quadratic work.
    let n = 1_000_000;
    let deep: Vec<u8> = [b"<b>".repeat(n), b"</i>".repeat(n / 2)].concat();
    assert!(analyze(&deep).is_ok());
    let names: Vec<u8> = (0..300_000u32)
        .flat_map(|i| format!("<x{i}>").into_bytes())
        .collect();
    assert!(analyze(&names).is_ok());
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
}
