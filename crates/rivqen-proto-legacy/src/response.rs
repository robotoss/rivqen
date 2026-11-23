// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The legacy response head (legacy client CL-14, CL-15, CL-16): the legacy
//! response test, the etag field with `sonic-etag-key`, the weak validator
//! normalization, `template-change` and the charset of `Content-Type`.
//!
//! Owner: WP-08 S1, T-01.
//!
//! # Rules
//!
//! - Field names compare ASCII case-insensitively (D-32). A field value is
//!   read without SP and HTAB at its ends (legacy wire §10, input line).
//! - [`etag_field_name`]: the first `sonic-etag-key` value when it is not
//!   empty, else `etag` (CL-14, legacy wire §6).
//! - [`is_legacy_response`] (function fixture `is_legacy_response`): exactly
//!   one `Content-Type` line of media type `text/html` (DL-125), the field
//!   rules of [`is_safe_head`] hold (DL-226), and each of the etag field,
//!   `template-tag`, `template-change` and `cache-offline` has at least one
//!   line, all its lines have the same value, and the value is not empty
//!   (D-28, D-33). [`LegacyHead::read`] gives the four values.
//! - [`is_safe_head`] (DL-226): every line is a valid field line (a token
//!   name, no control byte other than HTAB in the value); no non-empty
//!   `sonic-etag-key` value names `Content-Type`, a field of
//!   `NOT_PAGE_FIELDS`, a protected field or the `extra` member `isReload`,
//!   and each one is a token; no `Connection` token names `Content-Type` or
//!   a protected field. `sonic-etag-key: etag` (the default name) is
//!   allowed. Every `sonic-etag-key` line counts, not only the first one.
//! - [`bare_etag`] (function fixture `etag`): one leading `W/` (exact case),
//!   then one pair of outer `"` (D-31).
//! - [`is_template_change`] (function fixture `template_change`): only
//!   exactly `false` or `0` is a data update (D-29).
//! - [`charset`] (function fixture `charset`): the first `charset` parameter
//!   of `Content-Type`, ASCII lowercase, default `utf-8` (D-44, legacy wire
//!   §10 item 1). [`is_utf8_text`] is the CL-16 test of the body.
//!
//! The status, `cache-offline` and `Cache-Control` are not read here: the
//! session tests the status (CL-13) and reads the cache fields with
//! `rivqen-cache`.
//!
//! # Safety
//!
//! No function panics or allocates more than its input. The work is linear
//! in the field bytes ([`is_safe_head`] compares each name with fixed lists
//! of constant size). No `Debug` output holds a field value.

use core::fmt;
use std::borrow::Cow;

use rivqen_model::http::FieldLine;

use crate::fields::{
    CONNECTION, NOT_PAGE_FIELDS, is_protected_field, is_token, is_valid_field_line,
};
use crate::message::RELOAD_KEY;

/// Field name `etag`, lower case: the etag field without `sonic-etag-key`.
pub const ETAG: &str = "etag";
/// Field name `sonic-etag-key`, lower case: names the etag field (legacy
/// wire §6).
pub const SONIC_ETAG_KEY: &str = "sonic-etag-key";
/// Field name `template-tag`, lower case (request and response).
pub const TEMPLATE_TAG: &str = "template-tag";
/// Field name `template-change`, lower case.
pub const TEMPLATE_CHANGE: &str = "template-change";
/// Field name `cache-offline`, lower case.
pub const CACHE_OFFLINE: &str = "cache-offline";
/// Field name `content-type`, lower case.
pub const CONTENT_TYPE: &str = "content-type";
/// The media type of a legacy response (DL-125).
pub const HTML_MEDIA_TYPE: &str = "text/html";
/// The charset when `Content-Type` has no `charset` parameter (CL-16).
pub const DEFAULT_CHARSET: &str = "utf-8";

/// OWS: SP and HTAB (RFC 9110 section 5.6.3).
pub(crate) fn trim_ows(value: &str) -> &str {
    value.trim_matches([' ', '\t'])
}

/// The values of every line with this name, without OWS at the ends, in
/// order.
pub(crate) fn values_of<'a>(
    fields: &'a [FieldLine],
    name: &'a str,
) -> impl Iterator<Item = &'a str> + 'a {
    fields
        .iter()
        .filter(move |line| line.is(name))
        .map(|line| trim_ows(&line.value))
}

