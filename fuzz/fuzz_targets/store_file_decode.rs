// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The decoder of the entry file plain text of `rivqen-store` (WP-07 S2,
//! T-03; ADR-011 section 3).
//!
//! Invariants beyond "no panic":
//! - a decoded value encodes to the same bytes (one encoding for each
//!   value), with the default limits and with small limits;
//! - a value that decodes with the small limits also decodes with the
//!   default limits, to the same value;
//! - a decoded value is not larger than the input.
#![no_main]

use libfuzzer_sys::fuzz_target;
use rivqen_store::crypto::{DecodeLimits, decode_file};

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

fuzz_target!(|data: &[u8]| {
    let default = decode_file(data, &DecodeLimits::default());
    if let Ok(content) = &default {
        assert_eq!(content.encode(), data);
        let mut size = content.page.len() + content.template.as_ref().map_or(0, |t| t.len());
        for (key, value) in &content.data {
            size += key.len() + value.len();
        }
        assert!(size <= data.len());
    }
    if let Ok(content) = decode_file(data, &small()) {
        assert_eq!(content.encode(), data);
        assert_eq!(default.as_ref().ok(), Some(&content));
    }
});
