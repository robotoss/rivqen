// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Known answers of `rivqen-url` (WP-08 S1 T-04): the binding URL U, the
//! errors, the origin and the path, and the canonical URL of the cache key.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use rivqen_url::{MAX_URL_BYTES, ParsedUrl, Scheme, UrlError, parse};

fn ok(input: &str) -> ParsedUrl {
    parse(input).unwrap_or_else(|error| panic!("{input:?}: {error:?}"))
}

fn binding(input: &str) -> String {
    ok(input).binding_url().to_owned()
}

// --- U (manifest and patch section 2.2) ---------------------------------

#[test]
fn url_binding_known_answers() {
    // `kat.json` member `binding_with_query`, read from the file.
    let kat: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../spec/rqp/examples/kat/kat.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let entry = &kat["binding_with_query"];
    let input = entry["url_input"].as_str().unwrap();
    let expected = entry["url"].as_str().unwrap();
    assert_eq!(
        input,
        "https://SHOP.Example.com:443/catalog?sort=price&page=2#reviews"
    );
    assert_eq!(binding(input), expected);
    // Every other URL of the known-answer set is its own U.
    for binding_entry in kat["bindings"].as_array().unwrap() {
        let url = binding_entry["url"].as_str().unwrap();
        assert_eq!(binding(url), url);
    }

    // The measurement of the sprint record (section 1).
    assert_eq!(
        binding("HTTPS://EXAMPLE.com:443/a/../b?x#f"),
        "https://example.com/b?x"
    );
    // No percent-encoding normalization in U (only the cache URL has it).
    assert_eq!(
        binding("https://a.example/%7e%41/%c3%a9?q=%2d%7E"),
        "https://a.example/%7e%41/%c3%a9?q=%2d%7E"
    );
    // The path and the query as the WHATWG parser writes them.
    assert_eq!(
        binding("https://a.example/a b?c d'e"),
        "https://a.example/a%20b?c%20d%27e"
    );
    assert_eq!(
        binding("https://a.example/é?é"),
        "https://a.example/%C3%A9?%C3%A9"
    );
    // Default ports.
    assert_eq!(binding("http://a.example:80/"), "http://a.example/");
    assert_eq!(binding("https://a.example:443/"), "https://a.example/");
    assert_eq!(binding("https://a.example:80/"), "https://a.example:80/");
    assert_eq!(binding("http://a.example:443/"), "http://a.example:443/");
    // No fragment, also an empty one.
    assert_eq!(binding("https://a.example/p#"), "https://a.example/p");
    assert_eq!(binding("https://a.example/p?#x"), "https://a.example/p?");
}

#[test]
fn url_binding_scheme() {
    assert_eq!(ok("http://a.example/").scheme(), Scheme::Http);
    assert_eq!(ok("HtTpS://a.example/").scheme(), Scheme::Https);
}

// --- Errors ---------------------------------------------------------------

#[test]
fn url_errors_too_long_counts_bytes() {
    let prefix = "https://a.example/";
    let at_limit = format!("{prefix}{}", "a".repeat(MAX_URL_BYTES - prefix.len()));
    assert_eq!(at_limit.len(), MAX_URL_BYTES);
    assert_eq!(ok(&at_limit).binding_url(), at_limit);
    // One byte more: rejected before the parse, whatever the text is.
    let over = format!("{at_limit}a");
    assert_eq!(parse(&over), Err(UrlError::TooLong));
    let over_wide = format!("https://bücher.example/{}", "a".repeat(MAX_URL_BYTES));
    assert_eq!(parse(&over_wide), Err(UrlError::TooLong));
    // A multi-byte character counts with its UTF-8 bytes.
    let wide_path = format!(
        "{prefix}{}",
        "é".repeat((MAX_URL_BYTES - prefix.len()) / 2 + 1)
    );
    assert!(wide_path.chars().count() < MAX_URL_BYTES);
    assert_eq!(parse(&wide_path), Err(UrlError::TooLong));
    assert_eq!(
        parse(&" ".repeat(MAX_URL_BYTES + 1)),
        Err(UrlError::TooLong)
    );
}

