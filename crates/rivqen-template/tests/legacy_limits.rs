// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Limits of the legacy split and rebuild (DL-163): each limit passes at its
//! value and fails one above it.

#![cfg(feature = "legacy")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_proto::types::{MAX_BLOCK_BYTES, MAX_BLOCKS};
use rivqen_template::legacy::{
    DataBlock, LimitError, RenderError, Split, SplitOutcome, render_v1, split_v1,
};
use rivqen_template::limits::{
    MAX_LEGACY_NAME_BYTES, MAX_MARKUP_INPUT_BYTES, MAX_OPEN_ELEMENTS, TemplateLimits,
};

fn block(name: &str, content: &str) -> String {
    format!("<!--sonicdiff-{name}-->{content}<!--sonicdiff-{name}-end-->")
}

fn applied(html: &str, limits: &TemplateLimits) -> Split {
    match split_v1(html.as_bytes(), limits) {
        Ok(SplitOutcome::Applied(split)) => split,
        other => panic!("not applied: {other:?}"),
    }
}

#[test]
fn legacy_limits_document_at_the_limit_passes_and_one_byte_more_fails() {
    let limits = TemplateLimits::default();
    let mut html = String::from("<title>T</title>");
    html.push_str(&"x".repeat(limits.max_document_bytes() - html.len()));
    assert_eq!(html.len(), limits.max_document_bytes());
    let split = applied(&html, &limits);
    assert_eq!(
        split.template.len(),
        html.len() - "<title>T</title>".len() + 7
    );
    html.push('x');
    assert_eq!(
        split_v1(html.as_bytes(), &limits),
        Err(LimitError::DocumentTooLarge)
    );
    // The size check comes before the UTF-8 check.
    let mut bytes = vec![0xff; limits.max_document_bytes()];
    assert!(matches!(
        split_v1(&bytes, &limits),
        Ok(SplitOutcome::NotApplied(_))
    ));
    bytes.push(0xff);
    assert_eq!(split_v1(&bytes, &limits), Err(LimitError::DocumentTooLarge));
}

#[test]
fn legacy_limits_a_lower_document_limit_applies() {
    let limits = TemplateLimits::new(
        10,
        MAX_BLOCKS,
        MAX_BLOCK_BYTES,
        MAX_LEGACY_NAME_BYTES,
        MAX_OPEN_ELEMENTS,
    );
    assert!(split_v1(b"0123456789", &limits).is_ok());
    assert_eq!(
        split_v1(b"0123456789a", &limits),
        Err(LimitError::DocumentTooLarge)
    );
}

#[test]
fn legacy_limits_256_blocks_pass_and_257_fail() {
    let limits = TemplateLimits::default();
    let blocks: String = (0..256).map(|i| block(&format!("b{i}"), "x")).collect();
    // The title entry does not count.
    let html = format!("<title>T</title>{blocks}");
    let split = applied(&html, &limits);
    assert_eq!(split.data.len(), 257);
    let html = format!("{html}{}", block("last", "x"));
    assert_eq!(
        split_v1(html.as_bytes(), &limits),
        Err(LimitError::TooManyBlocks)
    );
}

#[test]
fn legacy_limits_blocks_without_key_do_not_count() {
    let limits = TemplateLimits::new(
        MAX_MARKUP_INPUT_BYTES,
        1,
        MAX_BLOCK_BYTES,
        MAX_LEGACY_NAME_BYTES,
        MAX_OPEN_ELEMENTS,
    );
    let html = format!("{}{}{}", block("", "x"), block("", "y"), block("a", "z"));
    let split = applied(&html, &limits);
    assert_eq!(split.data.len(), 2);
    let html = format!("{html}{}", block("b", "w"));
    assert_eq!(
        split_v1(html.as_bytes(), &limits),
        Err(LimitError::TooManyBlocks)
    );
    let none = TemplateLimits::new(
        MAX_MARKUP_INPUT_BYTES,
        0,
        MAX_BLOCK_BYTES,
        MAX_LEGACY_NAME_BYTES,
        MAX_OPEN_ELEMENTS,
    );
    assert_eq!(
        split_v1(block("a", "x").as_bytes(), &none),
        Err(LimitError::TooManyBlocks)
    );
}

#[test]
fn legacy_limits_block_at_the_limit_passes_and_one_byte_more_fails() {
    let limits = TemplateLimits::default();
    let markers = block("a", "").len();
    let at = block("a", &"x".repeat(limits.max_block_bytes() - markers));
    assert_eq!(at.len(), limits.max_block_bytes());
    let split = applied(&at, &limits);
    assert_eq!(split.data[1].value.len(), limits.max_block_bytes());
    let over = block("a", &"x".repeat(limits.max_block_bytes() - markers + 1));
    assert_eq!(
        split_v1(over.as_bytes(), &limits),
        Err(LimitError::BlockTooLarge)
    );
}

#[test]
fn legacy_limits_title_value_is_not_a_block() {
    let limits = TemplateLimits::new(
        MAX_MARKUP_INPUT_BYTES,
        MAX_BLOCKS,
        4,
        MAX_LEGACY_NAME_BYTES,
        MAX_OPEN_ELEMENTS,
    );
    let split = applied("<title>long title</title>", &limits);
    assert_eq!(split.data[0].value, "<title>long title</title>");
}

