// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The page fields of a response (legacy client CL-21) and the merge by name
//! of a `304` (CL-17).
//!
//! Owner: WP-08 S1, T-01; field line rules: F-03 (DL-226).
//!
//! # Rules
//!
//! [`page_fields`] keeps every field line of a response except:
//!
//! - `Set-Cookie`, `Set-Cookie2` (CL-11 item 3);
//! - the hop-by-hop fields `Connection`, `Keep-Alive`, `Proxy-Connection`,
//!   `TE`, `Trailer`, `Transfer-Encoding`, `Upgrade`, and every name that a
//!   `Connection` line lists (comma-separated tokens);
//! - `Cache-Control`, `Expires`; `Content-Encoding`, `Content-Length`;
//! - the legacy control fields: the etag field (the field that
//!   `sonic-etag-key` names, else `etag`), `sonic-etag-key`, `template-tag`,
//!   `template-change`, `cache-offline`;
//! - `Vary`, `CDN-Cache-Control`, `Surrogate-Control`;
//! - the action fields `Clear-Site-Data`, `Refresh`, `WWW-Authenticate`,
//!   `Authentication-Info` (DL-129);
//! - a line that is not a valid field line ([`is_valid_field_line`], DL-226
//!   (b)): its name is not an RFC 9110 token, or its value has a control
//!   byte other than HTAB.
//!
//! Names and `Connection` tokens compare ASCII case-insensitively. The kept
//! lines have their names in ASCII lowercase and are in a stable order by
//! name: the lines of one name keep their response order (the form of the
//! client model, `pageFields` and `sortedFields`). Values are not changed.
//!
//! [`merge_page_fields`] is the field update of a `304` (CL-17, RFC 9111
//! §4.3.4): each name of the page fields of the response replaces all
//! stored lines of that name. It also drops a stored line that is not a
//! valid field line (a stored record can be old or damaged).
//!
//! # Invariants (DL-226)
//!
//! - No line that leaves this module has a name that is not a token or a
//!   value with a control byte other than HTAB.
//! - `sonic-etag-key` and `Connection` never remove `Content-Type` or a
//!   protected field ([`is_protected_field`]) from the page fields: a valid
//!   line of these names is always kept. A response whose `sonic-etag-key`
//!   or `Connection` names such a field is not a legacy response
//!   (`response::is_safe_head`), so the session never uses its page fields;
//!   the rule here is the second barrier.
//!
//! # Safety
//!
//! The work is `O(n log n)` in the number of lines and linear in their
//! bytes; the output is never longer than the input. No `Debug` output holds
//! a field value.

use std::collections::BTreeSet;

use rivqen_model::http::FieldLine;

use crate::response::{
    CACHE_OFFLINE, CONTENT_TYPE, SONIC_ETAG_KEY, TEMPLATE_CHANGE, TEMPLATE_TAG, etag_field_name,
    trim_ows, values_of,
};

/// Field name `connection`, lower case.
pub const CONNECTION: &str = "connection";

/// The names that never go to the web view or the cache, except the etag
/// field and the names that `Connection` lists (CL-21). Lower case.
pub const NOT_PAGE_FIELDS: [&str; 24] = [
    "set-cookie",
    "set-cookie2",
    CONNECTION,
    "keep-alive",
    "proxy-connection",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "cache-control",
    "expires",
    "content-encoding",
    "content-length",
    SONIC_ETAG_KEY,
    TEMPLATE_TAG,
    TEMPLATE_CHANGE,
    CACHE_OFFLINE,
    "vary",
    "cdn-cache-control",
    "surrogate-control",
    "clear-site-data",
    "refresh",
    "www-authenticate",
    "authentication-info",
];

/// The protected fields of DL-226 (a), lower case: the security fields of a
/// page. One response field line (`sonic-etag-key` or a `Connection` token)
/// must never take them out of the page fields. Every name that starts with
/// [`PROTECTED_FIELD_PREFIX`] is protected too; [`is_protected_field`] tests
/// both.
pub const PROTECTED_FIELDS: [&str; 7] = [
    "content-security-policy",
    "content-security-policy-report-only",
    "x-frame-options",
    "x-content-type-options",
    "permissions-policy",
    "referrer-policy",
    "strict-transport-security",
];

/// Every field name that starts with this text (ASCII case-insensitive) is
/// a protected field (DL-226 (a)), for example
/// `Cross-Origin-Opener-Policy`. Lower case.
pub const PROTECTED_FIELD_PREFIX: &str = "cross-origin-";

