// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The deterministic document of CONTRACT.md section 10 and a pull cursor over it.
//!
//! Byte `i` of the document is `(i * 31 + 7) mod 251`. The byte depends only on `i mod 251`,
//! so a static table of `251 + MAX_CHUNK` bytes holds every chunk as one contiguous slice:
//! the bytes `i .. i + n` of the document are the table bytes `i mod 251 .. i mod 251 + n`.
//! The document is never allocated; a chunk is a `&'static [u8]`.

use std::num::NonZeroUsize;

/// Largest chunk in bytes (64 KiB, the upper end of the streaming chunk size).
pub const MAX_CHUNK: u32 = 65_536;

/// Largest document in bytes (256 MiB). The bound limits the time of [`crate::doc_sha256`].
pub const MAX_SIZE: u64 = 256 * 1024 * 1024;

/// Period of the document bytes.
const PERIOD: u64 = 251;

/// Length of the static table: one period plus the largest chunk.
const TABLE_LEN: usize = 251 + 65_536;

/// The table: byte `j` is `byte_at(j)`. Built at compile time.
static TABLE: [u8; TABLE_LEN] = build_table();

/// Byte `i` of the document.
#[must_use]
pub const fn byte_at(i: u64) -> u8 {
    // (i mod 251) < 251, so (i mod 251) * 31 + 7 < 7 789: no overflow, and the result is < 251.
    let r = (i % PERIOD).wrapping_mul(31).wrapping_add(7) % PERIOD;
    // r < 251, so the cast keeps the value.
    r as u8
}

#[expect(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::large_stack_arrays,
    reason = "const evaluation at compile time: an index out of range or an overflow is a compile error, and the array is never on a run-time stack"
)]
const fn build_table() -> [u8; TABLE_LEN] {
    let mut table = [0_u8; TABLE_LEN];
    let mut j = 0;
    while j < TABLE_LEN {
        table[j] = byte_at(j as u64);
        j += 1;
    }
    table
}

/// Why a document cannot be opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenError {
    /// `chunk` is 0 or larger than [`MAX_CHUNK`].
    Chunk,
    /// `size` is larger than [`MAX_SIZE`].
    Size,
}

/// A pull cursor over the document of `size` bytes, in chunks of at most `chunk` bytes.
///
/// Invariants: `pos <= size`; the slices returned in order are the document bytes in order;
/// every slice has `1 ..= min(chunk, cap)` bytes; after the end every call returns `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    size: u64,
    chunk: u32,
    pos: u64,
}

impl Cursor {
    /// Opens a cursor at byte 0.
    ///
    /// # Errors
    ///
    /// [`OpenError::Chunk`] when `chunk` is not in `1 ..= MAX_CHUNK`;
    /// [`OpenError::Size`] when `size > MAX_SIZE`.
    pub fn new(size: u64, chunk: u32) -> Result<Self, OpenError> {
        if chunk == 0 || chunk > MAX_CHUNK {
            return Err(OpenError::Chunk);
        }
        if size > MAX_SIZE {
            return Err(OpenError::Size);
        }
        Ok(Self {
            size,
            chunk,
            pos: 0,
        })
    }

    /// Document size in bytes.
    #[must_use]
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Bytes already given out.
    #[must_use]
    pub fn position(&self) -> u64 {
        self.pos
    }

    /// The next slice of at most `min(chunk, cap)` bytes, without moving the cursor.
    /// `None` at the end of the document.
    #[must_use]
    pub fn peek(&self, cap: NonZeroUsize) -> Option<&'static [u8]> {
        let remaining = self.size.saturating_sub(self.pos);
        let limit = u64::from(self.chunk).min(remaining);
        // limit <= MAX_CHUNK, so it fits in usize on every target.
        let n = usize::try_from(limit).ok()?.min(cap.get());
        if n == 0 {
            return None;
        }
        // pos % 251 < 251, so it fits in usize.
        let start = usize::try_from(self.pos % PERIOD).ok()?;
        TABLE.get(start..start.checked_add(n)?)
    }

    /// Moves the cursor by `n` bytes, at most to the end.
    pub fn advance(&mut self, n: usize) {
        let n = u64::try_from(n).unwrap_or(u64::MAX);
        self.pos = self.pos.saturating_add(n).min(self.size);
    }

    /// The next slice (as [`Cursor::peek`]) and moves the cursor past it.
    pub fn next_slice(&mut self, cap: NonZeroUsize) -> Option<&'static [u8]> {
        let slice = self.peek(cap)?;
        self.advance(slice.len());
        Some(slice)
    }
}

