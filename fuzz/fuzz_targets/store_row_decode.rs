// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Placeholder of the target `store_row_decode` (WP-07 S2, T-03): the decoder of
//! `rivqen-store` gets its invariants here.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = data;
});
