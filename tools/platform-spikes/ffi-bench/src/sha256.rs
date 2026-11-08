// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! SHA-256 (FIPS 180-4) for the reference digest of the document.
//!
//! The benchmark may not add a hash crate (DL-137), so this is a small implementation from
//! FIPS 180-4 section 6.2. It is not used for security. The tests check the FIPS 180-4
//! examples and a digest computed by an independent tool; every harness run checks it again
//! against the platform SHA-256 (Java `MessageDigest`, Swift `CryptoKit`).

/// Round constants (FIPS 180-4 section 4.2.2).
const K: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

/// Initial hash value (FIPS 180-4 section 5.3.3).
const H0: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

/// An incremental SHA-256.
#[derive(Debug, Clone)]
pub struct Sha256 {
    state: [u32; 8],
    block: [u8; 64],
    /// Bytes in `block`, always `< 64` between calls.
    filled: usize,
    /// Message length in bytes, modulo 2^64.
    length: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256 {
    /// A new digest.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: H0,
            block: [0; 64],
            filled: 0,
            length: 0,
        }
    }

    /// Adds `data` to the message.
    pub fn update(&mut self, data: &[u8]) {
        self.length = self
            .length
            .wrapping_add(u64::try_from(data.len()).unwrap_or(u64::MAX));
        let mut rest = data;
        if self.filled > 0 {
            let room = 64_usize.saturating_sub(self.filled);
            let (head, tail) = rest.split_at(room.min(rest.len()));
            let end = self.filled.saturating_add(head.len());
            if let Some(dst) = self.block.get_mut(self.filled..end) {
                dst.copy_from_slice(head);
            }
            self.filled = end;
            rest = tail;
            if self.filled < 64 {
                return;
            }
            let block = self.block;
            compress(&mut self.state, &block);
            self.filled = 0;
        }
        let mut blocks = rest.chunks_exact(64);
        for b in &mut blocks {
            if let Ok(b) = <&[u8; 64]>::try_from(b) {
                compress(&mut self.state, b);
            }
        }
        let tail = blocks.remainder();
        if let Some(dst) = self.block.get_mut(..tail.len()) {
            dst.copy_from_slice(tail);
        }
        self.filled = tail.len();
    }

    /// The digest of the message.
    #[must_use]
    pub fn finalize(mut self) -> [u8; 32] {
        let bits = self.length.wrapping_mul(8);
        // Padding: 0x80, zeros up to 56 mod 64, then the bit length (big-endian).
        let zeros = (119_usize.saturating_sub(self.filled)) % 64;
        let mut pad = [0_u8; 72];
        if let Some(first) = pad.first_mut() {
            *first = 0x80;
        }
        let pad_len = zeros.saturating_add(1);
        let length_before = self.length;
        self.update(pad.get(..pad_len).unwrap_or(&[]));
        self.update(&bits.to_be_bytes());
        self.length = length_before;
        let mut out = [0_u8; 32];
        for (dst, word) in out.chunks_exact_mut(4).zip(self.state) {
            dst.copy_from_slice(&word.to_be_bytes());
        }
        out
    }
}

/// One SHA-256 block (FIPS 180-4 section 6.2.2).
#[expect(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::many_single_char_names,
    reason = "FIPS 180-4 names; every index is in 0..64 (t - 16 >= 0 for t >= 16) of fixed arrays"
)]
fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut w = [0_u32; 64];
    for (t, word) in block.chunks_exact(4).enumerate() {
        w[t] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
    }
    for t in 16..64 {
        let s0 = w[t - 15].rotate_right(7) ^ w[t - 15].rotate_right(18) ^ (w[t - 15] >> 3);
        let s1 = w[t - 2].rotate_right(17) ^ w[t - 2].rotate_right(19) ^ (w[t - 2] >> 10);
        w[t] = w[t - 16]
            .wrapping_add(s0)
            .wrapping_add(w[t - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for t in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ (!e & g);
        let t1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(K[t])
            .wrapping_add(w[t]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    for (s, v) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *s = s.wrapping_add(v);
    }
}

/// SHA-256 of `data`.
#[must_use]
pub fn digest(data: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]
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
    fn fips_180_examples() {
        // FIPS 180-4 / NIST CSRC examples: "", "abc", the 448-bit and 896-bit messages.
        assert_eq!(
            hex(digest(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(digest(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(digest(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(
            hex(digest(
                b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu"
            )),
            "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1"
        );
    }

    #[test]
    fn million_a() {
        let mut h = Sha256::new();
        let block = [b'a'; 1000];
        for _ in 0..1000 {
            h.update(&block);
        }
        assert_eq!(
            hex(h.finalize()),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn split_updates_equal_one_update() {
        let data: Vec<u8> = (0..1000_u32).map(|i| (i % 253) as u8).collect();
        let whole = digest(&data);
        for split in [0, 1, 55, 56, 63, 64, 65, 127, 128, 999, 1000] {
            for step in [1, 7, 64, 100] {
                let mut h = Sha256::new();
                h.update(&data[..split]);
                for c in data[split..].chunks(step) {
                    h.update(c);
                }
                assert_eq!(h.finalize(), whole, "split {split} step {step}");
            }
        }
    }

    #[test]
    fn padding_boundaries() {
        // Lengths around the 55/56/64 byte padding edges, against a one-shot reference
        // computed byte by byte through update.
        for len in [55_usize, 56, 57, 63, 64, 65, 119, 120] {
            let data = vec![0x5a_u8; len];
            let mut h = Sha256::new();
            for b in &data {
                h.update(std::slice::from_ref(b));
            }
            assert_eq!(h.finalize(), digest(&data), "len {len}");
        }
        // Known value: 64 bytes of 0x00 (python3 -c "import hashlib;print(hashlib.sha256(bytes(64)).hexdigest())").
        assert_eq!(
            hex(digest(&[0_u8; 64])),
            "f5a5fd42d16a20302798ef6ed309979b43003d2320d9f0e8ea9831a92759fb4b"
        );
        assert_eq!(
            hex(digest(&[0_u8; 56])),
            "d4817aa5497628e7c77e6b606107042bbba3130888c5f47a375e6179be789fbb"
        );
    }
}
