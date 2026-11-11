// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Response fields that the client reads (negotiation section 6.3).
//!
//! Each function gives the result of the reference client
//! (`tools/rqp-protocol/src/client.mjs` and `http.mjs`) for the same field
//! lines. Field names compare ASCII case-insensitively. White space is removed
//! with index scans, never with a pattern that tries a run again (DL-063).
//!
//! # The head gate (DL-153)
//!
//! The field reads are methods of [`ResponseHead`]. The only way to build a
//! [`ResponseHead`] is [`ResponseHead::check`], which tests every field line
//! against the per-line and the total limit of `maxHeaderBytes` before any
//! field is read. A read without the check does not compile:
//!
//! ```compile_fail
//! use rivqen_model::http::FieldLine;
//! use rivqen_proto::fields::ResponseHead;
//! let lines = vec![FieldLine::new("rq-version", "1")];
//! // The field of the head is private: no struct literal outside the crate.
//! let head = ResponseHead { fields: &lines };
//! ```
//!
//! ```compile_fail
//! use rivqen_model::http::FieldLine;
//! // The free function is not public.
//! let _ = rivqen_proto::fields::rq_version_is_1(&[FieldLine::new("rq-version", "1")]);
//! ```
//!
//! # Counting rule
//!
//! The size of a field line is the byte length of its name plus the byte
//! length of its value (the separator and the line end are not counted). One
//! line over the per-line limit, or a sum over the total limit, rejects the
//! whole head. A size equal to a limit passes. The sum saturates, so a huge
//! input cannot overflow it.

use core::fmt;
use std::borrow::Cow;

use rivqen_integrity::PageRevision;
use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_model::http::FieldLine;

use super::{CONTENT_TYPE, ETAG, RQ_VERSION, RQP_VERSION};
use crate::sf::{BareItem, parse_item};

/// The response head is over `maxHeaderBytes`: one field line is over the
/// per-line limit or the sum of all lines is over the total limit (DL-153).
/// The error code is `DOCUMENT_TOO_LARGE`; the adapter loads the page
/// normally. The error holds no field name and no field value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum HeadLimitError {
    /// One field line (name bytes plus value bytes) is over the per-line limit.
    LineTooLarge,
    /// The sum of all field lines is over the total limit.
    TotalTooLarge,
}

impl HeadLimitError {
    /// A static reason, safe to log.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::LineTooLarge => "a response field line is over maxHeaderBytes",
            Self::TotalTooLarge => "the response head is over maxHeaderBytes",
        }
    }
}

impl fmt::Display for HeadLimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.reason())
    }
}

impl core::error::Error for HeadLimitError {}

impl HasErrorCode for HeadLimitError {
    fn code(&self) -> ErrorCode {
        ErrorCode::DocumentTooLarge
    }
}

/// The field lines of one response head that passed the `maxHeaderBytes`
/// check. Every response field read is a method of this type (DL-153).
#[derive(Debug, Clone, Copy)]
pub struct ResponseHead<'a> {
    fields: &'a [FieldLine],
}

impl<'a> ResponseHead<'a> {
    /// Checks the field lines against the limits. The size of a line is
    /// name bytes plus value bytes. A line over `max_line_bytes`, or a sum over
    /// `max_total_bytes`, is an error; a size equal to a limit passes. One
    /// pass; the work is linear in the number of lines.
    ///
    /// # Errors
    ///
    /// [`HeadLimitError`] (code `DOCUMENT_TOO_LARGE`). The caller then reads
    /// no field of the response and loads the page normally; it never drops
    /// the one field and goes on (negotiation section 6.3, item 1).
    pub fn check(
        fields: &'a [FieldLine],
        max_line_bytes: usize,
        max_total_bytes: usize,
    ) -> Result<Self, HeadLimitError> {
        let mut total = 0_usize;
        for line in fields {
            let size = line.name.len().saturating_add(line.value.len());
            if size > max_line_bytes {
                return Err(HeadLimitError::LineTooLarge);
            }
            total = total.saturating_add(size);
            if total > max_total_bytes {
                return Err(HeadLimitError::TotalTooLarge);
            }
        }
        Ok(Self { fields })
    }

    /// The combined field value of all lines with this name (RFC 9110
    /// section 5.3). `None` when the field is absent.
    #[must_use]
    pub fn combined_value(&self, name: &str) -> Option<Cow<'a, str>> {
        combined_value(self.fields, name)
    }

    /// True when the response has `Rq-Version: 1` (section 6.4, item 2).
    #[must_use]
    pub fn rq_version_is_1(&self) -> bool {
        rq_version_is_1(self.fields)
    }

    /// The media type of the response, in ASCII lower case (section 6.3,
    /// items 2 and 3).
    #[must_use]
    pub fn media_type(&self) -> Option<String> {
        media_type(self.fields)
    }

    /// The charset of the `Content-Type` field (negotiation section 4).
    #[must_use]
    pub fn content_type_charset(&self) -> ContentTypeCharset {
        content_type_charset(self.fields)
    }

    /// True when the browser decodes the body as UTF-8 (manifest-patch
    /// section 1.7, client item 6).
    #[must_use]
    pub fn decoded_as_utf8(&self, body: &[u8]) -> bool {
        decoded_as_utf8(self.fields, body)
    }

    /// Section 6.3, item 1.3: a `304` has no `ETag`, or its `ETag` matches
    /// the snapshot by weak comparison.
    #[must_use]
    pub fn etag_matches_snapshot(&self, page_revision: &PageRevision) -> bool {
        etag_matches_snapshot(self.fields, page_revision)
    }
}

