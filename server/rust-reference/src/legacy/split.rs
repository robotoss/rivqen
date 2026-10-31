// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Template and data split of a legacy page: the title rule C-06, the block
//! rule C-07 and the template C-08 (legacy wire §10), with the profile rules
//! P-09 and P-10 (§10.1).
//!
//! The regular expressions of C-06 and C-07 define the result; this module
//! does not run them. It scans forward (C-16, DL-077): every search position
//! only moves forward, so the time is linear in the page length.
//!
//! The scans work on the UTF-8 bytes of the page. Every pattern is ASCII, and
//! a UTF-8 multi-byte sequence never contains an ASCII byte, so each match
//! starts and ends on a character boundary. "At least one character" of block
//! content is "at least one byte" for the same reason.

use std::collections::HashMap;

use crate::model::Profile;

/// The data key of the title rule C-06.
pub const TITLE_KEY: &str = "{title}";

/// Longest block name of `legacy-canonical` (C-07; the profiles have no limit, P-09).
const MAX_NAME: usize = 128;

const TITLE_OPEN: &[u8] = b"<title";
const TITLE_CLOSE: &[u8] = b"</title>";
const MARKER: &[u8] = b"<!--sonicdiff";
const OPEN_TAIL: &[u8] = b"-->";
const END_TAIL: &[u8] = b"-end-->";

/// The split of one page under one profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Split {
    /// The template (C-08): the page with the title match replaced by
    /// `{title}` and each extracted block replaced by its key.
    pub template: String,
    /// The data map in order: `{title}` first, then the block keys in
    /// document order. A key occurs once (P-10: a repeat keeps the later value
    /// at the position of the first).
    pub data: Vec<(String, String)>,
    /// The number of block matches that got a key (also the repeats).
    pub blocks: usize,
}

/// The split of `text` under `profile`, or `None` when the split fails
/// (`legacy-canonical` only: a repeated key, also a block named `title`; C-07).
#[must_use]
pub fn split(profile: Profile, text: &str) -> Option<Split> {
    let (stage, title_value) = match find_title(text.as_bytes()) {
        Some((start, end)) => {
            let mut s = String::with_capacity(text.len());
            s.push_str(text.get(..start)?);
            s.push_str(TITLE_KEY);
            s.push_str(text.get(end..)?);
            (s, text.get(start..end)?.to_owned())
        }
        None => (text.to_owned(), String::new()),
    };
    let mut data = DataMap::default();
    data.insert(TITLE_KEY.to_owned(), title_value);

    let mut template = String::with_capacity(stage.len());
    let mut copied = 0usize;
    let mut empty_names = 0usize;
    let mut blocks = 0usize;
    for m in find_blocks(stage.as_bytes()) {
        let name = stage.get(m.name_start..m.name_end)?;
        let key = match profile {
            Profile::Canonical if name.is_empty() || name.len() > MAX_NAME => continue,
            Profile::Node if name.is_empty() => {
                let key = format!("{{auto{empty_names}}}");
                empty_names = empty_names.saturating_add(1);
                key
            }
            _ => format!("{{{name}}}"),
        };
        if profile == Profile::Canonical && data.contains(&key) {
            return None;
        }
        template.push_str(stage.get(copied..m.start)?);
        template.push_str(&key);
        data.insert(key, stage.get(m.start..m.end)?.to_owned());
        copied = m.end;
        blocks = blocks.saturating_add(1);
    }
    template.push_str(stage.get(copied..)?);
    Some(Split {
        template,
        data: data.entries,
        blocks,
    })
}

/// Ordered map with "the later value at the position of the first" (P-10).
#[derive(Default)]
struct DataMap {
    entries: Vec<(String, String)>,
    index: HashMap<String, usize>,
}

impl DataMap {
    fn contains(&self, key: &str) -> bool {
        self.index.contains_key(key)
    }

    fn insert(&mut self, key: String, value: String) {
        if let Some(slot) = self.index.get(&key).and_then(|&i| self.entries.get_mut(i)) {
            slot.1 = value;
            return;
        }
        self.index.insert(key.clone(), self.entries.len());
        self.entries.push((key, value));
    }
}

/// The span `start..end` of the first match of `<title([^\n]*?)</title>`
/// (ASCII case-insensitive), by a forward scan (C-06, C-16).
///
/// For each `<title` opener, in order, the lazy group ends at the first
/// `</title>` after the opener; the opener matches when no LF comes before
/// that `</title>`. The positions of the next `</title>` and the next LF are
/// kept between openers and only move forward.
pub(crate) fn find_title(h: &[u8]) -> Option<(usize, usize)> {
    let mut from = 0usize;
    let mut close: Option<usize> = None;
    let mut lf = Lookahead::Unknown;
    loop {
        let open = find_ci(h, from, TITLE_OPEN)?;
        let after = open.saturating_add(TITLE_OPEN.len());
        let c = match close {
            Some(c) if c >= after => c,
            // No `</title>` after this opener: none after a later one either.
            _ => find_ci(h, after, TITLE_CLOSE)?,
        };
        close = Some(c);
        let next_lf = match lf {
            Lookahead::At(l) if l >= after => Some(l),
            Lookahead::Never => None,
            _ => {
                let l = find_byte(h, after, b'\n');
                lf = l.map_or(Lookahead::Never, Lookahead::At);
                l
            }
        };
        if next_lf.is_none_or(|l| l > c) {
            return Some((open, c.saturating_add(TITLE_CLOSE.len())));
        }
        // `<title` cannot overlap itself: the next opener starts after this one.
        from = after;
    }
}

