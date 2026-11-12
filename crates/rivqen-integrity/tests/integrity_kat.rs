// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Known answers of `rivqen-integrity`: the hashes and identifiers of
//! `spec/rqp/examples/kat/kat.json` and docs/engineering/protocol/manifest-patch.md
//! sections 1.1, 1.3, 2.1 and 2.2.
//!
//! This crate has no JSON reader (H-138), so the values are written here and
//! `kat_values_are_the_committed_ones` checks that each one is in the committed
//! file. `rivqen-proto` reads the same file with a JSON reader
//! (`tests/compute_kat.rs`).

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use rivqen_integrity::{
    Binding, DIGEST_B64U_CHARS, PREFIX_BYTES, PageRevision, Sha256Digest, TemplateRevision,
    b64u_encode, sha256, sha256_parts,
};

const KAT_JSON: &str = include_str!("../../../spec/rqp/examples/kat/kat.json");
const KAT_PAGE: &[u8] = include_bytes!("../../../spec/rqp/examples/kat/page.html");

const TEMPLATE_REVISION: &str = "t1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM";
const TITLE: &str = "0Aqua3-_d6RWbY3zaAXXrpc4lNqfUz7JZRazHgKLces";
const PRICE: &str = "Xg3oaiol3ywUdxTQZNp7JJJu7_R8kUL-HK4accfrDKs";
const CART: &str = "zyEL3KkZlLmAgTTFnonr3pdX-DnDM81i9xFoREUEaX4";
const PRICE_130: &str = "bB9ScYcQQzKWqPCBW62MTsc9Ff_djve7Denf3YMoCUg";
const PAGE_REVISION_A: &str = "r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8";
const PAGE_REVISION_POLICY: &str = "r1.pONNMoySlMlYw50BpgdvbUlGESnMzZBCOmQ4SlebDxs";
const PAGE_REVISION_POLICY_B: &str = "r1.kX-KQFiXmNtLNzjvXP7GR3K-_79Fc1l4WEtReCgqwQQ";
const NEXT_REVISION_PRICE: &str = "r1.psbsc6whgs8ooESisNWp6E3PrQfkg4D69WmXAo12pbI";
const BINDING_PRICE: &str = "d1.orrx-_-Iflnnjs7DvvuOOdxW3FzbZpU_PTlR0jdWw54";
const BINDING_PRICE_PREIMAGE: &str = "rqp-d1\nhttps://shop.example.com/catalog\nr1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8\nAAECAwQFBgcICQoLDA0ODw\nr1.psbsc6whgs8ooESisNWp6E3PrQfkg4D69WmXAo12pbI\n";
const BINDING_QUERY: &str = "d1.9tOh97k3psr9cLv3_twQfmdbRWJ2c1LXACllNPOGc-8";
const BINDING_QUERY_PREIMAGE: &str = "rqp-d1\nhttps://shop.example.com/catalog?sort=price&page=2\nr1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8\nAAECAwQFBgcICQoLDA0ODw\nr1.psbsc6whgs8ooESisNWp6E3PrQfkg4D69WmXAo12pbI\n";
const NONCE_TEXT: &str = "AAECAwQFBgcICQoLDA0ODw";
/// `Rq-Have` member `b` of the KAT: the three 8-byte prefixes in standard
/// base64 (a Structured Fields Byte Sequence).
const RQ_HAVE_B: &str = "0Aqua3+/d6ReDehqKiXfLM8hC9ypGZS5";

/// The page revision preimage P of section 1.3 for the KAT page, no policy line.
fn preimage_a(price: &str) -> String {
    format!(
        "rqp-r1\n{TEMPLATE_REVISION}\ntitle\thtml\t{TITLE}\nprice\thtml\t{price}\ncart\tjson\t{CART}\n"
    )
}

#[test]
fn kat_values_are_the_committed_ones() {
    for value in [
        TEMPLATE_REVISION,
        TITLE,
        PRICE,
        CART,
        PAGE_REVISION_A,
        PAGE_REVISION_POLICY,
        PAGE_REVISION_POLICY_B,
        NEXT_REVISION_PRICE,
        BINDING_PRICE,
        BINDING_QUERY,
        NONCE_TEXT,
        RQ_HAVE_B,
    ] {
        assert!(KAT_JSON.contains(value), "{value} not in kat.json");
    }
    // The preimages are written with JSON escapes in the file.
    for preimage in [BINDING_PRICE_PREIMAGE, BINDING_QUERY_PREIMAGE] {
        let escaped = preimage.replace('\n', "\\n");
        assert!(KAT_JSON.contains(&escaped), "{escaped} not in kat.json");
    }
}

