// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The block rule C-07 in linear time (C-16, DL-077) and the build of the
//! split (C-08).
//!
//! The expression is
//! `<!--sonicdiff-?(\w*)-->([\s\S]+?)<!--sonicdiff-?(\w*)-end-->`, ASCII
//! case-insensitive, matched from left to right without overlap. The scan
//! gives the same matches:
//!
//! - A marker is `<!--sonicdiff`, then `-?(\w*)`, then a tail (`-->` for the
//!   opener, `-end-->` for the end marker). `\w` does not match `-`, so the
//!   name is the whole word run and the tail must follow it. A backtracking
//!   engine first takes the `-` (greedy `?`) and then tries without it; only
//!   the second form has an empty name. [`marker`] tries the two forms in
//!   that order.
//! - The content has at least one byte, so the end marker search starts one
//!   byte after the opener. The first end marker found ends the match (lazy
//!   content).
//! - An opener without an end marker after it ends the scan: a later opener
//!   starts after it and needs an end marker even later.
//!
//! Each `<` is a candidate once for an opener or once for an end marker, and
//! a word run has no `<`, so the scan reads each byte a bounded number of
//! times.

use std::collections::BTreeSet;

use super::text::{TITLE_KEY, Text};
use super::{DataBlock, Diagnostic, LimitError, NotApplied, Split};
use crate::limits::TemplateLimits;

const MARK: &[u8] = b"<!--sonicdiff";
const OPEN_TAIL: &[u8] = b"-->";
const END_TAIL: &[u8] = b"-end-->";
/// The block name that collides with the title key (C-07, DL-076).
const TITLE_NAME: &[u8] = b"title";

/// Why the split stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Stop {
    NotApplied(NotApplied),
    Limit(LimitError),
}

/// A marker: the name range and the end of the marker.
#[derive(Debug, Clone, Copy)]
struct Marker {
    name_start: usize,
    name_end: usize,
    end: usize,
}

/// The marker with `tail` whose `<!--sonicdiff` is at `at`.
fn marker(text: &Text<'_>, at: usize, tail: &[u8]) -> Option<Marker> {
    let after = at.checked_add(MARK.len())?;
    let mut forms = [None, Some(after)];
    if text.byte(after) == Some(b'-') {
        forms[0] = Some(after.checked_add(1)?);
    }
    forms.into_iter().flatten().find_map(|name_start| {
        let name_end = text.word_end(name_start);
        text.eq_ci_at(name_end, tail).then_some(())?;
        Some(Marker {
            name_start,
            name_end,
            end: name_end.checked_add(tail.len())?,
        })
    })
}

/// One match of the block expression.
#[derive(Debug, Clone, Copy)]
struct Match {
    start: usize,
    open: Marker,
    close: Marker,
}

/// The next match at or after `from`, or `None` when no more matches exist.
fn next_match(text: &Text<'_>, from: usize) -> Option<Match> {
    let mut pos = from;
    loop {
        let start = text.find_ci(pos, MARK)?;
        let Some(open) = marker(text, start, OPEN_TAIL) else {
            pos = start.checked_add(1)?;
            continue;
        };
        // At least one byte of content.
        let mut search = open.end.checked_add(1)?;
        loop {
            // No end marker: no later opener can match either (C-16).
            let at = text.find_ci(search, MARK)?;
            if let Some(close) = marker(text, at, END_TAIL) {
                return Some(Match { start, open, close });
            }
            search = at.checked_add(1)?;
        }
    }
}

/// `true` when the logical ranges hold the same bytes.
fn same_bytes(text: &Text<'_>, a: (usize, usize), b: (usize, usize)) -> bool {
    let (Some(a_len), Some(b_len)) = (a.1.checked_sub(a.0), b.1.checked_sub(b.0)) else {
        return false;
    };
    a_len == b_len
        && (0..a_len).all(|k| {
            let (Some(x), Some(y)) = (a.0.checked_add(k), b.0.checked_add(k)) else {
                return false;
            };
            text.byte(x) == text.byte(y)
        })
}

/// A block that gets a key.
struct Keyed {
    start: usize,
    end: usize,
    key: String,
}

/// The split of `text` (the result of C-06) with the title value `title`.
pub(super) fn split(text: &Text<'_>, title: &str, limits: &TemplateLimits) -> Result<Split, Stop> {
    let mut blocks: Vec<Keyed> = Vec::new();
    let mut names: BTreeSet<Vec<u8>> = BTreeSet::new();
    let mut diagnostics = Vec::new();
    let mut pos = 0;
    while let Some(found) = next_match(text, pos) {
        pos = found.close.end;
        let name = (found.open.name_start, found.open.name_end);
        let name_len = name.1.saturating_sub(name.0);
        if name_len == 0 || name_len > limits.max_name_bytes() {
            // The match stays in the template, with no key (C-07, D-05).
            continue;
        }
        let name_bytes: Vec<u8> = (name.0..name.1).filter_map(|at| text.byte(at)).collect();
        if names.contains(&name_bytes) {
            return Err(Stop::NotApplied(NotApplied::DuplicateName));
        }
        if name_bytes == TITLE_NAME {
            return Err(Stop::NotApplied(NotApplied::BlockNamedTitle));
        }
        if blocks.len() >= limits.max_blocks() {
            return Err(Stop::Limit(LimitError::TooManyBlocks));
        }
        if found.close.end.saturating_sub(found.start) > limits.max_block_bytes() {
            return Err(Stop::Limit(LimitError::BlockTooLarge));
        }
        let mut key = String::with_capacity(name_len.saturating_add(2));
        key.push('{');
        key.extend(name_bytes.iter().map(|byte| char::from(*byte)));
        key.push('}');
        if !same_bytes(text, name, (found.close.name_start, found.close.name_end)) {
            diagnostics.push(Diagnostic::MismatchedEndName {
                index: blocks.len().saturating_add(1),
            });
        }
        names.insert(name_bytes);
        blocks.push(Keyed {
            start: found.start,
            end: found.close.end,
            key,
        });
    }
    build(text, title, &blocks, diagnostics)
}

/// The template and the data of the keyed blocks (C-08).
fn build(
    text: &Text<'_>,
    title: &str,
    blocks: &[Keyed],
    diagnostics: Vec<Diagnostic>,
) -> Result<Split, Stop> {
    // A cut is not on a character boundary: not possible (see `push_range`).
    let broken = Stop::NotApplied(NotApplied::InvalidUtf8);
    let mut template = String::with_capacity(text.len());
    let mut data = Vec::with_capacity(blocks.len().saturating_add(1));
    data.push(DataBlock {
        key: TITLE_KEY.to_owned(),
        value: title.to_owned(),
    });
    let mut cursor = 0;
    for block in blocks {
        text.push_range(&mut template, cursor, block.start)
            .ok_or(broken)?;
        template.push_str(&block.key);
        let mut value = String::with_capacity(block.end.saturating_sub(block.start));
        text.push_range(&mut value, block.start, block.end)
            .ok_or(broken)?;
        data.push(DataBlock {
            key: block.key.clone(),
            value,
        });
        cursor = block.end;
    }
    text.push_range(&mut template, cursor, text.len())
        .ok_or(broken)?;
    Ok(Split {
        template,
        data,
        diagnostics,
    })
}
