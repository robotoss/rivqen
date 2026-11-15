// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! HTTP-date parser of the cache (WP-07 S1, T-02).
//! Placeholder target: T-02 writes the invariants.
#![no_main]

use libfuzzer_sys::fuzz_target;
use rivqen_cache::header::parse_http_date;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = core::str::from_utf8(data) else {
        return;
    };
    let _ = parse_http_date(text);
});
