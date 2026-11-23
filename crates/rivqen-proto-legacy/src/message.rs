// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The page message of a result (legacy client CL-22, CL-24): the exact JSON
//! text.
//!
//! Owner: WP-08 S1, T-01.
//!
//! # Rules (CL-23 item 1, CL-24, CL-28; function `message`)
//!
//! [`message`] gives no text for a final code `-1` (CL-23 item 1) and for a
//! final `200` with an empty diff (CL-28). Else the text is the JSON object
//! `{"result":…,"code":…,"srcCode":…,"extra":{…}}` without white space:
//!
//! - `result`: only for a final `200`: the JSON text of the diff object
//!   (members in the order of the diff, the order of the server data) as a
//!   string.
//! - `code`, `srcCode`: the final and the source code as integers.
//! - `extra`: the etag field under the key `eTag` (or the name that
//!   `sonic-etag-key` gives, as written), then `template-tag`, then
//!   `cache-offline` (the raw value): each one only when the response that
//!   set the result has a line of that field (the value of the first line,
//!   without OWS at its ends); then `isReload`: `"true"` in a refresh
//!   generation (CL-35), else `"false"`.
//!
//! Strings are written as `JSON.stringify` writes them: `"` and `\` escaped,
//! `\b`, `\f`, `\n`, `\r`, `\t`, other code points below U+0020 as `\u00xx`
//! (lowercase hex); `/` and every other character, also non-ASCII, as is.
//! `local_refresh_time` is not part of the text (CONTRACT §6); the adapter
//! adds it.
//!
//! [`message`] does not test the fields. A `sonic-etag-key` that names
//! `template-tag`, `cache-offline` or `isReload` gives a member twice in
//! `extra`. A legacy response never has such a key (DL-226 (a),
//! `response::is_safe_head`); the session tests the fields of a response
//! that CL-13 handles before CL-14 (a `304`) itself.
//!
//! # Safety
//!
//! The text is at most about six times the diff and field bytes (escapes).
//! It holds page content, so the caller sends it only to the page bridge and
//! never logs it.

use core::fmt;

use rivqen_model::http::FieldLine;
use rivqen_template::legacy::DataBlock;

use crate::response::{CACHE_OFFLINE, TEMPLATE_TAG, etag_field_name, first_value, sonic_etag_key};

/// The `extra` key of the etag field without `sonic-etag-key`.
pub const DEFAULT_ETAG_KEY: &str = "eTag";

/// The `extra` member of the refresh flag. A `sonic-etag-key` that names it
/// makes the response not a legacy response (DL-226 (a)).
pub(crate) const RELOAD_KEY: &str = "isReload";

/// A result code of a legacy session (legacy client section 4, CL-22).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ResultCode {
    /// `-1`: no outcome.
    NoResult,
    /// `200`: data update.
    DataUpdate,
    /// `304`: not modified, or the web view got the current document
    /// complete.
    NotModified,
    /// `1000`: first load.
    FirstLoad,
    /// `2000`: template change.
    TemplateChange,
}

impl ResultCode {
    /// The integer of the code.
    #[must_use]
    pub const fn value(self) -> i32 {
        match self {
            Self::NoResult => -1,
            Self::DataUpdate => 200,
            Self::NotModified => 304,
            Self::FirstLoad => 1000,
            Self::TemplateChange => 2000,
        }
    }

    /// The code of an integer, or `None` for another integer.
    #[must_use]
    pub const fn from_value(value: i64) -> Option<Self> {
        match value {
            -1 => Some(Self::NoResult),
            200 => Some(Self::DataUpdate),
            304 => Some(Self::NotModified),
            1000 => Some(Self::FirstLoad),
            2000 => Some(Self::TemplateChange),
            _ => None,
        }
    }
}

