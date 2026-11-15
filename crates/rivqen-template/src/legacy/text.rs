// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The text that the block rule C-07 reads, and the byte helpers of the
//! scanner (C-16).
//!
//! C-07 runs on the result of C-06: the body with the title match replaced
//! by `{title}`. [`Text`] is that result as three borrowed parts (before
//! the title match, `{title}` or nothing, after the title match), so the
//! scanner reads it without a copy of the page. Positions are logical
//! positions in the joined text.
//!
//! Every pattern of this module starts with `<`, which has no case, so the
//! first-byte search is exact. A pattern compare reads at most the pattern
//! length, so a search is linear in the bytes it passes.

/// The placeholder that C-06 puts in the text.
pub(super) const TITLE_KEY: &str = "{title}";

/// `true` for a byte of `\w` (`[A-Za-z0-9_]`, legacy wire C-07).
pub(super) const fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// `true` when `pattern` (lowercase ASCII) is at `at` in `bytes`, ASCII
/// case-insensitive.
pub(super) fn eq_ci_at(bytes: &[u8], at: usize, pattern: &[u8]) -> bool {
    let Some(end) = at.checked_add(pattern.len()) else {
        return false;
    };
    bytes
        .get(at..end)
        .is_some_and(|window| window.eq_ignore_ascii_case(pattern))
}

/// The first position `>= from` of `byte` in `bytes`.
pub(super) fn find_byte(bytes: &[u8], from: usize, byte: u8) -> Option<usize> {
    let offset = bytes.get(from..)?.iter().position(|b| *b == byte)?;
    from.checked_add(offset)
}

/// The first position `>= from` of `pattern` (lowercase ASCII, first byte
/// without case) in `bytes`, ASCII case-insensitive.
pub(super) fn find_ci(bytes: &[u8], from: usize, pattern: &[u8]) -> Option<usize> {
    let first = *pattern.first()?;
    let mut pos = from;
    loop {
        let at = find_byte(bytes, pos, first)?;
        if eq_ci_at(bytes, at, pattern) {
            return Some(at);
        }
        pos = at.checked_add(1)?;
    }
}

/// The result of C-06 as borrowed parts.
#[derive(Debug, Clone, Copy)]
pub(super) struct Text<'a> {
    head: &'a str,
    mid: &'a str,
    tail: &'a str,
    /// Logical start of `mid`.
    mid_start: usize,
    /// Logical start of `tail`.
    tail_start: usize,
    /// Logical length.
    len: usize,
}

impl<'a> Text<'a> {
    /// The text of `html` with the byte range `title` (a C-06 match)
    /// replaced by `{title}`. `None` when the range is not on character
    /// boundaries of `html` (not possible for a C-06 match: it starts at
    /// `<` and ends at `>`).
    pub(super) fn new(html: &'a str, title: Option<(usize, usize)>) -> Option<Self> {
        let (head, mid, tail) = match title {
            Some((start, end)) => (html.get(..start)?, TITLE_KEY, html.get(end..)?),
            None => (html, "", ""),
        };
        let mid_start = head.len();
        let tail_start = mid_start.checked_add(mid.len())?;
        let len = tail_start.checked_add(tail.len())?;
        Some(Self {
            head,
            mid,
            tail,
            mid_start,
            tail_start,
            len,
        })
    }

    /// Logical length.
    pub(super) const fn len(&self) -> usize {
        self.len
    }

    /// The parts with their logical start.
    fn parts(&self) -> [(usize, &'a str); 3] {
        [
            (0, self.head),
            (self.mid_start, self.mid),
            (self.tail_start, self.tail),
        ]
    }

    /// The byte at logical position `at`.
    pub(super) fn byte(&self, at: usize) -> Option<u8> {
        let (start, part) = if at < self.mid_start {
            (0, self.head)
        } else if at < self.tail_start {
            (self.mid_start, self.mid)
        } else {
            (self.tail_start, self.tail)
        };
        part.as_bytes().get(at.checked_sub(start)?).copied()
    }

    /// `true` when `pattern` (lowercase ASCII) is at `at`, ASCII
    /// case-insensitive. Reads byte by byte, so a pattern across two parts
    /// compares like one in a part.
    pub(super) fn eq_ci_at(&self, at: usize, pattern: &[u8]) -> bool {
        let mut pos = at;
        for expected in pattern {
            match self.byte(pos) {
                Some(byte) if byte.to_ascii_lowercase() == *expected => {}
                _ => return false,
            }
            let Some(next) = pos.checked_add(1) else {
                return false;
            };
            pos = next;
        }
        true
    }

    /// The first position `>= from` of `pattern` (lowercase ASCII, first
    /// byte without case), ASCII case-insensitive.
    pub(super) fn find_ci(&self, from: usize, pattern: &[u8]) -> Option<usize> {
        let first = *pattern.first()?;
        let mut pos = from;
        loop {
            let at = self.find_byte(pos, first)?;
            if self.eq_ci_at(at, pattern) {
                return Some(at);
            }
            pos = at.checked_add(1)?;
        }
    }

    /// The first logical position `>= from` of `byte`.
    fn find_byte(&self, from: usize, byte: u8) -> Option<usize> {
        for (start, part) in self.parts() {
            let local = from.saturating_sub(start);
            if let Some(found) = find_byte(part.as_bytes(), local, byte) {
                return start.checked_add(found);
            }
        }
        None
    }

    /// The end of the run of `\w` bytes that starts at `from`.
    pub(super) fn word_end(&self, from: usize) -> usize {
        let mut pos = from;
        while self.byte(pos).is_some_and(is_word) {
            match pos.checked_add(1) {
                Some(next) => pos = next,
                None => break,
            }
        }
        pos
    }

    /// Append the logical range `start..end` to `out`. `None` when a cut is
    /// not on a character boundary (not possible for the cuts of the
    /// scanner: each is next to an ASCII byte of a marker or of `{title}`).
    pub(super) fn push_range(&self, out: &mut String, start: usize, end: usize) -> Option<()> {
        for (part_start, part) in self.parts() {
            let part_end = part_start.checked_add(part.len())?;
            let from = start.max(part_start);
            let to = end.min(part_end);
            if from < to {
                out.push_str(part.get(from.checked_sub(part_start)?..to.checked_sub(part_start)?)?);
            }
        }
        Some(())
    }
}
