// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Response selection of negotiation §4 (lane T-06): steps 1 to 9, the charset
//! rule and the inputs that the steps leave open (DL-089).
//!
//! # Invariants
//!
//! - Every outcome removes the `Rq-*` fields of A and the `Rq-*` members of its
//!   `Vary` (DL-030, DL-063); only F, D and `304` get `Rq-Version: 1`, and only D
//!   lists `Rq-Nonce` in `Vary`.
//! - F depends only on A, the configuration and the request URL, never on
//!   another request field (H-51).
//! - D is sent only for a delta-eligible request (step 5), with a non-empty
//!   change set, every changed `html` block passing the patch content rule, and a
//!   size at most the patch limit and smaller than F (RQP-10, RQP-11, RQP-23).
//! - An internal defect (an offset outside the input, a scan error, two
//!   revision algorithms that disagree) never panics: before F is chosen it
//!   gives A without its `Rq-*` fields ([`Outcome::Internal`], logged by the
//!   caller as `SERVER_PASSTHROUGH`); in step 7 it gives F.
//!
//! # Resource limits
//!
//! The work is linear in the size of the page and the request fields: one
//! markup analysis (at most 5 MiB, M-01), one scan for the head walk (it stops
//! at the insertion point), one scan for the patch content rule (only for a
//! delta candidate), one SHA-256 per block for the change set. The size of D is
//! counted before D is built, so the server never builds a delta larger than
//! the patch limit or than F. Memory: F and D are at most the page plus the
//! manifest (a few KiB; the origin and the path have limits, §1.6).

pub mod content;
pub mod fields;
pub mod head;
pub mod manifest;
pub mod url;

use rqp_markup::{Document, analyze};
use sha2::{Digest, Sha256};

use self::fields::ContentType;
use self::manifest::{BlockLine, MANIFEST_END, MANIFEST_START, Out};
use self::url::Target;
use crate::model::{AppResponse, Body, Field, Request, Response, pass_through};
use crate::rqp::config::RqpConfig;
use crate::rqp::request;
use crate::rqp::types::{IfNoneMatch, RqpRequest};

/// The delta media type (negotiation §3.6).
pub const DELTA_MEDIA_TYPE: &str = "application/vnd.rivqen.delta+json";

/// The `Content-Type` of every F (H-66).
pub const FULL_CONTENT_TYPE: &str = "text/html; charset=utf-8";

/// Which step of negotiation §4 sent the response.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Step 1, and DL-089 items 1 and 5: A without its `Rq-*` fields.
    OutOfScope,
    /// Step 2, and DL-089 item 6: A without its `Rq-*` fields.
    Invalid(Invalid),
    /// An internal defect before F: A without its `Rq-*` fields
    /// (`SERVER_PASSTHROUGH`).
    Internal,
    /// Steps 4 and 9: `304`.
    NotModified,
    /// Step 7: D.
    Delta,
    /// Step 8: F.
    Full(Full),
}

/// Why a page is invalid for RQP (step 2).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invalid {
    /// The charset rule: not UTF-8, or more than one field line or media type.
    Charset,
    /// The markup analysis (markup §2).
    Markup,
    /// No head start tag of the document and no implied head (H-72, H-79a).
    NoInsertionPoint,
    /// The request URL has no serialization in the subset of [`url`], or its
    /// origin or path is longer than the limit of manifest and patch §1.6.
    Url,
}

/// Why the server sent F (step 8).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Full {
    /// Step 5: the request is not delta-eligible.
    NotEligible,
    /// Step 6: the change set is empty.
    EmptyChangeSet,
    /// Step 7: a changed `html` block fails the patch content rule (SHOULD).
    PatchContent,
    /// Step 7: D is larger than the patch limit.
    TooLarge,
    /// Step 7: D is not smaller than F.
    NotSmaller,
    /// Step 7: an internal defect while D was built (fail closed: F).
    Defect,
}

/// The valid page with everything that F needs (steps 2 and 3).
struct Page<'a> {
    doc: Document,
    body: &'a [u8],
    before: &'a [u8],
    after: &'a [u8],
    target: Target,
    revision: String,
    manifest: Vec<u8>,
    full_len: usize,
}

