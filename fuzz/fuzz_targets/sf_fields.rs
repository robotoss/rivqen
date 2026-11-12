// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Structured Fields: the three parse functions on the same text.
//! Invariant: a value that parses serializes, and the text parses again to the same value.
#![no_main]

use libfuzzer_sys::fuzz_target;
use rivqen_proto::sf::{
    parse_dictionary, parse_item, parse_list, serialize_dictionary, serialize_item, serialize_list,
};

fuzz_target!(|data: &[u8]| {
    let Ok(text) = core::str::from_utf8(data) else {
        return;
    };
    if let Ok(item) = parse_item(text) {
        let out = serialize_item(&item).expect("a parsed item serializes");
        assert_eq!(parse_item(&out), Ok(item));
    }
    if let Ok(list) = parse_list(text) {
        let out = serialize_list(&list).expect("a parsed list serializes");
        assert_eq!(parse_list(&out), Ok(list));
    }
    if let Ok(dictionary) = parse_dictionary(text) {
        let out = serialize_dictionary(&dictionary).expect("a parsed dictionary serializes");
        assert_eq!(parse_dictionary(&out), Ok(dictionary));
    }
});
