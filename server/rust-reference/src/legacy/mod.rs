// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Legacy engine: `legacy-canonical` (legacy wire §10, C-01…C-16) and the
//! upstream profiles (§10.1, P-01…P-11). Lane T-03.
//!
//! Written from the spec pages, not from the model that generated the
//! fixtures (DL-087). Pure: no I/O, no clock, no state between calls (U-01).
//!
//! Order of the steps (one pass each, linear in the input, C-16):
//!
//! 1. scope C-01 and text C-03 (every profile);
//! 2. the legacy request C-02 / P-01; a request that is not a legacy request
//!    is pass-through, except in `legacy-java` (P-02);
//! 3. the split C-06…C-08 (P-09, P-10); a failed split is pass-through, also
//!    before a matching `If-None-Match` (C-07, DL-076 a);
//! 4. `304` C-05 / P-03 (legacy request only);
//! 5. data or full C-11, C-12 / P-04;
//! 6. the fields C-09, C-10, C-13, C-15 / P-05…P-08.

mod fields;
mod json;
mod split;

pub use split::{Split, TITLE_KEY, split};

use sha1::{Digest, Sha1};

use crate::model::{AppResponse, Body, Field, Profile, Request, Response, pass_through};
use fields::{
    accepts_gzip_or_deflate, charset_is_utf8, entity_tag_text, first_value, has_no_store, is_html,
    only_value, trim, vary_covers_template_tag,
};

/// The response of a server in `profile` to `request`, given the application
/// response `app`.
///
/// Contract: never fails and never panics; every input that the rules do not
/// process gives the pass-through. Linear in the size of the input (C-16).
#[must_use]
pub fn respond(profile: Profile, request: &Request, app: &AppResponse) -> Response {
    legacy_response(profile, request, app).unwrap_or_else(|| pass_through(request, app))
}

/// The split of the application page under `profile` when the split runs:
/// the request and response are in the scope of C-01 and C-03, and the split
/// does not fail (C-07). The legacy request rules (C-02, P-01) do not take
/// part: this is `split` of the fixtures (`fixtures/legacy/README.md`).
#[must_use]
pub fn split_of(profile: Profile, request: &Request, app: &AppResponse) -> Option<Split> {
    split(profile, page_text(request, app)?)
}

/// The lowercase hex SHA-1 of `bytes` (C-04, C-08).
#[must_use]
pub fn sha1_hex(bytes: &[u8]) -> String {
    let digest = Sha1::digest(bytes);
    let mut out = String::with_capacity(40);
    for b in digest {
        out.push(json::hex_digit(u32::from(b >> 4)));
        out.push(json::hex_digit(u32::from(b & 0xf)));
    }
    out
}

/// The kind of a legacy response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// C-05: `304`.
    NotModified,
    /// C-11: the data response.
    Data,
    /// C-12: the full response.
    Full,
}

fn legacy_response(profile: Profile, request: &Request, app: &AppResponse) -> Option<Response> {
    let text = page_text(request, app)?;
    let legacy = is_legacy_request(profile, &request.fields);
    if !legacy && profile != Profile::Java {
        return None;
    }
    let split = split(profile, text)?;
    let etag = sha1_hex(&app.body);
    let tag = sha1_hex(split.template.as_bytes());

    let kind = if legacy && is_not_modified(profile, &request.fields, &etag) {
        Kind::NotModified
    } else if wants_data(profile, &request.fields, &tag, &split) {
        Kind::Data
    } else {
        Kind::Full
    };
    let (status, body) = match kind {
        Kind::NotModified => (304, Body::None),
        Kind::Data => (200, Body::Bytes(json::data_body(&split.data, &tag, &etag))),
        Kind::Full => (200, Body::Bytes(app.body.clone())),
    };
    Some(Response {
        status,
        fields: response_fields(profile, legacy, kind, &app.fields, &etag, &tag),
        body,
    })
}

/// C-01 and C-03: the page text when the request and the application response
/// can get legacy processing.
fn page_text<'a>(request: &Request, app: &'a AppResponse) -> Option<&'a str> {
    if request.method != "GET" || app.status != 200 {
        return None;
    }
    let content_type = only_value(&app.fields, "content-type")?;
    if !is_html(content_type) || !charset_is_utf8(content_type) {
        return None;
    }
    std::str::from_utf8(&app.body).ok()
}

/// C-02 and P-01.
fn is_legacy_request(profile: Profile, fields: &[Field]) -> bool {
    match profile {
        Profile::Canonical => only_value(fields, "accept-diff") == Some("true"),
        Profile::Java => first_value(fields, "accept-diff") == Some("true"),
        Profile::Node => {
            first_value(fields, "accept-diff").is_some_and(|v| !v.is_empty())
                && fields
                    .iter()
                    .any(|f| f.is("accept-encoding") && accepts_gzip_or_deflate(&f.value))
        }
        Profile::Php => fields
            .iter()
            .find(|f| f.name == "accept-diff")
            .is_some_and(|f| trim(&f.value) == "true"),
    }
}