#[test]
fn url_errors_result_over_the_limit() {
    // The limit applies to the input. U can be up to 3 times longer (each
    // byte over 0x7F becomes `%XX`), and the cache URL up to 3 times U (a `%`
    // that starts no escape becomes `%25`). A parse of such a text gives
    // `TooLong`.
    let prefix = "https://a.example/";
    let input = format!("{prefix}{}", "é".repeat((MAX_URL_BYTES - prefix.len()) / 2));
    assert!(input.len() <= MAX_URL_BYTES);
    let parsed = ok(&input);
    assert_eq!(
        parsed.binding_url().len(),
        prefix.len() + 3 * (input.len() - prefix.len())
    );
    assert_eq!(parse(parsed.binding_url()), Err(UrlError::TooLong));

    let input = format!("{prefix}{}", "%".repeat(MAX_URL_BYTES - prefix.len()));
    let parsed = ok(&input);
    assert_eq!(parsed.binding_url(), input);
    assert_eq!(
        parsed.cache_url(&[]).len(),
        prefix.len() + 3 * (input.len() - prefix.len())
    );
}

#[test]
fn url_errors_scheme() {
    for input in [
        "ftp://a.example/",
        "ws://a.example/",
        "wss://a.example/",
        "file:///etc/passwd",
        "mailto:a@a.example",
        "javascript:alert(1)",
        "data:text/html,x",
        "blob:https://a.example/0",
        "about:blank",
        "httpx://a.example/",
        "ftp://bücher.example/",
    ] {
        assert_eq!(parse(input), Err(UrlError::Scheme), "{input}");
    }
}

#[test]
fn url_errors_syntax() {
    for input in [
        "",
        "   ",
        "a.example/p",
        "/p",
        "//a.example/p",
        "?q=1",
        "https://a b.example/",
        "https://a.example:65536/",
        "https://a.example:44x/",
        "https://[::1/",
        "https://[::g]/",
        "https://0x100000000/",
        "https://a<b/",
        "https://a%2fb/",
        "https://a%25/",
        "https://xn--example-.org/",
        "https://xn--/",
    ] {
        assert_eq!(parse(input), Err(UrlError::Syntax), "{input:?}");
    }
}

#[test]
fn url_errors_userinfo() {
    for input in [
        "https://user:pw@a.example/p",
        "https://user@a.example/p",
        "https://:pw@a.example/p",
        "https://user:@a.example/p",
        "https://:@a.example/p",
        "https://@a.example/p",
        "https://us er:p@ss@a.example/p",
        "https://a@b@c.example/",
        "https:user@a.example/",
        "https:\\\\user@a.example/",
        "https://\t@a.example/",
        "https://bücher@a.example/",
        "HTTP://u@a.example:80/",
    ] {
        assert_eq!(parse(input), Err(UrlError::UserInfo), "{input:?}");
    }
    // An at sign after the authority is path or query text.
    assert_eq!(binding("https://a.example/@b"), "https://a.example/@b");
    assert_eq!(binding("https://a.example?@b"), "https://a.example/?@b");
    assert_eq!(binding("https://a.example#@b"), "https://a.example/");
}

#[test]
fn url_errors_host_absent() {
    for input in [
        "https://",
        "https:",
        "https:///",
        "https://:443/",
        "https://?q",
        "https://#f",
        "http://\\",
    ] {
        assert_eq!(parse(input), Err(UrlError::Host), "{input:?}");
    }
}

