// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The text forms of a digest and of the identifiers `t1.`, `r1.`, `d1.`:
//! only the canonical form of manifest-patch section 1.3, rule 2 parses.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use proptest::prelude::*;
use rivqen_integrity::{
    Binding, DIGEST_B64U_CHARS, DIGEST_BYTES, IdError, PageRevision, Sha256Digest,
    TemplateRevision, b64u_encode,
};

/// A canonical digest text (the block hash of "Shop").
const HASH: &str = "0Aqua3-_d6RWbY3zaAXXrpc4lNqfUz7JZRazHgKLces";
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
/// The last characters whose two low bits are zero (section 1.3, rule 2).
const CANONICAL_LAST: &[u8; 16] = b"AEIMQUYcgkosw048";

fn with_last(last: char) -> String {
    let mut text = HASH[..DIGEST_B64U_CHARS - 1].to_owned();
    text.push(last);
    text
}

#[test]
fn integrity_from_b64u_rejects_padding() {
    // 42 characters and a pad: the length is right, the pad is not.
    let padded = format!("{}=", &HASH[..42]);
    assert_eq!(padded.len(), DIGEST_B64U_CHARS);
    assert_eq!(Sha256Digest::from_b64u(&padded), Err(IdError::Encoding));
    // The padded form of the whole digest.
    assert_eq!(
        Sha256Digest::from_b64u(&format!("{HASH}=")),
        Err(IdError::Length)
    );
    assert_eq!(
        Sha256Digest::from_b64u(&format!("{}==", &HASH[..41])),
        Err(IdError::Encoding)
    );
}

#[test]
fn integrity_from_b64u_rejects_another_length() {
    assert_eq!(Sha256Digest::from_b64u(&HASH[..42]), Err(IdError::Length));
    assert_eq!(
        Sha256Digest::from_b64u(&format!("{HASH}A")),
        Err(IdError::Length)
    );
    assert_eq!(Sha256Digest::from_b64u(""), Err(IdError::Length));
    assert_eq!(
        Sha256Digest::from_b64u(&format!("{HASH}{HASH}")),
        Err(IdError::Length)
    );
}

#[test]
fn integrity_from_b64u_accepts_only_a_canonical_last_character() {
    for &last in ALPHABET {
        let text = with_last(char::from(last));
        let result = Sha256Digest::from_b64u(&text);
        if CANONICAL_LAST.contains(&last) {
            assert_eq!(result.unwrap().to_b64u(), text);
        } else {
            assert_eq!(result, Err(IdError::Encoding), "last character {last}");
        }
    }
}

#[test]
fn integrity_from_b64u_rejects_bytes_outside_the_alphabet() {
    for bad in [
        b'+', b'/', b'=', b' ', b'.', b'\n', b'\t', 0, 0x7F, b'%', b'~',
    ] {
        for position in [0, 20, DIGEST_B64U_CHARS - 1] {
            let mut bytes = HASH.as_bytes().to_vec();
            bytes[position] = bad;
            let text = String::from_utf8(bytes).unwrap();
            assert_eq!(
                Sha256Digest::from_b64u(&text),
                Err(IdError::Encoding),
                "byte {bad:#04x} at {position}"
            );
        }
    }
    // A two-byte character: 41 characters and "é" are 43 bytes.
    let text = format!("{}é", &HASH[..41]);
    assert_eq!(text.len(), DIGEST_B64U_CHARS);
    assert_eq!(Sha256Digest::from_b64u(&text), Err(IdError::Encoding));
}

