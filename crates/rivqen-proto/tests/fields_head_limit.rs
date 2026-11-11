// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The `maxHeaderBytes` gate before every response field read (DL-153,
//! resource limits section 1). Size of a line = name bytes + value bytes.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use proptest::prelude::*;
use rivqen_integrity::PageRevision;
use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_model::http::FieldLine;
use rivqen_proto::fields::{HeadLimitError, ResponseHead};

const R: &str = "r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8";
const LINE: usize = 16 * 1024;
const TOTAL: usize = 64 * 1024;

fn line(name: &str, value_len: usize) -> FieldLine {
    FieldLine::new(name, "v".repeat(value_len))
}

#[test]
fn fields_head_limit_boundaries() {
    // Per line: name 4 + value 8 = 12.
    let l = [line("abcd", 8)];
    assert!(ResponseHead::check(&l, 12, 100).is_ok(), "equal passes");
    assert_eq!(
        ResponseHead::check(&l, 11, 100).unwrap_err(),
        HeadLimitError::LineTooLarge
    );
    // Total: 12 + 12 = 24.
    let two = [line("abcd", 8), line("abcd", 8)];
    assert!(ResponseHead::check(&two, 12, 24).is_ok(), "equal passes");
    assert_eq!(
        ResponseHead::check(&two, 12, 23).unwrap_err(),
        HeadLimitError::TotalTooLarge
    );
    // The real limits, and the empty head.
    let max_line = [line("x", LINE - 1)];
    assert!(ResponseHead::check(&max_line, LINE, TOTAL).is_ok());
    let over_line = [line("x", LINE)];
    assert!(ResponseHead::check(&over_line, LINE, TOTAL).is_err());
    assert!(ResponseHead::check(&[], 0, 0).is_ok());
}

#[test]
fn fields_head_limit_name_bytes_count() {
    // The name counts: value 0, name 5 > limit 4.
    let l = [FieldLine::new("abcde", "")];
    assert!(ResponseHead::check(&l, 5, 5).is_ok());
    assert!(ResponseHead::check(&l, 4, 5).is_err());
}

#[test]
fn fields_head_limit_counts_bytes_not_chars() {
    // "é" is 2 bytes: name 1 + value 2 = 3.
    let l = [FieldLine::new("a", "é")];
    assert!(ResponseHead::check(&l, 3, 3).is_ok());
    assert!(ResponseHead::check(&l, 2, 3).is_err());
}

#[test]
fn fields_head_error_holds_no_field() {
    let secret = "SECRET-TOKEN-1234";
    let lines = [FieldLine::new("x-private-name", secret.repeat(10))];
    let err = ResponseHead::check(&lines, 10, 100).unwrap_err();
    let total = ResponseHead::check(&[lines[0].clone(), lines[0].clone()], 1000, 200).unwrap_err();
    for e in [err, total] {
        for text in [format!("{e}"), format!("{e:?}"), e.reason().to_owned()] {
            assert!(!text.contains("SECRET"), "{text}");
            assert!(!text.contains("x-private-name"), "{text}");
        }
        assert_eq!(e.code(), ErrorCode::DocumentTooLarge);
        assert_eq!(e.code().as_str(), "DOCUMENT_TOO_LARGE");
    }
    assert_ne!(err.reason(), total.reason());
}

#[test]
fn fields_head_oversized_etag_is_rejected() {
    // A 304 whose ETag is over the line limit: the ETag is not dropped (an
    // absent ETag would match the snapshot), the whole head is rejected.
    let etag = format!("\"{}\"", "x".repeat(LINE));
    let lines = [FieldLine::new("ETag", etag)];
    assert_eq!(
        ResponseHead::check(&lines, LINE, TOTAL).unwrap_err(),
        HeadLimitError::LineTooLarge
    );
    // A small matching ETag passes and matches.
    let ok = [FieldLine::new("ETag", format!("\"{R}\""))];
    let head = ResponseHead::check(&ok, LINE, TOTAL).unwrap();
    assert!(head.etag_matches_snapshot(&PageRevision::parse(R).unwrap()));
}

#[test]
fn fields_head_reads_need_check() {
    // The compile-fail doc tests in `fields/response.rs` show that a head
    // cannot be built without `check`. Here: a passing head reads fields.
    let lines = [FieldLine::new("Rq-Version", "1")];
    let head = ResponseHead::check(&lines, LINE, TOTAL).unwrap();
    assert!(head.rq_version_is_1());
}

proptest! {
    #[test]
    fn fields_head_prop_any_lines(
        lines in proptest::collection::vec((".{0,40}", ".{0,200}"), 0..20),
        max_line in 0_usize..400,
        max_total in 0_usize..2000,
    ) {
        let fields: Vec<FieldLine> =
            lines.iter().map(|(n, v)| FieldLine::new(n.clone(), v.clone())).collect();
        let sizes: Vec<usize> = fields.iter().map(|f| f.name.len() + f.value.len()).collect();
        let expect_ok = sizes.iter().all(|&s| s <= max_line) && sizes.iter().sum::<usize>() <= max_total;
        let got = ResponseHead::check(&fields, max_line, max_total);
        prop_assert_eq!(got.is_ok(), expect_ok);
        if let Ok(head) = got {
            // A read on an accepted head never panics.
            std::hint::black_box(head.rq_version_is_1());
            std::hint::black_box(head.media_type());
        }
    }
}
