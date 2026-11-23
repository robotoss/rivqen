// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The legacy request fields of a session (legacy client CL-09): `accept-diff`,
//! `if-none-match`, `template-tag` and `cookie`.
//!
//! Owner: WP-08 S1, T-01.
//!
//! # Rules (CL-09, CL-11)
//!
//! [`request_fields`] gives the field lines of one legacy `GET`, in this
//! order, with lowercase names:
//!
//! 1. `accept-diff`: `true`, or `false` when the session does not accept a
//!    data update.
//! 2. `if-none-match` and `template-tag`: the etag and the template tag of
//!    the visible entry. Without a visible entry, neither field (upstream
//!    Android sends both empty). The validators are [`Sha1Hex`] values, so a
//!    field is never empty and never holds a byte that is not a hex digit.
//! 3. `cookie`: the cookie that the platform store gives for the URL just
//!    before the request (CL-11 item 1), when it is not empty.
//!
//! No `sonic-sdk-version` (legacy wire §2, privacy deviation). The content
//! coding (`accept-encoding`) belongs to the transport, not to this list.
//!
//! # Safety
//!
//! A cookie value with a control character (CR, LF, NUL and the other bytes
//! below `0x20` except HTAB, or DEL) is an error, so no input can add a field
//! line. Neither `Debug` nor an error holds an etag, a tag or a cookie byte.

use core::fmt;

use rivqen_integrity::legacy::Sha1Hex;
use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_model::http::FieldLine;

use crate::response::TEMPLATE_TAG;

/// Field name `accept-diff`, lower case.
pub const ACCEPT_DIFF: &str = "accept-diff";
/// Field name `if-none-match`, lower case.
pub const IF_NONE_MATCH: &str = "if-none-match";
/// Field name `cookie`, lower case.
pub const COOKIE: &str = "cookie";

/// The validators of a visible entry (CL-12, CL-40): the SHA-1 of its page
/// (the stored etag) and of its template (the stored template tag).
///
/// `Debug` writes no digest: together with the URL a digest identifies the
/// page that the user saw.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct EntryValidators {
    /// The stored etag: lowercase hex SHA-1 of the page.
    pub etag: Sha1Hex,
    /// The stored template tag: lowercase hex SHA-1 of the template.
    pub template_tag: Sha1Hex,
}

impl fmt::Debug for EntryValidators {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EntryValidators").finish_non_exhaustive()
    }
}

/// What one legacy request needs (CL-09).
///
/// `Debug` writes the cookie length only.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct LegacyRequest<'a> {
    /// `accept-diff: true` when set, else `false` (configuration
    /// `accept_diff`, CL-03).
    pub accept_diff: bool,
    /// The validators of the visible entry, or `None` without one.
    pub entry: Option<EntryValidators>,
    /// The cookie of the platform store for the URL, read just before the
    /// request (CL-11 item 1). `None` or an empty text: no `cookie` field.
    pub cookie: Option<&'a str>,
}

impl fmt::Debug for LegacyRequest<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LegacyRequest")
            .field("accept_diff", &self.accept_diff)
            .field("entry", &self.entry)
            .field("cookie_len", &self.cookie.map(str::len))
            .finish()
    }
}

/// A request that [`request_fields`] does not build. The session sends no
/// legacy request and loads the page normally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RequestError {
    /// The cookie has a control character (a byte below `0x20` other than
    /// HTAB, or DEL; RFC 9110 §5.5). It holds no byte of the cookie.
    CookieValue,
}

impl RequestError {
    /// A static reason, safe to log.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::CookieValue => "the cookie of the platform store has a control character",
        }
    }
}

impl fmt::Display for RequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.reason())
    }
}

impl core::error::Error for RequestError {}

impl HasErrorCode for RequestError {
    /// `INTERNAL_ERROR`: the platform cookie store gave a value that no
    /// HTTP field can hold.
    fn code(&self) -> ErrorCode {
        ErrorCode::InternalError
    }
}

/// True when a field value has no control character (RFC 9110 §5.5: VCHAR,
/// obs-text, SP and HTAB).
fn is_field_value(value: &str) -> bool {
    !value.bytes().any(|b| (b < 0x20 && b != b'\t') || b == 0x7f)
}

/// CL-09: the field lines of one legacy request (see the module rules).
///
/// # Errors
///
/// [`RequestError::CookieValue`] when the cookie has a control character.
pub fn request_fields(request: &LegacyRequest<'_>) -> Result<Vec<FieldLine>, RequestError> {
    let mut fields = Vec::with_capacity(4);
    let accept = if request.accept_diff { "true" } else { "false" };
    fields.push(FieldLine::new(ACCEPT_DIFF, accept));
    if let Some(entry) = &request.entry {
        fields.push(FieldLine::new(IF_NONE_MATCH, entry.etag.as_str()));
        fields.push(FieldLine::new(TEMPLATE_TAG, entry.template_tag.as_str()));
    }
    if let Some(cookie) = request.cookie.filter(|c| !c.is_empty()) {
        if !is_field_value(cookie) {
            return Err(RequestError::CookieValue);
        }
        fields.push(FieldLine::new(COOKIE, cookie));
    }
    Ok(fields)
}

#[cfg(test)]
mod tests {
    use super::is_field_value;

    #[test]
    fn field_value_allows_htab_and_obs_text_only() {
        assert!(is_field_value("a=b; c=d\té"));
        assert!(is_field_value("~ !"));
        for bad in ["\r", "\n", "\0", "\x1f", "\x7f", "\x01"] {
            assert!(!is_field_value(bad), "{bad:?}");
        }
    }
}
