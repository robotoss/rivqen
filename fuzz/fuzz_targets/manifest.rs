// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Manifest JSON: `parse_manifest` on any bytes (no panic, no abort).
#![no_main]

use libfuzzer_sys::fuzz_target;
use rivqen_proto::manifest::parse_manifest;

fuzz_target!(|data: &[u8]| {
    // Only a crash is a finding; the result of a parse is not.
    drop(parse_manifest(data));
});
