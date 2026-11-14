// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Port of `tools/rqp-markup/rust/tests/api.rs` (commit `92c708e1`; DL-161).
//!
//! Tests of the public token API (`rivqen_template::rqp::scan`, DL-085): coverage of
//! the input, raw attributes with duplicates, the M-04 state changes, the
//! foreign region of M-05, and agreement with `analyze`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use proptest::prelude::*;
use rivqen_template::limits::TemplateLimits;
use rivqen_template::rqp::scan::{Span, Tag, Token, scan};

mod rqp_common;

use rqp_common::{MAX_INPUT_BYTES, analyze};

fn tokens(input: &[u8]) -> Vec<Token<'_>> {
    scan(input, &TemplateLimits::default())
        .map(Result::unwrap)
        .collect()
}

/// Short descriptions: `gap "x"`, `<name>`, `</name>`, `comment`, ...
fn describe(input: &str) -> Vec<String> {
    let text = |span: Span| &input[span.start..span.end];
    tokens(input.as_bytes())
        .iter()
        .map(|token| match token {
            Token::Gap(span) => format!("gap {:?}", text(*span)),
            Token::StartTag(tag) => {
                let mut d = format!("<{}>", String::from_utf8_lossy(tag.name()));
                if tag.self_closing() {
                    d.push_str(" /");
                }
                if tag.in_foreign_region() {
                    d.push_str(" foreign");
                }
                d
            }
            Token::EndTag(tag) => format!("</{}>", String::from_utf8_lossy(tag.name())),
            Token::Comment(span) => format!("comment {:?}", text(*span)),
            Token::Doctype(span) => format!("doctype {:?}", text(*span)),
            Token::Cdata { span, well_formed } => format!("cdata {:?} {well_formed}", text(*span)),
            other => panic!("unknown token {other:?}"),
        })
        .collect()
}

/// The spans of the tokens and gaps cover `input` once, in order, and no
/// gap is empty.
fn assert_covers(input: &[u8]) {
    let mut at = 0;
    let mut gap_before = false;
    for token in scan(input, &TemplateLimits::default()) {
        let token = token.unwrap();
        let span = token.span();
        assert_eq!(span.start, at, "a hole or an overlap at {at}");
        assert!(span.end > span.start, "an empty token at {at}");
        let gap = matches!(token, Token::Gap(_));
        assert!(!(gap && gap_before), "two gaps in a row at {at}");
        gap_before = gap;
        at = span.end;
    }
    assert_eq!(at, input.len());
}

fn start_tags(input: &[u8]) -> Vec<Tag<'_>> {
    tokens(input)
        .into_iter()
        .filter_map(|t| match t {
            Token::StartTag(tag) => Some(tag),
            _ => None,
        })
        .collect()
}

/// (name, value, has value) of each attribute of the first start tag.
fn attributes(input: &str) -> Vec<(String, String, bool)> {
    let tags = start_tags(input.as_bytes());
    tags[0]
        .attributes()
        .map(|a| {
            let a = a.unwrap();
            if let Some(span) = a.value_span() {
                assert_eq!(&input.as_bytes()[span.start..span.end], a.value());
            } else {
                assert_eq!(a.value(), b"");
            }
            let span = a.name_span();
            assert_eq!(&input.as_bytes()[span.start..span.end], a.name());
            (
                String::from_utf8_lossy(a.name()).into_owned(),
                String::from_utf8_lossy(a.value()).into_owned(),
                a.value_span().is_some(),
            )
        })
        .collect()
}

fn attr(name: &str, value: &str, has_value: bool) -> (String, String, bool) {
    (name.to_owned(), value.to_owned(), has_value)
}

