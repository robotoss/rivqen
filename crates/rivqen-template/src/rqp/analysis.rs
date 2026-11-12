// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Port of the analysis of `tools/rqp-markup/rust/src/lib.rs` (`analyze`,
//! `document`, `template_revision`; commit `92c708e1`; DL-161). Keep the
//! rules equal to the candidate: a change of the markup rules changes both
//! copies (WP-06 S1, risk R-2).
//!
//! Port changes: the limits come from [`TemplateLimits`] (DL-163); the
//! blocks are [`RqpBlock`] values with workspace types; the template
//! revision is a [`TemplateRevision`]; the page revision is
//! [`RqpDocument::page_revision`](crate::rqp::RqpDocument::page_revision)
//! (it needs the policy line of the manifest). No CLI and no `result_json`.
//!
//! Resource limits: the input size is checked first (M-01); the depth
//! counter D of M-26 bounds the token stack; memory and work are linear in
//! the input size.

use rivqen_integrity::{TemplateRevision, sha256, sha256_parts};
use rivqen_proto::types::{BlockDescriptor, BlockId};

use crate::limits::{MAX_MARKUP_INPUT_BYTES, TemplateLimits};
use crate::rqp::rules::{self, RawBlock, RuleLimits};
use crate::rqp::{MarkupCode, MarkupError, RqpBlock, RqpDocument};

/// The UTF-8 byte order mark. The tokenizer starts after it; the offsets
/// keep it (M-02).
pub(crate) const BOM: &[u8] = b"\xEF\xBB\xBF";

/// The tag of the template revision preimage (manifest and patch section
/// 1.3, step 3).
const TEMPLATE_TAG: &[u8] = b"rqp-t1\n";

/// The rule of a [`MarkupError`] for an internal defect of this module (for
/// example an offset outside the input). No rule of `markup.md` causes it.
pub(crate) const INTERNAL_RULE: &str = "internal";

/// The internal defect error of the port (the candidate has it as inherent
/// functions of its error type; the contract type here has no such item).
pub(crate) trait Internal {
    /// An internal defect. The document fails closed: it is invalid with
    /// `RQP_MARKUP_STRUCTURE` and the rule [`INTERNAL_RULE`].
    fn internal() -> Self;

    /// True for an internal defect ([`INTERNAL_RULE`]).
    #[cfg_attr(not(test), allow(dead_code))]
    fn is_internal(&self) -> bool;
}

impl Internal for MarkupError {
    fn internal() -> Self {
        Self::new(MarkupCode::Structure, INTERNAL_RULE)
    }

    fn is_internal(&self) -> bool {
        self.rule() == INTERNAL_RULE
    }
}

/// Checks `input` by the rules of `markup.md` section 2 within `limits` and
/// returns its blocks and its template revision, or the first rule that
/// fails.
pub(crate) fn analyze(input: &[u8], limits: &TemplateLimits) -> Result<RqpDocument, MarkupError> {
    // M-01 first, before any work that grows with the input. A lower
    // `max_document_bytes` lowers the limit (DL-163).
    let max = limits.max_document_bytes.min(MAX_MARKUP_INPUT_BYTES);
    if input.len() > max {
        return Err(MarkupError::new(MarkupCode::Limit, "M-01"));
    }
    // M-02: RFC 3629 (no overlong forms, no surrogates, nothing above
    // U+10FFFF, no truncated sequence).
    if core::str::from_utf8(input).is_err() {
        return Err(MarkupError::new(MarkupCode::Encoding, "M-02"));
    }
    let start = if input.starts_with(BOM) { BOM.len() } else { 0 };
    let found = rules::run(input, start, RuleLimits::new(limits), |_, _| {})?;
    document(input, &found)
}

