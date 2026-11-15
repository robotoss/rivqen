// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! `parse_cache_offline` (WP-07 S1 T-02, feature `legacy`): legacy wire
//! section 7, CL-15 (D-30).

#![cfg(feature = "legacy")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rivqen_cache::header::{CacheOffline, parse_cache_offline};

#[test]
fn header_cache_offline_four_values() {
    assert_eq!(parse_cache_offline(Some("true")), CacheOffline::True);
    assert_eq!(parse_cache_offline(Some("false")), CacheOffline::False);
    assert_eq!(parse_cache_offline(Some("store")), CacheOffline::Store);
    assert_eq!(parse_cache_offline(Some("http")), CacheOffline::Http);
}

#[test]
fn header_cache_offline_ascii_lower_case_first() {
    for value in ["TRUE", "True", "tRUE"] {
        assert_eq!(
            parse_cache_offline(Some(value)),
            CacheOffline::True,
            "{value}"
        );
    }
    for value in ["STORE", "Store", "stORE"] {
        assert_eq!(
            parse_cache_offline(Some(value)),
            CacheOffline::Store,
            "{value}"
        );
    }
    for value in ["HTTP", "Http", "hTtP"] {
        assert_eq!(
            parse_cache_offline(Some(value)),
            CacheOffline::Http,
            "{value}"
        );
    }
    assert_eq!(parse_cache_offline(Some("FALSE")), CacheOffline::False);
    // The seed `fixtures/malformed/seeds/fields/cache-offline-case.txt`.
    let seed = include_str!("../../../fixtures/malformed/seeds/fields/cache-offline-case.txt");
    assert_eq!(parse_cache_offline(Some(seed)), CacheOffline::True);
}

#[test]
fn header_cache_offline_absent_or_other_value_gives_false() {
    assert_eq!(parse_cache_offline(None), CacheOffline::False);
    for value in [
        "",
        "1",
        "0",
        "yes",
        "tru",
        "truex",
        "stor",
        "https",
        " true",
        "true ",
        "\"true\"",
        "true, store",
        "\u{FF54}rue",
        "h\u{0442}tp",
    ] {
        assert_eq!(
            parse_cache_offline(Some(value)),
            CacheOffline::False,
            "{value:?}"
        );
    }
}