#[test]
fn legacy_limits_name_of_128_bytes_is_a_key_and_129_stays() {
    let limits = TemplateLimits::default();
    let at = "n".repeat(128);
    let split = applied(&block(&at, "x"), &limits);
    assert_eq!(split.data[1].key, format!("{{{at}}}"));
    let over = block(&"n".repeat(129), "x");
    let split = applied(&over, &limits);
    assert_eq!(split.template, over);
    assert_eq!(split.data.len(), 1);
}

#[test]
fn legacy_limits_a_lower_name_limit_applies() {
    let limits = TemplateLimits::new(
        MAX_MARKUP_INPUT_BYTES,
        MAX_BLOCKS,
        MAX_BLOCK_BYTES,
        2,
        MAX_OPEN_ELEMENTS,
    );
    let html = format!("{}{}", block("ab", "x"), block("abc", "y"));
    let split = applied(&html, &limits);
    assert_eq!(split.template, format!("{{ab}}{}", block("abc", "y")));
    // A long name `title` stays in the template: no collision.
    let split = applied(&block("title", "x"), &limits);
    assert_eq!(split.data.len(), 1);
}

#[test]
fn legacy_limits_first_problem_in_document_order_wins() {
    let limits = TemplateLimits::new(
        MAX_MARKUP_INPUT_BYTES,
        MAX_BLOCKS,
        50,
        MAX_LEGACY_NAME_BYTES,
        MAX_OPEN_ELEMENTS,
    );
    let html = format!(
        "{}{}{}",
        block("a", &"x".repeat(40)),
        block("b", "1"),
        block("b", "2")
    );
    assert_eq!(
        split_v1(html.as_bytes(), &limits),
        Err(LimitError::BlockTooLarge)
    );
    let html = format!(
        "{}{}{}",
        block("b", "1"),
        block("b", "2"),
        block("a", &"x".repeat(40))
    );
    assert!(matches!(
        split_v1(html.as_bytes(), &limits),
        Ok(SplitOutcome::NotApplied(_))
    ));
}

#[test]
fn legacy_limits_error_code_is_document_too_large() {
    for error in [
        LimitError::DocumentTooLarge,
        LimitError::TooManyBlocks,
        LimitError::BlockTooLarge,
    ] {
        assert_eq!(error.code(), ErrorCode::DocumentTooLarge);
        assert_eq!(error.to_string(), error.reason());
        assert_eq!(RenderError::Limit(error).to_string(), error.reason());
    }
}

#[test]
fn legacy_limits_reason_names_the_limit() {
    for (error, limit) in [
        (LimitError::DocumentTooLarge, "maxDocumentBytes"),
        (LimitError::TooManyBlocks, "maxBlocks"),
        (LimitError::BlockTooLarge, "maxBlockBytes"),
    ] {
        assert!(error.reason().contains(limit), "{error:?}");
    }
}

fn entries(list: &[(&str, String)]) -> Vec<DataBlock> {
    list.iter()
        .map(|(key, value)| DataBlock {
            key: (*key).to_owned(),
            value: value.clone(),
        })
        .collect()
}

#[test]
fn legacy_limits_render_output_at_the_limit_passes_and_one_byte_more_fails() {
    let limits = TemplateLimits::new(
        100,
        MAX_BLOCKS,
        MAX_BLOCK_BYTES,
        MAX_LEGACY_NAME_BYTES,
        MAX_OPEN_ELEMENTS,
    );
    // The template is only the two placeholders: 7 + 93 bytes give 100.
    let template = "{title}{a}";
    let at = entries(&[("{title}", "T".repeat(7)), ("{a}", "v".repeat(93))]);
    assert_eq!(render_v1(template, &at, &limits).unwrap().len(), 100);
    let over = entries(&[("{title}", "T".repeat(7)), ("{a}", "v".repeat(94))]);
    assert_eq!(
        render_v1(template, &over, &limits),
        Err(RenderError::Limit(LimitError::DocumentTooLarge))
    );
}

#[test]
fn legacy_limits_render_input_limits() {
    let limits = TemplateLimits::new(16, 1, 2, MAX_LEGACY_NAME_BYTES, MAX_OPEN_ELEMENTS);
    let empty_title = || ("{title}", String::new());
    // Template over the document limit, also when the output would fit.
    assert_eq!(
        render_v1(&"x".repeat(17), &entries(&[empty_title()]), &limits),
        Err(RenderError::Limit(LimitError::DocumentTooLarge))
    );
    let shrinks = entries(&[empty_title(), ("{a}", String::new())]);
    assert_eq!(
        render_v1("{a}xxxxxxxxxxxxxx", &shrinks, &limits),
        Err(RenderError::Limit(LimitError::DocumentTooLarge))
    );
    assert_eq!(
        render_v1("{a}xxxxxxxxxxxxx", &shrinks, &limits).unwrap(),
        "xxxxxxxxxxxxx"
    );
    // More entries than max_blocks + 1.
    let three = entries(&[empty_title(), ("{a}", "1".into()), ("{b}", "2".into())]);
    assert_eq!(
        render_v1("{a}{b}", &three, &limits),
        Err(RenderError::Limit(LimitError::TooManyBlocks))
    );
    // A block value over max_block_bytes; the title value is not a block.
    let large = entries(&[empty_title(), ("{a}", "123".into())]);
    assert_eq!(
        render_v1("{a}", &large, &limits),
        Err(RenderError::Limit(LimitError::BlockTooLarge))
    );
    let at = entries(&[("{title}", "TTT".into()), ("{a}", "12".into())]);
    assert_eq!(render_v1("{title}{a}", &at, &limits).unwrap(), "TTT12");
}
