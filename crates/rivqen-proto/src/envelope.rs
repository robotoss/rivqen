// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Patch envelope text to [`crate::types::PatchEnvelope`] (manifest-patch
//! sections 2.2, 2.3 and 2.4 steps without apply). Owner: WP-05 S1 T-04.
//!
//! [`parse_envelope`] runs checks 4 to 6 of diff-engine section 3 (the size
//! and the parse that manifest-patch section 2.4 runs before step 1):
//!
//! 1. Check 4: the body is at most `E = min(maxPatchBytes,
//!    limits.max_patch_bytes)` bytes ([`envelope_limit`]), checked before the
//!    JSON parse. Else `PATCH_TOO_LARGE`.
//! 2. Check 5: the body is a JSON text by manifest-patch section 3.2
//!    ([`crate::json`]). Else `PATCH_INVALID`.
//! 3. Check 6: the value is valid against `spec/rqp/schemas/patch.schema.json`:
//!    exactly `binding` and 1 to 256 operations of exactly `block_id` and
//!    `content`. An unknown member, also a member of the envelope before
//!    H-104 and H-107, fails. Else `PATCH_INVALID`.
//!
//! The check reads the body once, stops at the first failure and keeps only
//! the binding and at most 256 operations.
//!
//! Check 9 (the operations against the snapshot block list) runs next, in a
//! later step; then [`check_content_bytes`] is the size part of check 10.

use core::fmt;

use rivqen_integrity::Binding;
use rivqen_model::error::{ErrorCode, HasErrorCode};

use crate::json::{
    Item, JsonError, Reader, SchemaKeyword, SchemaViolation, next_item, next_member,
};
use crate::types::{
    BlockId, Limits, MAX_BLOCK_BYTES, MAX_BLOCKS, MAX_JSON_DEPTH, PatchEnvelope, PatchOp,
};

/// Schema `maxLength` of `content`, in code points (manifest-patch section
/// 2.3, rule 7).
const MAX_CONTENT_CODE_POINTS: usize = 1_048_576;

/// Why an envelope is rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EnvelopeError {
    /// The body is larger than the envelope limit `E` (`PATCH_TOO_LARGE`).
    TooLarge,
    /// The body breaks a JSON rule of section 3.2 (`PATCH_INVALID`).
    Json(JsonError),
    /// The value fails `patch.schema.json` (`PATCH_INVALID`).
    Schema(SchemaViolation),
    /// The UTF-8 bytes of the `content` of an operation are more than
    /// `maxBlockBytes` (`PATCH_TOO_LARGE`; check 10). The index of the
    /// operation.
    ContentTooLarge(usize),
}

impl EnvelopeError {
    /// A static reason without content.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::TooLarge => "envelope larger than min(maxPatchBytes, max_patch_bytes)",
            Self::Json(_) => "envelope breaks a JSON rule",
            Self::Schema(_) => "envelope fails the schema",
            Self::ContentTooLarge(_) => "operation content larger than maxBlockBytes",
        }
    }
}

impl fmt::Display for EnvelopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code(), self.reason())?;
        match self {
            Self::Json(e) => write!(f, ": {e}"),
            Self::Schema(v) => write!(f, ": {v}"),
            Self::ContentTooLarge(i) => write!(f, " (operation {i})"),
            Self::TooLarge => Ok(()),
        }
    }
}

impl core::error::Error for EnvelopeError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Json(e) => Some(e),
            Self::Schema(v) => Some(v),
            Self::TooLarge | Self::ContentTooLarge(_) => None,
        }
    }
}

impl HasErrorCode for EnvelopeError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::TooLarge | Self::ContentTooLarge(_) => ErrorCode::PatchTooLarge,
            Self::Json(_) | Self::Schema(_) => ErrorCode::PatchInvalid,
        }
    }
}

impl From<JsonError> for EnvelopeError {
    fn from(e: JsonError) -> Self {
        Self::Json(e)
    }
}

impl From<SchemaViolation> for EnvelopeError {
    fn from(v: SchemaViolation) -> Self {
        Self::Schema(v)
    }
}

fn violation(keyword: SchemaKeyword, pointer: &'static str) -> EnvelopeError {
    EnvelopeError::Schema(SchemaViolation::at(keyword, pointer))
}

fn op_violation(keyword: SchemaKeyword, pointer: &'static str, index: usize) -> EnvelopeError {
    EnvelopeError::Schema(SchemaViolation::in_operation(keyword, pointer, index))
}

/// The envelope limit `E = min(maxPatchBytes, max_patch_bytes)` of
/// manifest-patch section 1.5: `max_patch_bytes` is the client limit, `limits`
/// the `limits` member of the snapshot manifest (no hint: the client limit).
#[must_use]
pub fn envelope_limit(max_patch_bytes: usize, limits: Option<Limits>) -> usize {
    match limits.and_then(|l| l.max_patch_bytes) {
        // A hint that does not fit in usize bounds nothing.
        Some(hint) => max_patch_bytes.min(usize::try_from(hint).unwrap_or(usize::MAX)),
        None => max_patch_bytes,
    }
}

