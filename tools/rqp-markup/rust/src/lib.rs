// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! RQP markup parser candidate (`rqp/1`).
//!
//! [`analyze`] decides whether a document is valid for RQP and finds its
//! data blocks, by the rules M-01…M-27 of `docs/engineering/protocol/markup.md`
//! section 2. [`result_json`] writes the result object of
//! `tools/rqp-markup/CONTRACT.md` section 3.
//!
//! Validity is decided on tokens only (DL-008). The crate has its own
//! tokenizer for the WHATWG states that the rules need (DL-012).
//!
//! Resource limits: the input size is checked first (M-01); memory is
//! linear in the input size; the work is linear in the input size.

#![forbid(unsafe_code)]

mod json;
mod names;
mod rules;
mod tokenizer;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

pub use json::{MAX_DEPTH as MAX_JSON_DEPTH, is_json_text};

/// Maximum input size in bytes (M-01): 5 MiB.
pub const MAX_INPUT_BYTES: usize = 5_242_880;
/// Maximum number of blocks in one document (M-26).
pub const MAX_BLOCKS: usize = 256;
/// Maximum content size of one block in bytes (M-26): 1 MiB.
pub const MAX_BLOCK_BYTES: usize = 1_048_576;

const BOM: &[u8] = b"\xEF\xBB\xBF";

/// Error codes of `CONTRACT.md` section 5.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    /// `RQP_MARKUP_ENCODING`
    Encoding,
    /// `RQP_MARKUP_LIMIT`
    Limit,
    /// `RQP_MARKUP_INVALID_ID`
    InvalidId,
    /// `RQP_MARKUP_FORBIDDEN_ELEMENT`
    ForbiddenElement,
    /// `RQP_MARKUP_DUPLICATE`
    Duplicate,
    /// `RQP_MARKUP_NESTED`
    Nested,
    /// `RQP_MARKUP_STRUCTURE`
    Structure,
    /// `RQP_MARKUP_BAD_JSON`
    BadJson,
    /// `RQP_MARKUP_RESERVED`
    Reserved,
}

impl ErrorCode {
    /// The wire code, for example `RQP_MARKUP_DUPLICATE`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Encoding => "RQP_MARKUP_ENCODING",
            Self::Limit => "RQP_MARKUP_LIMIT",
            Self::InvalidId => "RQP_MARKUP_INVALID_ID",
            Self::ForbiddenElement => "RQP_MARKUP_FORBIDDEN_ELEMENT",
            Self::Duplicate => "RQP_MARKUP_DUPLICATE",
            Self::Nested => "RQP_MARKUP_NESTED",
            Self::Structure => "RQP_MARKUP_STRUCTURE",
            Self::BadJson => "RQP_MARKUP_BAD_JSON",
            Self::Reserved => "RQP_MARKUP_RESERVED",
        }
    }
}

/// Why a document is invalid for RQP: the error code and the rule
/// (`M-xx`) that failed first. It never contains document content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarkupError {
    /// The code of `CONTRACT.md` section 5.
    pub code: ErrorCode,
    /// The rule ID of `markup.md`, for example `"M-23"`.
    pub rule: &'static str,
}

impl MarkupError {
    pub(crate) const fn new(code: ErrorCode, rule: &'static str) -> Self {
        Self { code, rule }
    }
}

impl std::fmt::Display for MarkupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.code.as_str(), self.rule)
    }
}

impl std::error::Error for MarkupError {}

/// Block format (M-16).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// `html`
    Html,
    /// `json`
    Json,
}

impl Format {
    /// The wire name: `html` or `json`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Json => "json",
        }
    }
}

/// One block of a valid document (M-27).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// The block id (raw `data-rq-block` value; it matches M-15).
    pub id: String,
    /// The block format.
    pub format: Format,
    /// Offset of the first content byte (after the start tag).
    pub start: usize,
    /// Offset of the `<` of the block end tag.
    pub end: usize,
    /// `B64U(SHA-256(content))`.
    pub sha256: String,
}

/// The result for a valid document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    /// Blocks in document order.
    pub blocks: Vec<Block>,
    /// `t1.` revision of the template (`CONTRACT.md` section 4).
    pub template_revision: String,
    /// `r1.` revision of the page (`CONTRACT.md` section 4).
    pub page_revision: String,
}

