// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! Tests of `rivqen_testkit::rqp_client` (WP-08 S1, T-06).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use rivqen_proto::types::{BlockFormat, Limits, Mode, Policy};
use rivqen_testkit::fixtures::{BlockData, LimitsMembers, Members, PolicyMembers};
use rivqen_testkit::rqp_client::{
    ClientFixtureError, b64u_decode, canonical_url, client_fixture, descriptor, ledger, members,
    nonce, origin_and_path, protocol, snapshot_meta,
};
use rivqen_url::UrlError;

/// The client fixtures whose URL `rivqen_url::parse` rejects, with the
/// variant. `FX-RQ-C25-IDN-HOST` has the host `faß.example` (H-156: the
/// product parser takes the ASCII form that the platform URL object gives).
/// Its snapshot URL `https://xn--fa-hia.example/catalog` is also rejected
/// (DL-225).
/// The USERINFO fixtures carry user information (H-68). All five are
/// session-outcome fixtures of S3; no check of S1 passes their URL to the
/// helpers.
const REJECTED: [(&str, UrlError); 5] = [
    ("FX-RQ-C25-IDN-HOST", UrlError::Host),
    ("FX-RQ-C25-USERINFO-FINAL", UrlError::UserInfo),
    ("FX-RQ-C25-USERINFO-FINAL-304", UrlError::UserInfo),
    ("FX-RQ-C25-USERINFO-FINAL-DELTA", UrlError::UserInfo),
    ("FX-RQ-C25-USERINFO-REQUEST", UrlError::UserInfo),
];

#[test]
fn rqp_client_fixture_urls_parse_except_the_named_ones() {
    let mut rejected = Vec::new();
    for fixture in &protocol().unwrap().client {
        let Ok(context) = fixture.client_context_data() else {
            continue;
        };
        for url in [&context.url, &context.final_url] {
            if let Err(error) = canonical_url(url) {
                assert_eq!(error, ClientFixtureError::Url(error_url(&error)));
                rejected.push((fixture.id.clone(), error_url(&error)));
            }
        }
    }
    rejected.sort_by(|a, b| a.0.cmp(&b.0));
    rejected.dedup();
    let want: Vec<(String, UrlError)> = REJECTED
        .iter()
        .map(|(id, e)| ((*id).to_owned(), *e))
        .collect();
    assert_eq!(rejected, want);
}

fn error_url(error: &ClientFixtureError) -> UrlError {
    match error {
        ClientFixtureError::Url(e) => *e,
        other => panic!("not a URL error: {other}"),
    }
}

#[test]
fn rqp_client_canonical_url_lowercases_the_host_and_drops_default_port_and_fragment() {
    assert_eq!(
        canonical_url("http://Example.COM:80?q=1#f").unwrap(),
        "http://example.com/?q=1"
    );
    assert_eq!(
        canonical_url("https://example.com:8443/a").unwrap(),
        "https://example.com:8443/a"
    );
}

#[test]
fn rqp_client_url_that_the_parser_rejects_is_a_typed_error() {
    assert_eq!(
        canonical_url("https://faß.example/catalog"),
        Err(ClientFixtureError::Url(UrlError::Host))
    );
    assert_eq!(
        origin_and_path("ftp://example.com/"),
        Err(ClientFixtureError::Url(UrlError::Scheme))
    );
}

#[test]
fn rqp_client_origin_and_path_split_the_final_url() {
    assert_eq!(
        origin_and_path("https://Shop.Example.com:443/catalog?x=1").unwrap(),
        ("https://shop.example.com".to_owned(), "/catalog".to_owned())
    );
}

#[test]
fn rqp_client_ledger_lists_exactly_the_client_fixtures() {
    let ledger = ledger().unwrap();
    assert_eq!(ledger.entries().count(), protocol().unwrap().client.len());
}

#[test]
fn rqp_client_fixture_by_id_and_missing_id() {
    let first = &protocol().unwrap().client[0];
    assert_eq!(client_fixture(&first.id).unwrap().id, first.id);
    assert_eq!(
        client_fixture("FX-RQ-NO-SUCH").unwrap_err(),
        ClientFixtureError::NoFixture("FX-RQ-NO-SUCH".to_owned())
    );
}

#[test]
fn rqp_client_snapshot_meta_keeps_every_block_and_revision() {
    let mut checked = 0;
    for fixture in &protocol().unwrap().client {
        let Ok(context) = fixture.client_context_data() else {
            continue;
        };
        let Some(data) = &context.snapshot else {
            continue;
        };
        let meta = snapshot_meta(data).unwrap();
        assert_eq!(meta.blocks.len(), data.blocks.len(), "{}", fixture.id);
        assert_eq!(meta.page_revision.to_string(), data.page_revision);
        assert_eq!(meta.template_revision.to_string(), data.template_revision);
        checked += 1;
    }
    assert!(checked > 0);
}

