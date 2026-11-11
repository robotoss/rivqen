// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Strict JSON reader: read events to the end or to the first error.
//! Invariants: after an error the reader keeps failing with the same error;
//! the event count is bounded by the input length (linear work).
//! `next_item` and `next_member` are `pub(crate)`, so the target drives the
//! public `Reader::next_event`, which they call.
#![no_main]

use libfuzzer_sys::fuzz_target;
use rivqen_proto::json::Reader;

const MAX_DEPTH: usize = 32;

fuzz_target!(|data: &[u8]| {
    let Ok(mut reader) = Reader::new(data, MAX_DEPTH) else {
        return;
    };
    let mut events = 0usize;
    loop {
        match reader.next_event() {
            Ok(Some(_)) => {
                events += 1;
                // Every event consumes at least one input byte.
                assert!(events <= data.len());
            }
            Ok(None) => {
                assert!(reader.finish().is_ok());
                return;
            }
            Err(first) => {
                assert_eq!(reader.next_event().err(), Some(first));
                return;
            }
        }
    }
});