/// The result of a valid document (M-27). A span outside the input, an id
/// that is not a block id, or blocks out of order is an internal defect: the
/// document fails closed and no hash of missing bytes is ever made.
fn document(input: &[u8], found: &[RawBlock]) -> Result<RqpDocument, MarkupError> {
    let mut blocks = Vec::with_capacity(found.len());
    for raw in found {
        let id = input
            .get(raw.id.start..raw.id.end)
            .ok_or_else(MarkupError::internal)?;
        // The pass checked M-15 on these bytes; `BlockId::parse` checks the
        // same rule again.
        let id = core::str::from_utf8(id)
            .ok()
            .and_then(|text| BlockId::parse(text).ok())
            .ok_or_else(MarkupError::internal)?;
        let content = input
            .get(raw.content.start..raw.content.end)
            .ok_or_else(MarkupError::internal)?;
        blocks.push(RqpBlock {
            descriptor: BlockDescriptor {
                id,
                format: raw.format,
                sha256: sha256(content),
            },
            start: raw.content.start,
            end: raw.content.end,
        });
    }
    let template_revision = template_revision(input, &blocks)?;
    Ok(RqpDocument {
        template_revision,
        blocks,
    })
}

/// `"t1." || B64U(SHA-256("rqp-t1\n" || template))`; the template is the
/// input without the content bytes of every block. Blocks must be in order
/// and inside the input; else this is an internal defect.
fn template_revision(input: &[u8], blocks: &[RqpBlock]) -> Result<TemplateRevision, MarkupError> {
    let mut parts: Vec<&[u8]> = Vec::with_capacity(blocks.len().saturating_add(2));
    parts.push(TEMPLATE_TAG);
    let mut kept_from = 0;
    for block in blocks {
        parts.push(
            input
                .get(kept_from..block.start)
                .ok_or_else(MarkupError::internal)?,
        );
        kept_from = block.end;
    }
    parts.push(input.get(kept_from..).ok_or_else(MarkupError::internal)?);
    Ok(TemplateRevision::from_digest(sha256_parts(&parts)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use rivqen_proto::types::BlockFormat;

    use super::*;
    use crate::rqp::tokenizer::Span;

    fn raw(id: (usize, usize), content: (usize, usize)) -> RawBlock {
        RawBlock {
            id: Span {
                start: id.0,
                end: id.1,
            },
            format: BlockFormat::Html,
            content: Span {
                start: content.0,
                end: content.1,
            },
        }
    }

    /// L-3 of the candidate: a block span outside the input, an id that is
    /// not a block id, or blocks out of order, is an internal defect. The
    /// result fails closed; it never hashes missing bytes as empty content.
    #[test]
    fn spans_outside_the_input_fail_closed() {
        let input = b"<p data-rq-block=\"a\">x</p>";
        let good = raw((18, 19), (21, 22));
        let doc = document(input, &[good]).unwrap();
        assert_eq!(doc.blocks[0].descriptor.id.as_str(), "a");
        assert_eq!(doc, analyze(input, &TemplateLimits::default()).unwrap());
        for bad in [
            vec![raw((18, 19), (21, 99))],
            vec![raw((18, 99), (21, 22))],
            vec![raw((18, 19), (23, 22))],
            // The id bytes `"` are not a block id.
            vec![raw((17, 18), (21, 22))],
            // Out of order: the template range of the second block is empty
            // backwards.
            vec![good, raw((18, 19), (2, 3))],
        ] {
            let error = document(input, &bad).unwrap_err();
            assert!(error.is_internal(), "{bad:?}");
            assert_eq!(error.code(), MarkupCode::Structure);
        }
        assert!(!MarkupError::new(MarkupCode::Structure, "M-23").is_internal());
    }

    #[test]
    fn template_revision_known_answer() {
        // SHA-256("rqp-t1\n") with no blocks equals the revision of an empty
        // document.
        let empty = analyze(b"", &TemplateLimits::default()).unwrap();
        assert_eq!(
            empty.template_revision,
            TemplateRevision::from_digest(sha256(b"rqp-t1\n"))
        );
        // The template keeps the tags and drops the content bytes.
        let input = b"<p data-rq-block=\"a\">xyz</p>";
        let doc = analyze(input, &TemplateLimits::default()).unwrap();
        assert_eq!(
            doc.template_revision,
            TemplateRevision::from_digest(sha256(b"rqp-t1\n<p data-rq-block=\"a\"></p>"))
        );
    }
}