#[test]
fn api_tokens_cover_document() {
    for input in [
        &b""[..],
        b"x",
        b"<p>",
        b"\xEF\xBB\xBF<!DOCTYPE html>\n<html><head>",
        b"a<b>c</b>d",
        b"<p></><p>",
        b"<div a=\"x",
        b"<!-- open",
        b"<script>x</script><style>",
        b"<plaintext></plaintext><p>",
        b"<svg><![CDATA[a]]></svg><![CDATA[x>",
        b"<<<>>></",
    ] {
        assert_covers(input);
    }
    assert_eq!(
        describe("a<b>c</b>d"),
        ["gap \"a\"", "<b>", "gap \"c\"", "</b>", "gap \"d\""]
    );
    assert_eq!(tokens(b"").len(), 0);
}

#[test]
fn api_gaps_hold_text_and_the_bytes_that_give_no_token() {
    // `</>` gives no token: its bytes are in a gap with the text around it.
    assert_eq!(describe(" </> <p>"), ["gap \" </> \"", "<p>"]);
    // A `<` that starts no tag is text.
    assert_eq!(describe("a < b<p>"), ["gap \"a < b\"", "<p>"]);
    // A tag that the end of the input cuts gives no token.
    assert_eq!(describe("<p><head"), ["<p>", "gap \"<head\""]);
    assert_eq!(describe("<p a=\"x>"), ["gap \"<p a=\\\"x>\""]);
    // A byte order mark at offset 0 is in the first gap.
    assert_eq!(describe("\u{feff}<head>"), ["gap \"\\u{feff}\"", "<head>"]);
}

#[test]
fn api_comments_doctype_and_cdata() {
    assert_eq!(
        describe("<!DOCTYPE html><!-- c --><?x?><!x></1><![CDATA[a]]><![cdata[b]]>"),
        [
            "doctype \"<!DOCTYPE html>\"",
            "comment \"<!-- c -->\"",
            "comment \"<?x?>\"",
            "comment \"<!x>\"",
            "comment \"</1>\"",
            "cdata \"<![CDATA[a]]>\" true",
            "comment \"<![cdata[b]]>\"",
        ]
    );
    assert_eq!(
        describe("<![CDATA[a>b]]>"),
        ["cdata \"<![CDATA[a>\" false", "gap \"b]]>\""]
    );
    assert_eq!(describe("<!-- x"), ["comment \"<!-- x\""]);
}

#[test]
fn api_tags_have_name_span_and_flag() {
    let input = b"<DiV\tclass=x/><br/></P a=b>";
    let all = tokens(input);
    let Token::StartTag(div) = &all[0] else {
        panic!("a start tag");
    };
    assert_eq!(div.span(), Span { start: 0, end: 14 });
    assert_eq!(div.name(), b"DiV");
    assert_eq!(div.name_span(), Span { start: 1, end: 4 });
    assert!(!div.self_closing(), "the slash is part of the value");
    let Token::StartTag(br) = &all[1] else {
        panic!("a start tag");
    };
    assert!(br.self_closing());
    let Token::EndTag(p) = &all[2] else {
        panic!("an end tag");
    };
    assert_eq!(p.name(), b"P");
    assert_eq!(p.span(), Span { start: 19, end: 27 });
}

#[test]
fn api_attributes_keep_duplicates() {
    // M-03 drops the second `onclick` and `TYPE`; the API keeps them, in
    // source order, with their raw names.
    assert_eq!(
        attributes("<a href=\"/x\" onclick=1 ONCLICK='2' type=t TYPE=u href>"),
        [
            attr("href", "/x", true),
            attr("onclick", "1", true),
            attr("ONCLICK", "2", true),
            attr("type", "t", true),
            attr("TYPE", "u", true),
            attr("href", "", false),
        ]
    );
    // `analyze` uses the first one: the block id is `a`, not `b`.
    let doc = analyze(b"<div data-rq-block=\"a\" data-rq-block=\"b\">x</div>").unwrap();
    assert_eq!(doc.blocks[0].id, "a");
    assert_eq!(
        attributes("<div data-rq-block=\"a\" data-rq-block=\"b\">"),
        [
            attr("data-rq-block", "a", true),
            attr("data-rq-block", "b", true)
        ]
    );
}

