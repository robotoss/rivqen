// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Standard base64 (RFC 4648 section 4) for Byte Sequences (RFC 9651
//! sections 3.3.5, 4.1.8 and 4.2.7).
//!
//! Own code: `rivqen-proto` has no `base64` dependency (H-138 gives it to
//! `rivqen-integrity`, whose API has only base64url). The decoder checks the
//! alphabet and the padding in one pass (DL-063) and applies the padding rule
//! of DL-089: the padding is absent or complete, non-zero pad bits are
//! accepted.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// The 6-bit value of one base64 character, or `None` outside the alphabet.
const fn sextet(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c.wrapping_sub(b'A')),
        b'a'..=b'z' => Some(c.wrapping_sub(b'a').wrapping_add(26)),
        b'0'..=b'9' => Some(c.wrapping_sub(b'0').wrapping_add(52)),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Decode the content of a Byte Sequence, or `None` when it is not valid
/// (RFC 9651 section 4.2.7 steps 6 and 7; DL-089).
///
/// Valid: the characters `A-Z a-z 0-9 + /`, then at most two `=` at the end,
/// and with `=` the length is a multiple of 4 (complete padding). Without
/// padding every length but `4k + 1` is valid. The output is at most 3/4 of
/// the input length.
pub(super) fn decode(content: &[u8]) -> Option<Vec<u8>> {
    let data_len = content
        .iter()
        .position(|&c| c == b'=')
        .unwrap_or(content.len());
    let (data, padding) = content.split_at_checked(data_len)?;
    if padding.len() > 2 || padding.iter().any(|&c| c != b'=') {
        return None;
    }
    if !padding.is_empty() && !content.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(data.len().div_ceil(4).saturating_mul(3));
    for chunk in data.chunks(4) {
        let mut s = [0_u8; 4];
        for (slot, &c) in s.iter_mut().zip(chunk) {
            *slot = sextet(c)?;
        }
        let [s0, s1, s2, s3] = s;
        let bytes = [
            s0.wrapping_shl(2) | s1.wrapping_shr(4),
            (s1 & 0x0f).wrapping_shl(4) | s2.wrapping_shr(2),
            (s2 & 0x03).wrapping_shl(6) | s3,
        ];
        // A group of n characters (2 to 4) holds n - 1 bytes; one character
        // holds no whole byte and is invalid.
        let whole = match chunk.len() {
            2 => 1,
            3 => 2,
            4 => 3,
            _ => return None,
        };
        out.extend(bytes.iter().take(whole));
    }
    Some(out)
}

/// Encode bytes as base64 with padding (RFC 9651 section 4.1.8).
pub(super) fn encode_into(bytes: &[u8], out: &mut String) {
    out.reserve(bytes.len().div_ceil(3).saturating_mul(4));
    for chunk in bytes.chunks(3) {
        let b0 = chunk.first().copied().unwrap_or(0);
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        let sextets = [
            b0.wrapping_shr(2),
            (b0 & 0x03).wrapping_shl(4) | b1.wrapping_shr(4),
            (b1 & 0x0f).wrapping_shl(2) | b2.wrapping_shr(6),
            b2 & 0x3f,
        ];
        // n bytes (1 to 3) give n + 1 characters, then `=` up to 4.
        let used = chunk.len().saturating_add(1);
        for (i, &s) in sextets.iter().enumerate() {
            if i < used {
                out.push(char::from(
                    ALPHABET.get(usize::from(s)).copied().unwrap_or(b'='),
                ));
            } else {
                out.push('=');
            }
        }
    }
}