/// OWS: SP and HTAB (RFC 9110 section 5.6.3).
const fn is_ows(c: char) -> bool {
    matches!(c, ' ' | '\t')
}

/// HTTP white space of the WHATWG MIME Sniffing Standard: HTAB, LF, CR, SP.
const fn is_http_ws(c: u8) -> bool {
    matches!(c, b'\t' | b'\n' | b'\r' | b' ')
}

/// ASCII white space of the WHATWG Infra Standard: HTAB, LF, FF, CR, SP.
const fn is_ascii_ws(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\x0c' | '\r' | ' ')
}

/// The combined field value of all lines with this name (RFC 9110 section
/// 5.3): each value without leading and trailing OWS, joined with `", "`.
/// `None` when the field is absent. Borrows when there is one line.
fn combined_value<'a>(fields: &'a [FieldLine], name: &str) -> Option<Cow<'a, str>> {
    let mut values = fields
        .iter()
        .filter(|f| f.is(name))
        .map(|f| f.value.trim_matches(is_ows));
    let first = values.next()?;
    let Some(second) = values.next() else {
        return Some(Cow::Borrowed(first));
    };
    let mut out = String::with_capacity(first.len().saturating_add(second.len()).saturating_add(2));
    out.push_str(first);
    for value in core::iter::once(second).chain(values) {
        out.push_str(", ");
        out.push_str(value);
    }
    Some(Cow::Owned(out))
}

/// True when the response has `Rq-Version: 1`: the combined field value is an
/// Item (RFC 9651) whose bare item is the Integer 1. Parameters are ignored
/// (negotiation section 3.2). Any other value, a parse failure or no field is
/// false (section 6.4, item 2).
fn rq_version_is_1(fields: &[FieldLine]) -> bool {
    combined_value(fields, RQ_VERSION)
        .and_then(|v| parse_item(&v).ok())
        .is_some_and(|item| item.bare_item == BareItem::Integer(RQP_VERSION))
}

/// The media type of the response: the text of the combined `Content-Type`
/// value before the first `;`, without OWS, in ASCII lower case. `None` when
/// the field is absent or that text is empty. Compare it with
/// [`super::DELTA_MEDIA_TYPE`] or [`super::HTML_MEDIA_TYPE`] (section 6.3,
/// items 2 and 3).
fn media_type(fields: &[FieldLine]) -> Option<String> {
    let value = combined_value(fields, CONTENT_TYPE)?;
    let essence = value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim_matches(is_ows);
    (!essence.is_empty()).then(|| essence.to_ascii_lowercase())
}

/// The charset of the `Content-Type` field (the charset rule of negotiation
/// section 4; H-62, item 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentTypeCharset {
    /// No `Content-Type` field, or no charset parameter.
    Absent,
    /// One field line with one media type whose charset is a UTF-8 label.
    Utf8,
    /// Any other charset, and every field with more than one line or more
    /// than one media type (fail closed).
    Other,
}

/// The charset of the `Content-Type` field lines (negotiation section 4,
/// charset rule items 1 to 3).
fn content_type_charset(fields: &[FieldLine]) -> ContentTypeCharset {
    let mut lines = fields.iter().filter(|f| f.is(CONTENT_TYPE));
    let Some(line) = lines.next() else {
        return ContentTypeCharset::Absent;
    };
    if lines.next().is_some() {
        return ContentTypeCharset::Other;
    }
    let mut values = split_list(&line.value);
    let (Some(mime), None) = (values.next(), values.next()) else {
        return ContentTypeCharset::Other;
    };
    match charset_parameter(mime) {
        None => ContentTypeCharset::Absent,
        Some(label) if is_utf8_label(&label) => ContentTypeCharset::Utf8,
        Some(_) => ContentTypeCharset::Other,
    }
}

/// True when the browser decodes the body as UTF-8 (manifest-patch section
/// 1.7, client item 6; H-62, item 3): the body starts with the UTF-8 byte
/// order mark `EF BB BF`, or the `Content-Type` has a UTF-8 charset.
fn decoded_as_utf8(fields: &[FieldLine], body: &[u8]) -> bool {
    body.starts_with(&[0xef, 0xbb, 0xbf])
        || content_type_charset(fields) == ContentTypeCharset::Utf8
}