#[test]
fn integrity_sha256_known_answers() {
    // FIPS 180-4 example "abc" (ba7816bf…15ad).
    assert_eq!(
        sha256(b"abc").to_b64u(),
        "ungWv48Bz-pBQUDeXa4iI7ADYaOWF3qctBD_YfIAFa0"
    );
    assert_eq!(
        sha256(b"abc").as_bytes()[..4],
        [0xba, 0x78, 0x16, 0xbf],
        "digest bytes"
    );
    // manifest-patch section 2.3: the hash of no bytes.
    assert_eq!(
        sha256(b"").to_b64u(),
        "47DEQpj8HBSa-_TImW-5JCeuQeRkm5NMpJWZG3hSuFU"
    );
    // The block contents of the KAT page (section 1.1) and the new price (2.1).
    assert_eq!(sha256(b"Shop").to_b64u(), TITLE);
    assert_eq!(sha256(b"120.00").to_b64u(), PRICE);
    assert_eq!(sha256(br#"{"items":3}"#).to_b64u(), CART);
    assert_eq!(sha256(b"130.00").to_b64u(), PRICE_130);
}

#[test]
fn integrity_template_revision_kat() {
    // Section 1.3, steps 2 and 3: the KAT page without its three block
    // contents, after the tag "rqp-t1" LF.
    let page = core::str::from_utf8(KAT_PAGE).unwrap();
    let template = page
        .replacen(">Shop<", "><", 1)
        .replacen(">120.00<", "><", 1)
        .replacen(r#">{"items":3}<"#, "><", 1);
    assert_eq!(template.len() + 4 + 6 + 11, page.len());
    let digest = sha256_parts(&[b"rqp-t1\n", template.as_bytes()]);
    assert_eq!(
        TemplateRevision::from_digest(digest).to_string(),
        TEMPLATE_REVISION
    );
}

#[test]
fn integrity_sha256_parts_is_the_hash_of_the_concatenation() {
    let whole = preimage_a(PRICE);
    let (a, b) = whole.as_bytes().split_at(17);
    let (b, c) = b.split_at(40);
    assert_eq!(sha256_parts(&[a, b, c]), sha256(whole.as_bytes()));
    assert_eq!(sha256_parts(&[a, b"", b, c]), sha256(whole.as_bytes()));
    assert_eq!(sha256_parts(&[]), sha256(b""));
    assert_ne!(sha256_parts(&[a, c, b]), sha256(whole.as_bytes()));
}

#[test]
fn integrity_page_revision_kat() {
    let digest = sha256(preimage_a(PRICE).as_bytes());
    assert_eq!(
        PageRevision::from_digest(digest).to_string(),
        PAGE_REVISION_A
    );
    let digest = sha256(preimage_a(PRICE_130).as_bytes());
    assert_eq!(
        PageRevision::from_digest(digest).to_string(),
        NEXT_REVISION_PRICE
    );
}

#[test]
fn integrity_page_revision_with_policy_line_kat() {
    let line = "PL\tpolicy;ttl_seconds=60;offline=true;limits;max_patch_bytes=65536\n";
    let with_line = |price: &str| {
        format!(
            "rqp-r1\n{TEMPLATE_REVISION}\n{line}title\thtml\t{TITLE}\nprice\thtml\t{price}\ncart\tjson\t{CART}\n"
        )
    };
    let revision = PageRevision::from_digest(sha256(with_line(PRICE).as_bytes()));
    assert_eq!(revision.to_string(), PAGE_REVISION_POLICY);
    let revision = PageRevision::from_digest(sha256(with_line(PRICE_130).as_bytes()));
    assert_eq!(revision.to_string(), PAGE_REVISION_POLICY_B);
}

#[test]
fn integrity_binding_kat() {
    let binding = Binding::from_digest(sha256(BINDING_PRICE_PREIMAGE.as_bytes()));
    assert_eq!(binding.to_string(), BINDING_PRICE);
    let binding = Binding::from_digest(sha256(BINDING_QUERY_PREIMAGE.as_bytes()));
    assert_eq!(binding.to_string(), BINDING_QUERY);
}

#[test]
fn integrity_nonce_text_kat() {
    let bytes: Vec<u8> = (0_u8..16).collect();
    assert_eq!(b64u_encode(&bytes), NONCE_TEXT);
    let bytes: Vec<u8> = (0xF0_u8..=0xFF).collect();
    assert_eq!(b64u_encode(&bytes), "8PHy8_T19vf4-fr7_P3-_w");
    assert_eq!(b64u_encode(b""), "");
}

#[test]
fn integrity_identifiers_parse_and_display_kat() {
    let template = TemplateRevision::parse(TEMPLATE_REVISION).unwrap();
    assert_eq!(template.to_string(), TEMPLATE_REVISION);
    let page = PageRevision::parse(PAGE_REVISION_A).unwrap();
    assert_eq!(page.to_string(), PAGE_REVISION_A);
    assert_eq!(page.digest(), &sha256(preimage_a(PRICE).as_bytes()));
    let binding = Binding::parse(BINDING_PRICE).unwrap();
    assert_eq!(binding.to_string(), BINDING_PRICE);
    assert_eq!(format!("{page:?}"), "PageRevision(r1.RdZd06nC...)");
    assert_eq!(TemplateRevision::PREFIX, "t1.");
    assert_eq!(PageRevision::PREFIX, "r1.");
    assert_eq!(Binding::PREFIX, "d1.");
}

#[test]
fn integrity_prefix_is_the_rq_have_member() {
    let mut joined = Vec::new();
    for hash in [TITLE, PRICE, CART] {
        let digest = Sha256Digest::from_b64u(hash).unwrap();
        let prefix = digest.prefix();
        assert_eq!(prefix.len(), PREFIX_BYTES);
        assert_eq!(prefix[..], digest.as_bytes()[..PREFIX_BYTES]);
        joined.extend_from_slice(&prefix);
    }
    // The KAT writes the Byte Sequence in standard base64.
    let standard = b64u_encode(&joined).replace('-', "+").replace('_', "/");
    assert_eq!(standard, RQ_HAVE_B);
}

#[test]
fn integrity_digest_text_round_trip_kat() {
    for hash in [TITLE, PRICE, CART, PRICE_130] {
        let digest = Sha256Digest::from_b64u(hash).unwrap();
        assert_eq!(digest.to_b64u(), hash);
        assert_eq!(digest.to_b64u().len(), DIGEST_B64U_CHARS);
        assert_eq!(
            format!("{digest:?}"),
            format!("Sha256Digest({}...)", &hash[..8])
        );
    }
}