/// The result that a message reports.
///
/// `Debug` writes the codes, counts and lengths only.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct MessageInput<'a> {
    /// The source code (CL-22).
    pub src: ResultCode,
    /// The final code (CL-22, CL-28, CL-30).
    pub final_code: ResultCode,
    /// The diff (CL-27, CL-42), in the order of the server data. Empty when
    /// the result has no diff.
    pub diff: &'a [DataBlock],
    /// The field lines of the response that set the result.
    pub fields: &'a [FieldLine],
    /// True in a refresh generation (CL-35).
    pub reload: bool,
}

impl fmt::Debug for MessageInput<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MessageInput")
            .field("src", &self.src)
            .field("final_code", &self.final_code)
            .field("diff_len", &self.diff.len())
            .field("fields_len", &self.fields.len())
            .field("reload", &self.reload)
            .finish()
    }
}

const HEX: &[u8; 16] = b"0123456789abcdef";

/// Append `text` as a JSON string, as `JSON.stringify` writes it.
fn push_json_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0'..='\u{1f}' => {
                let byte = u32::from(c);
                out.push_str("\\u00");
                for nibble in [byte >> 4, byte & 0xf] {
                    let digit = usize::try_from(nibble)
                        .ok()
                        .and_then(|n| HEX.get(n))
                        .copied()
                        .unwrap_or(b'0');
                    out.push(char::from(digit));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Append `"key":"value"`.
fn push_member(out: &mut String, key: &str, value: &str) {
    push_json_string(out, key);
    out.push(':');
    push_json_string(out, value);
}

/// The compact JSON object text of the diff, members in order.
fn diff_text(diff: &[DataBlock]) -> String {
    let mut out = String::from("{");
    for (i, block) in diff.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_member(&mut out, &block.key, &block.value);
    }
    out.push('}');
    out
}

/// CL-24 (function `message`): the message text of a result, or `None` when
/// the result sends no message (see the module rules).
#[must_use]
pub fn message(input: &MessageInput<'_>) -> Option<String> {
    let final_code = input.final_code;
    if final_code == ResultCode::NoResult
        || (final_code == ResultCode::DataUpdate && input.diff.is_empty())
    {
        return None;
    }
    let mut out = String::from("{");
    if final_code == ResultCode::DataUpdate {
        out.push_str("\"result\":");
        push_json_string(&mut out, &diff_text(input.diff));
        out.push(',');
    }
    out.push_str("\"code\":");
    out.push_str(&final_code.value().to_string());
    out.push_str(",\"srcCode\":");
    out.push_str(&input.src.value().to_string());
    out.push_str(",\"extra\":{");
    let fields = input.fields;
    let etag_key = sonic_etag_key(fields).unwrap_or(DEFAULT_ETAG_KEY);
    let extra = [
        (etag_key, first_value(fields, etag_field_name(fields))),
        (TEMPLATE_TAG, first_value(fields, TEMPLATE_TAG)),
        (CACHE_OFFLINE, first_value(fields, CACHE_OFFLINE)),
    ];
    for (key, value) in extra {
        if let Some(value) = value {
            push_member(&mut out, key, value);
            out.push(',');
        }
    }
    push_member(
        &mut out,
        RELOAD_KEY,
        if input.reload { "true" } else { "false" },
    );
    out.push_str("}}");
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::push_json_string;

    fn json(text: &str) -> String {
        let mut out = String::new();
        push_json_string(&mut out, text);
        out
    }

    #[test]
    fn json_string_escapes_as_json_stringify() {
        assert_eq!(json("a\"b\\c/é"), "\"a\\\"b\\\\c/é\"");
        assert_eq!(json("\u{8}\u{c}\n\r\t"), "\"\\b\\f\\n\\r\\t\"");
        assert_eq!(
            json("\0\u{1}\u{1a}\u{1f} \u{7f}"),
            "\"\\u0000\\u0001\\u001a\\u001f \u{7f}\""
        );
        assert_eq!(json("\u{2028}"), "\"\u{2028}\"");
    }
}
