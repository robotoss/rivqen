// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Legacy data diff and checked rebuild (legacy client CL-27, CL-28, CL-29,
//! CL-42; DL-160, DL-169 S1-5; diff engine section 2).
//!
//! Owner: WP-08 S1, T-02.
//!
//! - [`diff_v1`] gives the entries of a data update that the page must
//!   receive: every server entry whose value differs from the stored value or
//!   that the stored data does not have, in server order (CL-42, D-25).
//! - [`Rebuilt`] is a rebuilt page that exists only after its check: the
//!   stored template with the server data ([`render_v1`]), and its SHA-1
//!   equal to the `html-sha1` member of the body **and** to the bare response
//!   etag (CL-29). Its only constructor is [`Rebuilt::new`].
//!
//! # Invariants
//!
//! 1. No [`Rebuilt`] without the check (R-CORE-05): the fields are private,
//!    and [`Rebuilt::new`] is the only constructor. So
//!    `etag_v1(page) == html_sha1 == etag` for every value.
//! 2. On every error, no page leaves this module: the rebuilt text is
//!    dropped with the error.
//! 3. [`diff_v1`] gives a subsequence of the server list, in server order.
//!    Applied to the stored data (replace or add each entry by key), it gives
//!    the server value for every server key.
//! 4. A key repeats in neither list. A repeat is an error, never a choice of
//!    one of the values.
//!
//! # Bounds
//!
//! [`diff_v1`] takes `O(n log n)` key comparisons for `n` entries in total
//! and allocates one map of references and the result list (references).
//! [`Rebuilt::new`] first calls [`render_v1`], which applies the limits of
//! [`TemplateLimits`] (template and page at most `max_document_bytes`, at most
//! `max_blocks` blocks plus `{title}`, each block value at most
//! `max_block_bytes`). Every value that it accepts has a placeholder in the
//! page, so the copy of the diff entries is at most the page size. Then one
//! SHA-1 pass over the page and one diff.
//!
//! # The error token `value-not-string`
//!
//! CL-42 gives the token `value-not-string` when a value of the stored or of
//! the server data is not a JSON string. A [`DataBlock`] value is always a
//! string, so that case cannot reach [`diff_v1`]: the reader of the response
//! body rejects such a body (CL-29), and the stored data comes from the
//! split or from a [`Rebuilt`].

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use rivqen_integrity::legacy::Sha1Hex;
use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_template::legacy::{DataBlock, RenderError, etag_v1, render_v1};
use rivqen_template::limits::TemplateLimits;

/// Why [`diff_v1`] gives no diff. The message has no key and no value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DiffError {
    /// A key repeats in the stored data. The stored entry is damaged.
    /// Code `CACHE_CORRUPTED`.
    DuplicateLocalKey,
    /// A key repeats in the server data. Code `INTEGRITY_ERROR`.
    DuplicateServerKey,
}

impl DiffError {
    /// A static reason, safe to log.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::DuplicateLocalKey => "a key repeats in the stored legacy data",
            Self::DuplicateServerKey => "a key repeats in the legacy server data",
        }
    }
}

impl fmt::Display for DiffError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.reason())
    }
}

impl core::error::Error for DiffError {}

impl HasErrorCode for DiffError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::DuplicateLocalKey => ErrorCode::CacheCorrupted,
            Self::DuplicateServerKey => ErrorCode::IntegrityError,
        }
    }
}