#[test]
fn url_errors_host_not_ascii() {
    for input in [
        "https://bücher.example/",
        "https://faß.example/catalog",
        "https://ａ.example/",
        "https://a。example/",
        "https://a.example\u{a0}/",
        "https://b%C3%BCcher.example/",
        "https://a.example%C2%A0/",
        // An upper-case ASCII letter before a byte over 0x7F: `idna_adapter`
        // 1.0.0 has a `debug_assert!` on this form (the module `scan`).
        "https://Aé.example/",
        "https://A%C3%A9.example/",
        "https://A%c3%a9.example:8080/",
        "https://\u{1F600}.example/",
        "https://a.example:443\u{e9}/",
    ] {
        assert_eq!(parse(input), Err(UrlError::Host), "{input:?}");
    }
}

#[test]
fn url_errors_punycode_label_passes() {
    assert_eq!(
        binding("https://xn--fa-hia.example/catalog"),
        "https://xn--fa-hia.example/catalog"
    );
    assert_eq!(
        binding("https://XN--BCHER-KVA.Example/"),
        "https://xn--bcher-kva.example/"
    );
    assert_eq!(
        ok("https://xn--bcher-kva.example/").origin(),
        "https://xn--bcher-kva.example"
    );
}

#[test]
fn url_errors_have_no_input_bytes() {
    let secret = "s3cr3t-token";
    for input in [
        format!("https://u:{secret}@a.example/"),
        format!("ftp://a.example/?{secret}"),
        format!("https://a b/?{secret}"),
        format!("https://bücher.example/?{secret}"),
        format!("https://a.example/?{secret}{}", "a".repeat(MAX_URL_BYTES)),
    ] {
        let error = parse(&input).unwrap_err();
        assert!(!error.to_string().contains(secret));
        assert!(!format!("{error:?}").contains(secret));
    }
    let parsed = ok(&format!("https://a.example/p?token={secret}#{secret}"));
    let debug = format!("{parsed:?}");
    assert!(
        !debug.contains(secret) && !debug.contains("a.example"),
        "{debug}"
    );
}

// --- Origin and path (manifest and patch section 1.6) ----------------------

#[test]
fn url_origin_path_default_port() {
    let parsed = ok("HTTPS://Shop.Example.COM:443/Catalog/Item?x=1#f");
    assert_eq!(parsed.origin(), "https://shop.example.com");
    assert_eq!(parsed.path(), "/Catalog/Item");
    let parsed = ok("http://a.example:80");
    assert_eq!(parsed.origin(), "http://a.example");
    assert_eq!(parsed.path(), "/");
}

#[test]
fn url_origin_path_other_port() {
    assert_eq!(
        ok("https://a.example:8443/p").origin(),
        "https://a.example:8443"
    );
    assert_eq!(
        ok("https://a.example:80/p").origin(),
        "https://a.example:80"
    );
    assert_eq!(
        ok("http://a.example:443/p").origin(),
        "http://a.example:443"
    );
    assert_eq!(ok("https://a.example:0443/p").origin(), "https://a.example");
    assert_eq!(ok("https://a.example:/p").origin(), "https://a.example");
    assert_eq!(ok("https://a.example:0/p").origin(), "https://a.example:0");
}

#[test]
fn url_origin_path_ip_hosts() {
    let parsed = ok("https://[2001:DB8:0:0:0:0:0:1]:8443/p?q");
    assert_eq!(parsed.origin(), "https://[2001:db8::1]:8443");
    assert_eq!(parsed.path(), "/p");
    assert_eq!(ok("http://[::1]:80/").origin(), "http://[::1]");
    assert_eq!(
        ok("https://[::ffff:192.0.2.1]/").origin(),
        "https://[::ffff:c000:201]"
    );
    assert_eq!(ok("https://127.1/").origin(), "https://127.0.0.1");
    assert_eq!(
        ok("https://0x7f.0.0.1:8080/").origin(),
        "https://127.0.0.1:8080"
    );
}

