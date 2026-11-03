// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Property: no request path maps to a file outside the site directory, and the
//! site never panics, for request paths made of the parts that an attack uses
//! (`..`, `.`, percent-encoding, links that point outside, hidden names) and
//! for any string.

#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

#[path = "../src/site/mod.rs"]
#[allow(dead_code)]
mod site;

use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};
use rivqen_reference::http::handler::Conn;
use rivqen_reference::http::reader::{RequestHead, Version};
use rivqen_reference::model::{Body, Field};
use rivqen_reference::rqp::config::RqpConfig;
use site::{Engine, Site, map_path};

const SECRET: &[u8] = b"SECRET-OUTSIDE";

/// One site for all cases of a test: `<temp>/site` with links that point
/// outside, and `<temp>/outside` with the secret. Removed at the end.
struct Fixture {
    temp: PathBuf,
    dir: PathBuf,
    site: Site,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.temp).ok();
    }
}

fn fixture(tag: &str) -> Fixture {
    {
        let base = fs::canonicalize(std::env::temp_dir()).unwrap();
        let t = base.join(format!("rivqen-site-props-{tag}-{}", std::process::id()));
        if t.exists() {
            fs::remove_dir_all(&t).unwrap();
        }
        let dir = t.join("site");
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::create_dir_all(t.join("outside/sub")).unwrap();
        for p in ["index.html", "a.html", "sub/index.html", "sub/a.txt"] {
            fs::write(dir.join(p), b"inside").unwrap();
        }
        for p in ["secret.html", "index.html", "sub/a.txt", "a.html"] {
            fs::write(t.join("outside").join(p), SECRET).unwrap();
        }
        fs::write(dir.join(".secret.html"), SECRET).unwrap();
        symlink(t.join("outside"), dir.join("out")).unwrap();
        symlink(t.join("outside/secret.html"), dir.join("s.html")).unwrap();
        symlink(t.join("outside/sub"), dir.join("sub/deeper")).unwrap();
        symlink("../outside", dir.join("rel")).unwrap();
        let dir = fs::canonicalize(&dir).unwrap();
        let site = Site::new(
            &dir,
            Engine::Rqp(RqpConfig::default()),
            "https://shop.example.com".to_owned(),
        )
        .unwrap();
        Fixture { temp: t, dir, site }
    }
}

fn run<S: Strategy>(
    tag: &str,
    strategy: &S,
    test: impl Fn(&Fixture, S::Value) -> Result<(), TestCaseError>,
) {
    let f = fixture(tag);
    let config = Config {
        cases: 4000,
        failure_persistence: None,
        ..Config::default()
    };
    let result = TestRunner::new(config).run(strategy, |value| test(&f, value));
    if let Err(e) = result {
        panic!("{e}");
    }
}

/// A loopback connection on port 8080 (the `Host` of `head` names it).
fn local() -> Conn {
    Conn {
        peer: "127.0.0.1".parse().unwrap(),
        local_port: 8080,
    }
}

fn head(target: &str) -> RequestHead {
    RequestHead {
        method: "GET".to_owned(),
        target: target.to_owned(),
        version: Version::Http11,
        fields: vec![Field::new("Host", "127.0.0.1:8080")],
        content_length: 0,
    }
}

fn check(f: &Fixture, target: &str) -> Result<(), TestCaseError> {
    let (dir, site) = (&f.dir, &f.site);
    let o = site.answer(&local(), &head(target));
    if let Body::Bytes(b) = &o.response.body {
        prop_assert!(
            !b.windows(SECRET.len()).any(|w| w == SECRET),
            "{target:?} served the secret"
        );
    }
    let path = target.split('?').next().unwrap_or_default();
    if let Some(mapped) = map_path(path) {
        if let Ok((file, _)) = site.resolve(&mapped) {
            let canonical = fs::canonicalize(&file).unwrap();
            prop_assert!(canonical.starts_with(dir), "{target:?} -> {file:?}");
            prop_assert_eq!(canonical, file);
        } else {
            prop_assert_eq!(o.response.status, 404);
        }
    } else {
        prop_assert_eq!(o.response.status, 404);
    }
    if o.response.status == 200 {
        prop_assert!(map_path(path).is_some());
    }
    Ok(())
}

fn part() -> impl Strategy<Value = &'static str> {
    prop::sample::select(vec![
        "/",
        "/",
        "/",
        "..",
        ".",
        "%2e",
        "%2E",
        "%2f",
        "%5c",
        "\\",
        "a.html",
        "index.html",
        "secret.html",
        ".secret.html",
        "out",
        "s.html",
        "sub",
        "deeper",
        "rel",
        "a.txt",
        "outside",
        "?",
        "#",
        ";",
        "\u{0}",
        "\u{e9}",
        "~",
        "-",
        "_",
        "a",
        "A",
    ])
}

#[test]
fn site_no_request_path_maps_outside() {
    run(
        "parts",
        &prop::collection::vec(part(), 0..12),
        |f, parts| {
            check(f, &parts.concat())?;
            check(f, &format!("/{}", parts.concat()))
        },
    );
}

#[test]
fn site_any_string_is_safe() {
    run("any", &any::<String>(), |f, target| {
        check(f, &target)?;
        check(f, &format!("/{target}"))
    });
}

#[test]
fn site_props_fixture_serves_inside_and_refuses_outside() {
    let f = fixture("fixed");
    let get = |t: &str| f.site.answer(&local(), &head(t)).response.status;
    assert_eq!(get("/a.html"), 200);
    assert_eq!(get("/sub/a.txt"), 200);
    for t in [
        "/out/secret.html",
        "/s.html",
        "/sub/deeper/a.txt",
        "/rel/secret.html",
    ] {
        assert_eq!(get(t), 404, "{t}");
    }
}