/// The response of an RQP server and the step that sent it.
#[must_use]
pub fn select(config: &RqpConfig, request: &Request, app: &AppResponse) -> (Response, Outcome) {
    let cleaned = fields::clean(&app.fields);
    let pass = |outcome: Outcome| {
        let mut response = pass_through(request, app);
        response.fields.clone_from(&cleaned);
        (response, outcome)
    };
    let rq = request::parse(request);
    // Step 1, with DL-089 items 1 and 5.
    if !matches!(request.method.as_str(), "GET" | "HEAD")
        || app.status != 200
        || rq.other_precondition
        || fields::has_content_coding(&cleaned)
    {
        return pass(Outcome::OutOfScope);
    }
    match fields::content_type(&cleaned) {
        ContentType::NotHtml => return pass(Outcome::OutOfScope),
        ContentType::NotUtf8 => return pass(Outcome::Invalid(Invalid::Charset)),
        ContentType::Utf8 => {}
    }
    // Steps 2 and 3.
    let page = match page(config, request, &app.body) {
        Ok(page) => page,
        Err(outcome) => return pass(outcome),
    };
    let full_fields = full_fields(&cleaned, &page.revision);
    // Step 4.
    if not_modified(&rq.if_none_match, &page.revision) {
        let fields = full_fields
            .into_iter()
            .filter(|f| {
                [
                    "etag",
                    "cache-control",
                    "expires",
                    "vary",
                    "date",
                    "content-location",
                    "rq-version",
                ]
                .iter()
                .any(|n| f.is(n))
            })
            .collect();
        return (
            Response {
                status: 304,
                fields,
                body: Body::None,
            },
            Outcome::NotModified,
        );
    }
    // Steps 5 to 7.
    let reason = match delta(config, request, &rq, &page) {
        Ok(body) => {
            return (
                Response {
                    status: 200,
                    fields: delta_fields(&cleaned),
                    body: Body::Bytes(body),
                },
                Outcome::Delta,
            );
        }
        Err(reason) => reason,
    };
    // Step 8.
    let body = if request.method == "HEAD" {
        Body::None
    } else {
        let mut bytes = Vec::with_capacity(page.full_len);
        bytes.extend_from_slice(page.before);
        bytes.extend_from_slice(MANIFEST_START);
        bytes.extend_from_slice(&page.manifest);
        bytes.extend_from_slice(MANIFEST_END);
        bytes.extend_from_slice(page.after);
        Body::Bytes(bytes)
    };
    (
        Response {
            status: 200,
            fields: full_fields,
            body,
        },
        Outcome::Full(reason),
    )
}

/// Step 2 (markup analysis, insertion point, origin and path) and step 3 (the
/// page revision with the policy line, the manifest, the size of F).
fn page<'a>(config: &RqpConfig, request: &Request, body: &'a [u8]) -> Result<Page<'a>, Outcome> {
    let doc = analyze(body).map_err(|e| {
        if e.is_internal() {
            Outcome::Internal
        } else {
            Outcome::Invalid(Invalid::Markup)
        }
    })?;
    let at = head::insertion_point(body)
        .map_err(|_| Outcome::Internal)?
        .ok_or(Outcome::Invalid(Invalid::NoInsertionPoint))?;
    let (before, after) = body.split_at_checked(at).ok_or(Outcome::Internal)?;
    let target = url::parse(&request.url)
        .filter(Target::within_limits)
        .ok_or(Outcome::Invalid(Invalid::Url))?;
    let lines: Vec<BlockLine<'_>> = doc
        .blocks
        .iter()
        .map(|b| BlockLine {
            id: &b.id,
            format: b.format.as_str(),
            sha256: &b.sha256,
        })
        .collect();
    let policy_line = config.policy_line_text();
    let revision = manifest::page_revision(&doc.template_revision, policy_line.as_deref(), &lines);
    // Two implementations of §1.3 must agree on a page without a policy line.
    if policy_line.is_none() && revision != doc.page_revision {
        return Err(Outcome::Internal);
    }
    let manifest = manifest::manifest_text(config, &target.origin, &target.path, &revision);
    // §1.7 server item 3: M has no `<`.
    if manifest.contains(&b'<') {
        return Err(Outcome::Internal);
    }
    let full_len = body
        .len()
        .checked_add(MANIFEST_START.len())
        .and_then(|n| n.checked_add(manifest.len()))
        .and_then(|n| n.checked_add(MANIFEST_END.len()))
        .ok_or(Outcome::Internal)?;
    Ok(Page {
        doc,
        body,
        before,
        after,
        target,
        revision,
        manifest,
        full_len,
    })
}

/// The header fields of F (step 3): those of A with exactly one
/// `Content-Type: text/html; charset=utf-8` (H-66), the entity-tag E in place of an
/// `ETag` of A (strong: the engine applies no content coding, H-52), no
/// `Last-Modified` (DL-032) and one `Rq-Version: 1`.
fn full_fields(cleaned: &[Field], revision: &str) -> Vec<Field> {
    let mut out = Vec::with_capacity(cleaned.len().saturating_add(3));
    out.push(Field::new("Content-Type", FULL_CONTENT_TYPE));
    out.extend(
        cleaned
            .iter()
            .filter(|f| !f.is("content-type") && !f.is("etag") && !f.is("last-modified"))
            .cloned(),
    );
    out.push(Field::new("ETag", &format!("\"{revision}\"")));
    out.push(Field::new("Rq-Version", "1"));
    out
}