#[test]
fn api_attribute_values_are_raw() {
    assert_eq!(
        attributes("<a href=\"java&#x09;script:x\" title='a\"b' u=a'b\" e= \"v\" >"),
        [
            attr("href", "java&#x09;script:x", true),
            attr("title", "a\"b", true),
            attr("u", "a'b\"", true),
            attr("e", "v", true),
        ]
    );
    // A quoted `>` does not end the tag.
    assert_eq!(
        attributes("<p t=\"a>b\" x>"),
        [attr("t", "a>b", true), attr("x", "", false)]
    );
    // No value, an empty value, and `=` before the end of the tag.
    assert_eq!(
        attributes("<p a b=\"\" c=>"),
        [
            attr("a", "", false),
            attr("b", "", true),
            attr("c", "", false)
        ]
    );
    assert_eq!(attributes("<p a=/>"), [attr("a", "/", true)]);
    assert_eq!(attributes("<p a/>"), [attr("a", "", false)]);
    assert_eq!(
        attributes("<p a = 1 / b>"),
        [attr("a", "1", true), attr("b", "", false)]
    );
    // No white space after a quoted value starts a new attribute.
    assert_eq!(
        attributes("<p a=\"1\"b='2'c>"),
        [
            attr("a", "1", true),
            attr("b", "2", true),
            attr("c", "", false)
        ]
    );
    // `=` before a name starts a name with `=`; a quote is a name byte.
    assert_eq!(
        attributes("<p =x \"y=1>"),
        [attr("=x", "", false), attr("\"y", "1", true)]
    );
    assert_eq!(attributes("<p>"), []);
    assert_eq!(attributes("<p/>"), []);
}

#[test]
fn api_attributes_can_be_read_twice() {
    let input = b"<img src=a srcset='b 1x' alt>";
    let tags = start_tags(input);
    let first: Vec<_> = tags[0].attributes().map(Result::unwrap).collect();
    let again: Vec<_> = tags[0].attributes().map(Result::unwrap).collect();
    assert_eq!(first.len(), 3);
    assert_eq!(first, again);
    let mut iter = tags[0].attributes();
    for _ in 0..3 {
        assert!(iter.next().is_some());
    }
    assert!(iter.next().is_none());
    assert!(iter.next().is_none(), "fused");
}

#[test]
fn api_end_tag_attributes_are_read_too() {
    let all = tokens(b"</p a=1 a=2>");
    let Token::EndTag(tag) = &all[0] else {
        panic!("an end tag");
    };
    let values: Vec<_> = tag.attributes().map(|a| a.unwrap().value()).collect();
    assert_eq!(values, [b"1", b"2"]);
}

#[test]
fn api_m04_state_changes() {
    // RCDATA, RAWTEXT and script data end at the appropriate end tag.
    assert_eq!(
        describe("<title><p></title><textarea><b></TEXTAREA ><style><i></style>"),
        [
            "<title>",
            "gap \"<p>\"",
            "</title>",
            "<textarea>",
            "gap \"<b>\"",
            "</TEXTAREA>",
            "<style>",
            "gap \"<i>\"",
            "</style>",
        ]
    );
    assert_eq!(
        describe("<script><!--<script></script>--></script><p>"),
        [
            "<script>",
            "gap \"<!--<script></script>-->\"",
            "</script>",
            "<p>"
        ]
    );
    for name in ["iframe", "noembed", "noframes", "noscript", "xmp"] {
        assert_eq!(
            describe(&format!("<{name}><b></{name}>")),
            [
                format!("<{name}>"),
                "gap \"<b>\"".to_owned(),
                format!("</{name}>")
            ]
        );
    }
    // Also with the self-closing flag (M-04).
    assert_eq!(
        describe("<script/><p></script>"),
        ["<script> /", "gap \"<p>\"", "</script>"]
    );
    // PLAINTEXT never ends.
    assert_eq!(
        describe("<plaintext></plaintext><p>"),
        ["<plaintext>", "gap \"</plaintext><p>\""]
    );
    // In a foreign region no start tag changes the state.
    assert_eq!(
        describe("<svg><style><b></b></style></svg><style><b></style>"),
        [
            "<svg>",
            "<style> foreign",
            "<b> foreign",
            "</b>",
            "</style>",
            "</svg>",
            "<style>",
            "gap \"<b>\"",
            "</style>",
        ]
    );
}

