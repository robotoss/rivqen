// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The scan of `rivqen_template::rqp::scan` after the WP-06 S1 security
//! review: no input bytes in `Debug` (finding S1-1), and the M-01 input
//! limit and the token stack limit `max_open_elements() + 2` (finding S1-2);
//! the implied closure of M-09 on the token stack (WP-06 S2, DL-170).

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
use std::fs;
use std::path::{Path, PathBuf};

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

/// Every `*.html` file under `dir`, in path order.
fn html_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            html_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "html") {
            out.push(path);
        }
    }
}

/// DL-170: of the HTML inputs of `fixtures/rqp` (the markup fixtures and
/// the pages of the protocol fixtures), the scan with the default limits
/// stops only where `split_rqp` gives `RQP_MARKUP_LIMIT` too, and it reads
/// every input that `split_rqp` accepts to the end. Before the implied
/// closure of M-09, `FX-RQ-MARKUP-TABLE-CLOSE-DEPTH` (valid) stopped it.
#[test]
fn scan_reads_every_valid_markup_fixture() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rqp");
    let mut paths = Vec::new();
    html_files(&root, &mut paths);
    assert_eq!(paths.len(), 401);
    let limits = TemplateLimits::default();
    let (mut valid, mut stopped) = (0, Vec::new());
    for path in &paths {
        let input = fs::read(path).unwrap();
        let name = path.strip_prefix(&root).unwrap().display().to_string();
        let errors: Vec<MarkupError> = scan(&input, &limits).filter_map(Result::err).collect();
        let result = split_rqp(&input, &limits);
        valid += usize::from(result.is_ok());
        match (errors.as_slice(), result) {
            ([], _) => {}
            ([stop], Err(error)) => {
                assert_eq!(stop.code(), MarkupCode::Limit, "{name}");
                assert_eq!(error.code(), MarkupCode::Limit, "{name}");
                stopped.push(name);
            }
            (more, result) => panic!("{name}: {more:?}, split_rqp {:?}", result.err()),
        }
    }
    assert_eq!(valid, 185);
    assert_eq!(stopped, ["markup/FX-RQ-MARKUP-LIMIT-INPUT-SIZE/input.html"]);
    // The markup fixture of DL-170 is among the inputs and valid.
    let table = load()
        .unwrap()
        .into_iter()
        .find(|fixture| fixture.id == "FX-RQ-MARKUP-TABLE-CLOSE-DEPTH")
        .unwrap();
    assert!(split_rqp(&table.input, &limits).is_ok());
    assert!(items(&table.input, &limits).iter().all(Result::is_ok));
}

/// DL-170: the token stack of the scan applies the implied closure of M-09
/// (H-85a) as `split_rqp` does, so a table without the end tags of its
/// cells, rows, row groups, caption or column group keeps a small stack.
/// The cap `max_open_elements() + 2` stays.
#[test]
fn scan_stack_applies_implied_closure() {
    // One unit of D gives a stack of at most 3 in the scan.
    let one = limits(MAX_MARKUP_INPUT_BYTES, 1);
    let tight = limits(MAX_MARKUP_INPUT_BYTES, 8);
    let ok = |input: &str, limits: &TemplateLimits| {
        items(input.as_bytes(), limits).iter().all(Result::is_ok)
    };
    let stops = |input: &str, limits: &TemplateLimits| {
        last_error(&items(input.as_bytes(), limits)) == (MarkupCode::Limit, "M-26")
    };
    for (shape, open) in [
        // Cells close cells and rows close rows (stack table, tr, td).
        ("<table><tr>{}</table>", "<td>a"),
        ("<table><tr>{}</table>", "<td>a<th>b"),
        ("<table>{}</table>", "<tr><td>a<td>b"),
        // A row group closes the cell, the row and the row group.
        ("<table>{}</table>", "<thead><tr><th>h<tbody><tr><td>d"),
        // A caption and a column group close before a row.
        (
            "<table>{}</table>",
            "<caption>c<colgroup><col><tr><td>x<tbody>",
        ),
        // The closure crosses elements as the end tag of F does.
        ("<table><tr><td><div>a{}</table>", "<td>b"),
    ] {
        let input = shape.replace("{}", &open.repeat(300));
        assert_eq!(split_rqp(input.as_bytes(), &tight).err(), None, "{open}");
        assert!(ok(&input, &tight), "{open}");
    }
    // The fixture shape: 300 rows of `<tr><td>a<td>b`, D at most 5.
    let rows = format!("<table>{}</table>", "<tr><td>a<td>b".repeat(300));
    let five = limits(MAX_MARKUP_INPUT_BYTES, 5);
    assert!(split_rqp(rows.as_bytes(), &five).is_ok());
    assert!(ok(&rows, &one));

    // No closure: a `template` ends the search for F, and F that must be
    // the current node is not (step 2; `split_rqp` rejects that document).
    assert!(stops("<table><td><template><td><td>", &one));
    assert!(ok("<table><td><template></template><td><td>", &one));
    assert!(stops("<table><tr><b><td>", &one));
    assert!(ok("<table><td><b><td>", &one));
    // In a foreign region a table part is a foreign element: no closure.
    assert!(stops("<svg><td><td><td>", &one));
    assert!(ok("<table><td><svg></svg><td><td>", &one));
    // A table part closes only table parts: `table` is never closed.
    assert!(stops("<table><td><table><td>", &one));
    // The cap stays: elements that are not table parts still count.
    assert!(stops(&"<div>".repeat(4), &one));
    assert!(ok(&"<div>".repeat(3), &one));
}
