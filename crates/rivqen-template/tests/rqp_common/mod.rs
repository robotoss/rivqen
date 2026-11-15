// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Shared helpers of the `rqp_*` tests.
//!
//! The tests `rqp_rules`, `rqp_api` and `rqp_props` are ports of the
//! candidate tests `tools/rqp-markup/rust/tests/{rules,api,props}.rs`
//! (DL-161). To keep them close to the candidate text (risk R-2: a rule
//! change changes both copies), [`analyze`] gives the result object of
//! `tools/rqp-markup/CONTRACT.md` section 3 in the shape of the candidate
//! (ids, hashes and revisions as their wire text), made from
//! [`split_rqp`] with the default limits and
//! [`RqpDocument::page_revision`] without a policy line.

// Each test binary uses a part of these helpers.
#![allow(dead_code, unused_imports, unreachable_pub)]

pub use rivqen_proto::types::{BlockFormat as Format, MAX_BLOCK_BYTES};
pub use rivqen_template::limits::{
    MAX_MARKUP_INPUT_BYTES as MAX_INPUT_BYTES, MAX_OPEN_ELEMENTS, TemplateLimits,
};
pub use rivqen_template::rqp::{MarkupCode as ErrorCode, MarkupError, RqpDocument, split_rqp};

/// One block of a valid document, with the wire text of its id and hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// The block id.
    pub id: String,
    /// The block format.
    pub format: Format,
    /// Offset of the first content byte.
    pub start: usize,
    /// Offset of the `<` of the block end tag.
    pub end: usize,
    /// `B64U(SHA-256(content))`.
    pub sha256: String,
}

/// The result of a valid document, with the wire text of the revisions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    /// Blocks in document order.
    pub blocks: Vec<Block>,
    /// `t1.` revision of the template.
    pub template_revision: String,
    /// `r1.` revision of the page without a policy line.
    pub page_revision: String,
}

impl From<&RqpDocument> for Document {
    fn from(doc: &RqpDocument) -> Self {
        Self {
            blocks: doc
                .blocks
                .iter()
                .map(|block| Block {
                    id: block.descriptor.id.as_str().to_owned(),
                    format: block.descriptor.format,
                    start: block.start,
                    end: block.end,
                    sha256: block.descriptor.sha256.to_b64u(),
                })
                .collect(),
            template_revision: doc.template_revision.to_string(),
            page_revision: doc.page_revision(None, None).to_string(),
        }
    }
}

/// [`split_rqp`] with the default limits (M-26), as the candidate result.
///
/// # Errors
///
/// The [`MarkupError`] of [`split_rqp`].
pub fn analyze(input: &[u8]) -> Result<Document, MarkupError> {
    split_rqp(input, &TemplateLimits::default()).map(|doc| Document::from(&doc))
}

/// `B64U(SHA-256(bytes))`.
#[must_use]
pub fn b64u_sha256(bytes: &[u8]) -> String {
    rivqen_integrity::sha256(bytes).to_b64u()
}
