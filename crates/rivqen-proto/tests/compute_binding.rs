// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The binding of manifest-patch section 2.2 (H-107, H-108): the preimage
//! form, and every input byte changes the value.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use proptest::prelude::*;
use rivqen_integrity::{Binding, PageRevision, Sha256Digest, b64u_encode, sha256};
use rivqen_proto::compute::binding;
use rivqen_proto::types::Nonce;

const BASE: &str = "r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8";
const NEXT: &str = "r1.psbsc6whgs8ooESisNWp6E3PrQfkg4D69WmXAo12pbI";

fn revision(bytes: &[u8; 32]) -> PageRevision {
    PageRevision::from_digest(Sha256Digest::from_b64u(&b64u_encode(bytes)).unwrap())
}

#[test]
fn compute_binding_preimage_form() {
    let base = PageRevision::parse(BASE).unwrap();
    let next = PageRevision::parse(NEXT).unwrap();
    let nonce = Nonce([0xAB; 16]);
    let url = "https://shop.example.com/a?b=c";
    let preimage = format!("rqp-d1\n{url}\n{BASE}\n{}\n{NEXT}\n", b64u_encode(&nonce.0));
    assert_eq!(b64u_encode(&nonce.0).len(), 22);
    assert_eq!(
        binding(url, &base, &nonce, &next),
        Binding::from_digest(sha256(preimage.as_bytes()))
    );
}

#[test]
fn compute_binding_uses_the_url_as_given() {
    // U is the canonical text; the function does not canonicalize (WP-08).
    let base = PageRevision::parse(BASE).unwrap();
    let next = PageRevision::parse(NEXT).unwrap();
    let nonce = Nonce([0; 16]);
    let canonical = binding("https://shop.example.com/catalog", &base, &nonce, &next);
    for other in [
        "https://SHOP.example.com/catalog",
        "https://shop.example.com:443/catalog",
        "https://shop.example.com/catalog#reviews",
        "https://shop.example.com/catalog/",
        "",
    ] {
        assert_ne!(binding(other, &base, &nonce, &next), canonical, "{other}");
    }
}

#[test]
fn compute_binding_base_and_next_are_not_interchangeable() {
    let base = PageRevision::parse(BASE).unwrap();
    let next = PageRevision::parse(NEXT).unwrap();
    let nonce = Nonce([7; 16]);
    let url = "https://shop.example.com/catalog";
    assert_ne!(
        binding(url, &base, &nonce, &next),
        binding(url, &next, &nonce, &base)
    );
}

#[test]
fn compute_nonce_debug_is_redacted() {
    let nonce = Nonce([0x5A; 16]);
    let shown = format!("{nonce:?}");
    assert_eq!(shown, "Nonce(<redacted>)");
    assert!(!shown.contains(&b64u_encode(&nonce.0)));
}

/// Which input of the binding a property changes.
#[derive(Debug, Clone, Copy)]
enum Input {
    Url,
    Base,
    Nonce,
    Next,
}

proptest! {
    #[test]
    fn compute_binding_changes_with_any_input_byte(
        url in "https://[a-z]{1,12}\\.example/[ -~]{0,40}",
        base in prop::array::uniform32(any::<u8>()),
        nonce in prop::array::uniform16(any::<u8>()),
        next in prop::array::uniform32(any::<u8>()),
        input in prop_oneof![Just(Input::Url), Just(Input::Base), Just(Input::Nonce), Just(Input::Next)],
        position in any::<prop::sample::Index>(),
        delta in 1_u8..=255,
    ) {
        let value = binding(&url, &revision(&base), &Nonce(nonce), &revision(&next));
        let (mut url2, mut base2, mut nonce2, mut next2) = (url.clone().into_bytes(), base, nonce, next);
        match input {
            Input::Url => {
                // Another printable ASCII character keeps the text valid UTF-8.
                let i = position.index(url2.len());
                let old = url2[i] - b' ';
                url2[i] = b' ' + (old + delta % 94 + 1) % 95;
                prop_assume!(url2[i] != url.as_bytes()[i]);
            }
            Input::Base => base2[position.index(32)] ^= delta,
            Input::Nonce => nonce2[position.index(16)] ^= delta,
            Input::Next => next2[position.index(32)] ^= delta,
        }
        let url2 = String::from_utf8(url2).unwrap();
        let changed = binding(&url2, &revision(&base2), &Nonce(nonce2), &revision(&next2));
        prop_assert_ne!(changed, value);
    }

    #[test]
    fn compute_binding_is_the_hash_of_the_preimage(
        url in "[ -~]{0,64}",
        base in prop::array::uniform32(any::<u8>()),
        nonce in prop::array::uniform16(any::<u8>()),
        next in prop::array::uniform32(any::<u8>()),
    ) {
        let (b, r) = (revision(&base), revision(&next));
        let preimage = format!("rqp-d1\n{url}\n{b}\n{}\n{r}\n", b64u_encode(&nonce));
        let value = binding(&url, &b, &Nonce(nonce), &r);
        prop_assert_eq!(value, Binding::from_digest(sha256(preimage.as_bytes())));
        prop_assert_eq!(value.to_string().len(), 46);
    }
}
