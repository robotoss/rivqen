// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The canonical rebuild rule (legacy markers section 5): each placeholder
//! exactly once, in template order; the placeholder of a key is its first
//! occurrence in the template.

#![cfg(feature = "legacy")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use rivqen_proto::types::{MAX_BLOCK_BYTES, MAX_BLOCKS};
use rivqen_template::legacy::{DataBlock, RenderError, SplitOutcome, render_v1, split_v1};
use rivqen_template::limits::{MAX_MARKUP_INPUT_BYTES, MAX_OPEN_ELEMENTS, TemplateLimits};

fn data(entries: &[(&str, &str)]) -> Vec<DataBlock> {
    entries
        .iter()
        .map(|(key, value)| DataBlock {
            key: (*key).to_owned(),
            value: (*value).to_owned(),
        })
        .collect()
}

fn render(template: &str, entries: &[(&str, &str)]) -> Result<String, RenderError> {
    render_v1(template, &data(entries), &TemplateLimits::default())
}

#[test]
fn legacy_render_replaces_each_placeholder_once_in_template_order() {
    let page = render(
        "<head>{title}</head>{a}<hr>{b}",
        &[("{title}", "<title>T</title>"), ("{a}", "A"), ("{b}", "B")],
    );
    assert_eq!(page.unwrap(), "<head><title>T</title></head>A<hr>B");
}

#[test]
fn legacy_render_data_order_does_not_change_the_page() {
    let template = "{b}{title}{a}";
    let forward = render(template, &[("{title}", "T"), ("{a}", "A"), ("{b}", "B")]);
    let backward = render(template, &[("{b}", "B"), ("{a}", "A"), ("{title}", "T")]);
    assert_eq!(forward.unwrap(), "BTA");
    assert_eq!(backward.unwrap(), "BTA");
}

#[test]
fn legacy_render_first_occurrence_of_a_key_is_its_placeholder() {
    // The shape of FX-LEG-MAL-PLACEHOLDER-TEXT: key text before the block.
    let html = "<title>Shop</title><header>Static {price}</header><!--sonicdiff-price-->130<!--sonicdiff-price-end-->";
    let limits = TemplateLimits::default();
    let SplitOutcome::Applied(split) = split_v1(html.as_bytes(), &limits).unwrap() else {
        panic!("not applied");
    };
    assert_eq!(
        split.template,
        "{title}<header>Static {price}</header>{price}"
    );
    let page = render_v1(&split.template, &split.data, &limits).unwrap();
    assert_eq!(
        page,
        "<title>Shop</title><header>Static <!--sonicdiff-price-->130<!--sonicdiff-price-end--></header>{price}"
    );
    assert_ne!(page, html);
}

#[test]
fn legacy_render_later_key_text_stays_text() {
    let page = render("{title}{a}<p>{a}</p>", &[("{title}", "T"), ("{a}", "A")]);
    assert_eq!(page.unwrap(), "TA<p>{a}</p>");
}

#[test]
fn legacy_render_empty_title_has_no_placeholder() {
    // A page without a title: `{title}` text in it is page text.
    let html = "<p>{{title}}</p><!--sonicdiff-a-->A<!--sonicdiff-a-end-->";
    let limits = TemplateLimits::default();
    let SplitOutcome::Applied(split) = split_v1(html.as_bytes(), &limits).unwrap() else {
        panic!("not applied");
    };
    assert_eq!(split.data[0].value, "");
    assert_eq!(
        render_v1(&split.template, &split.data, &limits).unwrap(),
        html
    );
    // No `{title}` text at all.
    assert_eq!(
        render("x{a}", &[("{title}", ""), ("{a}", "A")]).unwrap(),
        "xA"
    );
}

#[test]
fn legacy_render_key_text_inside_a_value_is_not_replaced() {
    let page = render(
        "{title}{a}{b}",
        &[("{title}", "{b}"), ("{a}", "{title}"), ("{b}", "B")],
    );
    assert_eq!(page.unwrap(), "{b}{title}B");
}

#[test]
fn legacy_render_brace_text_without_an_entry_stays() {
    let page = render("{x}{ {}{a b}{{a}}{title", &[("{title}", ""), ("{a}", "A")]);
    assert_eq!(page.unwrap(), "{x}{ {}{a b}{A}{title");
}