/// Negotiation section 6.3, item 1.3: a `304` has no `ETag` field, or its
/// combined `ETag` value matches `"R"` by weak comparison (`"R"` or `W/"R"`),
/// where R is the page revision of the snapshot.
fn etag_matches_snapshot(fields: &[FieldLine], page_revision: &PageRevision) -> bool {
    let Some(etag) = combined_value(fields, ETAG) else {
        return true;
    };
    let opaque = etag.strip_prefix("W/").unwrap_or(&etag);
    opaque == format!("\"{page_revision}\"")
}

/// The labels of the encoding UTF-8 (WHATWG Encoding Standard section 4.2,
/// checked on 2026-10-17 for the reference).
const UTF8_LABELS: [&str; 6] = [
    "unicode-1-1-utf-8",
    "unicode11utf8",
    "unicode20utf8",
    "utf-8",
    "utf8",
    "x-unicode20utf8",
];

/// "Get an encoding" restricted to UTF-8: the label without leading and
/// trailing ASCII white space is a UTF-8 label, ASCII case-insensitively.
fn is_utf8_label(label: &str) -> bool {
    let label = label.trim_matches(is_ascii_ws);
    UTF8_LABELS.iter().any(|l| l.eq_ignore_ascii_case(label))
}

/// The members of a field value split on commas outside quoted strings, each
/// without OWS, empty members dropped (`splitList` of the reference). A
/// backslash in a quoted string escapes the next byte. One pass.
fn split_list(value: &str) -> impl Iterator<Item = &str> {
    let bytes = value.as_bytes();
    let mut quoted = false;
    let mut escaped = false;
    let mut start = 0_usize;
    let mut cuts: Vec<(usize, usize)> = Vec::new();
    for (i, &c) in bytes.iter().enumerate() {
        if escaped {
            escaped = false;
        } else if quoted {
            match c {
                b'\\' => escaped = true,
                b'"' => quoted = false,
                _ => {}
            }
        } else if c == b'"' {
            quoted = true;
        } else if c == b',' {
            cuts.push((start, i));
            start = i.saturating_add(1);
        }
    }
    cuts.push((start, bytes.len()));
    cuts.into_iter()
        .filter_map(move |(s, e)| value.get(s..e))
        .map(|part| part.trim_matches(is_ows))
        .filter(|part| !part.is_empty())
}

/// The value of the first valid `charset` parameter of one MIME type, or
/// `None` (WHATWG MIME Sniffing section 4.4, "parse a MIME type", steps 11.1
/// to 11.10; `charsetParameter` of the reference). A quoted value loses its
/// quotes and backslash escapes; an unquoted value loses its trailing HTTP
/// white space and is skipped when empty. The essence is not checked.
fn charset_parameter(mime: &str) -> Option<String> {
    let bytes = mime.as_bytes();
    let len = bytes.len();
    let at = |i: usize| bytes.get(i).copied();
    let mut i = bytes.iter().position(|&c| c == b';')?;
    while i < len {
        i = i.saturating_add(1); // past ';'
        while at(i).is_some_and(is_http_ws) {
            i = i.saturating_add(1);
        }
        let name_start = i;
        while at(i).is_some_and(|c| c != b';' && c != b'=') {
            i = i.saturating_add(1);
        }
        let is_charset = mime
            .get(name_start..i)
            .is_some_and(|n| n.eq_ignore_ascii_case("charset"));
        match at(i) {
            Some(b';') => continue,
            Some(_) => i = i.saturating_add(1), // past '='
            None => {}
        }
        if i >= len {
            break;
        }
        let value: Vec<u8> = if at(i) == Some(b'"') {
            i = i.saturating_add(1);
            let mut value = Vec::new();
            while let Some(c) = at(i) {
                i = i.saturating_add(1);
                match c {
                    b'"' => break,
                    b'\\' => match at(i) {
                        None => value.push(b'\\'),
                        Some(next) => {
                            value.push(next);
                            i = i.saturating_add(1);
                        }
                    },
                    other => value.push(other),
                }
            }
            while at(i).is_some_and(|c| c != b';') {
                i = i.saturating_add(1);
            }
            value
        } else {
            let value_start = i;
            while at(i).is_some_and(|c| c != b';') {
                i = i.saturating_add(1);
            }
            let raw = bytes.get(value_start..i).unwrap_or_default();
            let trimmed_len = raw
                .iter()
                .rposition(|&c| !is_http_ws(c))
                .map_or(0, |p| p.saturating_add(1));
            let trimmed = raw.get(..trimmed_len).unwrap_or_default();
            if trimmed.is_empty() {
                continue;
            }
            trimmed.to_vec()
        };
        if is_charset
            && let Ok(text) = String::from_utf8(value)
            && is_quoted_string_text(&text)
        {
            return Some(text);
        }
    }
    None
}

/// Every character is HTAB, `0x20` to `0x7E` or `U+0080` to `U+00FF`: the
/// code points of an HTTP quoted string (WHATWG MIME Sniffing section 4.4,
/// step 11.9).
fn is_quoted_string_text(text: &str) -> bool {
    text.chars()
        .all(|c| c == '\t' || (' '..='~').contains(&c) || ('\u{80}'..='\u{ff}').contains(&c))
}