/// The value of the first line with this name, without OWS at the ends.
pub(crate) fn first_value<'a>(fields: &'a [FieldLine], name: &'a str) -> Option<&'a str> {
    values_of(fields, name).next()
}

/// The value of `sonic-etag-key` when it is present and not empty: the name
/// of the etag field as the response writes it.
pub(crate) fn sonic_etag_key(fields: &[FieldLine]) -> Option<&str> {
    first_value(fields, SONIC_ETAG_KEY).filter(|key| !key.is_empty())
}

/// The name of the etag field (CL-14): the first `sonic-etag-key` value when
/// it is present and not empty, else [`ETAG`]. The name is as the response
/// writes it; compare it ASCII case-insensitively.
#[must_use]
pub fn etag_field_name(fields: &[FieldLine]) -> &str {
    sonic_etag_key(fields).unwrap_or(ETAG)
}

/// The value of the first line of the etag field ([`etag_field_name`]), or
/// `None` when the field is absent. The session compares its
/// [`bare_etag`] with the stored etag for a `200` (CL-13 item 2, D-45).
#[must_use]
pub fn etag_value(fields: &[FieldLine]) -> Option<&str> {
    first_value(fields, etag_field_name(fields))
}

/// The weak validator normalization of CL-15 (D-31; function `etag`):
/// remove one leading `W/` (exact case), then one pair of outer `"` when
/// both are present. The rest is compared byte for byte. The value is not
/// trimmed.
#[must_use]
pub fn bare_etag(value: &str) -> &str {
    unquote(value.strip_prefix("W/").unwrap_or(value))
}

/// `value` without one pair of outer `"` when both are present (so the
/// value has at least two bytes); else `value`.
fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(value)
}

/// CL-15 (D-29; function `template_change`): `true` when the
/// `template-change` value is a template change. Only exactly `false` or `0`
/// is a data update.
#[must_use]
pub fn is_template_change(value: &str) -> bool {
    !matches!(value, "false" | "0")
}

/// The media type of a `Content-Type` value: the text before the first `;`
/// without OWS at its ends.
fn media_type(content_type: &str) -> &str {
    trim_ows(content_type.split(';').next().unwrap_or_default())
}

/// The value of the first `charset` parameter of a `Content-Type` value, as
/// written: without OWS at its ends and without one pair of outer `"`
/// (legacy wire §10 item 1). A parameter without `=` is skipped. A quoted
/// string is not parsed: every `;` splits.
fn charset_parameter(content_type: &str) -> Option<&str> {
    content_type.split(';').skip(1).find_map(|parameter| {
        let (name, value) = parameter.split_once('=')?;
        if !trim_ows(name).eq_ignore_ascii_case("charset") {
            return None;
        }
        Some(unquote(trim_ows(value)))
    })
}

/// CL-16 (D-44; function `charset`): the `charset` parameter of a
/// `Content-Type` value in ASCII lowercase, or [`DEFAULT_CHARSET`] without
/// one. The first `charset` parameter counts. The value is not checked
/// against a list of charsets.
#[must_use]
pub fn charset(content_type: &str) -> Cow<'_, str> {
    match charset_parameter(content_type) {
        None => Cow::Borrowed(DEFAULT_CHARSET),
        Some(value) => Cow::Owned(value.to_ascii_lowercase()),
    }
}

/// CL-14 (DL-125): exactly one `Content-Type` line, and its media type is
/// `text/html` (ASCII case-insensitive; parameters ignored).
#[must_use]
pub fn is_html_response(fields: &[FieldLine]) -> bool {
    let mut types = values_of(fields, CONTENT_TYPE);
    match (types.next(), types.next()) {
        (Some(only), None) => media_type(only).eq_ignore_ascii_case(HTML_MEDIA_TYPE),
        _ => false,
    }
}

/// CL-16: the charset of the first `Content-Type` line (default `utf-8`) is
/// `utf-8`, and `body` is valid UTF-8. A response that fails this test is
/// handled as not a legacy response.
#[must_use]
pub fn is_utf8_text(fields: &[FieldLine], body: &[u8]) -> bool {
    let utf8 = first_value(fields, CONTENT_TYPE)
        .and_then(charset_parameter)
        .is_none_or(|value| value.eq_ignore_ascii_case(DEFAULT_CHARSET));
    utf8 && core::str::from_utf8(body).is_ok()
}

