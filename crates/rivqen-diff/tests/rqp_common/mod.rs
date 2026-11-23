// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Builders of the `rqp` tests: stored snapshots and the deltas that a server
//! makes for them (manifest and patch sections 2.2 and 2.5).
//!
//! A server delta here is built from two documents, as a server does: the
//! operations carry the content bytes of the next document, and R′ is the
//! page revision of the markup analysis of the next document. The client
//! functions (`next_page_revision`, `verify_delta`) are not used to build
//! a delta.

#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::fmt::Write as _;

use rivqen_diff::rqp::ValidatedPatch;
use rivqen_diff::rqp::{DeltaRequest, PatchLimits, StoredSnapshot, ValidateError, validate};
use rivqen_integrity::PageRevision;
use rivqen_model::limits::ResourceLimits;
use rivqen_proto::compute::{binding, next_page_revision};
use rivqen_proto::types::{
    BlockFormat, BlockId, Limits, MAX_BLOCK_BYTES, Nonce, PatchOp, Policy, SnapshotMeta,
};
use rivqen_template::limits::TemplateLimits;
use rivqen_template::rqp::{RqpDocument, split_rqp};

pub const URL: &str = "https://shop.example.com/catalog";
pub const OTHER_URL: &str = "https://shop.example.com/catalog?page=2";
pub const NONCE: Nonce = Nonce([7; 16]);
pub const OTHER_NONCE: Nonce = Nonce([8; 16]);

/// title (html), price (html), cart (json).
pub const PAGE: &str = "<!DOCTYPE html><html><head>\
<title data-rq-block=\"title\">Shop</title></head><body>\
<p data-rq-block=\"price\">120.00</p>\
<script type=\"application/json\" data-rq-block=\"cart\">{\"items\":3}</script>\
</body></html>";

/// The client limits of `ResourceLimits::default()`.
pub fn limits() -> PatchLimits {
    PatchLimits::try_from(&ResourceLimits::default()).unwrap()
}

/// A page with a static marker (part of the template) and blocks `b0`,
/// `b1`, … of the given formats and contents.
pub fn page_of(marker: &str, blocks: &[(BlockFormat, String)]) -> String {
    let mut page = format!(
        "<!DOCTYPE html><html><head><title>Shop</title></head><body><header>{marker}</header>"
    );
    for (index, (format, content)) in blocks.iter().enumerate() {
        match format {
            BlockFormat::Html => {
                write!(page, "<div data-rq-block=\"b{index}\">{content}</div>").unwrap();
            }
            BlockFormat::Json => write!(
                page,
                "<script type=\"application/json\" data-rq-block=\"b{index}\">{content}</script>"
            )
            .unwrap(),
        }
    }
    page.push_str("</body></html>");
    page
}

/// A stored snapshot: the original document, its analysis and the values
/// that the client computed for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snap {
    pub original: Vec<u8>,
    pub analysis: RqpDocument,
    pub meta: SnapshotMeta,
}

impl Snap {
    pub fn new(document: &str) -> Self {
        Self::with_members(document, None, None)
    }

    pub fn with_members(document: &str, policy: Option<Policy>, limits: Option<Limits>) -> Self {
        Self::from_bytes(document.as_bytes(), policy, limits)
    }

    pub fn from_bytes(document: &[u8], policy: Option<Policy>, limits: Option<Limits>) -> Self {
        let analysis = split_rqp(document, &TemplateLimits::default()).unwrap();
        let meta = SnapshotMeta {
            template_revision: analysis.template_revision,
            page_revision: analysis.page_revision(policy.as_ref(), limits.as_ref()),
            blocks: analysis
                .blocks
                .iter()
                .map(|b| b.descriptor.clone())
                .collect(),
            policy,
            limits,
        };
        Self {
            original: document.to_vec(),
            analysis,
            meta,
        }
    }

    /// The snapshot of a validated patch, as the session stores it.
    pub fn from_patch(patch: &ValidatedPatch) -> Self {
        Self {
            original: patch.document().to_vec(),
            analysis: patch.analysis().clone(),
            meta: patch.snapshot_meta().clone(),
        }
    }