/// The header fields of D (step 7; DL-063 L1).
fn delta_fields(cleaned: &[Field]) -> Vec<Field> {
    const REPLACED: [&str; 6] = [
        "etag",
        "last-modified",
        "expires",
        "content-type",
        "x-content-type-options",
        "cache-control",
    ];
    let mut out = Vec::with_capacity(cleaned.len().saturating_add(5));
    out.push(Field::new("Content-Type", DELTA_MEDIA_TYPE));
    out.extend(
        cleaned
            .iter()
            .filter(|f| !REPLACED.iter().any(|n| f.is(n)))
            .cloned(),
    );
    out.push(Field::new("X-Content-Type-Options", "nosniff"));
    out.push(Field::new("Rq-Version", "1"));
    out.push(Field::new("Cache-Control", "no-store"));
    out.push(Field::new("Vary", "Rq-Nonce"));
    out
}

/// Step 4: the `If-None-Match` condition is false (RFC 9110 §13.1.2, weak
/// comparison against E). DL-089 item 3: another syntax keeps it true.
/// `If-Modified-Since` is ignored (DL-089 item 2).
fn not_modified(if_none_match: &IfNoneMatch, revision: &str) -> bool {
    match if_none_match {
        IfNoneMatch::Any => true,
        IfNoneMatch::Tags(tags) => tags.iter().any(|t| t.opaque == revision),
        IfNoneMatch::Absent | IfNoneMatch::Invalid => false,
    }
}

/// Steps 5, 6 and 7: the body of D, or the reason for F.
fn delta(
    config: &RqpConfig,
    request: &Request,
    rq: &RqpRequest,
    page: &Page<'_>,
) -> Result<Vec<u8>, Full> {
    // Step 5.
    let have = rq
        .have
        .as_ref()
        .filter(|h| h.template == page.doc.template_revision);
    let (Some(base), Some(have), Some(nonce)) = (rq.base.as_deref(), have, rq.nonce.as_ref())
    else {
        return Err(Full::NotEligible);
    };
    let eligible = request.method == "GET"
        && !rq.has_range
        && rq.version_1
        && rq.capabilities.iter().any(|c| c == "block-patch")
        && rq.accepts_delta;
    if !eligible {
        return Err(Full::NotEligible);
    }
    // Step 6. §3.4 step 6: `b` with L ≠ 8 × N is absent.
    let blocks = &page.doc.blocks;
    let digests = have
        .digests
        .as_deref()
        .filter(|d| blocks.len().checked_mul(8) == Some(d.len()));
    let mut changed = Vec::new();
    for (i, block) in blocks.iter().enumerate() {
        let content = page.body.get(block.start..block.end).ok_or(Full::Defect)?;
        let is_changed = match digests {
            None => true,
            Some(d) => {
                let digest = Sha256::digest(content);
                let at = i.checked_mul(8).ok_or(Full::Defect)?;
                d.get(at..at.saturating_add(8)) != digest.get(..8)
            }
        };
        if is_changed {
            changed.push((block, content));
        }
    }
    if changed.is_empty() {
        return Err(Full::EmptyChangeSet);
    }
    // Step 7: the patch content rule on the changed `html` blocks.
    let spans: Vec<(usize, usize)> = changed
        .iter()
        .filter(|(b, _)| b.format == rqp_markup::Format::Html)
        .map(|(b, _)| (b.start, b.end))
        .collect();
    if !content::blocks_pass(page.body, &spans).unwrap_or(false) {
        return Err(Full::PatchContent);
    }
    let binding = manifest::binding(&page.target.canonical(), base, nonce, &page.revision);
    let operations: Vec<(&str, &[u8])> = changed.iter().map(|(b, c)| (b.id.as_str(), *c)).collect();
    let mut count = Out::counter();
    manifest::envelope(&mut count, &binding, &operations);
    if count.len() > config.patch_limit() {
        return Err(Full::TooLarge);
    }
    if count.len() >= page.full_len {
        return Err(Full::NotSmaller);
    }
    let mut out = Out::buffer(count.len());
    manifest::envelope(&mut out, &binding, &operations);
    if out.len() != count.len() {
        return Err(Full::Defect);
    }
    Ok(out.into_bytes())
}
