// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Client checks of a full document (WP-06 S2 contract; H-142).
//!
//! Normative text: docs/engineering/protocol/manifest-patch.md section 1.7
//! (client items 1 to 5) and section 1.8 (items 1, 2, 4 and 5).
//!
//! [`read_full_document`] runs, in this order:
//!
//! 1. [`locate_manifest`](crate::rqp::locate::locate_manifest) on F
//!    (section 1.7, items 1, 3, 4, 5);
//! 2. [`rivqen_proto::manifest::read_manifest`] on M (section 1.8, items 1,
//!    2 and 4);
//! 3. [`split_rqp`](crate::rqp::split_rqp) on O and the page revision of
//!    the result with the `policy` and `limits` members of the manifest; it
//!    MUST be equal to `page_revision` of the manifest (section 1.8, item 5).
//!
//! The caller does section 1.7 item 6 (UTF-8 from the byte order mark or
//! the `Content-Type` field) before it calls this function (WP-08). Every
//! error is `MANIFEST_INVALID`. An error holds no document byte.

use core::fmt;

use rivqen_integrity::PageRevision;
use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_proto::manifest::{ManifestError, read_manifest};
use rivqen_proto::types::PageManifest;

use crate::limits::TemplateLimits;
use crate::rqp::analysis::Internal as _;
use crate::rqp::locate::{LocateError, locate_manifest};
use crate::rqp::{MarkupError, RqpDocument, split_rqp};

/// A full document that passed the client checks. Contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullDocument {
    original: Vec<u8>,
    document: RqpDocument,
    manifest: PageManifest,
}

impl FullDocument {
    /// The original document O (F without the manifest element). The client
    /// stores these bytes as the snapshot.
    #[must_use]
    pub fn original(&self) -> &[u8] {
        &self.original
    }

    /// The markup analysis of O: template revision and blocks with offsets
    /// in O.
    #[must_use]
    pub const fn document(&self) -> &RqpDocument {
        &self.document
    }

    /// The manifest. Its `page_revision` is equal to the page revision of
    /// [`Self::document`] with its `policy` and `limits`.
    #[must_use]
    pub const fn manifest(&self) -> &PageManifest {
        &self.manifest
    }

    /// The page revision of the snapshot (the checked value of the manifest).
    #[must_use]
    pub const fn page_revision(&self) -> &PageRevision {
        &self.manifest.page_revision
    }

    /// O, the analysis and the manifest, by value.
    #[must_use]
    pub fn into_parts(self) -> (Vec<u8>, RqpDocument, PageManifest) {
        (self.original, self.document, self.manifest)
    }
}

/// Why a full document has no valid manifest. The code of each variant is
/// `MANIFEST_INVALID` (section 1.8). Contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FullDocumentError {
    /// Section 1.7, items 1 to 5.
    Locate(LocateError),
    /// Section 1.8, items 1, 2 and 4.
    Manifest(ManifestError),
    /// Section 1.8, item 5: O is not valid RQP markup.
    Markup(MarkupError),
    /// Section 1.8, item 5: the page revision of O is not `page_revision`
    /// of the manifest.
    PageRevisionDiffers,
}

impl FullDocumentError {
    /// A static reason without document bytes.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Locate(error) => error.reason(),
            Self::Manifest(error) => error.reason(),
            Self::Markup(_) => "the original document is not valid RQP markup",
            Self::PageRevisionDiffers => {
                "the page revision of the document differs from the manifest"
            }
        }
    }
}

impl fmt::Display for FullDocumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.reason())
    }
}

impl core::error::Error for FullDocumentError {}

impl HasErrorCode for FullDocumentError {
    fn code(&self) -> ErrorCode {
        ErrorCode::ManifestInvalid
    }
}

/// The client checks of the full document `full` (see the module comment).
/// `final_origin` and `final_path` are the origin and the path of the final
/// response URL, as [`rivqen_proto::manifest::read_manifest`] takes them.
/// Contract.
///
/// # Errors
///
/// [`FullDocumentError`] for the first failed check.
pub fn read_full_document(
    full: &[u8],
    final_origin: &str,
    final_path: &str,
    limits: &TemplateLimits,
) -> Result<FullDocument, FullDocumentError> {
    // Section 1.7, items 1, 3, 4 and 5.
    let location = locate_manifest(full, limits).map_err(FullDocumentError::Locate)?;
    let text = location.manifest_text(full).ok_or_else(internal)?;
    // Section 1.8, items 1, 2 and 4.
    let manifest =
        read_manifest(text, final_origin, final_path).map_err(FullDocumentError::Manifest)?;
    // Section 1.7, item 5: the only copy of O. Section 1.7, item 2: a second
    // manifest element is a `script` start tag of O that M-13 rejects
    // (DL-172).
    let original = location.original(full).ok_or_else(internal)?;
    // Section 1.8, item 5.
    let document = split_rqp(&original, limits).map_err(FullDocumentError::Markup)?;
    let computed = document.page_revision(manifest.policy.as_ref(), manifest.limits.as_ref());
    if computed != manifest.page_revision {
        return Err(FullDocumentError::PageRevisionDiffers);
    }
    Ok(FullDocument {
        original,
        document,
        manifest,
    })
}

/// An internal defect of this crate (a location outside `full`): fail
/// closed.
fn internal() -> FullDocumentError {
    FullDocumentError::Locate(LocateError::Markup(MarkupError::internal()))
}