/// A cached forward search for one byte.
#[derive(Clone, Copy)]
enum Lookahead {
    /// Not searched yet.
    Unknown,
    /// Found at this position.
    At(usize),
    /// Not present at or after the last search start.
    Never,
}

/// One match of the block expression of C-07.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BlockMatch {
    /// Start of the opening marker.
    pub start: usize,
    /// The name (group 1) of the opening marker.
    pub name_start: usize,
    /// End of the name.
    pub name_end: usize,
    /// End of the end marker (exclusive).
    pub end: usize,
}

/// The matches of
/// `<!--sonicdiff-?(\w*)-->([\s\S]+?)<!--sonicdiff-?(\w*)-end-->`
/// (ASCII case-insensitive), left to right, without overlap, by a forward
/// scan (C-07, C-16).
///
/// For each opening marker, the lazy content ends at the first end marker
/// that starts at least one byte after the opening marker. When no end marker
/// follows an opening marker, no end marker follows a later one either, so
/// the scan stops.
pub(crate) fn find_blocks(h: &[u8]) -> Vec<BlockMatch> {
    let mut out = Vec::new();
    let mut from = 0usize;
    'openers: while let Some(open) = find_ci(h, from, MARKER) {
        let Some((name_start, name_end, open_end)) = parse_marker(h, open, OPEN_TAIL) else {
            // `<!--sonicdiff` cannot overlap itself.
            from = open.saturating_add(MARKER.len());
            continue;
        };
        let mut q = open_end.saturating_add(1);
        loop {
            let Some(k) = find_ci(h, q, MARKER) else {
                break 'openers;
            };
            if let Some((_, _, end)) = parse_marker(h, k, END_TAIL) {
                out.push(BlockMatch {
                    start: open,
                    name_start,
                    name_end,
                    end,
                });
                from = end;
                continue 'openers;
            }
            q = k.saturating_add(MARKER.len());
        }
    }
    out
}

/// The marker `<!--sonicdiff-?(\w*)<tail>` at `at` (where `<!--sonicdiff`
/// starts): the name span and the end of the marker.
///
/// The expression tries `-?` with the dash first. `\w*` is greedy, and the
/// tail starts with `-` (not a word character), so the name is the whole
/// word run. Without the dash, the name starts at the `-`, so it is empty
/// and the tail must start there. At most one of the two forms matches.
fn parse_marker(h: &[u8], at: usize, tail: &[u8]) -> Option<(usize, usize, usize)> {
    let p = at.checked_add(MARKER.len())?;
    if h.get(p) == Some(&b'-') {
        let s = p.saturating_add(1);
        let e = word_end(h, s);
        if at_ci(h, e, tail) {
            return Some((s, e, e.saturating_add(tail.len())));
        }
        return at_ci(h, p, tail).then(|| (p, p, p.saturating_add(tail.len())));
    }
    let e = word_end(h, p);
    at_ci(h, e, tail).then(|| (p, e, e.saturating_add(tail.len())))
}

/// The end of the run of `[A-Za-z0-9_]` that starts at `from`.
fn word_end(h: &[u8], from: usize) -> usize {
    match h.get(from..) {
        Some(rest) => {
            let run = rest
                .iter()
                .take_while(|b| b.is_ascii_alphanumeric() || **b == b'_')
                .count();
            from.saturating_add(run)
        }
        None => from,
    }
}

/// True when `pat` (lowercase ASCII) is at `at`, ASCII case-insensitively.
fn at_ci(h: &[u8], at: usize, pat: &[u8]) -> bool {
    at.checked_add(pat.len())
        .and_then(|end| h.get(at..end))
        .is_some_and(|s| s.eq_ignore_ascii_case(pat))
}

/// The first position at or after `from` where `pat` starts (ASCII
/// case-insensitive). `pat` starts with `<`, which occurs nowhere else in
/// it, so the search never needs to go back.
fn find_ci(h: &[u8], from: usize, pat: &[u8]) -> Option<usize> {
    let first = *pat.first()?;
    let mut i = from;
    loop {
        let at = find_byte(h, i, first)?;
        if at_ci(h, at, pat) {
            return Some(at);
        }
        i = at.saturating_add(1);
    }
}

