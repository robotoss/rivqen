// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Patch envelope: `parse_envelope` with a fixed client limit and with the
//! manifest `limits` member absent, empty and set.
//! Invariant: `TooLarge` exactly when the body is over `envelope_limit`.
#![no_main]

use libfuzzer_sys::fuzz_target;
use rivqen_proto::envelope::{EnvelopeError, envelope_limit, parse_envelope};
use rivqen_proto::types::Limits;

const MAX_PATCH_BYTES: usize = 65_536;

fuzz_target!(|data: &[u8]| {
    let cases = [
        None,
        Some(Limits {
            max_patch_bytes: None,
        }),
        Some(Limits {
            max_patch_bytes: Some(1_024),
        }),
    ];
    for limits in cases {
        let result = parse_envelope(data, MAX_PATCH_BYTES, limits);
        let over = data.len() > envelope_limit(MAX_PATCH_BYTES, limits);
        assert_eq!(over, matches!(result, Err(EnvelopeError::TooLarge)));
    }
});
