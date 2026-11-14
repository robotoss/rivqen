// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The rebuild rule of legacy markers section 5 (canonical).
//!
//! The template is text: nothing marks a placeholder apart from the same
//! text written in the page. The canonical rule replaces each placeholder
//! exactly once, in template order, and the caller then checks
//! `html-sha1` (legacy client CL-29). This module makes the rule exact:
//!
//! 1. The placeholder of a data entry is the **first** occurrence of its key
//!    in the template. So the output does not depend on the order of the
//!    data, and a page with the key text before its block rebuilds to
//!    another page, which the `html-sha1` check rejects (fixture
//!    `FX-LEG-MAL-PLACEHOLDER-TEXT`; `split.rebuild_sha1` of
//!    `fixtures/legacy/README.md`).
//! 2. The entry `{title}` with an empty value has no placeholder: C-06 gives
//!    an empty value only when no title matches, and then a `{title}` in the
//!    template is page text.
//! 3. A value is copied as it is; a key text inside a value is not replaced.
//!
//! The data does not fit ([`RenderError::Mismatch`]) when a key is not
//! `{title}` or `{name}` with a name of 1 to `max_name_bytes` bytes of
//! `[A-Za-z0-9_]`, when a key repeats, when `{title}` is missing, or when an
//! entry that needs a placeholder has none.
//!
//! The scan reads the template once: at each `{` it reads the word run and
//! the `}`, and it looks the token up among the keys not placed yet. Keys
//! cannot overlap (a key ends with `}` and starts with `{`), so the first
//! token of a key is its first occurrence.

use std::collections::BTreeMap;

use super::text::{TITLE_KEY, find_byte, is_word};
use super::{DataBlock, LimitError, RenderError};
use crate::limits::TemplateLimits;

/// `true` for a block key `{name}`, name `[A-Za-z0-9_]{1,max_name_bytes}`.
fn is_block_key(key: &str, max_name_bytes: usize) -> bool {
    let Some(name) = key
        .strip_prefix('{')
        .and_then(|rest| rest.strip_suffix('}'))
    else {
        return false;
    };
    !name.is_empty() && name.len() <= max_name_bytes && name.bytes().all(is_word)
}

/// The entries that need a placeholder, by key, and the longest key.
fn wanted<'d>(
    data: &'d [DataBlock],
    limits: &TemplateLimits,
) -> Result<(BTreeMap<&'d [u8], &'d DataBlock>, usize), RenderError> {
    let mut wanted = BTreeMap::new();
    let mut title = false;
    let mut longest = 0;
    for entry in data {
        if entry.key == TITLE_KEY {
            if title {
                return Err(RenderError::Mismatch);
            }
            title = true;
            if entry.value.is_empty() {
                continue;
            }
        } else {
            if !is_block_key(&entry.key, limits.max_name_bytes()) {
                return Err(RenderError::Mismatch);
            }
            if entry.value.len() > limits.max_block_bytes() {
                return Err(RenderError::Limit(LimitError::BlockTooLarge));
            }
        }
        if wanted.insert(entry.key.as_bytes(), entry).is_some() {
            return Err(RenderError::Mismatch);
        }
        longest = longest.max(entry.key.len());
    }
    if title {
        Ok((wanted, longest))
    } else {
        Err(RenderError::Mismatch)
    }
}

/// The placeholders: position in the template and entry, in template order.
fn place<'d>(
    template: &[u8],
    mut wanted: BTreeMap<&'d [u8], &'d DataBlock>,
    longest: usize,
) -> Result<Vec<(usize, &'d DataBlock)>, RenderError> {
    let mut places = Vec::with_capacity(wanted.len());
    let mut pos = 0;
    while !wanted.is_empty() {
        let Some(open) = find_byte(template, pos, b'{') else {
            break;
        };
        let name_start = open.checked_add(1).ok_or(RenderError::Mismatch)?;
        // A run longer than every key is not a key; stop reading it there.
        let mut close = name_start;
        while close.saturating_sub(open) < longest
            && template.get(close).copied().is_some_and(is_word)
        {
            close = close.checked_add(1).ok_or(RenderError::Mismatch)?;
        }
        if template.get(close) == Some(&b'}') {
            let end = close.checked_add(1).ok_or(RenderError::Mismatch)?;
            let token = template.get(open..end).ok_or(RenderError::Mismatch)?;
            if let Some(entry) = wanted.remove(token) {
                places.push((open, entry));
            }
            pos = end;
        } else {
            // The run has no `{`: the next candidate is after it.
            pos = close.max(name_start);
        }
    }
    if wanted.is_empty() {
        Ok(places)
    } else {
        Err(RenderError::Mismatch)
    }
}

/// Rebuild a page (see the module text).
pub(super) fn render(
    template: &str,
    data: &[DataBlock],
    limits: &TemplateLimits,
) -> Result<String, RenderError> {
    let too_large = RenderError::Limit(LimitError::DocumentTooLarge);
    if template.len() > limits.max_document_bytes() {
        return Err(too_large);
    }
    if data.len() > limits.max_blocks().saturating_add(1) {
        return Err(RenderError::Limit(LimitError::TooManyBlocks));
    }
    let (wanted, longest) = wanted(data, limits)?;
    let places = place(template.as_bytes(), wanted, longest)?;
    let mut len = template.len();
    for (_, entry) in &places {
        len = len
            .checked_sub(entry.key.len())
            .and_then(|rest| rest.checked_add(entry.value.len()))
            .ok_or(too_large)?;
    }
    if len > limits.max_document_bytes() {
        return Err(too_large);
    }
    let mut out = String::with_capacity(len);
    let mut cursor = 0;
    for (at, entry) in places {
        out.push_str(template.get(cursor..at).ok_or(RenderError::Mismatch)?);
        out.push_str(&entry.value);
        cursor = at
            .checked_add(entry.key.len())
            .ok_or(RenderError::Mismatch)?;
    }
    out.push_str(template.get(cursor..).ok_or(RenderError::Mismatch)?);
    Ok(out)
}