/// Checks 4 to 6 of diff-engine section 3: the size limit
/// [`envelope_limit`]`(max_patch_bytes, limits)` before the parse, then the
/// JSON rules and the schema.
///
/// `body` is the envelope after content decoding. `max_patch_bytes` is the
/// client limit `maxPatchBytes`; `limits` is the `limits` member of the
/// snapshot manifest.
///
/// # Errors
///
/// [`EnvelopeError::TooLarge`] before the parse; then the first JSON or schema
/// failure in text order.
pub fn parse_envelope(
    body: &[u8],
    max_patch_bytes: usize,
    limits: Option<Limits>,
) -> Result<PatchEnvelope, EnvelopeError> {
    if body.len() > envelope_limit(max_patch_bytes, limits) {
        return Err(EnvelopeError::TooLarge);
    }
    let mut reader = Reader::new(body, MAX_JSON_DEPTH)?;
    let Item::Object = next_item(&mut reader)? else {
        return Err(violation(SchemaKeyword::Type, ""));
    };
    let mut binding = None;
    let mut operations = None;
    // The reader rejects a second member with the same name (rule 2).
    while let Some(name) = next_member(&mut reader)? {
        match &*name {
            "binding" => binding = Some(read_binding(&mut reader)?),
            "operations" => operations = Some(read_operations(&mut reader)?),
            _ => return Err(violation(SchemaKeyword::AdditionalProperties, "")),
        }
    }
    let (Some(binding), Some(operations)) = (binding, operations) else {
        return Err(violation(SchemaKeyword::Required, ""));
    };
    reader.finish()?;
    Ok(PatchEnvelope {
        binding,
        operations,
    })
}

/// The size part of check 10 (manifest-patch section 2.4, step 4): the UTF-8
/// bytes of each `content` are at most [`MAX_BLOCK_BYTES`]. It runs after the
/// operation checks against the snapshot (check 9).
///
/// # Errors
///
/// [`EnvelopeError::ContentTooLarge`] with the index of the first operation
/// over the limit.
pub fn check_content_bytes(envelope: &PatchEnvelope) -> Result<(), EnvelopeError> {
    match envelope
        .operations
        .iter()
        .position(|op| op.content.len() > MAX_BLOCK_BYTES)
    {
        Some(i) => Err(EnvelopeError::ContentTooLarge(i)),
        None => Ok(()),
    }
}

/// `binding`: `d1.` and a canonical digest (the schema pattern).
fn read_binding(reader: &mut Reader<'_>) -> Result<Binding, EnvelopeError> {
    let Item::String(s) = next_item(reader)? else {
        return Err(violation(SchemaKeyword::Type, "/binding"));
    };
    Binding::parse(&s).map_err(|_| violation(SchemaKeyword::Pattern, "/binding"))
}

/// `operations`: an array of 1 to [`MAX_BLOCKS`] operations.
fn read_operations(reader: &mut Reader<'_>) -> Result<Vec<PatchOp>, EnvelopeError> {
    const AT: &str = "/operations";
    let Item::Array = next_item(reader)? else {
        return Err(violation(SchemaKeyword::Type, AT));
    };
    let mut operations = Vec::new();
    loop {
        let item = next_item(reader)?;
        if let Item::End = item {
            break;
        }
        let index = operations.len();
        if index >= MAX_BLOCKS {
            return Err(violation(SchemaKeyword::MaxItems, AT));
        }
        let Item::Object = item else {
            return Err(op_violation(SchemaKeyword::Type, "/operations/*", index));
        };
        operations.push(read_operation(reader, index)?);
    }
    if operations.is_empty() {
        return Err(violation(SchemaKeyword::MinItems, AT));
    }
    Ok(operations)
}

/// One operation: exactly `block_id` and `content`.
fn read_operation(reader: &mut Reader<'_>, index: usize) -> Result<PatchOp, EnvelopeError> {
    let mut block_id = None;
    let mut content = None;
    while let Some(name) = next_member(reader)? {
        match &*name {
            "block_id" => {
                const AT: &str = "/operations/*/block_id";
                let Item::String(s) = next_item(reader)? else {
                    return Err(op_violation(SchemaKeyword::Type, AT, index));
                };
                let id = BlockId::parse(&s)
                    .map_err(|_| op_violation(SchemaKeyword::Pattern, AT, index))?;
                block_id = Some(id);
            }
            "content" => {
                const AT: &str = "/operations/*/content";
                let Item::String(s) = next_item(reader)? else {
                    return Err(op_violation(SchemaKeyword::Type, AT, index));
                };
                // A string of at most MAX_CONTENT_CODE_POINTS bytes has at most
                // as many code points: count only longer strings.
                if s.len() > MAX_CONTENT_CODE_POINTS && s.chars().count() > MAX_CONTENT_CODE_POINTS
                {
                    return Err(op_violation(SchemaKeyword::MaxLength, AT, index));
                }
                content = Some(s.into_owned());
            }
            _ => {
                return Err(op_violation(
                    SchemaKeyword::AdditionalProperties,
                    "/operations/*",
                    index,
                ));
            }
        }
    }
    let (Some(block_id), Some(content)) = (block_id, content) else {
        return Err(op_violation(
            SchemaKeyword::Required,
            "/operations/*",
            index,
        ));
    };
    Ok(PatchOp { block_id, content })
}
