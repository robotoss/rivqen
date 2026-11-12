// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Limits of the RQP markup analysis (DL-163): M-01 is the smaller of
//! `max_document_bytes` and the `rqp/1` input limit; the M-26 limits come
//! from `max_blocks`, `max_block_bytes` and `max_open_elements`. A lower
//! value lowers the limit, a higher value has no effect, and the default is
//! M-26.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::format_collect
)]

use rivqen_model::limits::ResourceLimits;
use rivqen_proto::types::{MAX_BLOCK_BYTES, MAX_BLOCKS};
use rivqen_template::limits::{MAX_MARKUP_INPUT_BYTES, MAX_OPEN_ELEMENTS, TemplateLimits};
use rivqen_template::rqp::{MarkupCode, split_rqp};

fn limits(edit: impl FnOnce(&mut TemplateLimits)) -> TemplateLimits {
    let mut limits = TemplateLimits::default();
    edit(&mut limits);
    limits
}

/// The code and the rule of an invalid input.
fn error(input: &[u8], limits: &TemplateLimits) -> (MarkupCode, &'static str) {
    match split_rqp(input, limits) {
        Ok(_) => panic!("expected an invalid document"),
        Err(error) => (error.code(), error.rule()),
    }
}

fn block_count(input: &[u8], limits: &TemplateLimits) -> usize {
    match split_rqp(input, limits) {
        Ok(doc) => doc.blocks.len(),
        Err(error) => panic!("expected a valid document, got {error}"),
    }
}

/// `count` blocks `<p data-rq-block="bN">x</p>`.
fn blocks(count: usize) -> Vec<u8> {
    (0..count)
        .map(|n| format!("<p data-rq-block=\"b{n}\">x</p>"))
        .collect::<String>()
        .into_bytes()
}

/// One block with `size` content bytes.
fn sized(size: usize) -> Vec<u8> {
    format!("<div data-rq-block=\"a\">{}</div>", "x".repeat(size)).into_bytes()
}

/// `depth` nested `div` elements, closed.
fn nested(depth: usize) -> Vec<u8> {
    format!("{}{}", "<div>".repeat(depth), "</div>".repeat(depth)).into_bytes()
}

const LIMIT_M01: (MarkupCode, &str) = (MarkupCode::Limit, "M-01");
const LIMIT_M26: (MarkupCode, &str) = (MarkupCode::Limit, "M-26");

#[test]
fn rqp_limits_input_size_follows_max_document_bytes() {
    let small = limits(|l| l.max_document_bytes = 10);
    assert_eq!(block_count(&[b'a'; 10], &small), 0);
    assert_eq!(error(&[b'a'; 11], &small), LIMIT_M01);
    // M-01 comes before M-02: invalid UTF-8 over the limit is a limit error.
    assert_eq!(error(&[0xFF; 11], &small), LIMIT_M01);
    assert_eq!(error(&[0xFF; 10], &small), (MarkupCode::Encoding, "M-02"));
    let zero = limits(|l| l.max_document_bytes = 0);
    assert_eq!(block_count(b"", &zero), 0);
    assert_eq!(error(b" ", &zero), LIMIT_M01);
}

#[test]
fn rqp_limits_input_size_is_capped_at_m01() {
    let large = limits(|l| l.max_document_bytes = usize::MAX);
    let mut input = vec![b'a'; MAX_MARKUP_INPUT_BYTES];
    assert_eq!(block_count(&input, &large), 0);
    input.push(b'a');
    assert_eq!(error(&input, &large), LIMIT_M01);
    assert_eq!(error(&input, &TemplateLimits::default()), LIMIT_M01);
}

#[test]
fn rqp_limits_block_count_follows_max_blocks() {
    let two = limits(|l| l.max_blocks = 2);
    assert_eq!(block_count(&blocks(2), &two), 2);
    assert_eq!(error(&blocks(3), &two), LIMIT_M26);
    let none = limits(|l| l.max_blocks = 0);
    assert_eq!(block_count(b"<p>x</p>", &none), 0);
    assert_eq!(error(&blocks(1), &none), LIMIT_M26);
    // A higher value has no effect.
    let large = limits(|l| l.max_blocks = usize::MAX);
    assert_eq!(block_count(&blocks(MAX_BLOCKS), &large), MAX_BLOCKS);
    assert_eq!(error(&blocks(MAX_BLOCKS + 1), &large), LIMIT_M26);
}

