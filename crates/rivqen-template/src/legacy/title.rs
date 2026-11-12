// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The title rule C-06 in linear time (C-16, DL-077).
//!
//! The rule is the first match of `<title([^\n]*?)</title>`, ASCII
//! case-insensitive. A match at an opener `<title` at `i` exists exactly
//! when the first `</title>` at or after `i + 6` has no LF before it (the
//! lazy group takes the shortest content, and a later `</title>` has the
//! same LF before it). The scan tries the openers from left to right; the
//! next `</title>` and the next LF are kept between openers, so each byte
//! is read a bounded number of times.

use super::text::{find_byte, find_ci};

const OPEN: &[u8] = b"<title";
const CLOSE: &[u8] = b"</title>";

/// The next position `>= x` of a pattern, kept between queries with a
/// growing `x`.
#[derive(Default)]
struct Next {
    /// The query of the last search, and its result.
    last: Option<(usize, Option<usize>)>,
}

impl Next {
    fn get(&mut self, x: usize, search: impl Fn(usize) -> Option<usize>) -> Option<usize> {
        if let Some((from, found)) = self.last {
            // No match in `from..found`, so the result holds for every
            // `x` in `from..=found` (for every `x >= from` when none).
            if from <= x && found.is_none_or(|at| x <= at) {
                return found;
            }
        }
        let found = search(x);
        self.last = Some((x, found));
        found
    }
}

/// The byte range of the first C-06 match in `html`, or `None`.
pub(super) fn find_title(html: &[u8]) -> Option<(usize, usize)> {
    let mut closes = Next::default();
    let mut newlines = Next::default();
    let mut from = 0;
    loop {
        let open = find_ci(html, from, OPEN)?;
        let content = open.checked_add(OPEN.len())?;
        // No `</title>` after this opener: none after a later one either.
        let close = closes.get(content, |x| find_ci(html, x, CLOSE))?;
        let newline = newlines.get(content, |x| find_byte(html, x, b'\n'));
        if newline.is_none_or(|at| at > close) {
            return Some((open, close.checked_add(CLOSE.len())?));
        }
        from = open.checked_add(1)?;
    }
}

#[cfg(test)]
mod tests {
    use super::find_title;

    #[test]
    fn title_needs_close_after_the_opener_name() {
        assert_eq!(find_title(b"<title</title>"), Some((0, 14)));
        assert_eq!(find_title(b"</title><title"), None);
        assert_eq!(
            find_title(b"<title>\n</title><title>b</title>"),
            Some((16, 32))
        );
    }
}
