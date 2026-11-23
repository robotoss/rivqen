// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The page fields of a response (legacy client CL-21) and the merge by name
//! of a `304` (CL-17).
//!
//! Owner: WP-08 S1, T-01.
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
//!   `Authentication-Info` (DL-129).
//!
//! Names and `Connection` tokens compare ASCII case-insensitively. The kept
//! lines have their names in ASCII lowercase and are in a stable order by
//! name: the lines of one name keep their response order (the form of the
//! client model, `pageFields` and `sortedFields`). Values are not changed.
//!
//! [`merge_page_fields`] is the field update of a `304` (CL-17, RFC 9111
//! §4.3.4): each name of the page fields of the response replaces all
//! stored lines of that name.
//!
//! # Safety
//!
//! The work is `O(n log n)` in the number of lines and linear in their
//! bytes; the output is never longer than the input. No `Debug` output holds
//! a field value.

use std::collections::BTreeSet;

use rivqen_model::http::FieldLine;

use crate::response::{
    CACHE_OFFLINE, SONIC_ETAG_KEY, TEMPLATE_CHANGE, TEMPLATE_TAG, etag_field_name, trim_ows,
    values_of,
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

/// The lowercase names that a response removes from its page fields: the
/// etag field and the `Connection` tokens. The fixed names are tested apart.
fn response_drops(fields: &[FieldLine]) -> BTreeSet<String> {
    let mut drop = BTreeSet::new();
    drop.insert(etag_field_name(fields).to_ascii_lowercase());
    for value in values_of(fields, CONNECTION) {
        for token in value.split(',').map(trim_ows) {
            if !token.is_empty() {
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
/// stored lines stay. Names in ASCII lowercase, stable order by name.
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
    use super::NOT_PAGE_FIELDS;

    #[test]
    fn not_page_fields_are_lowercase_and_unique() {
        for name in NOT_PAGE_FIELDS {
            assert_eq!(name, name.to_ascii_lowercase());
            let count = NOT_PAGE_FIELDS.iter().filter(|n| **n == name).count();
            assert_eq!(count, 1, "{name} repeats");
        }
    }
}