    pub fn stored(&self) -> StoredSnapshot<'_> {
        StoredSnapshot {
            original: &self.original,
            analysis: &self.analysis,
            meta: &self.meta,
        }
    }

    pub fn revision(&self) -> PageRevision {
        self.meta.page_revision
    }

    /// The content of block `index`, as text.
    pub fn content(&self, index: usize) -> &str {
        let block = &self.analysis.blocks[index];
        std::str::from_utf8(&self.original[block.start..block.end]).unwrap()
    }

    pub fn id(&self, index: usize) -> String {
        self.analysis.blocks[index].descriptor.id.to_string()
    }
}

/// The envelope text with a binding text and the operations (id, content).
pub fn envelope(binding: &str, operations: &[(String, String)]) -> Vec<u8> {
    let operations: Vec<String> = operations
        .iter()
        .map(|(id, content)| {
            format!(
                r#"{{"block_id":{},"content":{}}}"#,
                serde_json::to_string(id).unwrap(),
                serde_json::to_string(content).unwrap()
            )
        })
        .collect();
    format!(
        r#"{{"binding":"{binding}","operations":[{}]}}"#,
        operations.join(",")
    )
    .into_bytes()
}

/// The operations of a server delta from `base` to `next` for the blocks
/// `indices` (document order): the content of each block in `next`.
pub fn operations(next: &Snap, indices: &[usize]) -> Vec<(String, String)> {
    indices
        .iter()
        .map(|&i| (next.id(i), next.content(i).to_owned()))
        .collect()
}

/// The indices of the blocks whose content differs between two snapshots
/// of one template; every block when none differs (the H-7 form).
pub fn changed(base: &Snap, next: &Snap) -> Vec<usize> {
    let count = base.analysis.blocks.len();
    let changed: Vec<usize> = (0..count)
        .filter(|&i| base.content(i) != next.content(i))
        .collect();
    if changed.is_empty() {
        (0..count).collect()
    } else {
        changed
    }
}

/// The delta that a server sends for a request with U `url`, base `base`
/// and nonce `nonce` when its current document is `next`, with an operation
/// for each block of `indices`.
pub fn server_delta(
    base: &Snap,
    next: &Snap,
    indices: &[usize],
    url: &str,
    nonce: &Nonce,
) -> Vec<u8> {
    let bound = binding(url, &base.revision(), nonce, &next.revision());
    envelope(&bound.to_string(), &operations(next, indices))
}

/// The server delta for the changed blocks, for `URL` and `NONCE`.
pub fn delta(base: &Snap, next: &Snap) -> Vec<u8> {
    server_delta(base, next, &changed(base, next), URL, &NONCE)
}

/// A delta whose binding fits `base`, `URL` and `NONCE` for operations whose
/// result is not valid RQP markup (no server document exists). R′ comes
/// from the block hashes of `base` and the new contents, as check 11
/// computes it; only for the inputs that check 12 must reject. Operations
/// that check 9 rejects get [`UNBOUND`]: no R′ exists for them.
pub fn bound_delta(base: &Snap, operations: &[(String, String)]) -> Vec<u8> {
    let ops: Vec<PatchOp> = operations
        .iter()
        .map(|(id, content)| PatchOp {
            block_id: BlockId::parse(id).unwrap(),
            content: content.clone(),
        })
        .collect();
    match next_page_revision(&base.meta, &ops, MAX_BLOCK_BYTES) {
        Ok(next) => {
            let bound = binding(URL, &base.revision(), &NONCE, &next);
            envelope(&bound.to_string(), operations)
        }
        Err(_) => envelope(UNBOUND, operations),
    }
}

/// A binding text in the canonical form that fits no request.
pub const UNBOUND: &str = "d1.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

pub fn request<'a>(url: &'a str, nonce: &'a Nonce) -> DeltaRequest<'a> {
    DeltaRequest {
        final_url: Some(url),
        nonce: Some(nonce),
    }
}

/// `validate` with `URL`, `NONCE` and the default limits.
pub fn run(body: &[u8], snap: &Snap) -> Result<ValidatedPatch, ValidateError> {
    validate(body, &snap.stored(), &request(URL, &NONCE), &limits())
}