#[test]
fn url_origin_path_forms() {
    for (input, path) in [
        ("https://a.example", "/"),
        ("https://a.example?q", "/"),
        ("https://a.example/a/./b/../c", "/a/c"),
        ("https://a.example/%2e%2E/x", "/x"),
        ("https://a.example//a//b/", "//a//b/"),
        (
            "https://a.example/a b/\"<>`{}|^[]",
            "/a%20b/%22%3C%3E%60%7B%7D|%5E[]",
        ),
        ("https://a.example/p\\q", "/p/q"),
        ("https://a.example/p?q/r", "/p"),
        ("https://a.example/p?", "/p"),
    ] {
        let parsed = ok(input);
        assert_eq!(parsed.path(), path, "{input}");
        // U is origin, path and the query.
        let rest = &parsed.binding_url()[parsed.origin().len()..];
        assert!(rest.starts_with(parsed.path()), "{input}");
        assert!(
            rest[parsed.path().len()..].is_empty() || rest[parsed.path().len()..].starts_with('?')
        );
    }
}

#[test]
fn url_origin_path_equals_url_origin() {
    // The serialized origin of the WHATWG parser (`Origin::ascii_serialization`).
    for input in [
        "https://a.example/",
        "HTTP://A.EXAMPLE:80/x",
        "https://a.example:8443/",
        "https://[2001:db8::1]:444/",
        "https://127.1/",
        "https://xn--fa-hia.example./",
        "https://a_b.example/",
    ] {
        let expected = url::Url::parse(input)
            .unwrap()
            .origin()
            .ascii_serialization();
        assert_eq!(ok(input).origin(), expected, "{input}");
    }
}

#[test]
fn url_backslash_host() {
    let parsed = ok("https://a.example\\@b.example/p");
    assert_eq!(parsed.origin(), "https://a.example");
    assert_eq!(parsed.path(), "/@b.example/p");
    assert_eq!(parsed.binding_url(), "https://a.example/@b.example/p");
    // The two allowlist rows of cache identity section 4.
    assert_eq!(
        ok("https://attacker.example\\@shop.example.com/x").origin(),
        "https://attacker.example"
    );
    assert_eq!(
        ok("https://shop.example.com\\@attacker.example/x").origin(),
        "https://shop.example.com"
    );
    // Backslashes as the authority slashes.
    assert_eq!(
        ok("https:\\\\a.example\\p").binding_url(),
        "https://a.example/p"
    );
}

// --- The canonical URL of the cache key (cache identity section 2) --------

#[test]
fn url_cache_unreserved_decoded() {
    let parsed = ok("https://a.example/%7e%41%5A%7a%30%39%2d%2E%5f/x?q=%7E%2d&%61=1");
    assert_eq!(
        parsed.cache_url(&[]),
        "https://a.example/~AZz09-._/x?q=~-&a=1"
    );
    // The neighbours of the unreserved ranges stay encoded.
    let parsed = ok("https://a.example/%40%5B%60%7B%2F%3A%2C%7F%00");
    assert_eq!(
        parsed.cache_url(&[]),
        "https://a.example/%40%5B%60%7B%2F%3A%2C%7F%00"
    );
}

#[test]
fn url_cache_hex_upper_case() {
    let parsed = ok("https://a.example/%c3%a9/%2f?q=%c3%a9%3d%26%25");
    assert_eq!(
        parsed.cache_url(&[]),
        "https://a.example/%C3%A9/%2F?q=%C3%A9%3D%26%25"
    );
}

#[test]
fn url_cache_percent_without_escape() {
    // A `%` that starts no escape is the text `%`: it becomes `%25`.
    let parsed = ok("https://a.example/a%zzb/%?q=%zz%&r=%4");
    assert_eq!(
        parsed.cache_url(&[]),
        "https://a.example/a%25zzb/%25?q=%25zz%25&r=%254"
    );
    // So a decoded character never forms a new escape: `%2%46` (the text
    // `%2F`) and `%2F` (the byte `/`) keep two keys.
    let text = ok("https://a.example/a%2%46b?x=%2%46").cache_url(&[]);
    let byte = ok("https://a.example/a%2Fb?x=%2F").cache_url(&[]);
    assert_eq!(text, "https://a.example/a%252Fb?x=%252F");
    assert_eq!(byte, "https://a.example/a%2Fb?x=%2F");
    // The text `%` and its escape decode to the same byte: one key.
    assert_eq!(
        ok("https://a.example/%?%").cache_url(&[]),
        ok("https://a.example/%25?%25").cache_url(&[])
    );
}

