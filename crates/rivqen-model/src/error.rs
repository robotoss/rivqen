// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The error catalog (docs/engineering/core/errors.md sections 1 to 3).
//!
//! Contract of WP-05 S1: the codes, their category and their fallback hint.
//! A code never changes its meaning (ERR-03). The reserved codes
//! `PATCH_BASE_MISMATCH` and `TEMPLATE_MISMATCH` (not produced since H-104),
//! the notice `LEGACY_MODE_USED` and the server code `SERVER_PASSTHROUGH` are
//! not in this enum.

use core::fmt;

/// Error category (errors.md section 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Category {
    /// Connection, DNS, timeout, reset.
    Network,
    /// Malformed header, body, marker or JSON.
    Protocol,
    /// Integrity or schema check failed on a cache entry.
    CacheCorruption,
    /// Partition changed during the session.
    AuthChanged,
    /// The platform or the server lacks a needed feature.
    UnsupportedCapability,
    /// The user or app cancelled.
    Cancelled,
    /// Origin, partition, TLS or content-policy check failed.
    SecurityViolation,
    /// A deadline passed.
    Timeout,
    /// The OS refused an operation.
    PlatformConstraint,
    /// A size, memory or time budget was exceeded.
    ResourceLimit,
    /// A bug: broken invariant or caught panic.
    Internal,
}

/// What the adapter does after an error (errors.md section 3, rule 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FallbackHint {
    /// No action.
    None,
    /// Retry later.
    RetryLater,
    /// Load the page normally over HTTPS.
    NormalLoad,
    /// Show the cached page if the policy allows it.
    ShowCachedIfAllowed,
    /// Stop; show nothing from Rivqen.
    Stop,
}

/// A stable error code (errors.md section 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorCode {
    /// `PROTOCOL_UNSUPPORTED`
    ProtocolUnsupported,
    /// `MANIFEST_INVALID`
    ManifestInvalid,
    /// `PATCH_INVALID`
    PatchInvalid,
    /// `INTEGRITY_ERROR`
    IntegrityError,
    /// `PATCH_CONTENT_REJECTED`
    PatchContentRejected,
    /// `RQP_MARKUP_LIMIT` (resource-limits.md, `maxOpenElements`)
    RqpMarkupLimit,
    /// `CACHE_CORRUPTED`
    CacheCorrupted,
    /// `AUTH_PARTITION_CHANGED`
    AuthPartitionChanged,
    /// `TLS_ERROR`
    TlsError,
    /// `ORIGIN_NOT_ALLOWED`
    OriginNotAllowed,
    /// `WEBVIEW_CAPABILITY_MISSING`
    WebviewCapabilityMissing,
    /// `TRANSPORT_DISCONNECTED`
    TransportDisconnected,
    /// `MEMORY_BUDGET_EXCEEDED`
    MemoryBudgetExceeded,
    /// `PATCH_TOO_LARGE`
    PatchTooLarge,
    /// `DOCUMENT_TOO_LARGE`
    DocumentTooLarge,
    /// `TIMEOUT_FIRST_BYTE`
    TimeoutFirstByte,
    /// `TIMEOUT_TOTAL`
    TimeoutTotal,
    /// `INTERNAL_ERROR`
    InternalError,
}

impl ErrorCode {
    /// Every code, in catalog order.
    pub const ALL: [ErrorCode; 18] = [
        Self::ProtocolUnsupported,
        Self::ManifestInvalid,
        Self::PatchInvalid,
        Self::IntegrityError,
        Self::PatchContentRejected,
        Self::RqpMarkupLimit,
        Self::CacheCorrupted,
        Self::AuthPartitionChanged,
        Self::TlsError,
        Self::OriginNotAllowed,
        Self::WebviewCapabilityMissing,
        Self::TransportDisconnected,
        Self::MemoryBudgetExceeded,
        Self::PatchTooLarge,
        Self::DocumentTooLarge,
        Self::TimeoutFirstByte,
        Self::TimeoutTotal,
        Self::InternalError,
    ];

    /// The stable code text, for example `"MANIFEST_INVALID"`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProtocolUnsupported => "PROTOCOL_UNSUPPORTED",
            Self::ManifestInvalid => "MANIFEST_INVALID",
            Self::PatchInvalid => "PATCH_INVALID",
            Self::IntegrityError => "INTEGRITY_ERROR",
            Self::PatchContentRejected => "PATCH_CONTENT_REJECTED",
            Self::RqpMarkupLimit => "RQP_MARKUP_LIMIT",
            Self::CacheCorrupted => "CACHE_CORRUPTED",
            Self::AuthPartitionChanged => "AUTH_PARTITION_CHANGED",
            Self::TlsError => "TLS_ERROR",
            Self::OriginNotAllowed => "ORIGIN_NOT_ALLOWED",
            Self::WebviewCapabilityMissing => "WEBVIEW_CAPABILITY_MISSING",
            Self::TransportDisconnected => "TRANSPORT_DISCONNECTED",
            Self::MemoryBudgetExceeded => "MEMORY_BUDGET_EXCEEDED",
            Self::PatchTooLarge => "PATCH_TOO_LARGE",
            Self::DocumentTooLarge => "DOCUMENT_TOO_LARGE",
            Self::TimeoutFirstByte => "TIMEOUT_FIRST_BYTE",
            Self::TimeoutTotal => "TIMEOUT_TOTAL",
            Self::InternalError => "INTERNAL_ERROR",
        }
    }

    /// The category of the code (errors.md section 2, column "Category").
    #[must_use]
    pub const fn category(self) -> Category {
        match self {
            Self::ProtocolUnsupported | Self::WebviewCapabilityMissing => {
                Category::UnsupportedCapability
            }
            Self::ManifestInvalid
            | Self::PatchInvalid
            | Self::IntegrityError
            | Self::RqpMarkupLimit => Category::Protocol,
            Self::PatchContentRejected | Self::TlsError | Self::OriginNotAllowed => {
                Category::SecurityViolation
            }
            Self::CacheCorrupted => Category::CacheCorruption,
            Self::AuthPartitionChanged => Category::AuthChanged,
            Self::TransportDisconnected => Category::Network,
            Self::MemoryBudgetExceeded | Self::PatchTooLarge | Self::DocumentTooLarge => {
                Category::ResourceLimit
            }
            Self::TimeoutFirstByte | Self::TimeoutTotal => Category::Timeout,
            Self::InternalError => Category::Internal,
        }
    }

    /// The fallback hint of the code (errors.md section 2, column "Adapter
    /// behavior"; section 3, rule 4: the code has priority over its category).
    #[must_use]
    pub const fn fallback(self) -> FallbackHint {
        match self {
            Self::AuthPartitionChanged | Self::TlsError => FallbackHint::Stop,
            Self::TransportDisconnected => FallbackHint::RetryLater,
            Self::ProtocolUnsupported
            | Self::ManifestInvalid
            | Self::PatchInvalid
            | Self::IntegrityError
            | Self::PatchContentRejected
            | Self::RqpMarkupLimit
            | Self::CacheCorrupted
            | Self::OriginNotAllowed
            | Self::WebviewCapabilityMissing
            | Self::MemoryBudgetExceeded
            | Self::PatchTooLarge
            | Self::DocumentTooLarge
            | Self::TimeoutFirstByte
            | Self::TimeoutTotal
            | Self::InternalError => FallbackHint::NormalLoad,
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Every error type of the core maps to exactly one catalog code.
pub trait HasErrorCode {
    /// The catalog code of this error.
    fn code(&self) -> ErrorCode;
}