/// C-05 and P-03.
fn is_not_modified(profile: Profile, fields: &[Field], etag: &str) -> bool {
    match profile {
        Profile::Canonical => only_value(fields, "if-none-match")
            .is_some_and(|v| entity_tag_text(v).eq_ignore_ascii_case(etag)),
        Profile::Java => {
            first_value(fields, "if-none-match").is_some_and(|v| v.eq_ignore_ascii_case(etag))
        }
        Profile::Node | Profile::Php => first_value(fields, "if-none-match") == Some(etag),
    }
}

/// C-11 and P-04.
fn wants_data(profile: Profile, fields: &[Field], tag: &str, split: &Split) -> bool {
    match profile {
        Profile::Canonical => only_value(fields, "template-tag") == Some(tag),
        Profile::Node => split.blocks > 0 && first_value(fields, "template-tag") == Some(tag),
        Profile::Java | Profile::Php => first_value(fields, "template-tag") == Some(tag),
    }
}

/// C-09, C-10, C-13, C-15 and P-02, P-05…P-08.
///
/// The kept lines of A stay in their order; the legacy lines follow them.
fn response_fields(
    profile: Profile,
    legacy: bool,
    kind: Kind,
    app: &[Field],
    etag: &str,
    tag: &str,
) -> Vec<Field> {
    let no_store = app
        .iter()
        .any(|f| f.is("cache-control") && has_no_store(&f.value));
    // P-02 (`legacy-java`, not a legacy request): `cache-control` and
    // `cache-offline` of A stay as they are. P-05 `legacy-node`: the
    // `cache-control` lines of A stay. C-09 / P-05: `no-store` keeps them.
    let keep_cache_control = !legacy || profile == Profile::Node || no_store;
    let keep_app_offline = !legacy;
    // P-08.
    let drop_content_type =
        kind == Kind::NotModified && matches!(profile, Profile::Java | Profile::Node);

    let mut out: Vec<Field> = app
        .iter()
        .filter(|f| {
            !(f.is("content-length")
                || f.is("transfer-encoding")
                || f.is("connection")
                || f.is("etag")
                || f.is("template-tag")
                || f.is("template-change")
                || f.is("sonic-etag-key")
                || f.is("cdn-cache-control")
                || f.is("surrogate-control")
                || (f.is("cache-offline") && !keep_app_offline)
                || (f.is("cache-control") && !keep_cache_control)
                || (f.is("content-type") && drop_content_type))
        })
        .cloned()
        .collect();

    if kind != Kind::NotModified {
        out.push(Field::new("etag", etag));
        out.push(Field::new("template-tag", tag));
        let change = if kind == Kind::Data { "false" } else { "true" };
        out.push(Field::new("template-change", change));
    }
    if legacy {
        push_cache_offline(&mut out, profile, kind, app);
    }
    if !keep_cache_control {
        let value = if profile == Profile::Canonical {
            "no-cache, private"
        } else {
            "no-cache"
        };
        out.push(Field::new("cache-control", value));
    }
    if !app
        .iter()
        .any(|f| f.is("vary") && vary_covers_template_tag(&f.value))
    {
        out.push(Field::new("vary", "template-tag"));
    }
    out
}

/// C-10, P-06 and P-07 (legacy request only).
fn push_cache_offline(out: &mut Vec<Field>, profile: Profile, kind: Kind, app: &[Field]) {
    match profile {
        Profile::Canonical => {
            let value = if kind == Kind::NotModified {
                "store"
            } else {
                canonical_offline(app)
            };
            out.push(Field::new("cache-offline", value));
        }
        Profile::Java => {
            out.push(Field::new("cache-offline", "true"));
            out.extend(app.iter().filter(|f| f.is("cache-offline")).cloned());
            if kind == Kind::NotModified {
                out.push(Field::new("cache-offline", "store"));
            }
        }
        Profile::Node | Profile::Php => {
            let value = if kind == Kind::NotModified {
                "store"
            } else {
                "true"
            };
            out.push(Field::new("cache-offline", value));
        }
    }
}

/// C-10 on `200` and DL-088 note 4: the value of the only `cache-offline`
/// line of A when it is `true`, `false`, `store` or `http`; else `true`.
fn canonical_offline(app: &[Field]) -> &'static str {
    match only_value(app, "cache-offline") {
        Some("false") => "false",
        Some("store") => "store",
        Some("http") => "http",
        _ => "true",
    }
}
