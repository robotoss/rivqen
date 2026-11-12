// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The scan of `rivqen_template::rqp::scan` after the WP-06 S1 security
//! review: no input bytes in `Debug` (finding S1-1), and the M-01 input
//! limit and the token stack limit `max_open_elements() + 2` (finding S1-2).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use rivqen_template::limits::{
    MAX_LEGACY_NAME_BYTES, MAX_MARKUP_INPUT_BYTES, MAX_OPEN_ELEMENTS, TemplateLimits,
};
use rivqen_template::rqp::scan::{Token, scan};
use rivqen_template::rqp::{MarkupCode, MarkupError, split_rqp};
use rivqen_testkit::markup::load;

const MARKER: &str = "qzsecretmarkerqz";

/// The default limits with another `max_document_bytes` and
/// `max_open_elements`.
fn limits(document: usize, open: usize) -> TemplateLimits {
    let default = TemplateLimits::default();
    TemplateLimits::new(
        document,
        default.max_blocks(),
        default.max_block_bytes(),
        MAX_LEGACY_NAME_BYTES,
        open,
    )
}

/// All items of a scan, as `Ok` kinds or the error.
fn items(input: &[u8], limits: &TemplateLimits) -> Vec<Result<&'static str, MarkupError>> {
    scan(input, limits)
        .map(|item| {
            item.map(|token| match token {
                Token::Gap(_) => "gap",
                Token::StartTag(_) => "start",
                Token::EndTag(_) => "end",
                Token::Comment(_) => "comment",
                Token::Doctype(_) => "doctype",
                Token::Cdata { .. } => "cdata",
                _ => "other",
            })
        })
        .collect()
}