/// The first position at or after `from` of `byte`.
fn find_byte(h: &[u8], from: usize, byte: u8) -> Option<usize> {
    h.get(from..)?
        .iter()
        .position(|&b| b == byte)
        .map(|p| from.saturating_add(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_first_match_and_lf_rule() {
        assert_eq!(find_title(b"x<title>a</title>"), Some((1, 17)));
        assert_eq!(find_title(b"<TITLE>a</tItLe>"), Some((0, 16)));
        assert_eq!(find_title(b"<title</title>"), Some((0, 14)));
        assert_eq!(find_title(b"<title>a\nb</title>"), None);
        // The first opener fails on its LF, the second matches.
        assert_eq!(find_title(b"<title>\n<title>b</title>"), Some((8, 24)));
        assert_eq!(find_title(b"<title>\r</title>"), Some((0, 16)));
        assert_eq!(find_title(b"</title><title>"), None);
        assert_eq!(find_title(b""), None);
        // An LF after the close does not stop the match.
        assert_eq!(find_title(b"<title>a</title>\n"), Some((0, 16)));
        // Two openers before one close: the first wins.
        assert_eq!(find_title(b"<title><title>x</title>"), Some((0, 23)));
    }

    #[test]
    fn title_cached_close_is_reused_only_after_the_opener() {
        // Opener 1 fails (LF), opener 2 comes after the first close: a new close.
        let h = b"<title>\n</title><title>z</title>";
        assert_eq!(find_title(h), Some((16, 32)));
        // Opener 2 is before the cached close: the cached close is used.
        let h = b"<title>\n<title>z</title>";
        assert_eq!(find_title(h), Some((8, 24)));
    }

    #[test]
    fn markers_parse_both_dash_forms() {
        let m = |s: &[u8], tail: &[u8]| parse_marker(s, 0, tail);
        assert_eq!(m(b"<!--sonicdiff-a-->", OPEN_TAIL), Some((14, 15, 18)));
        assert_eq!(m(b"<!--sonicdiffa-->", OPEN_TAIL), Some((13, 14, 17)));
        assert_eq!(m(b"<!--sonicdiff-->", OPEN_TAIL), Some((13, 13, 16)));
        assert_eq!(m(b"<!--sonicdiff--->", OPEN_TAIL), Some((14, 14, 17)));
        assert_eq!(m(b"<!--sonicdiff-a-b-->", OPEN_TAIL), None);
        assert_eq!(m(b"<!--sonicdiff-end-->", OPEN_TAIL), Some((14, 17, 20)));
        assert_eq!(m(b"<!--sonicdiff-end-->", END_TAIL), Some((13, 13, 20)));
        assert_eq!(m(b"<!--sonicdiff-a-end-->", END_TAIL), Some((14, 15, 22)));
        assert_eq!(m(b"<!--sonicdiffa-END-->", END_TAIL), Some((13, 14, 21)));
        assert_eq!(m(b"<!--sonicdiff-a-end-->", OPEN_TAIL), None);
        assert_eq!(m(b"<!--sonicdiff", OPEN_TAIL), None);
        assert_eq!(m(b"<!--sonicdiff-", OPEN_TAIL), None);
    }

    #[test]
    fn blocks_need_content_and_do_not_overlap() {
        let h = b"<!--sonicdiff-a--><!--sonicdiff-a-end-->x<!--sonicdiff-b-end-->";
        let got = find_blocks(h);
        assert_eq!(got.len(), 1);
        assert_eq!(got.first().map(|m| (m.start, m.end)), Some((0, h.len())));
        // No end marker after an opener: the scan stops.
        assert_eq!(
            find_blocks(b"<!--sonicdiff-a-->x<!--sonicdiff-b-->y"),
            vec![]
        );
        // A failed opener is skipped.
        let h = b"<!--sonicdiff-a b--><!--sonicdiff-c-->x<!--sonicdiff-end-->";
        let got = find_blocks(h);
        assert_eq!(
            got.first().map(|m| (m.start, m.name_start, m.name_end)),
            Some((20, 34, 35))
        );
    }

    #[test]
    fn data_map_keeps_the_first_position() {
        let mut d = DataMap::default();
        d.insert("a".into(), "1".into());
        d.insert("b".into(), "2".into());
        d.insert("a".into(), "3".into());
        assert_eq!(
            d.entries,
            vec![("a".into(), "3".into()), ("b".into(), "2".into())]
        );
        assert!(d.contains("b"));
        assert!(!d.contains("c"));
    }

    #[test]
    fn word_end_stops_at_non_word_and_end() {
        assert_eq!(word_end(b"ab_9-x", 0), 4);
        assert_eq!(word_end(b"ab", 0), 2);
        assert_eq!(word_end(b"ab", 5), 5);
        assert_eq!(word_end("é".as_bytes(), 0), 0);
    }
}