/// The diff of a legacy data update (CL-42, D-25).
///
/// The result is every entry of `server` whose key `local` does not have, or
/// whose value differs byte for byte from the value of the same key in
/// `local`, in the order of `server`. `local` `None` gives the full server
/// data. Keys compare byte for byte (case-sensitive). An empty result is the
/// empty diff of CL-28: the session sends no message.
///
/// # Errors
///
/// [`DiffError`] when a key repeats in `local` or in `server`.
pub fn diff_v1<'s>(
    local: Option<&[DataBlock]>,
    server: &'s [DataBlock],
) -> Result<Vec<&'s DataBlock>, DiffError> {
    let mut stored: BTreeMap<&str, &str> = BTreeMap::new();
    for entry in local.unwrap_or_default() {
        if stored
            .insert(entry.key.as_str(), entry.value.as_str())
            .is_some()
        {
            return Err(DiffError::DuplicateLocalKey);
        }
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut diff = Vec::new();
    for entry in server {
        if !seen.insert(entry.key.as_str()) {
            return Err(DiffError::DuplicateServerKey);
        }
        if stored.get(entry.key.as_str()).copied() != Some(entry.value.as_str()) {
            diff.push(entry);
        }
    }
    Ok(diff)
}

/// The inputs of [`Rebuilt::new`]: the stored entry and the checked parts of
/// a legacy data response.
pub struct RebuildInput<'a> {
    /// The stored template (CL-38 record).
    pub template: &'a str,
    /// The stored data, in stored order (CL-38 record).
    pub stored: &'a [DataBlock],
    /// The `data` member of the response body, in body order. It becomes the
    /// data of the new entry (D-35).
    pub server: Vec<DataBlock>,
    /// The `html-sha1` member of the response body. An absent member is the
    /// empty text: it never matches.
    pub html_sha1: &'a str,
    /// The bare etag of the response (the value of the etag field without
    /// `W/` and quotes, CL-15). An absent etag is the empty text: it never
    /// matches.
    pub etag: &'a str,
}

impl fmt::Debug for RebuildInput<'_> {
    /// Lengths only: the template and the data are page content.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RebuildInput")
            .field("template_bytes", &self.template.len())
            .field("stored_entries", &self.stored.len())
            .field("server_entries", &self.server.len())
            .field("html_sha1_bytes", &self.html_sha1.len())
            .field("etag_bytes", &self.etag.len())
            .finish()
    }
}

/// Why [`Rebuilt::new`] gives no page. The session discards the update and
/// loads the page normally; the stored entry is removed (CL-29). The message
/// has no page byte, key, value or hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RebuildError {
    /// [`render_v1`] failed: the data does not fit the template, or a limit.
    Render(RenderError),
    /// The SHA-1 of the rebuilt page is not the `html-sha1` member.
    HtmlSha1Mismatch,
    /// The SHA-1 of the rebuilt page is not the response etag.
    EtagMismatch,
    /// [`diff_v1`] failed.
    Diff(DiffError),
}

impl fmt::Display for RebuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Render(error) => write!(f, "the legacy rebuild failed: {error}"),
            Self::HtmlSha1Mismatch => {
                f.write_str("the SHA-1 of the rebuilt legacy page is not html-sha1")
            }
            Self::EtagMismatch => {
                f.write_str("the SHA-1 of the rebuilt legacy page is not the response etag")
            }
            Self::Diff(error) => write!(f, "the legacy diff failed: {error}"),
        }
    }
}

impl core::error::Error for RebuildError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Render(error) => Some(error),
            Self::Diff(error) => Some(error),
            Self::HtmlSha1Mismatch | Self::EtagMismatch => None,
        }
    }
}

impl HasErrorCode for RebuildError {
    /// `DOCUMENT_TOO_LARGE` for a limit, `CACHE_CORRUPTED` for a damaged
    /// stored entry, else `INTEGRITY_ERROR`: the rebuilt page is not the page
    /// of the server.
    fn code(&self) -> ErrorCode {
        match self {
            Self::Render(RenderError::Limit(limit)) => limit.code(),
            Self::Diff(error) => error.code(),
            Self::Render(_) | Self::HtmlSha1Mismatch | Self::EtagMismatch => {
                ErrorCode::IntegrityError
            }
        }
    }
}