#[test]
fn legacy_render_reads_a_long_word_run() {
    let long = format!("{{{}}}", "a".repeat(10_000));
    let template = format!("{long}{{a}}");
    let page = render(&template, &[("{title}", ""), ("{a}", "A")]);
    assert_eq!(page.unwrap(), format!("{long}A"));
}

#[test]
fn legacy_render_keeps_non_ascii_text() {
    let page = render("é{a}\u{1F600}", &[("{title}", ""), ("{a}", "ü")]);
    assert_eq!(page.unwrap(), "éü\u{1F600}");
}

#[test]
fn legacy_render_entry_without_placeholder_is_a_mismatch() {
    assert_eq!(
        render("{title}", &[("{title}", "T"), ("{a}", "A")]),
        Err(RenderError::Mismatch)
    );
    // A title value without its placeholder.
    assert_eq!(
        render("{a}", &[("{title}", "T"), ("{a}", "A")]),
        Err(RenderError::Mismatch)
    );
}

#[test]
fn legacy_render_title_inside_a_block_value_is_a_mismatch() {
    // Function fixture FN-SPLIT-TITLE-06: the `{title}` placeholder is in
    // the value of `{a}`, not in the template; the page cannot be rebuilt
    // with one replacement per placeholder.
    let html =
        "<html><body><!--sonicdiff-a--><title>In block</title><!--sonicdiff-a-end--></body></html>";
    let limits = TemplateLimits::default();
    let SplitOutcome::Applied(split) = split_v1(html.as_bytes(), &limits).unwrap() else {
        panic!("not applied");
    };
    assert_eq!(
        render_v1(&split.template, &split.data, &limits),
        Err(RenderError::Mismatch)
    );
}

#[test]
fn legacy_render_missing_title_entry_is_a_mismatch() {
    assert_eq!(render("{a}", &[("{a}", "A")]), Err(RenderError::Mismatch));
    assert_eq!(render("", &[]), Err(RenderError::Mismatch));
    assert_eq!(render("", &[("{title}", "")]), Ok(String::new()));
}

#[test]
fn legacy_render_repeated_key_is_a_mismatch() {
    assert_eq!(
        render("{a}{a}", &[("{title}", ""), ("{a}", "1"), ("{a}", "2")]),
        Err(RenderError::Mismatch)
    );
    assert_eq!(
        render("{title}", &[("{title}", ""), ("{title}", "T")]),
        Err(RenderError::Mismatch)
    );
    assert_eq!(
        render("{title}", &[("{title}", "T"), ("{title}", "T")]),
        Err(RenderError::Mismatch)
    );
}

#[test]
fn legacy_render_malformed_key_is_a_mismatch() {
    let over = format!("{{{}}}", "n".repeat(129));
    for key in [
        "",
        "a",
        "{}",
        "{a",
        "a}",
        "{a b}",
        "{a-b}",
        "{é}",
        "{{a}}",
        over.as_str(),
    ] {
        let template = format!("{{title}}{key}");
        assert_eq!(
            render(&template, &[("{title}", "T"), (key, "V")]),
            Err(RenderError::Mismatch),
            "{key:?}"
        );
    }
    // 128 bytes is a name.
    let at = format!("{{{}}}", "n".repeat(128));
    assert_eq!(render(&at, &[("{title}", ""), (&at, "V")]).unwrap(), "V");
}

#[test]
fn legacy_render_title_is_valid_with_a_short_name_limit() {
    let limits = TemplateLimits::new(
        MAX_MARKUP_INPUT_BYTES,
        MAX_BLOCKS,
        MAX_BLOCK_BYTES,
        1,
        MAX_OPEN_ELEMENTS,
    );
    let entries = data(&[("{title}", "T"), ("{a}", "A")]);
    assert_eq!(render_v1("{title}{a}", &entries, &limits).unwrap(), "TA");
    let entries = data(&[("{title}", "T"), ("{ab}", "A")]);
    assert_eq!(
        render_v1("{title}{ab}", &entries, &limits),
        Err(RenderError::Mismatch)
    );
}

#[test]
fn legacy_render_error_display_holds_no_content() {
    assert_eq!(
        RenderError::Mismatch.to_string(),
        "the legacy data does not fit the template"
    );
}