/// The code and the rule of the last item, which must be the only error.
fn last_error(items: &[Result<&'static str, MarkupError>]) -> (MarkupCode, &'static str) {
    let (last, before) = items.split_last().expect("at least one item");
    assert!(
        before.iter().all(Result::is_ok),
        "an error before the last item"
    );
    let error = last.as_ref().expect_err("the last item is not an error");
    (error.code(), error.rule())
}

fn assert_no_marker(text: &str) {
    assert!(
        !text.to_ascii_lowercase().contains(MARKER),
        "Debug output holds input bytes: {text}"
    );
}

/// `{:?}` and `{:#?}` of a value.
fn debug_texts(value: &dyn core::fmt::Debug) -> [String; 2] {
    [format!("{value:?}"), format!("{value:#?}")]
}

#[test]
fn scan_debug_holds_no_input() {
    let m = MARKER;
    let input = format!(
        "<!DOCTYPE {m}><!-- {m} --><{m} {m}=\"{m}\" data-{m}='{m}' x={m} {m}>{m}\
         <svg><{m}/><![CDATA[{m}]]></svg><title>{m}<b></title></{m} {m}={m}>{m}"
    );
    let input = input.as_bytes();
    let limits = TemplateLimits::default();
    let mut scanner = scan(input, &limits);
    let mut seen = [0_usize; 6];
    for text in debug_texts(&scanner) {
        assert_no_marker(&text);
    }
    while let Some(item) = scanner.next() {
        let token = item.unwrap();
        for text in debug_texts(&scanner) {
            assert_no_marker(&text);
        }
        for text in debug_texts(&token) {
            assert_no_marker(&text);
        }
        for text in debug_texts(&token.span()) {
            assert_no_marker(&text);
        }
        let kind = match &token {
            Token::Gap(_) => 0,
            Token::StartTag(_) => 1,
            Token::EndTag(_) => 2,
            Token::Comment(_) => 3,
            Token::Doctype(_) => 4,
            Token::Cdata { .. } => 5,
            _ => panic!("unknown token"),
        };
        seen[kind] += 1;
        if let Token::StartTag(tag) | Token::EndTag(tag) = &token {
            for text in debug_texts(tag) {
                assert_no_marker(&text);
            }
            let mut attributes = tag.attributes();
            for text in debug_texts(&attributes) {
                assert_no_marker(&text);
            }
            while let Some(attribute) = attributes.next() {
                let attribute = attribute.unwrap();
                for text in debug_texts(&attribute) {
                    assert_no_marker(&text);
                }
                for text in debug_texts(&attributes) {
                    assert_no_marker(&text);
                }
            }
            for text in debug_texts(&attributes) {
                assert_no_marker(&text);
            }
        }
    }
    for text in debug_texts(&scanner) {
        assert_no_marker(&text);
    }
    // Every kind of token was formatted.
    assert!(seen.iter().all(|&count| count > 0), "{seen:?}");

    // The marker is in the input and in the raw accessors: the test would
    // see a leak.
    let tags: Vec<_> = scan(input, &limits)
        .filter_map(|item| match item.unwrap() {
            Token::StartTag(tag) => Some(tag),
            _ => None,
        })
        .collect();
    let tag = tags.iter().find(|tag| tag.name() == m.as_bytes()).unwrap();
    let attribute = tag.attributes().next().unwrap().unwrap();
    assert_eq!(attribute.name(), m.as_bytes());
    assert_eq!(attribute.value(), m.as_bytes());
    // Debug shows the lengths and the flags.
    let text = format!("{tag:?}");
    assert!(text.contains(&format!("name_len: {}", m.len())), "{text}");
    assert!(text.contains("self_closing: false"), "{text}");
    assert!(text.contains("in_foreign_region: false"), "{text}");
    let text = format!("{attribute:?}");
    assert!(text.contains(&format!("value_len: {}", m.len())), "{text}");
    let text = format!("{:?}", tag.attributes());
    assert!(text.contains("finished: false"), "{text}");
    let text = format!("{:?}", scan(input, &limits));
    assert!(
        text.contains(&format!("input_len: {}", input.len())),
        "{text}"
    );
    let first = scan(input, &limits).next().unwrap().unwrap();
    let text = format!("{first:?}");
    assert!(text.starts_with("Doctype("), "{text}");
}

#[test]
fn scan_debug_of_a_limit_stop_holds_no_input() {
    let input = format!("<{MARKER}>").repeat(10);
    let mut scanner = scan(input.as_bytes(), &limits(MAX_MARKUP_INPUT_BYTES, 0));
    let mut errors = 0;
    while let Some(item) = scanner.next() {
        for text in debug_texts(&scanner) {
            assert_no_marker(&text);
        }
        if let Err(error) = item {
            errors += 1;
            assert_no_marker(&format!("{error:?} {error}"));
        }
    }
    assert_eq!(errors, 1);
}

#[test]
fn scan_limit_input_over_m01() {
    let ten = limits(10, MAX_OPEN_ELEMENTS);
    assert_eq!(items(b"0123456789", &ten), [Ok("gap")]);
    let over = items(b"0123456789a", &ten);
    assert_eq!(over.len(), 1);
    assert_eq!(last_error(&over), (MarkupCode::Limit, "M-01"));
    // The scan ends after the error.
    let mut scanner = scan(b"<b>0123456789", &ten);
    assert!(scanner.next().unwrap().is_err());
    assert!(scanner.next().is_none());
    assert!(scanner.next().is_none());

    let zero = limits(0, MAX_OPEN_ELEMENTS);
    assert_eq!(items(b"", &zero), []);
    assert_eq!(last_error(&items(b"x", &zero)), (MarkupCode::Limit, "M-01"));

    // The limit is the smaller of `max_document_bytes` and M-01: 5 MiB also
    // when `max_document_bytes` is 32 MiB.
    let at = vec![b'x'; MAX_MARKUP_INPUT_BYTES];
    let over = vec![b'x'; MAX_MARKUP_INPUT_BYTES + 1];
    for limits in [
        TemplateLimits::default(),
        limits(usize::MAX, MAX_OPEN_ELEMENTS),
    ] {
        assert_eq!(items(&at, &limits), [Ok("gap")]);
        assert_eq!(
            last_error(&items(&over, &limits)),
            (MarkupCode::Limit, "M-01")
        );
        // `split_rqp` gives the same error for the same input.
        let error = split_rqp(&over, &limits).unwrap_err();
        assert_eq!((error.code(), error.rule()), (MarkupCode::Limit, "M-01"));
    }
}

#[test]
fn scan_limit_stack_over_open_elements() {
    // `max_open_elements() + 2` elements pass; one more stops the scan with
    // one error item in place of the start tag.
    for (open, max_stack) in [
        (0, 2),
        (3, 5),
        (MAX_OPEN_ELEMENTS, MAX_OPEN_ELEMENTS + 2),
        // Capped at 256 by `TemplateLimits::new`.
        (usize::MAX, MAX_OPEN_ELEMENTS + 2),
    ] {
        let limits = limits(MAX_MARKUP_INPUT_BYTES, open);
        let at = items(&b"<div>".repeat(max_stack), &limits);
        assert_eq!(at.len(), max_stack);
        assert!(at.iter().all(|item| *item == Ok("start")));
        let over = items(&b"<div>".repeat(max_stack + 1), &limits);
        assert_eq!(over.len(), max_stack + 1);
        assert_eq!(last_error(&over), (MarkupCode::Limit, "M-26"));
        // Nothing after the error, also with more input. The gap before the
        // start tag that stops the scan is not given.
        let more = items(&b"<div>x".repeat(max_stack + 10), &limits);
        assert_eq!(last_error(&more), (MarkupCode::Limit, "M-26"));
        assert_eq!(more.len(), 2 * max_stack);
    }

    let three = limits(MAX_MARKUP_INPUT_BYTES, 3);
    // Closed elements, void elements and self-closing region roots never
    // stay on the stack.
    let flat = b"<div></div><br><svg/><math/>".repeat(100);
    assert!(items(&flat, &three).iter().all(Result::is_ok));
    // Elements in a foreign region count; self-closing ones do not.
    let region = |children: usize| format!("<svg>{}<g/><g/>", "<g>".repeat(children));
    assert!(
        items(region(4).as_bytes(), &three)
            .iter()
            .all(Result::is_ok)
    );
    assert_eq!(
        last_error(&items(region(5).as_bytes(), &three)),
        (MarkupCode::Limit, "M-26")
    );
}

/// The markup fixtures that stop the scan with the default limits. A stop
/// is one `RQP_MARKUP_LIMIT` item. An invalid fixture that stops is one
/// that `split_rqp` rejects with `RQP_MARKUP_LIMIT` too. The one valid
/// fixture that stops is the known gap of the next test.
#[test]
fn scan_limit_stops_of_the_markup_fixtures() {
    let fixtures = load().unwrap();
    assert_eq!(fixtures.len(), 362);
    let limits = TemplateLimits::default();
    let (mut valid, mut stopped_valid, mut stopped_invalid) = (0, Vec::new(), Vec::new());
    for fixture in &fixtures {
        let errors: Vec<MarkupError> = scan(&fixture.input, &limits)
            .filter_map(Result::err)
            .collect();
        let result = split_rqp(&fixture.input, &limits);
        valid += usize::from(result.is_ok());
        match errors.as_slice() {
            [] => continue,
            [stop] => assert_eq!(stop.code(), MarkupCode::Limit, "{}", fixture.id),
            more => panic!("{}: {more:?}", fixture.id),
        }
        match result {
            Ok(_) => stopped_valid.push(fixture.id.as_str()),
            Err(error) => {
                assert_eq!(error.code(), MarkupCode::Limit, "{}", fixture.id);
                stopped_invalid.push(fixture.id.as_str());
            }
        }
    }
    assert_eq!(valid, 148);
    assert_eq!(stopped_valid, ["FX-RQ-MARKUP-TABLE-CLOSE-DEPTH"]);
    assert_eq!(stopped_invalid, ["FX-RQ-MARKUP-LIMIT-INPUT-SIZE"]);
}

/// Known gap (WP-06 S1 T-06 report): the scan does not apply the implied
/// closure of M-09, so a table row with more than 258 cells without `</td>`
/// passes `split_rqp` and stops the scan. The stop fails closed.
#[test]
fn scan_limit_stack_can_stop_a_document_that_split_rqp_accepts() {
    let limits = TemplateLimits::default();
    let cells = |count: usize| format!("<table><tr>{}</table>", "<td>a".repeat(count)).into_bytes();
    let short = cells(256);
    assert!(split_rqp(&short, &limits).is_ok());
    assert!(items(&short, &limits).iter().all(Result::is_ok));
    let long = cells(300);
    assert!(split_rqp(&long, &limits).is_ok());
    assert_eq!(
        last_error(&items(&long, &limits)),
        (MarkupCode::Limit, "M-26")
    );
}
