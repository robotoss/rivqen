// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! WP-04 FFI benchmark (CONTRACT.md section 10).
//!
//! The Rust side makes a deterministic document and gives it in chunks, pull model, through
//! two paths: UniFFI ([`api`]) and a hand-written C ABI with JNI glue ([`abi`]). This is
//! evidence code for ADR-002, not an SDK API.
//!
//! Unsafe code: the crate denies `unsafe_code`; only [`abi`] allows it, and every unsafe block
//! there has a `SAFETY` comment (clippy `undocumented_unsafe_blocks` is denied).

pub mod doc;
pub mod registry;
pub mod sha256;

pub mod api;

// The only module with hand-written unsafe code.
#[allow(unsafe_code)]
pub mod abi;

uniffi::setup_scaffolding!("ffi_bench");

use doc::{Cursor, MAX_CHUNK};
use std::num::NonZeroUsize;

/// SHA-256 of the document of `size` bytes, or `None` when `size > doc::MAX_SIZE`.
#[must_use]
pub fn doc_sha256(size: u64) -> Option<[u8; 32]> {
    let mut cursor = Cursor::new(size, MAX_CHUNK).ok()?;
    let mut h = sha256::Sha256::new();
    while let Some(slice) = cursor.next_slice(NonZeroUsize::MAX) {
        h.update(slice);
    }
    Some(h.finalize())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn hex(d: [u8; 32]) -> String {
        use std::fmt::Write as _;
        d.iter().fold(String::new(), |mut s, b| {
            write!(s, "{b:02x}").unwrap();
            s
        })
    }

    #[test]
    fn doc_sha256_known_values() {
        // Python hashlib over bytes(((i % 251) * 31 + 7) % 251 for i in range(n)).
        assert_eq!(
            hex(doc_sha256(1 << 20).unwrap()),
            "1c59b8670027384143781a8a8bff2f3b44bd8818d0f53b13b064c2375a1afe38"
        );
        assert_eq!(
            hex(doc_sha256(40_000).unwrap()),
            "5e6661e36e13375914435efc990ad16030cb5e2ec659683bc877a59d844732ff"
        );
        assert_eq!(
            hex(doc_sha256(0).unwrap()),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(doc_sha256(doc::MAX_SIZE + 1), None);
    }
}