/// Checks `input` by the rules of `markup.md` section 2 and returns its
/// blocks and revisions, or the first rule that fails.
///
/// # Errors
///
/// Returns a [`MarkupError`] when the document is invalid for RQP.
pub fn analyze(input: &[u8]) -> Result<Document, MarkupError> {
    // M-01 first, before any work that grows with the input.
    if input.len() > MAX_INPUT_BYTES {
        return Err(MarkupError::new(ErrorCode::Limit, "M-01"));
    }
    // M-02: RFC 3629 (no overlong forms, no surrogates, nothing above
    // U+10FFFF, no truncated sequence).
    if std::str::from_utf8(input).is_err() {
        return Err(MarkupError::new(ErrorCode::Encoding, "M-02"));
    }
    // The tokenizer does not see a byte order mark; offsets keep it.
    let start = if input.starts_with(BOM) { BOM.len() } else { 0 };
    let found = rules::run(input, start)?;

    let mut blocks = Vec::with_capacity(found.len());
    for raw in &found {
        let id = input.get(raw.id.start..raw.id.end).unwrap_or_default();
        let content = input
            .get(raw.content.start..raw.content.end)
            .unwrap_or_default();
        blocks.push(Block {
            id: String::from_utf8_lossy(id).into_owned(),
            format: raw.format,
            start: raw.content.start,
            end: raw.content.end,
            sha256: b64u_sha256(&[content]),
        });
    }
    let template_revision = template_revision(input, &blocks);
    let page_revision = page_revision(&template_revision, &blocks);
    Ok(Document {
        blocks,
        template_revision,
        page_revision,
    })
}

/// `B64U(SHA-256(parts[0] || parts[1] || ...))`.
fn b64u_sha256(parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    URL_SAFE_NO_PAD.encode(hasher.finalize())
}

/// `"t1." || B64U(SHA-256("rqp-t1\n" || template))`; the template is the
/// input without the content bytes of every block.
fn template_revision(input: &[u8], blocks: &[Block]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"rqp-t1\n");
    let mut kept_from = 0;
    for block in blocks {
        hasher.update(input.get(kept_from..block.start).unwrap_or_default());
        kept_from = block.end;
    }
    hasher.update(input.get(kept_from..).unwrap_or_default());
    format!("t1.{}", URL_SAFE_NO_PAD.encode(hasher.finalize()))
}

/// `"r1." || B64U(SHA-256(P))` with the preimage `P` of `CONTRACT.md`
/// section 4.
fn page_revision(template_revision: &str, blocks: &[Block]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"rqp-r1\n");
    hasher.update(template_revision.as_bytes());
    hasher.update(b"\n");
    for block in blocks {
        hasher.update(block.id.as_bytes());
        hasher.update(b"\t");
        hasher.update(block.format.as_str().as_bytes());
        hasher.update(b"\t");
        hasher.update(block.sha256.as_bytes());
        hasher.update(b"\n");
    }
    format!("r1.{}", URL_SAFE_NO_PAD.encode(hasher.finalize()))
}

/// Appends `text` as a JSON string.
fn push_json_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                out.push_str("\\u00");
                out.extend(
                    [u32::from(c) >> 4, u32::from(c) & 0xF]
                        .iter()
                        .filter_map(|&digit| char::from_digit(digit, 16)),
                );
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// The result object of `CONTRACT.md` section 3 as one JSON line (without
/// the line feed). With `fixture`, the object has the batch field
/// `"fixture"`.
#[must_use]
pub fn result_json(result: &Result<Document, MarkupError>, fixture: Option<&str>) -> String {
    let mut out = String::from("{");
    if let Some(name) = fixture {
        out.push_str("\"fixture\":");
        push_json_string(&mut out, name);
        out.push(',');
    }
    match result {
        Ok(doc) => {
            out.push_str("\"valid\":true,\"error\":null,\"blocks\":[");
            for (index, block) in doc.blocks.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str("{\"id\":");
                push_json_string(&mut out, &block.id);
                out.push_str(",\"format\":\"");
                out.push_str(block.format.as_str());
                out.push_str("\",\"start\":");
                out.push_str(&block.start.to_string());
                out.push_str(",\"end\":");
                out.push_str(&block.end.to_string());
                out.push_str(",\"sha256\":\"");
                out.push_str(&block.sha256);
                out.push_str("\"}");
            }
            out.push_str("],\"template_revision\":");
            push_json_string(&mut out, &doc.template_revision);
            out.push_str(",\"page_revision\":");
            push_json_string(&mut out, &doc.page_revision);
        }
        Err(error) => {
            out.push_str("\"valid\":false,\"error\":\"");
            out.push_str(error.code.as_str());
            out.push_str("\",\"blocks\":[],\"template_revision\":null,\"page_revision\":null");
        }
    }
    out.push('}');
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn known_answer_hashes() {
        // SHA-256("") and SHA-256("abc") in base64url without padding.
        assert_eq!(
            b64u_sha256(&[]),
            "47DEQpj8HBSa-_TImW-5JCeuQeRkm5NMpJWZG3hSuFU"
        );
        assert_eq!(
            b64u_sha256(&[b"a", b"bc"]),
            "ungWv48Bz-pBQUDeXa4iI7ADYaOWF3qctBD_YfIAFa0"
        );
    }

    #[test]
    fn json_string_escapes() {
        let mut out = String::new();
        push_json_string(&mut out, "a\"b\\c\n\r\t\u{1}\u{1f}\u{20}\u{e9}");
        assert_eq!(out, "\"a\\\"b\\\\c\\n\\r\\t\\u0001\\u001f \u{e9}\"");
    }

    #[test]
    fn error_display_names_code_and_rule() {
        let error = MarkupError::new(ErrorCode::Nested, "M-17");
        assert_eq!(error.to_string(), "RQP_MARKUP_NESTED (M-17)");
    }
}
