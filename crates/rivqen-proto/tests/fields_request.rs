// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The RQP request fields for inputs that no fixture has (oracle:
//! `requestHeaders` of `tools/rqp-protocol/src/client.mjs`), the server's
//! reading of the values and the `Rq-Have` limit of negotiation section 3.4.
//!
//! The comparison with the request lines of the 150 client fixtures with
//! the ledger check `request-fields` is `fixtures_request_fields_match_ledger`
//! (`fixtures_ledger.rs`, through the test kit).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

mod common;

use proptest::prelude::*;
use rivqen_integrity::{PageRevision, Sha256Digest, TemplateRevision, sha256};
use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_proto::fields::{RequestBasis, RequestFieldsError, request_fields};
use rivqen_proto::sf::{BareItem, Item, Member, parse_dictionary, parse_item, serialize_item};
use rivqen_proto::types::{
    BlockDescriptor, BlockFormat, BlockId, MAX_BLOCKS, MAX_RQ_HAVE_BYTES, MAX_RQ_NONCE_BYTES,
    Nonce, SnapshotMeta,
};

const T: &str = "t1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM";
const R: &str = "r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8";

fn block(id: &str, format: BlockFormat, digest: Sha256Digest) -> BlockDescriptor {
    BlockDescriptor {
        id: BlockId::parse(id).unwrap(),
        format,
        sha256: digest,
    }
}

/// A snapshot with the revisions of the known-answer page and `n` blocks.
fn snapshot(n: usize) -> SnapshotMeta {
    SnapshotMeta {
        template_revision: TemplateRevision::parse(T).unwrap(),
        page_revision: PageRevision::parse(R).unwrap(),
        blocks: (0..n)
            .map(|i| {
                block(
                    &format!("b{i}"),
                    BlockFormat::Html,
                    Sha256Digest::from_b64u(&rivqen_integrity::b64u_encode(
                        &[u8::try_from(i % 256).unwrap(); 32],
                    ))
                    .unwrap(),
                )
            })
            .collect(),
        policy: None,
        limits: None,
    }
}

fn lines(fields: &[rivqen_model::http::FieldLine]) -> Vec<(String, String)> {
    fields
        .iter()
        .map(|f| (f.name.clone(), f.value.clone()))
        .collect()
}

fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
    expected
        .iter()
        .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
        .collect()
}

/// No fixture has a snapshot with 0 blocks; the expected lines are the output
/// of `requestHeaders` (client.mjs) for this snapshot and the nonce 00…0F.
#[test]
fn fields_request_snapshot_with_0_blocks_matches_oracle() {
    let snap = snapshot(0);
    let nonce = common::nonce("AAECAwQFBgcICQoLDA0ODw");
    let got = request_fields(RequestBasis::Snapshot {
        snapshot: &snap,
        nonce: Some(&nonce),
    })
    .unwrap();
    assert_eq!(
        lines(&got),
        pairs(&[
            ("rq-version", "1"),
            ("rq-capabilities", "(block-patch)"),
            ("accept", "text/html, application/vnd.rivqen.delta+json"),
            (
                "if-none-match",
                "\"r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8\""
            ),
            (
                "rq-have",
                "t=t1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM, b=::"
            ),
            ("rq-nonce", ":AAECAwQFBgcICQoLDA0ODw==:"),
        ])
    );
}

/// The output of `requestHeaders` (client.mjs) for the nonce of 16 bytes FF
/// (standard base64 with padding in the Byte Sequence).
#[test]
fn fields_request_nonce_ff_matches_oracle() {
    let snap = snapshot(1);
    let nonce = common::nonce("_____________________w");
    let got = request_fields(RequestBasis::Snapshot {
        snapshot: &snap,
        nonce: Some(&nonce),
    })
    .unwrap();
    assert_eq!(
        got.last().map(|f| (f.name.as_str(), f.value.as_str())),
        Some(("rq-nonce", "://///////////////////w==:"))
    );
}

/// No fixture has a context without a nonce; the expected lines are the
/// output of `requestHeaders` (client.mjs) with `nonce: null`: no delta offer,
/// `If-None-Match` stays (negotiation section 3.9, fail closed).
#[test]
fn fields_request_without_nonce_offers_no_delta() {
    let snap = snapshot(1);
    let got = request_fields(RequestBasis::Snapshot {
        snapshot: &snap,
        nonce: None,
    })
    .unwrap();
    assert_eq!(
        lines(&got),
        pairs(&[
            ("rq-version", "1"),
            ("rq-capabilities", "(block-patch)"),
            ("accept", "text/html"),
            (
                "if-none-match",
                "\"r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8\""
            ),
        ])
    );
}

#[test]
fn fields_request_first_visit_and_fallback_are_equal() {
    let first = request_fields(RequestBasis::FirstVisit).unwrap();
    assert_eq!(first, request_fields(RequestBasis::FallbackLoad).unwrap());
    assert_eq!(
        lines(&first),
        pairs(&[
            ("rq-version", "1"),
            ("rq-capabilities", "(block-patch)"),
            ("accept", "text/html"),
        ])
    );
}

