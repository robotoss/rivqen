// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Shared RQP types (contract of WP-05 S1; data-model section 4.2).
//!
//! The lanes of S1 use these types and do not change them; a change goes
//! through the architect.

use core::fmt;

use rivqen_integrity::{Binding, PageRevision, Sha256Digest, TemplateRevision};

/// `maxManifestBytes` (resource-limits section 1; H-50).
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;
/// Default `maxPatchBytes` of the client (resource-limits section 1).
pub const MAX_PATCH_BYTES: usize = 256 * 1024;
/// `maxJsonDepth` (resource-limits section 1).
pub const MAX_JSON_DEPTH: usize = 16;
/// `maxBlocks` (resource-limits section 1).
pub const MAX_BLOCKS: usize = 256;
/// `maxBlockBytes`: one block content in UTF-8 bytes (resource-limits section 1).
pub const MAX_BLOCK_BYTES: usize = 1024 * 1024;
/// `maxRqHaveBytes`, fixed by `rqp/1` (H-44).
pub const MAX_RQ_HAVE_BYTES: usize = 4096;
/// `maxRqCapabilitiesBytes`, fixed by `rqp/1`.
pub const MAX_RQ_CAPABILITIES_BYTES: usize = 1024;
/// `maxRqNonceBytes`, fixed by `rqp/1` (DL-063).
pub const MAX_RQ_NONCE_BYTES: usize = 128;
/// Length of the nonce N in bytes (H-108).
pub const NONCE_BYTES: usize = 16;
/// Maximum length of a block id (markup M-15).
pub const MAX_BLOCK_ID_BYTES: usize = 64;

/// A block id: `^[a-z0-9][a-z0-9_-]{0,63}$` (markup M-15).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(String);

impl BlockId {
    /// Check and wrap a block id.
    ///
    /// # Errors
    ///
    /// [`InvalidBlockId`] when the text does not match M-15.
    pub fn parse(text: &str) -> Result<Self, InvalidBlockId> {
        let bytes = text.as_bytes();
        let Some((first, rest)) = bytes.split_first() else {
            return Err(InvalidBlockId);
        };
        let lower_digit = |b: &u8| b.is_ascii_lowercase() || b.is_ascii_digit();
        if bytes.len() > MAX_BLOCK_ID_BYTES
            || !lower_digit(first)
            || !rest
                .iter()
                .all(|b| lower_digit(b) || *b == b'_' || *b == b'-')
        {
            return Err(InvalidBlockId);
        }
        Ok(Self(text.to_owned()))
    }

    /// The id text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BlockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A text that is not a block id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidBlockId;

impl fmt::Display for InvalidBlockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not a block id (markup M-15)")
    }
}

impl core::error::Error for InvalidBlockId {}

/// Block format (markup M-16; `json` covers both JSON types, H-86).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockFormat {
    /// `html`
    Html,
    /// `json`
    Json,
}

impl BlockFormat {
    /// `"html"` or `"json"`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Json => "json",
        }
    }
}

/// A block as the client computes it (data-model `BlockDescriptor`; H-103).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockDescriptor {
    /// Block id.
    pub id: BlockId,
    /// Block format.
    pub format: BlockFormat,
    /// `SHA-256` of the exact content bytes.
    pub sha256: Sha256Digest,
}

/// Manifest `policy.mode` (manifest-patch section 1.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    /// `"private"` (the default).
    Private,
    /// `"public"`.
    Public,
}

/// Manifest member `policy`. Each field is `Some` only when the member is
/// present: presence changes the page revision (section 1.3, policy line).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Policy {
    /// `mode`
    pub mode: Option<Mode>,
    /// `ttl_seconds`, 0 to 31 536 000.
    pub ttl_seconds: Option<u32>,
    /// `offline`
    pub offline: Option<bool>,
}

/// Manifest member `limits` (manifest-patch section 1.5). `Some` only when the
/// member is present.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Limits {
    /// `max_patch_bytes`, 1 024 to 5 242 880.
    pub max_patch_bytes: Option<u32>,
}

/// What the client keeps of a stored RQP page for protocol work: the values
/// it computed and checked (manifest-patch section 1.2, "Computed values").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotMeta {
    /// Template revision `t1.`.
    pub template_revision: TemplateRevision,
    /// Page revision `r1.` of the stored document.
    pub page_revision: PageRevision,
    /// Blocks in document order, 0 to [`MAX_BLOCKS`].
    pub blocks: Vec<BlockDescriptor>,
    /// `policy` of the manifest, `None` when the member was absent.
    pub policy: Option<Policy>,
    /// `limits` of the manifest, `None` when the member was absent.
    pub limits: Option<Limits>,
}

/// The nonce N of one request (`Rq-Nonce`, H-108). Never logged (RQP-17):
/// [`fmt::Debug`] writes no byte of it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Nonce(pub [u8; NONCE_BYTES]);

impl fmt::Debug for Nonce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Nonce(<redacted>)")
    }
}

/// A checked page manifest (manifest-patch section 1.2). `protocol` is always
/// `"rqp/1"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageManifest {
    /// ASCII serialization of the origin.
    pub origin: String,
    /// Path of the URL, no query.
    pub path: String,
    /// Page revision.
    pub page_revision: PageRevision,
    /// `policy`, `None` when absent.
    pub policy: Option<Policy>,
    /// `limits`, `None` when absent.
    pub limits: Option<Limits>,
}

/// One operation: replace the content of a block (H-47).
#[derive(Clone, PartialEq, Eq)]
pub struct PatchOp {
    /// Block id.
    pub block_id: BlockId,
    /// New content: page content, never logged.
    pub content: String,
}

impl fmt::Debug for PatchOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PatchOp({}, content len={})",
            self.block_id,
            self.content.len()
        )
    }
}

/// A patch envelope after the JSON and schema checks (manifest-patch section
/// 2.2): no check against a snapshot yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchEnvelope {
    /// `binding`
    pub binding: Binding,
    /// 1 to [`MAX_BLOCKS`] operations, in envelope order.
    pub operations: Vec<PatchOp>,
}