#[test]
fn api_foreign_region() {
    // The region root is in HTML context; the region ends when it is popped.
    assert_eq!(
        describe("<svg><g><a></g></svg><p>"),
        [
            "<svg>",
            "<g> foreign",
            "<a> foreign",
            "</g>",
            "</svg>",
            "<p>"
        ]
    );
    // A self-closing `svg` or `math` in HTML context starts no region.
    assert_eq!(describe("<svg/><math/><p>"), ["<svg> /", "<math> /", "<p>"]);
    // An `svg` or `math` in a region is a foreign element, not a new root.
    assert_eq!(
        describe("<math><svg></svg><mi></math><p>"),
        [
            "<math>",
            "<svg> foreign",
            "</svg>",
            "<mi> foreign",
            "</math>",
            "<p>"
        ]
    );
    // A self-closing tag in a region pushes nothing: `</g>` pops the outer `g`.
    assert_eq!(
        describe("<svg><g><g/></g><path></svg><b>"),
        [
            "<svg>",
            "<g> foreign",
            "<g> / foreign",
            "</g>",
            "<path> foreign",
            "</svg>",
            "<b>"
        ]
    );
    // Upper case names (`<SVG>`, `</Svg>`) are the same names.
    assert_eq!(
        describe("<SVG><G></Svg><style><g></style>"),
        [
            "<SVG>",
            "<G> foreign",
            "</Svg>",
            "<style>",
            "gap \"<g>\"",
            "</style>"
        ]
    );
    // A void element in HTML context is not pushed: `</br>` does not end the
    // region.
    assert_eq!(
        describe("<br><svg></br><g>"),
        ["<br>", "<svg>", "</br>", "<g> foreign"]
    );
}

/// For block content, scan the whole document and keep the tokens in the
/// content span: a `title` block has no start tags, a `json` block neither.
#[test]
fn api_block_content_is_read_in_the_document() {
    let input = b"<title data-rq-block=\"t\"><script>x</title>\
        <div data-rq-block=\"d\"><svg><a href=x></a></svg><b onclick=y></b></div>\
        <script type=\"application/json\" data-rq-block=\"j\">[\"<p>\"]</script>";
    let doc = analyze(input).unwrap();
    let all = start_tags(input);
    let in_block = |id: &str| -> Vec<(String, bool)> {
        let block = doc.blocks.iter().find(|b| b.id == id).unwrap();
        all.iter()
            .filter(|t| t.span().start >= block.start && t.span().end <= block.end)
            .map(|t| {
                (
                    String::from_utf8_lossy(t.name()).into_owned(),
                    t.in_foreign_region(),
                )
            })
            .collect()
    };
    assert_eq!(in_block("t"), []);
    assert_eq!(in_block("j"), []);
    assert_eq!(
        in_block("d"),
        [
            ("svg".to_owned(), false),
            ("a".to_owned(), true),
            ("b".to_owned(), false)
        ]
    );
    // The content bytes alone, from the data state, read the RCDATA of the
    // `title` block as a tag: that is why the document is scanned.
    let title = &doc.blocks[0];
    assert_eq!(start_tags(&input[title.start..title.end]).len(), 1);
}

#[test]
fn api_head_walk_tokens() {
    // The tokens that the walk "Document head" reads (manifest-patch §1.7).
    assert_eq!(
        describe("\u{feff}<!DOCTYPE html>\n<!-- c --></p><html lang=en>\n<head lang=en><title>"),
        [
            "gap \"\\u{feff}\"",
            "doctype \"<!DOCTYPE html>\"",
            "gap \"\\n\"",
            "comment \"<!-- c -->\"",
            "</p>",
            "<html>",
            "gap \"\\n\"",
            "<head>",
            "<title>",
        ]
    );
    assert_eq!(
        describe("<html><head><!-- <head> -->"),
        ["<html>", "<head>", "comment \"<!-- <head> -->\""]
    );
}

