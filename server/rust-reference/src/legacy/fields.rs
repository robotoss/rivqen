// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Field-line helpers of the legacy engine (legacy wire §10 and §10.1).
//!
//! A field name compares ASCII case-insensitively ([`Field::is`]); a value is
//! trimmed of spaces and tabs at both ends. Every list split here is a split
//! at each separator byte: a quoted string has no special meaning (DL-088
//! note 1 for `charset`; the same rule for the other lists, see the task
//! report). Every helper is a single pass over the value.

use crate::model::Field;

/// The value without spaces and tabs at both ends.
pub(crate) fn trim(value: &str) -> &str {
    value.trim_matches(|c| c == ' ' || c == '\t')
}

/// The trimmed value of the only field line with this name; `None` when there
/// are zero or several lines (the "exactly one" rules of §10).
pub(crate) fn only_value<'a>(fields: &'a [Field], name: &str) -> Option<&'a str> {
    let mut lines = fields.iter().filter(|f| f.is(name));
    let first = lines.next()?;
    if lines.next().is_some() {
        return None;
    }
    Some(trim(&first.value))
}

/// The trimmed value of the first field line with this name ("first line" of §10.1).
pub(crate) fn first_value<'a>(fields: &'a [Field], name: &str) -> Option<&'a str> {
    fields.iter().find(|f| f.is(name)).map(|f| trim(&f.value))
}

/// C-01: the media type (before any `;`) is `text/html`, ASCII case-insensitive.
pub(crate) fn is_html(content_type: &str) -> bool {
    content_type
        .split(';')
        .next()
        .is_some_and(|t| trim(t).eq_ignore_ascii_case("text/html"))
}

/// C-03 and DL-088 note 1: no `charset` parameter, or the first parameter
/// named `charset` (with `=`) has the value `utf-8` after one pair of
/// enclosing quotes is removed (ASCII case-insensitive; no escapes).
pub(crate) fn charset_is_utf8(content_type: &str) -> bool {
    let charset = content_type.split(';').skip(1).find_map(|param| {
        let (name, value) = param.split_once('=')?;
        trim(name)
            .eq_ignore_ascii_case("charset")
            .then(|| trim(value))
    });
    charset.is_none_or(|v| unquote(v).eq_ignore_ascii_case("utf-8"))
}

/// The value without one pair of enclosing `"`.
pub(crate) fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value)
}

/// C-09 and DL-088 note 3: the value has the directive `no-store`, by its name
/// before any `=` (ASCII case-insensitive, trimmed).
pub(crate) fn has_no_store(cache_control: &str) -> bool {
    cache_control.split(',').any(|d| {
        d.split('=')
            .next()
            .is_some_and(|n| trim(n).eq_ignore_ascii_case("no-store"))
    })
}

/// C-15: a `vary` value lists `template-tag` or `*` (members split at `,`,
/// trimmed, ASCII case-insensitive).
pub(crate) fn vary_covers_template_tag(vary: &str) -> bool {
    vary.split(',').any(|m| {
        let m = trim(m);
        m == "*" || m.eq_ignore_ascii_case("template-tag")
    })
}

/// P-01 `legacy-node`: an `accept-encoding` value lists `gzip` or `deflate`
/// (items split at `,`; the coding before any `;`, trimmed, ASCII
/// case-insensitive; q-values ignored, DL-076 g).
pub(crate) fn accepts_gzip_or_deflate(accept_encoding: &str) -> bool {
    accept_encoding.split(',').any(|item| {
        item.split(';').next().is_some_and(|c| {
            let c = trim(c);
            c.eq_ignore_ascii_case("gzip") || c.eq_ignore_ascii_case("deflate")
        })
    })
}

/// C-05 and DL-088 note 2: the `If-None-Match` value without one leading `W/`
/// (case-sensitive) and without one pair of enclosing `"`.
pub(crate) fn entity_tag_text(if_none_match: &str) -> &str {
    let v = trim(if_none_match);
    unquote(v.strip_prefix("W/").unwrap_or(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_removes_only_spaces_and_tabs() {
        assert_eq!(trim(" \t true \t"), "true");
        assert_eq!(trim("true\r"), "true\r");
        assert_eq!(trim(""), "");
    }

    #[test]
    fn line_lookups() {
        let f = vec![
            Field::new("A", " 1 "),
            Field::new("b", "2"),
            Field::new("a", "3"),
        ];
        assert_eq!(only_value(&f, "a"), None);
        assert_eq!(only_value(&f, "B"), Some("2"));
        assert_eq!(only_value(&f, "c"), None);
        assert_eq!(first_value(&f, "a"), Some("1"));
        assert_eq!(first_value(&f, "c"), None);
    }

    #[test]
    fn media_type_and_charset() {
        assert!(is_html("text/html"));
        assert!(is_html("TEXT/HTML ;charset=UTF-8"));
        assert!(!is_html("text/htmlx"));
        assert!(!is_html("application/json; text/html"));
        assert!(charset_is_utf8("text/html"));
        assert!(charset_is_utf8("text/html; charset=\"UTF-8\""));
        assert!(charset_is_utf8("text/html; level=1; CHARSET = utf-8 "));
        assert!(charset_is_utf8("text/html; charset; x=1"));
        assert!(!charset_is_utf8("text/html; charset; charset=iso-8859-1"));
        assert!(!charset_is_utf8("text/html; charset=utf-8x"));
        assert!(!charset_is_utf8("text/html; charset=\"utf\\-8\""));
        assert!(!charset_is_utf8("text/html; charset=latin1; charset=utf-8"));
        assert!(!charset_is_utf8("text/html; charset=\"utf-8"));
    }

    #[test]
    fn unquote_needs_a_pair() {
        assert_eq!(unquote("\"a\""), "a");
        assert_eq!(unquote("\""), "\"");
        assert_eq!(unquote("\"\""), "");
        assert_eq!(unquote("\"a"), "\"a");
        assert_eq!(unquote("a\""), "a\"");
    }

    #[test]
    fn directive_and_member_lists() {
        assert!(has_no_store("max-age=1, NO-STORE"));
        assert!(has_no_store("no-store=1"));
        assert!(!has_no_store("no-storex, private"));
        assert!(vary_covers_template_tag("Accept, *"));
        assert!(vary_covers_template_tag(" Template-Tag "));
        assert!(!vary_covers_template_tag("Cookie"));
        assert!(accepts_gzip_or_deflate("br, GZIP;q=0"));
        assert!(accepts_gzip_or_deflate("deflate"));
        assert!(!accepts_gzip_or_deflate("identity, x-gzip"));
    }

    #[test]
    fn entity_tag_text_strips_one_layer() {
        assert_eq!(entity_tag_text("W/\"abc\""), "abc");
        assert_eq!(entity_tag_text(" \"abc\" "), "abc");
        assert_eq!(entity_tag_text("w/\"abc\""), "w/\"abc\"");
        assert_eq!(entity_tag_text("W/\"W/\"abc\"\""), "W/\"abc\"");
        assert_eq!(entity_tag_text("W/"), "");
        assert_eq!(entity_tag_text("abc"), "abc");
    }
}
