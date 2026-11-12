// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! SHA-1 of the legacy protocol (H-143, DL-164).
//!
//! Compatibility only, never a security check
//! (docs/engineering/security/crypto.md). The legacy `etag`, `template-tag`
//! and `html-sha1` values are the lowercase hex text of a SHA-1 digest
//! (legacy wire C-04, C-08).

use core::fmt;

use sha1::{Digest as _, Sha1};

/// Length of the lowercase hex text of a SHA-1 digest.
pub const SHA1_HEX_CHARS: usize = 40;

/// Characters of the `Debug` prefix (as the RQP identifiers).
const DEBUG_PREFIX_CHARS: usize = 8;

const HEX: &[u8; 16] = b"0123456789abcdef";

/// The lowercase hex text of a SHA-1 digest: 40 characters `[0-9a-f]`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sha1Hex([u8; SHA1_HEX_CHARS]);

impl Sha1Hex {
    /// The hex text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        // The bytes are always ASCII hex digits (see `sha1_hex`).
        core::str::from_utf8(&self.0).unwrap_or_default()
    }
}

impl fmt::Display for Sha1Hex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Sha1Hex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prefix = self.as_str().get(..DEBUG_PREFIX_CHARS).unwrap_or_default();
        write!(f, "Sha1Hex({prefix}…)")
    }
}

/// The hex digit of a value `0..16`.
fn hex_digit(nibble: u8) -> u8 {
    HEX.get(usize::from(nibble)).copied().unwrap_or(b'0')
}

/// SHA-1 of `bytes` as lowercase hex text.
#[must_use]
pub fn sha1_hex(bytes: &[u8]) -> Sha1Hex {
    let digest = Sha1::digest(bytes);
    let mut out = [0_u8; SHA1_HEX_CHARS];
    for ([high, low], byte) in out.as_chunks_mut::<2>().0.iter_mut().zip(digest.iter()) {
        *high = hex_digit(byte >> 4);
        *low = hex_digit(byte & 0x0f);
    }
    Sha1Hex(out)
}