/// A rebuilt legacy page that passed the check of CL-29.
///
/// It holds the page, its SHA-1 (equal to `html-sha1` and to the response
/// etag), the full server data and the diff against the stored data.
#[derive(Clone, PartialEq, Eq)]
pub struct Rebuilt {
    page: String,
    etag: Sha1Hex,
    data: Vec<DataBlock>,
    diff: Vec<DataBlock>,
}

impl Rebuilt {
    /// Rebuild the page of a legacy data update and check it (CL-27, CL-29).
    ///
    /// Steps, in this order; the first failure stops:
    ///
    /// 1. [`render_v1`] of the stored template with the server data, within
    ///    `limits`.
    /// 2. The lowercase hex SHA-1 of the page ([`etag_v1`]) equals
    ///    `html_sha1`, byte for byte.
    /// 3. It equals `etag`, byte for byte.
    /// 4. [`diff_v1`] of the stored data and the server data.
    ///
    /// The comparisons are plain: the values are public hashes of the page.
    /// The caller checks the other CL-29 conditions before: the response
    /// `template-tag` equals the stored template tag, and the body is a JSON
    /// object whose `data` member is an object of strings.
    ///
    /// # Errors
    ///
    /// [`RebuildError`] at the first failed step. No page is given.
    pub fn new(input: RebuildInput<'_>, limits: &TemplateLimits) -> Result<Self, RebuildError> {
        let RebuildInput {
            template,
            stored,
            server,
            html_sha1,
            etag,
        } = input;
        let page = render_v1(template, &server, limits).map_err(RebuildError::Render)?;
        let sha1 = etag_v1(page.as_bytes());
        if sha1.as_str() != html_sha1 {
            return Err(RebuildError::HtmlSha1Mismatch);
        }
        if sha1.as_str() != etag {
            return Err(RebuildError::EtagMismatch);
        }
        let diff = diff_v1(Some(stored), &server)
            .map_err(RebuildError::Diff)?
            .into_iter()
            .cloned()
            .collect();
        Ok(Self {
            page,
            etag: sha1,
            data: server,
            diff,
        })
    }

    /// The rebuilt page.
    #[must_use]
    pub fn page(&self) -> &str {
        &self.page
    }

    /// The SHA-1 of the page: the etag of the new entry.
    #[must_use]
    pub fn etag(&self) -> Sha1Hex {
        self.etag
    }

    /// The full server data, in body order: the data of the new entry (D-35).
    #[must_use]
    pub fn data(&self) -> &[DataBlock] {
        &self.data
    }

    /// The diff against the stored data ([`diff_v1`]), in server order.
    /// Empty: no message (CL-28).
    #[must_use]
    pub fn diff(&self) -> &[DataBlock] {
        &self.diff
    }

    /// The parts, for the commit of the new entry.
    #[must_use]
    pub fn into_parts(self) -> RebuiltParts {
        RebuiltParts {
            page: self.page,
            etag: self.etag,
            data: self.data,
            diff: self.diff,
        }
    }
}

impl fmt::Debug for Rebuilt {
    /// Lengths only: the page and the data are page content.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Rebuilt")
            .field("page_bytes", &self.page.len())
            .field("data_entries", &self.data.len())
            .field("diff_entries", &self.diff.len())
            .finish_non_exhaustive()
    }
}

/// The parts of a checked [`Rebuilt`]. Only [`Rebuilt::into_parts`] makes
/// one.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RebuiltParts {
    /// The rebuilt page.
    pub page: String,
    /// The SHA-1 of the page.
    pub etag: Sha1Hex,
    /// The full server data, in body order.
    pub data: Vec<DataBlock>,
    /// The diff, in server order.
    pub diff: Vec<DataBlock>,
}

impl fmt::Debug for RebuiltParts {
    /// Lengths only: the page and the data are page content.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RebuiltParts")
            .field("page_bytes", &self.page.len())
            .field("data_entries", &self.data.len())
            .field("diff_entries", &self.diff.len())
            .finish_non_exhaustive()
    }
}