/// A cursor and the count of document bytes that the Rust side copied out of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stream {
    cursor: Cursor,
    copied: u64,
}

impl Stream {
    /// A stream at byte 0.
    #[must_use]
    pub fn new(cursor: Cursor) -> Self {
        Self { cursor, copied: 0 }
    }

    /// Bytes that `copy` callbacks reported as copied (saturating).
    #[must_use]
    pub fn copied(&self) -> u64 {
        self.copied
    }

    /// The cursor.
    #[must_use]
    pub fn cursor(&self) -> &Cursor {
        &self.cursor
    }

    /// Gives the next slice of at most `min(chunk, cap)` bytes to `copy`. When `copy` returns
    /// `Ok`, the cursor moves past the slice and the slice length counts as copied. When `copy`
    /// fails, the stream does not change. `Ok(None)` at the end of the document.
    ///
    /// # Errors
    ///
    /// The error of `copy`.
    pub fn read_with<E>(
        &mut self,
        cap: NonZeroUsize,
        copy: impl FnOnce(&'static [u8]) -> Result<(), E>,
    ) -> Result<Option<usize>, E> {
        let Some(slice) = self.cursor.peek(cap) else {
            return Ok(None);
        };
        copy(slice)?;
        let n = slice.len();
        self.cursor.advance(n);
        self.copied = self
            .copied
            .saturating_add(u64::try_from(n).unwrap_or(u64::MAX));
        Ok(Some(n))
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation
)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn nz(n: usize) -> NonZeroUsize {
        NonZeroUsize::new(n).unwrap()
    }

    fn collect(size: u64, chunk: u32, cap: usize) -> (Vec<u8>, Vec<usize>) {
        let mut c = Cursor::new(size, chunk).unwrap();
        let mut doc = Vec::new();
        let mut lens = Vec::new();
        while let Some(s) = c.next_slice(nz(cap)) {
            doc.extend_from_slice(s);
            lens.push(s.len());
        }
        (doc, lens)
    }

    #[test]
    fn doc_bytes() {
        // Byte i = (i * 31 + 7) mod 251, computed here without the period trick.
        for i in [
            0_u64,
            1,
            2,
            7,
            8,
            250,
            251,
            252,
            1000,
            65_535,
            65_536,
            1 << 20,
            u64::MAX,
        ] {
            let want = ((u128::from(i) * 31 + 7) % 251) as u8;
            assert_eq!(byte_at(i), want, "byte {i}");
        }
        assert_eq!(byte_at(0), 7);
        assert_eq!(byte_at(1), 38);
        assert_eq!(byte_at(8), 4); // 255 mod 251
        // The table is the document prefix.
        for (j, b) in TABLE.iter().enumerate() {
            assert_eq!(*b, byte_at(j as u64));
        }
        let (doc, _) = collect(1000, 16_384, 16_384);
        let want: Vec<u8> = (0..1000_u64).map(byte_at).collect();
        assert_eq!(doc, want);
    }

    #[test]
    fn chunks_cover_document() {
        // 1 MiB in 16 KiB: 64 full chunks.
        let (doc, lens) = collect(1 << 20, 16_384, 16_384);
        assert_eq!(lens.len(), 64);
        assert!(lens.iter().all(|&n| n == 16_384));
        assert_eq!(doc.len(), 1 << 20);
        // 1 MiB in 64 KiB: 16 full chunks.
        let (_, lens) = collect(1 << 20, 65_536, 65_536);
        assert_eq!(lens, vec![65_536; 16]);
        // Last short chunk.
        let (doc, lens) = collect(40_000, 16_384, 16_384);
        assert_eq!(lens, vec![16_384, 16_384, 7_232]);
        assert_eq!(doc, (0..40_000_u64).map(byte_at).collect::<Vec<_>>());
        // A smaller caller buffer limits the slice.
        let (_, lens) = collect(10, 4, 3);
        assert_eq!(lens, vec![3, 3, 3, 1]);
        // Size 0: no chunk.
        let (doc, lens) = collect(0, 16_384, 16_384);
        assert!(doc.is_empty() && lens.is_empty());
    }

    #[test]
    fn read_after_end() {
        let mut c = Cursor::new(5, 4).unwrap();
        assert_eq!(c.next_slice(nz(4)).map(<[u8]>::len), Some(4));
        assert_eq!(c.next_slice(nz(4)).map(<[u8]>::len), Some(1));
        assert_eq!(c.position(), 5);
        for _ in 0..3 {
            assert_eq!(c.next_slice(nz(4)), None);
            assert_eq!(c.position(), 5);
        }
        c.advance(usize::MAX);
        assert_eq!(c.position(), 5);
    }

    #[test]
    fn open_limits() {
        assert_eq!(Cursor::new(1, 0), Err(OpenError::Chunk));
        assert_eq!(Cursor::new(1, MAX_CHUNK + 1), Err(OpenError::Chunk));
        assert!(Cursor::new(1, 1).is_ok());
        assert!(Cursor::new(1, MAX_CHUNK).is_ok());
        assert_eq!(Cursor::new(MAX_SIZE + 1, 1), Err(OpenError::Size));
        let c = Cursor::new(MAX_SIZE, MAX_CHUNK).unwrap();
        assert_eq!(c.size(), MAX_SIZE);
        assert_eq!(c.position(), 0);
    }

    #[test]
    fn peek_does_not_move() {
        let c = Cursor::new(300, 256).unwrap();
        let a = c.peek(nz(1000)).unwrap();
        let b = c.peek(nz(1000)).unwrap();
        assert_eq!(a.len(), 256);
        assert_eq!(a, b);
        assert_eq!(c.position(), 0);
    }

    #[test]
    fn largest_chunk_at_every_phase() {
        // The slice starts at pos mod 251; check the table bound for the worst phase.
        let mut c = Cursor::new(250 + 65_536, MAX_CHUNK).unwrap();
        c.advance(250);
        let s = c.next_slice(nz(65_536)).unwrap();
        assert_eq!(s.len(), 65_536);
        assert_eq!(s[0], byte_at(250));
        assert_eq!(s[65_535], byte_at(250 + 65_535));
    }

    #[test]
    fn stream_counts_only_successful_copies() {
        let mut s = Stream::new(Cursor::new(10, 4).unwrap());
        let r: Result<Option<usize>, &str> = s.read_with(nz(8), |_| Err("no"));
        assert_eq!(r, Err("no"));
        assert_eq!(s.copied(), 0);
        assert_eq!(s.cursor().position(), 0);
        let mut got = Vec::new();
        while let Some(n) = s
            .read_with(nz(8), |b| {
                got.extend_from_slice(b);
                Ok::<(), ()>(())
            })
            .unwrap()
        {
            assert!((1..=4).contains(&n));
        }
        assert_eq!(got, (0..10_u64).map(byte_at).collect::<Vec<_>>());
        assert_eq!(s.copied(), 10);
        assert_eq!(s.read_with(nz(8), |_| Ok::<(), ()>(())), Ok(None));
        assert_eq!(s.copied(), 10);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn concatenated_chunks_equal_document(size in 0_u64..=4 << 20, chunk in 1_u32..=65_536) {
            let mut c = Cursor::new(size, chunk).unwrap();
            let mut i = 0_u64;
            let mut reads = 0_u64;
            while let Some(s) = c.next_slice(nz(chunk as usize)) {
                prop_assert!(!s.is_empty() && s.len() <= chunk as usize);
                // Every chunk but the last is full.
                if (i + s.len() as u64) < size {
                    prop_assert_eq!(s.len(), chunk as usize);
                }
                for b in s {
                    prop_assert_eq!(*b, byte_at(i));
                    i += 1;
                }
                reads += 1;
            }
            prop_assert_eq!(i, size);
            prop_assert_eq!(reads, size.div_ceil(u64::from(chunk)));
            prop_assert_eq!(c.next_slice(nz(1)), None);
        }
    }
}