#[test]
fn rqp_limits_block_size_follows_max_block_bytes() {
    let four = limits(|l| l.max_block_bytes = 4);
    assert_eq!(block_count(&sized(4), &four), 1);
    assert_eq!(error(&sized(5), &four), LIMIT_M26);
    // The limit is on the content of each block, not on their sum.
    let two_blocks = b"<p data-rq-block=\"a\">xxxx</p><p data-rq-block=\"b\">yyyy</p>";
    assert_eq!(block_count(two_blocks, &four), 2);
    let empty = limits(|l| l.max_block_bytes = 0);
    assert_eq!(block_count(&sized(0), &empty), 1);
    assert_eq!(error(&sized(1), &empty), LIMIT_M26);
    // A higher value has no effect.
    let large = limits(|l| l.max_block_bytes = usize::MAX);
    assert_eq!(block_count(&sized(MAX_BLOCK_BYTES), &large), 1);
    assert_eq!(error(&sized(MAX_BLOCK_BYTES + 1), &large), LIMIT_M26);
}

#[test]
fn rqp_limits_depth_follows_max_open_elements() {
    let three = limits(|l| l.max_open_elements = 3);
    assert_eq!(block_count(&nested(3), &three), 0);
    assert_eq!(error(&nested(4), &three), LIMIT_M26);
    // The block element holds a unit of D too.
    assert_eq!(
        block_count(
            b"<div><div><p data-rq-block=\"a\">x</p></div></div>",
            &three
        ),
        1
    );
    assert_eq!(
        error(
            b"<div><div><div><p data-rq-block=\"a\">x</p></div></div></div>",
            &three
        ),
        LIMIT_M26
    );
    // The implied table parts of M-26 item 3 count: table, row group, row,
    // cell.
    let table = b"<table><tr><td>x</td></tr></table>";
    assert_eq!(block_count(table, &limits(|l| l.max_open_elements = 4)), 0);
    assert_eq!(error(table, &three), LIMIT_M26);
    // The pass over a `noscript` content (M-11 item 8) uses the same limit.
    let noscript = b"<noscript><b><i>x</i></b></noscript>";
    assert_eq!(block_count(noscript, &three), 0);
    assert_eq!(
        error(noscript, &limits(|l| l.max_open_elements = 2)),
        LIMIT_M26
    );
    let none = limits(|l| l.max_open_elements = 0);
    assert_eq!(block_count(b"x<br>", &none), 0);
    assert_eq!(error(b"<b>", &none), LIMIT_M26);
}

#[test]
fn rqp_limits_depth_is_capped_at_m26() {
    let large = limits(|l| l.max_open_elements = usize::MAX);
    assert_eq!(block_count(&nested(MAX_OPEN_ELEMENTS), &large), 0);
    assert_eq!(error(&nested(MAX_OPEN_ELEMENTS + 1), &large), LIMIT_M26);
}

#[test]
fn rqp_limits_default_equals_m26() {
    let default = TemplateLimits::default();
    assert_eq!(default.max_document_bytes, 5_242_880);
    assert_eq!(default.max_blocks, 256);
    assert_eq!(default.max_block_bytes, 1_048_576);
    assert_eq!(default.max_open_elements, 256);
    assert_eq!(
        TemplateLimits::try_from(&ResourceLimits::default()).unwrap(),
        default
    );
    assert_eq!(block_count(&blocks(256), &default), 256);
    assert_eq!(error(&blocks(257), &default), LIMIT_M26);
    assert_eq!(block_count(&sized(1_048_576), &default), 1);
    assert_eq!(error(&sized(1_048_577), &default), LIMIT_M26);
    assert_eq!(block_count(&nested(256), &default), 0);
    assert_eq!(error(&nested(257), &default), LIMIT_M26);
}