/// DL-226 (a): true when `name` is a protected field: a name of
/// [`PROTECTED_FIELDS`] or a name that starts with
/// [`PROTECTED_FIELD_PREFIX`], ASCII case-insensitive. The name is compared
/// as given (not trimmed).
#[must_use]
pub fn is_protected_field(name: &str) -> bool {
    let prefix = PROTECTED_FIELD_PREFIX.as_bytes();
    PROTECTED_FIELDS
        .iter()
        .any(|protected| name.eq_ignore_ascii_case(protected))
        || name
            .as_bytes()
            .get(..prefix.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
}

/// RFC 9110 §5.6.2 `tchar`.
const fn is_tchar(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

/// RFC 9110 §5.6.2 `token` (`1*tchar`): not empty, only `tchar` bytes.
pub(crate) fn is_token(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(is_tchar)
}

/// A control byte that no field value may hold (DL-226 (b)): `0x00`–`0x08`,
/// `0x0A`–`0x1F`, `0x7F`. HTAB (`0x09`) is allowed.
const fn is_forbidden_value_byte(byte: u8) -> bool {
    (byte < 0x20 && byte != b'\t') || byte == 0x7f
}

/// DL-226 (b): true when the name is an RFC 9110 token (`1*tchar`) and the
/// value has no control byte other than HTAB (SP and bytes from `0x80` are
/// allowed). A response with a line that fails this test is not a legacy
/// response, and [`page_fields`] and [`merge_page_fields`] drop such a line.
#[must_use]
pub fn is_valid_field_line(line: &FieldLine) -> bool {
    is_token(&line.name) && !line.value.bytes().any(is_forbidden_value_byte)
}

/// True for a name that `sonic-etag-key` or `Connection` never removes from
/// the page fields: `Content-Type` and the protected fields (DL-226).
fn is_always_kept(name: &str) -> bool {
    name.eq_ignore_ascii_case(CONTENT_TYPE) || is_protected_field(name)
}

/// The lowercase names that a response removes from its page fields: the
/// etag field and the `Connection` tokens, but never `Content-Type` or a
/// protected field. The fixed names are tested apart.
fn response_drops(fields: &[FieldLine]) -> BTreeSet<String> {
    let mut drop = BTreeSet::new();
    let etag_field = etag_field_name(fields);
    if !is_always_kept(etag_field) {
        drop.insert(etag_field.to_ascii_lowercase());
    }
    for value in values_of(fields, CONNECTION) {
        for token in value.split(',').map(trim_ows) {
            if !token.is_empty() && !is_always_kept(token) {
                drop.insert(token.to_ascii_lowercase());
            }
        }
    }
    drop
}

/// CL-21: the page fields of a response (see the module rules). Names in
/// ASCII lowercase, stable order by name.
#[must_use]
pub fn page_fields(fields: &[FieldLine]) -> Vec<FieldLine> {
    let drop = response_drops(fields);
    let mut out: Vec<FieldLine> = fields
        .iter()
        .filter(|line| is_valid_field_line(line))
        .filter_map(|line| {
            let name = line.name.to_ascii_lowercase();
            let fixed = NOT_PAGE_FIELDS.contains(&name.as_str());
            (!fixed && !drop.contains(&name)).then(|| FieldLine::new(name, line.value.clone()))
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// CL-17 (RFC 9111 §4.3.4): the page fields of a stored entry after a `304`
/// with the field lines `response`. The page fields of the response
/// ([`page_fields`]) replace every stored line of their names; the other
/// stored lines stay, except a stored line that is not a valid field line
/// ([`is_valid_field_line`], DL-226 (b)). Names in ASCII lowercase, stable
/// order by name.
///
/// The session applies the cache rules of CL-17 first (`no-store`,
/// `Vary: *`, `cache-offline`, credentials); this function is only the
/// update by name.
#[must_use]
pub fn merge_page_fields(stored: &[FieldLine], response: &[FieldLine]) -> Vec<FieldLine> {
    let update = page_fields(response);
    let names: BTreeSet<&str> = update.iter().map(|line| line.name.as_str()).collect();
    let mut out: Vec<FieldLine> = stored
        .iter()
        .filter(|line| is_valid_field_line(line))
        .filter_map(|line| {
            let name = line.name.to_ascii_lowercase();
            (!names.contains(name.as_str())).then(|| FieldLine::new(name, line.value.clone()))
        })
        .collect();
    out.extend(update.iter().cloned());
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

#[cfg(test)]
mod tests {
    use super::{NOT_PAGE_FIELDS, PROTECTED_FIELD_PREFIX, PROTECTED_FIELDS, is_tchar, is_token};

    #[test]
    fn not_page_fields_are_lowercase_and_unique() {
        for name in NOT_PAGE_FIELDS {
            assert_eq!(name, name.to_ascii_lowercase());
            let count = NOT_PAGE_FIELDS.iter().filter(|n| **n == name).count();
            assert_eq!(count, 1, "{name} repeats");
        }
    }

    #[test]
    fn protected_fields_are_lowercase_tokens_and_never_not_page_fields() {
        for name in PROTECTED_FIELDS.iter().chain([&PROTECTED_FIELD_PREFIX]) {
            assert_eq!(*name, name.to_ascii_lowercase());
            assert!(is_token(name), "{name}");
            assert!(!NOT_PAGE_FIELDS.contains(name), "{name}");
        }
    }

    #[test]
    fn tchar_is_the_set_of_rfc_9110() {
        let tchars: String = (0_u8..=255)
            .filter(|b| is_tchar(*b))
            .map(char::from)
            .collect();
        assert_eq!(
            tchars,
            "!#$%&'*+-.0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ^_`abcdefghijklmnopqrstuvwxyz|~"
        );
        assert!(!is_token(""));
    }
}