#[test]
fn url_cache_steps_1_to_3() {
    let parsed = ok("HTTPS://A.Example:443/p?b=2&a=1#frag");
    assert_eq!(parsed.cache_url(&[]), "https://a.example/p?b=2&a=1");
}

#[test]
fn url_cache_parameter_order_kept() {
    let parsed = ok("https://a.example/p?b=2&a=1&b=1&&c");
    assert_eq!(parsed.cache_url(&[]), "https://a.example/p?b=2&a=1&b=1&&c");
    assert_eq!(
        parsed.cache_url(&["x"]),
        "https://a.example/p?b=2&a=1&b=1&&c"
    );
}

#[test]
fn url_cache_ignored_removed_by_exact_name() {
    let parsed = ok(
        "https://a.example/p?a=1&utm_source=x&b=2&utm_source_x=3&UTM_SOURCE=4&utm_source&x=utm_source&utm_source=",
    );
    assert_eq!(
        parsed.cache_url(&["utm_source"]),
        "https://a.example/p?a=1&b=2&utm_source_x=3&UTM_SOURCE=4&x=utm_source"
    );
    assert_eq!(
        parsed.cache_url(&["utm_source", "a", "b"]),
        "https://a.example/p?utm_source_x=3&UTM_SOURCE=4&x=utm_source"
    );
    // The path never loses a part.
    let parsed = ok("https://a.example/utm_source=1?utm_source=1&k=v");
    assert_eq!(
        parsed.cache_url(&["utm_source"]),
        "https://a.example/utm_source=1?k=v"
    );
}

#[test]
fn url_cache_name_after_step_4() {
    let parsed = ok("https://a.example/p?utm%5Fsource=1&a%3Db=2&k=v");
    // `%5F` is `_` (unreserved); `%3D` is `=` and stays encoded.
    assert_eq!(
        parsed.cache_url(&["utm_source", "a=b", "a"]),
        "https://a.example/p?a%3Db=2&k=v"
    );
    assert_eq!(
        parsed.cache_url(&["a%3Db"]),
        "https://a.example/p?utm_source=1&k=v"
    );
}

#[test]
fn url_cache_all_removed_drops_the_query() {
    let with_tracking = ok("https://a.example/p?utm_source=1&fbclid=2");
    let without = ok("https://a.example/p");
    let ignored = ["utm_source", "fbclid"];
    assert_eq!(with_tracking.cache_url(&ignored), "https://a.example/p");
    assert_eq!(
        with_tracking.cache_url(&ignored),
        without.cache_url(&ignored)
    );
    // An empty query of the URL stays when nothing is removed.
    assert_eq!(
        ok("https://a.example/p?").cache_url(&ignored),
        "https://a.example/p?"
    );
    assert_eq!(
        ok("https://a.example/p?").cache_url(&[]),
        "https://a.example/p?"
    );
}

#[test]
fn url_cache_empty_ignored_name_removes_nothing() {
    let parsed = ok("https://a.example/p?=1&&a");
    assert_eq!(parsed.cache_url(&[""]), "https://a.example/p?=1&&a");
}

#[test]
fn url_cache_binding_unchanged() {
    let parsed = ok("https://a.example/%7e?utm_source=1");
    assert_eq!(parsed.cache_url(&["utm_source"]), "https://a.example/~");
    assert_eq!(parsed.binding_url(), "https://a.example/%7e?utm_source=1");
}
