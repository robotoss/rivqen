// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Manifest locator and head walk (WP-06 S2 contract; H-142).
//!
//! Normative text: docs/engineering/protocol/manifest-patch.md section 1.7,
//! client items 1 to 5, and "Document head" (H-72, H-79a).
//!
//! [`locate_manifest`] finds the manifest element of a full document F: the
//! first `script` start tag of the [`scan`](crate::rqp::scan) whose first
//! `type` attribute has the raw value `application/rivqen-manifest+json`
//! (ASCII case-insensitive, exactly 32 bytes, item 1). The manifest text M
//! is the bytes from the byte after its `>` to the `<` of the next
//! `</script` (item 4). The original document O is F without the bytes of
//! the element (item 5). The `<` of the start tag MUST be at the insertion
//! point P of the head walk on O (item 3, client rule C2); a walk on F is not
//! sufficient.
//!
//! The locator does not check that F has only one manifest element (item
//! 2): a second one is in O, and the markup analysis of O rejects it (M-13,
//! [`crate::rqp::full`]). The locator reads F only up to the first match, so
//! a document whose first match is not at P fails without a full scan.
//!
//! Every error is `MANIFEST_INVALID`. An error holds no document byte.

use core::fmt;
use core::ops::Range;

use rivqen_model::error::{ErrorCode, HasErrorCode};

use crate::limits::TemplateLimits;
use crate::rqp::{MarkupCode, MarkupError};

/// The place of the manifest element in a full document. Contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestLocation {
    element: Range<usize>,
    text: Range<usize>,
}

impl ManifestLocation {
    /// The bytes of the element: from the `<` of the start tag to the byte
    /// after the `>` of the end tag.
    #[must_use]
    pub fn element(&self) -> Range<usize> {
        self.element.clone()
    }

    /// The bytes of the manifest text M (item 4).
    #[must_use]
    pub fn text(&self) -> Range<usize> {
        self.text.clone()
    }

    /// M in `full`; `None` when `full` is not the document of this location.
    #[must_use]
    pub fn manifest_text<'a>(&self, full: &'a [u8]) -> Option<&'a [u8]> {
        full.get(self.text.clone())
    }

    /// The original document O: `full` without the bytes of the element
    /// (item 5); `None` when `full` is not the document of this location.
    #[must_use]
    pub fn original(&self, full: &[u8]) -> Option<Vec<u8>> {
        let before = full.get(..self.element.start)?;
        let after = full.get(self.element.end..)?;
        let mut out = Vec::with_capacity(before.len().saturating_add(after.len()));
        out.extend_from_slice(before);
        out.extend_from_slice(after);
        Some(out)
    }
}

/// Why a full document has no valid manifest element (section 1.7). The
/// code of each variant is `MANIFEST_INVALID`. Contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LocateError {
    /// No manifest start tag in the document (item 2).
    NoManifest,
    /// No `</script` after the manifest start tag (item 4).
    Unterminated,
    /// O has no head start tag of the document and no implied head (item 3,
    /// "Document head" item 1).
    NoHead,
    /// The manifest start tag is not at the insertion point P of O (item 3).
    WrongPosition,
    /// The scan stopped before it found the manifest start tag (M-01, or
    /// the token stack limit).
    Markup(MarkupError),
}

impl LocateError {
    /// A static reason without document bytes.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::NoManifest => "the document has no manifest element",
            Self::Unterminated => "the manifest element has no end tag",
            Self::NoHead => "the document has no head start tag and no implied head",
            Self::WrongPosition => "the manifest element is not at the insertion point",
            Self::Markup(_) => "the scan of the document stopped",
        }
    }
}

impl fmt::Display for LocateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.reason())
    }
}

impl core::error::Error for LocateError {}

impl HasErrorCode for LocateError {
    fn code(&self) -> ErrorCode {
        ErrorCode::ManifestInvalid
    }
}

/// Find the manifest element of the full document `full` (section 1.7,
/// client items 1, 3, 4 and 5). Contract.
///
/// # Errors
///
/// [`LocateError`] for the first failed item.
pub fn locate_manifest(
    full: &[u8],
    limits: &TemplateLimits,
) -> Result<ManifestLocation, LocateError> {
    let _ = (full, limits);
    Err(LocateError::NoManifest)
}

/// The insertion point P of the head walk ("Document head", items 1 and 2)
/// on `document`: the byte after the `>` of the head start tag of the
/// document, or the `<` of the implied-head start tag. `Ok(None)` when the
/// document has neither. The server inserts the manifest at P; the client
/// computes P on O. Contract.
///
/// # Errors
///
/// [`MarkupError`] when the scan stops before the walk ends (M-01).
pub fn insertion_point(
    document: &[u8],
    limits: &TemplateLimits,
) -> Result<Option<usize>, MarkupError> {
    let _ = (document, limits);
    Err(MarkupError::new(MarkupCode::Structure, "stub"))
}
