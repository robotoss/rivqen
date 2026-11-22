// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The decoder of the row plain text of `rivqen-store` (WP-07 S2, T-03;
//! ADR-004 section 3, ADR-011 section 3).
//!
//! Invariants beyond "no panic":
//! - a decoded value encodes to the same bytes (one encoding for each
//!   value), with the default limits and with small limits;
//! - a value that decodes with the small limits also decodes with the
//!   default limits, to the same value;
//! - the page fields and the `Vary` names keep the limits.
#![no_main]

use libfuzzer_sys::fuzz_target;
use rivqen_store::crypto::{DecodeLimits, RowContent, decode_row};

fn small() -> DecodeLimits {
    DecodeLimits {
        max_document_bytes: 64,
        max_blocks: 2,
        max_block_key_bytes: 4,
        max_block_bytes: 16,
        max_header_bytes: 16,
        max_headers_total_bytes: 40,
    }
}

fn check_limits(row: &RowContent, limits: &DecodeLimits) {
    let header = usize::try_from(limits.max_header_bytes).unwrap_or(usize::MAX);
    let total = usize::try_from(limits.max_headers_total_bytes).unwrap_or(usize::MAX);
    let mut lines = 0;
    for line in &row.fields {
        assert!(line.name.len() + line.value.len() <= header);
        lines += line.name.len() + line.value.len();
    }
    assert!(lines <= total);
    let names: usize = row.vary.iter().map(|entry| entry.name.len()).sum();
    assert!(names <= total);
    assert!(row.vary.iter().all(|entry| entry.name.len() <= header));
}

fuzz_target!(|data: &[u8]| {
    let default = decode_row(data, &DecodeLimits::default());
    if let Ok(row) = &default {
        assert_eq!(row.encode(), data);
        check_limits(row, &DecodeLimits::default());
    }
    let small = small();
    if let Ok(row) = decode_row(data, &small) {
        assert_eq!(row.encode(), data);
        check_limits(&row, &small);
        assert_eq!(default.as_ref().ok(), Some(&row));
    }
});