#[test]
fn api_scan_is_linear_on_large_inputs() {
    // One tag of about 5 MiB with about 2.6 million attributes.
    let mut input = b"<p".to_vec();
    while input.len() < MAX_INPUT_BYTES - 2 {
        input.extend_from_slice(b" a");
    }
    input.push(b'>');
    let tags = start_tags(&input);
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].attributes().count(), (MAX_INPUT_BYTES - 3) / 2);
    // About 1.5 million elements, opened and closed, and a foreign region at
    // the end.
    let mut wide = b"<b></b>".repeat(740_000);
    wide.extend_from_slice(b"<svg><style><b></style>");
    let tags = start_tags(&wide);
    let last = tags.last().unwrap();
    assert_eq!(last.name(), b"b");
    assert!(last.in_foreign_region());
    assert_covers(&wide);
    // About 1.7 million open elements: the token stack limit (D + 2 = 258,
    // WP-06 S1 finding S1-2) stops the scan at the 259th start tag, with
    // one error item as the last item.
    let deep = b"<b>".repeat(1_700_000);
    let items: Vec<_> = scan(&deep, &TemplateLimits::default()).collect();
    assert_eq!(items.len(), 259);
    let error = items.last().unwrap().as_ref().unwrap_err();
    assert_eq!(error.to_string(), "RQP_MARKUP_LIMIT (M-26)");
}

/// Markup pieces for generated documents.
const PIECES: &[&str] = &[
    "<div data-rq-block=\"a\">",
    "<p data-rq-block=\"b\" class=x>",
    "<title data-rq-block=\"t\">",
    "<script type=\"application/json\" data-rq-block=\"j\">",
    "</div>",
    "</p>",
    "</title>",
    "</script>",
    "<svg>",
    "</svg>",
    "<g a=1 a=2>",
    "</g>",
    "<style>",
    "</style>",
    "<script>",
    "<textarea>",
    "</textarea>",
    "<plaintext>",
    "<b onclick=\"x\" ONCLICK=y>",
    "</b>",
    "<br/>",
    "<!-- c -->",
    "<!--",
    "-->",
    "<![CDATA[x]]>",
    "<!DOCTYPE html>",
    "<?x>",
    "</>",
    "<",
    ">",
    "\"",
    "'",
    "=",
    "/",
    " ",
    "x",
    "\u{feff}",
];

fn soup() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(prop::sample::select(PIECES), 0..60)
        .prop_map(|pieces| pieces.concat().into_bytes())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn api_tokens_cover_any_bytes(input in prop::collection::vec(any::<u8>(), 0..400)) {
        assert_covers(&input);
        for tag in start_tags(&input) {
            for attribute in tag.attributes() {
                let attribute = attribute.unwrap();
                let span = attribute.name_span();
                prop_assert!(span.start >= tag.name_span().end && span.end < tag.span().end);
            }
        }
    }

    #[test]
    fn api_tokens_cover_markup_soup(input in soup()) {
        assert_covers(&input);
    }

    /// For a valid document, the start tag before each block content has the
    /// block id as the raw value of its first `data-rq-block` attribute: the
    /// scan and `analyze` read the attributes in the same way.
    #[test]
    fn api_attributes_agree_with_analyze(input in soup()) {
        let Ok(doc) = analyze(&input) else {
            return Ok(());
        };
        let tags = start_tags(&input);
        for block in &doc.blocks {
            let tag = tags.iter().find(|t| t.span().end == block.start).unwrap();
            let id = tag
                .attributes()
                .map(Result::unwrap)
                .find(|a| a.name().eq_ignore_ascii_case(b"data-rq-block"))
                .unwrap();
            prop_assert_eq!(id.value(), block.id.as_bytes());
        }
    }
}