/// DL-226 (a): true when `sonic-etag-key` must not give this name (trimmed,
/// not empty): it is not a token, or it names `Content-Type`, a field of
/// `NOT_PAGE_FIELDS`, a protected field or the message `extra` member
/// `isReload` (ASCII case-insensitive). The other `extra` members
/// (`template-tag`, `cache-offline`) are in `NOT_PAGE_FIELDS`.
fn is_forbidden_etag_key(name: &str) -> bool {
    !is_token(name)
        || name.eq_ignore_ascii_case(CONTENT_TYPE)
        || name.eq_ignore_ascii_case(RELOAD_KEY)
        || NOT_PAGE_FIELDS
            .iter()
            .any(|not_page| name.eq_ignore_ascii_case(not_page))
        || is_protected_field(name)
}

/// DL-226 (a): true when a `Connection` token (trimmed) names
/// `Content-Type` or a protected field.
fn is_forbidden_connection_token(token: &str) -> bool {
    token.eq_ignore_ascii_case(CONTENT_TYPE) || is_protected_field(token)
}

/// DL-226 (a) and (b): the field rules of a legacy response head (part of
/// CL-14). True when every line is a valid field line
/// (`fields::is_valid_field_line`), every non-empty `sonic-etag-key` value
/// is a token that names no `Content-Type`, no field of `NOT_PAGE_FIELDS`,
/// no protected field (`fields::is_protected_field`) and not `isReload`,
/// and no `Connection` token names `Content-Type` or a protected field.
/// Names and tokens compare ASCII case-insensitively.
///
/// [`LegacyHead::read`] applies it. The session can also apply it to a
/// response that CL-13 handles before CL-14 (a `304`, CL-17).
#[must_use]
pub fn is_safe_head(fields: &[FieldLine]) -> bool {
    fields.iter().all(is_valid_field_line)
        && values_of(fields, SONIC_ETAG_KEY)
            .filter(|key| !key.is_empty())
            .all(|key| !is_forbidden_etag_key(key))
        && values_of(fields, CONNECTION)
            .flat_map(|value| value.split(','))
            .all(|token| !is_forbidden_connection_token(trim_ows(token)))
}

/// CL-14 (function `is_legacy_response`): the field test of a legacy
/// response. The caller tests the status (`200`) and CL-16 itself.
#[must_use]
pub fn is_legacy_response(fields: &[FieldLine]) -> bool {
    LegacyHead::read(fields).is_some()
}

/// The one value of a legacy field: at least one line, every line has the
/// same value, the value is not empty (CL-14, D-33).
fn one_value<'a>(fields: &'a [FieldLine], name: &'a str) -> Option<&'a str> {
    let mut values = values_of(fields, name);
    let first = values.next().filter(|value| !value.is_empty())?;
    values.all(|value| value == first).then_some(first)
}

/// The legacy fields of a response head that passed the field test of CL-14
/// ([`is_legacy_response`]). Each value is the one value of its field,
/// without OWS at its ends, never empty.
///
/// `Debug` writes the lengths only: the values identify the page and can
/// be personal.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct LegacyHead<'a> {
    etag_field: &'a str,
    etag: &'a str,
    template_tag: &'a str,
    template_change: &'a str,
    cache_offline: &'a str,
}

impl<'a> LegacyHead<'a> {
    /// The legacy fields, or `None` when the head is not a legacy response
    /// (CL-14): not exactly one `Content-Type` line of type `text/html`, a
    /// field rule of [`is_safe_head`] fails (DL-226), or one of the four
    /// fields is absent, empty, or on two lines with different values.
    #[must_use]
    pub fn read(fields: &'a [FieldLine]) -> Option<Self> {
        if !is_html_response(fields) || !is_safe_head(fields) {
            return None;
        }
        let etag_field = etag_field_name(fields);
        Some(Self {
            etag_field,
            etag: one_value(fields, etag_field)?,
            template_tag: one_value(fields, TEMPLATE_TAG)?,
            template_change: one_value(fields, TEMPLATE_CHANGE)?,
            cache_offline: one_value(fields, CACHE_OFFLINE)?,
        })
    }