#[test]
fn fields_request_values_parse_as_the_server_reads_them() {
    let snap = snapshot(3);
    let nonce = Nonce([0xa5; 16]);
    let got = request_fields(RequestBasis::Snapshot {
        snapshot: &snap,
        nonce: Some(&nonce),
    })
    .unwrap();
    let value = |name: &str| {
        got.iter()
            .find(|f| f.name == name)
            .map(|f| f.value.clone())
            .unwrap()
    };
    assert_eq!(
        parse_item(&value("rq-version")).unwrap().bare_item,
        BareItem::Integer(1)
    );
    let have = parse_dictionary(&value("rq-have")).unwrap();
    assert_eq!(
        have.get("t"),
        Some(&Member::Item(rivqen_proto::sf::Item::new(BareItem::Token(
            T.to_owned()
        ))))
    );
    let prefixes: Vec<u8> = snap.blocks.iter().flat_map(|b| b.sha256.prefix()).collect();
    assert_eq!(prefixes.len(), 24);
    assert_eq!(
        have.get("b"),
        Some(&Member::Item(rivqen_proto::sf::Item::new(
            BareItem::ByteSequence(prefixes)
        )))
    );
    let nonce_value = value("rq-nonce");
    assert_eq!(nonce_value.len(), 26);
    assert!(nonce_value.len() <= MAX_RQ_NONCE_BYTES);
    assert_eq!(
        parse_item(&nonce_value).unwrap().bare_item,
        BareItem::ByteSequence(vec![0xa5; 16])
    );
}

#[test]
fn fields_request_debug_has_no_nonce() {
    let snap = snapshot(1);
    let nonce = Nonce([0xfe; 16]);
    let basis = RequestBasis::Snapshot {
        snapshot: &snap,
        nonce: Some(&nonce),
    };
    let text = format!("{basis:?}");
    assert!(!text.contains("/v7+/v7+"), "{text}");
    assert!(!text.contains("_v7-_v7-"), "{text}");
    assert!(text.contains("Nonce(<redacted>)"), "{text}");
}

// --- Rq-Have limit (negotiation section 3.4) ------------------------------

fn rq_have(snap: &SnapshotMeta) -> Result<String, RequestFieldsError> {
    let nonce = Nonce([0; 16]);
    request_fields(RequestBasis::Snapshot {
        snapshot: snap,
        nonce: Some(&nonce),
    })
    .map(|f| {
        f.into_iter()
            .find(|l| l.name == "rq-have")
            .map(|l| l.value)
            .unwrap()
    })
}

#[test]
fn fields_rq_have_limit_256_blocks_is_2786_bytes() {
    // The length that requestHeaders (client.mjs) gives for 256 blocks.
    let value = rq_have(&snapshot(MAX_BLOCKS)).unwrap();
    assert_eq!(value.len(), 2786);
    assert!(value.len() <= MAX_RQ_HAVE_BYTES);
}

#[test]
fn fields_rq_have_limit_too_many_blocks_sends_no_fields() {
    let snap = snapshot(MAX_BLOCKS + 1);
    let nonce = Nonce([0; 16]);
    let err = request_fields(RequestBasis::Snapshot {
        snapshot: &snap,
        nonce: Some(&nonce),
    })
    .unwrap_err();
    assert_eq!(err, RequestFieldsError::TooManyBlocks);
    assert_eq!(err.code(), ErrorCode::InternalError);
    assert_ne!(err.to_string(), "");
    assert!(core::error::Error::source(&err).is_none());
    // Without a nonce the client sends no Rq-Have, so the count does not matter.
    assert!(
        request_fields(RequestBasis::Snapshot {
            snapshot: &snap,
            nonce: None,
        })
        .is_ok()
    );
}

#[test]
fn fields_rq_have_limit_digest_prefix_is_the_first_8_bytes() {
    let digest = sha256(b"block");
    let snap = SnapshotMeta {
        blocks: vec![block("a", BlockFormat::Json, digest)],
        ..snapshot(0)
    };
    let value = rq_have(&snap).unwrap();
    let have = parse_dictionary(&value).unwrap();
    assert_eq!(
        have.get("b"),
        Some(&Member::Item(rivqen_proto::sf::Item::new(
            BareItem::ByteSequence(digest.as_bytes()[..8].to_vec())
        )))
    );
}

#[test]
fn fields_request_error_display_names_no_content() {
    for e in [
        RequestFieldsError::TooManyBlocks,
        RequestFieldsError::RqHaveTooLong,
    ] {
        assert!(e.to_string().starts_with("request fields: "));
        assert_eq!(e.code(), ErrorCode::InternalError);
    }
    // A serialize error keeps its cause and its fixed reason, not the value.
    let cause = serialize_item(&Item::new(BareItem::Token("1SECRET".to_owned()))).unwrap_err();
    let e = RequestFieldsError::from(cause);
    assert_eq!(e, RequestFieldsError::Serialize(cause));
    assert_eq!(e.code(), ErrorCode::InternalError);
    assert_eq!(e.to_string(), format!("request fields: {cause}"));
    assert!(!e.to_string().contains("SECRET"));
    assert_eq!(
        core::error::Error::source(&e).map(ToString::to_string),
        Some(cause.to_string())
    );
}

proptest! {
    #[test]
    fn fields_rq_have_limit_holds_for_every_block_count(n in 0..=MAX_BLOCKS) {
        let value = rq_have(&snapshot(n)).unwrap();
        // 54 bytes around `b`, and 4 base64 characters per 3 bytes.
        prop_assert_eq!(value.len(), 54 + (8 * n).div_ceil(3) * 4);
        prop_assert!(value.len() <= MAX_RQ_HAVE_BYTES);
    }
}
