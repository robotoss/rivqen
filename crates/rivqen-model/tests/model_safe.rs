// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! Secret-safe wrapper tests.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use rivqen_model::safe::{PageBytes, SafeUrl, Scheme, Secret};

#[test]
fn model_safe_secret_debug_is_redacted_and_value_is_reachable() {
    let s = Secret::new("tok-123".to_owned());
    assert_eq!(format!("{s:?}"), "Secret(<redacted>)");
    assert_eq!(s.expose(), "tok-123");
    assert_eq!(s.into_inner(), "tok-123");
}

#[test]
fn model_safe_url_shows_origin_and_path_only() {
    let u = SafeUrl::new(Scheme::Https, "Shop.Example.com", Some(8443), "/catalog/1")
        .unwrap()
        .with_query(Secret::new("token=abc".to_owned()))
        .unwrap();
    assert_eq!(format!("{u}"), "https://shop.example.com:8443/catalog/1");
    assert_eq!(format!("{u:?}"), "https://shop.example.com:8443/catalog/1");
    assert_eq!(u.origin(), "https://shop.example.com:8443");
    assert_eq!(u.query().unwrap().expose(), "token=abc");
}

#[test]
fn model_safe_url_without_port_has_plain_origin() {
    let u = SafeUrl::new(Scheme::Http, "a.test", None, "/").unwrap();
    assert_eq!(u.origin(), "http://a.test");
    assert_eq!(u.scheme(), Scheme::Http);
    assert_eq!(u.host(), "a.test");
    assert_eq!(u.port(), None);
    assert_eq!(u.path(), "/");
    assert!(u.query().is_none());
}

#[test]
fn model_safe_url_refuses_bad_parts() {
    let bad_hosts = ["", "a b", "u@host", "host/x", "h:1", "h?x"];
    for h in bad_hosts {
        assert!(SafeUrl::new(Scheme::Https, h, None, "/").is_err(), "{h:?}");
    }
    let bad_paths = ["", "x", "/a?b", "/a#b", "/a b", "/a\n", "/a\u{7f}"];
    for p in bad_paths {
        assert!(
            SafeUrl::new(Scheme::Https, "a.test", None, p).is_err(),
            "{p:?}"
        );
    }
    let u = SafeUrl::new(Scheme::Https, "a.test", None, "/").unwrap();
    for q in ["a#b", "a b", "a\r"] {
        assert!(
            u.clone().with_query(Secret::new(q.to_owned())).is_err(),
            "{q:?}"
        );
    }
}

#[test]
fn model_safe_page_bytes_debug_shows_length_only() {
    let p = PageBytes::new(b"<html>secret</html>".to_vec());
    assert_eq!(format!("{p:?}"), "PageBytes(len=19)");
    assert_eq!(p.len(), 19);
    assert!(!p.is_empty());
    assert_eq!(p.as_slice(), b"<html>secret</html>");
    assert_eq!(p.into_vec().len(), 19);
    assert!(PageBytes::new(Vec::new()).is_empty());
}

#[test]
fn model_safe_url_port_is_the_explicit_port_only() {
    let with = SafeUrl::new(Scheme::Https, "a.test", Some(8443), "/").unwrap();
    assert_eq!(with.port(), Some(8443));
    // A default port is kept as given; there is no default-port rule.
    let default = SafeUrl::new(Scheme::Https, "a.test", Some(443), "/").unwrap();
    assert_eq!(default.port(), Some(443));
    let without = SafeUrl::new(Scheme::Https, "a.test", None, "/").unwrap();
    assert_eq!(without.port(), None);
}