#[test]
fn rqp_client_members_keep_presence_and_values() {
    let (policy, limits) = members(&Members::default()).unwrap();
    assert_eq!((policy, limits), (None, None));
    let all = Members {
        policy: Some(PolicyMembers {
            mode: Some("public".to_owned()),
            ttl_seconds: Some(60),
            offline: Some(true),
        }),
        limits: Some(LimitsMembers {
            max_patch_bytes: Some(1000),
        }),
    };
    let (policy, limits) = members(&all).unwrap();
    assert_eq!(
        policy,
        Some(Policy {
            mode: Some(Mode::Public),
            ttl_seconds: Some(60),
            offline: Some(true)
        })
    );
    assert_eq!(
        limits,
        Some(Limits {
            max_patch_bytes: Some(1000)
        })
    );
    let empty = Members {
        policy: Some(PolicyMembers::default()),
        limits: Some(LimitsMembers::default()),
    };
    let (policy, limits) = members(&empty).unwrap();
    assert_eq!(
        policy,
        Some(Policy {
            mode: None,
            ttl_seconds: None,
            offline: None
        })
    );
    assert_eq!(
        limits,
        Some(Limits {
            max_patch_bytes: None
        })
    );
}

#[test]
fn rqp_client_members_reject_a_mode_and_a_number_out_of_range() {
    let mode = Members {
        policy: Some(PolicyMembers {
            mode: Some("shared".to_owned()),
            ..PolicyMembers::default()
        }),
        limits: None,
    };
    assert!(matches!(
        members(&mode),
        Err(ClientFixtureError::Value {
            member: "policy.mode",
            ..
        })
    ));
    let ttl = Members {
        policy: Some(PolicyMembers {
            ttl_seconds: Some(u64::from(u32::MAX) + 1),
            ..PolicyMembers::default()
        }),
        limits: None,
    };
    assert!(matches!(
        members(&ttl),
        Err(ClientFixtureError::Value {
            member: "policy.ttl_seconds",
            ..
        })
    ));
    let patch = Members {
        policy: None,
        limits: Some(LimitsMembers {
            max_patch_bytes: Some(u64::from(u32::MAX) + 1),
        }),
    };
    assert!(matches!(
        members(&patch),
        Err(ClientFixtureError::Value {
            member: "limits.max_patch_bytes",
            ..
        })
    ));
}

#[test]
fn rqp_client_descriptor_reads_a_block_and_rejects_a_format() {
    let hash = "A".repeat(43);
    let block = BlockData {
        id: "main".to_owned(),
        format: "json".to_owned(),
        sha256: hash.clone(),
    };
    let d = descriptor(&block).unwrap();
    assert_eq!(d.format, BlockFormat::Json);
    assert_eq!(d.id.to_string(), "main");
    let bad = BlockData {
        format: "xml".to_owned(),
        ..block
    };
    assert!(matches!(
        descriptor(&bad),
        Err(ClientFixtureError::Value {
            member: "block format",
            ..
        })
    ));
}

#[test]
fn rqp_client_b64u_decode_reads_the_url_alphabet_without_padding() {
    assert_eq!(b64u_decode("").unwrap(), Vec::<u8>::new());
    assert_eq!(b64u_decode("AAECAwQ").unwrap(), vec![0, 1, 2, 3, 4]);
    assert_eq!(b64u_decode("-_8").unwrap(), vec![0xFB, 0xFF]);
    assert!(b64u_decode("AA=").is_err());
    assert!(b64u_decode("A+").is_err());
}

#[test]
fn rqp_client_nonce_needs_16_bytes_and_canonical_text() {
    assert_eq!(
        nonce("AAECAwQFBgcICQoLDA0ODw").unwrap().0,
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
    );
    assert_eq!(nonce("_____________________w").unwrap().0, [0xFF; 16]);
    assert!(matches!(
        nonce("AAECAwQFBgcICQoLDA0"),
        Err(ClientFixtureError::Value {
            member: "nonce",
            ..
        })
    ));
    // The last character of 22 has two spare bits; a set bit is not
    // canonical.
    assert!(matches!(
        nonce("AAECAwQFBgcICQoLDA0ODx"),
        Err(ClientFixtureError::Value {
            member: "nonce",
            ..
        })
    ));
}

#[test]
fn rqp_client_error_message_names_the_member() {
    assert_eq!(
        nonce("AAECAwQFBgcICQoLDA0").unwrap_err().to_string(),
        "nonce: not 16 bytes"
    );
}
