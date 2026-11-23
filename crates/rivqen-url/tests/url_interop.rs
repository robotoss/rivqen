// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The 67 inputs of the URL interop spike against the stored WHATWG results
//! (`tools/platform-spikes/url-interop/urls.json`,
//! `evidence/spikes/wp-04/url/whatwg-node.json`: Node.js v26.8.2, ada 4.0.0).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use rivqen_url::{UrlError, parse};
use serde_json::Value;

/// The inputs whose result is not the stored WHATWG `href` without the
/// fragment, with the expected error and the reason.
const EXCEPTIONS: &[(&str, UrlError, &str)] = &[
    ("host-space", UrlError::Syntax, "WHATWG failure"),
    ("port-too-big", UrlError::Syntax, "WHATWG failure"),
    ("port-non-digit", UrlError::Syntax, "WHATWG failure"),
    (
        "host-idn",
        UrlError::Host,
        "host not ASCII in the input (H-156)",
    ),
    (
        "host-idn-nfc",
        UrlError::Host,
        "host not ASCII in the input (H-156)",
    ),
    (
        "host-idn-nfd",
        UrlError::Host,
        "host not ASCII in the input (H-156)",
    ),
    (
        "host-fullwidth",
        UrlError::Host,
        "host not ASCII in the input (H-156)",
    ),
    ("userinfo", UrlError::UserInfo, "user information (H-68)"),
    (
        "userinfo-user-only",
        UrlError::UserInfo,
        "user information (H-68)",
    ),
    (
        "userinfo-special",
        UrlError::UserInfo,
        "user information (H-68)",
    ),
    (
        "userinfo-empty-pw",
        UrlError::UserInfo,
        "empty user information with an at sign (H-68); WHATWG drops it from href",
    ),
    (
        "rel-dotdot",
        UrlError::Syntax,
        "relative input: parse has no base URL",
    ),
    (
        "rel-authority",
        UrlError::Syntax,
        "relative input: parse has no base URL",
    ),
    (
        "rel-query",
        UrlError::Syntax,
        "relative input: parse has no base URL",
    ),
    (
        "rel-backslash",
        UrlError::Syntax,
        "relative input: parse has no base URL",
    ),
];

fn read(path: &str) -> Value {
    let full = format!("{}/../../{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(&full).unwrap()).unwrap()
}

fn without_fragment(href: &str) -> &str {
    href.split_once('#').map_or(href, |(before, _)| before)
}

#[test]
fn url_interop_table() {
    let inputs = read("tools/platform-spikes/url-interop/urls.json");
    let whatwg = read("evidence/spikes/wp-04/url/whatwg-node.json");
    let inputs = inputs.as_array().unwrap();
    let results = whatwg["results"].as_array().unwrap();
    assert_eq!(inputs.len(), 67);
    assert_eq!(results.len(), 67);

    let mut exceptions_seen = 0;
    let mut equal = 0;
    for entry in inputs {
        let id = entry["id"].as_str().unwrap();
        let input = entry["input"].as_str().unwrap();
        let reference = results.iter().find(|r| r["id"] == id).unwrap();
        let result = parse(input);

        if let Some((_, error, why)) = EXCEPTIONS.iter().find(|(name, _, _)| *name == id) {
            exceptions_seen += 1;
            assert_eq!(result, Err(*error), "{id}: {why}");
            if reference["ok"] == true {
                // The WHATWG result itself is a URL that the parser reads
                // as WHATWG does, unless it still has user information.
                let href = reference["href"].as_str().unwrap();
                if !id.starts_with("userinfo") || id == "userinfo-empty-pw" {
                    let parsed = parse(href).unwrap();
                    assert_eq!(parsed.binding_url(), without_fragment(href), "{id}");
                }
            } else {
                assert!(why.contains("WHATWG failure"), "{id}");
            }
            continue;
        }

        assert_eq!(reference["ok"], true, "{id}: not listed as an exception");
        let href = reference["href"].as_str().unwrap();
        let parsed = result.unwrap_or_else(|error| panic!("{id}: {error:?}"));
        assert_eq!(parsed.binding_url(), without_fragment(href), "{id}");
        assert_eq!(parsed.path(), reference["path"].as_str().unwrap(), "{id}");
        // The host of WHATWG is the host of the origin.
        let host = reference["host"].as_str().unwrap();
        let after_scheme = parsed.origin().split_once("://").unwrap().1;
        assert!(
            after_scheme == host
                || after_scheme
                    .strip_prefix(host)
                    .is_some_and(|port| port.starts_with(':')),
            "{id}: origin {} host {host}",
            parsed.origin()
        );
        equal += 1;
    }
    assert_eq!(exceptions_seen, EXCEPTIONS.len());
    assert_eq!(equal, 67 - EXCEPTIONS.len());
}