#[test]
fn integrity_identifiers_reject_another_prefix() {
    for text in [
        format!("r1.{HASH}"),
        format!("d1.{HASH}"),
        format!("T1.{HASH}"),
        format!("t2.{HASH}"),
        format!("t1{HASH}"),
        format!(" t1.{HASH}"),
        HASH.to_owned(),
        String::new(),
    ] {
        assert_eq!(
            TemplateRevision::parse(&text),
            Err(IdError::Prefix),
            "{text}"
        );
    }
    for text in [
        format!("t1.{HASH}"),
        format!("d1.{HASH}"),
        format!("R1.{HASH}"),
    ] {
        assert_eq!(PageRevision::parse(&text), Err(IdError::Prefix), "{text}");
    }
    for text in [
        format!("t1.{HASH}"),
        format!("r1.{HASH}"),
        format!("D1.{HASH}"),
    ] {
        assert_eq!(Binding::parse(&text), Err(IdError::Prefix), "{text}");
    }
}

#[test]
fn integrity_identifiers_check_the_digest_after_the_prefix() {
    assert_eq!(
        PageRevision::parse(&format!("r1.{HASH}\n")),
        Err(IdError::Length)
    );
    assert_eq!(
        PageRevision::parse(&format!("r1.{}", &HASH[..42])),
        Err(IdError::Length)
    );
    assert_eq!(
        PageRevision::parse(&format!("r1.{}", with_last('t'))),
        Err(IdError::Encoding)
    );
    assert_eq!(PageRevision::parse("r1."), Err(IdError::Length));
}

#[test]
fn integrity_id_error_texts_are_distinct_and_static() {
    let texts = [IdError::Prefix, IdError::Length, IdError::Encoding].map(|e| e.to_string());
    assert_eq!(texts[0], "identifier has another prefix");
    assert_eq!(texts[1], "identifier has another length");
    assert_eq!(texts[2], "identifier is not canonical base64url");
}

proptest! {
    #[test]
    fn integrity_b64u_round_trip_for_any_32_bytes(bytes in prop::array::uniform32(any::<u8>())) {
        let text = b64u_encode(&bytes);
        prop_assert_eq!(text.len(), DIGEST_B64U_CHARS);
        let digest = Sha256Digest::from_b64u(&text).unwrap();
        prop_assert_eq!(digest.as_bytes(), &bytes);
        prop_assert_eq!(digest.to_b64u(), text);
    }

    #[test]
    fn integrity_any_alphabet_text_parses_only_in_canonical_form(
        indices in prop::collection::vec(0_usize..64, DIGEST_B64U_CHARS),
    ) {
        let text: String = indices.iter().map(|&i| char::from(ALPHABET[i])).collect();
        let last = text.as_bytes()[DIGEST_B64U_CHARS - 1];
        match Sha256Digest::from_b64u(&text) {
            Ok(digest) => {
                prop_assert!(CANONICAL_LAST.contains(&last));
                prop_assert_eq!(digest.to_b64u(), text);
            }
            Err(error) => {
                prop_assert!(!CANONICAL_LAST.contains(&last));
                prop_assert_eq!(error, IdError::Encoding);
            }
        }
    }

    #[test]
    fn integrity_from_b64u_never_panics(text in ".{0,64}") {
        if let Ok(digest) = Sha256Digest::from_b64u(&text) {
            prop_assert_eq!(digest.to_b64u(), text);
        }
    }

    #[test]
    fn integrity_identifier_text_round_trip(bytes in prop::array::uniform32(any::<u8>())) {
        let digest = Sha256Digest::from_b64u(&b64u_encode(&bytes)).unwrap();
        prop_assert_eq!(digest.as_bytes().len(), DIGEST_BYTES);
        let template = TemplateRevision::from_digest(digest);
        prop_assert_eq!(TemplateRevision::parse(&template.to_string()), Ok(template));
        let page = PageRevision::from_digest(digest);
        prop_assert_eq!(PageRevision::parse(&page.to_string()), Ok(page));
        prop_assert_eq!(page.to_string().len(), 46);
        let binding = Binding::from_digest(digest);
        prop_assert_eq!(Binding::parse(&binding.to_string()), Ok(binding));
        prop_assert_eq!(binding.digest(), &digest);
    }
}