    /// The name of the etag field as the response writes it
    /// ([`etag_field_name`]).
    #[must_use]
    pub const fn etag_field(&self) -> &'a str {
        self.etag_field
    }

    /// The value of the etag field as sent.
    #[must_use]
    pub const fn etag(&self) -> &'a str {
        self.etag
    }

    /// The etag after the weak validator normalization ([`bare_etag`]).
    /// It can be empty (`W/""`).
    #[must_use]
    pub fn bare_etag(&self) -> &'a str {
        bare_etag(self.etag)
    }

    /// The `template-tag` value.
    #[must_use]
    pub const fn template_tag(&self) -> &'a str {
        self.template_tag
    }

    /// The `template-change` value as sent.
    #[must_use]
    pub const fn template_change(&self) -> &'a str {
        self.template_change
    }

    /// True for a template change, false for a data update
    /// ([`is_template_change`]).
    #[must_use]
    pub fn is_template_change(&self) -> bool {
        is_template_change(self.template_change)
    }

    /// The `cache-offline` value as sent (not lowercased). The session reads
    /// its meaning with `rivqen-cache` (CL-15).
    #[must_use]
    pub const fn cache_offline(&self) -> &'a str {
        self.cache_offline
    }
}

impl fmt::Debug for LegacyHead<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LegacyHead")
            .field("etag_field_len", &self.etag_field.len())
            .field("etag_len", &self.etag.len())
            .field("template_tag_len", &self.template_tag.len())
            .field("template_change_len", &self.template_change.len())
            .field("cache_offline_len", &self.cache_offline.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        charset_parameter, is_forbidden_connection_token, is_forbidden_etag_key, media_type,
        one_value, trim_ows,
    };
    use rivqen_model::http::FieldLine;

    #[test]
    fn trim_ows_removes_only_sp_and_htab() {
        assert_eq!(trim_ows(" \t a \t "), "a");
        assert_eq!(trim_ows("\na\r"), "\na\r");
    }

    #[test]
    fn media_type_is_the_text_before_the_first_semicolon() {
        assert_eq!(media_type(" text/html ; a=b"), "text/html");
        assert_eq!(media_type(""), "");
    }

    #[test]
    fn charset_parameter_skips_a_parameter_without_equals() {
        assert_eq!(
            charset_parameter("text/html; charset; charset=a"),
            Some("a")
        );
        assert_eq!(charset_parameter("text/html"), None);
        assert_eq!(charset_parameter("charset=a"), None);
        assert_eq!(charset_parameter("t; charset=\""), Some("\""));
    }

    #[test]
    fn one_value_needs_equal_non_empty_lines() {
        let f = |pairs: &[(&str, &str)]| -> Vec<FieldLine> {
            pairs.iter().map(|(n, v)| FieldLine::new(*n, *v)).collect()
        };
        assert_eq!(one_value(&f(&[("a", "x"), ("A", " x ")]), "a"), Some("x"));
        assert_eq!(one_value(&f(&[("a", "x"), ("a", "y")]), "a"), None);
        assert_eq!(one_value(&f(&[("a", "")]), "a"), None);
        assert_eq!(one_value(&f(&[("a", "x"), ("a", "")]), "a"), None);
        assert_eq!(one_value(&f(&[]), "a"), None);
    }

    #[test]
    fn forbidden_etag_keys_follow_dl_226() {
        for name in [
            "Content-Type",
            "content-security-policy",
            "Cross-Origin-Embedder-Policy",
            "isReload",
            "ISRELOAD",
            "template-tag",
            "Cache-Offline",
            "sonic-etag-key",
            "set-cookie",
            "a b",
            "set-cookie ",
            "x\0",
            "",
        ] {
            assert!(is_forbidden_etag_key(name), "{name:?}");
        }
        for name in ["etag", "ETag", "eTag", "custom-etag", "x-cross-origin-a"] {
            assert!(!is_forbidden_etag_key(name), "{name:?}");
        }
    }

    #[test]
    fn forbidden_connection_tokens_follow_dl_226() {
        for token in ["content-type", "X-Frame-Options", "cross-origin-x"] {
            assert!(is_forbidden_connection_token(token), "{token}");
        }
        for token in ["", "close", "keep-alive", "x-hop", "etag", "content-length"] {
            assert!(!is_forbidden_connection_token(token), "{token}");
        }
    }
}
