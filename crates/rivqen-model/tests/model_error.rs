// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! Error type and catalog tests.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::HashSet;
use std::error::Error;
use std::fmt;

use rivqen_model::context::{Context, RivqenError};
use rivqen_model::error::{Category, ErrorCode, FallbackHint, HasErrorCode};
use rivqen_model::ids::{Generation, RequestId, SessionId};

#[derive(Debug)]
struct Cause(&'static str);
impl fmt::Display for Cause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cause holds {}", self.0)
    }
}
impl Error for Cause {}

#[test]
fn model_error_display_has_code_and_op_only() {
    let ctx = Context::new("split_template")
        .with_session(SessionId::from_bytes([7; 16]))
        .with_bytes(99)
        .with_state("Streaming");
    let e = RivqenError::new(ErrorCode::ManifestInvalid, ctx).with_source(Cause("MARKER"));
    assert_eq!(e.to_string(), "MANIFEST_INVALID in split_template");
}

#[test]
fn model_error_source_chain_is_kept_but_not_printed() {
    let e =
        RivqenError::new(ErrorCode::PatchInvalid, Context::new("op")).with_source(Cause("MARKER"));
    assert!(e.source().unwrap().to_string().contains("MARKER"));
    assert!(!format!("{e:?}").contains("MARKER"));
    assert!(format!("{e:?}").contains("has_source: true"));
    let none = RivqenError::new(ErrorCode::PatchInvalid, Context::new("op"));
    assert!(none.source().is_none());
    assert!(format!("{none:?}").contains("has_source: false"));
}

#[test]
fn model_error_reports_code_category_fallback_and_context() {
    let ctx = Context::new("commit")
        .with_generation(Generation::new(3))
        .with_request(RequestId::new(5));
    let e = RivqenError::new(ErrorCode::TlsError, ctx.clone());
    assert_eq!(e.code(), ErrorCode::TlsError);
    assert_eq!(HasErrorCode::code(&e), ErrorCode::TlsError);
    assert_eq!(e.category(), Category::SecurityViolation);
    assert_eq!(e.fallback(), FallbackHint::Stop);
    assert_eq!(e.context(), &ctx);
    assert_eq!(e.context().generation, Some(Generation::new(3)));
    assert_eq!(e.context().request, Some(RequestId::new(5)));
}

#[test]
fn model_error_error_is_send_sync() {
    fn check<T: Send + Sync + 'static>() {}
    check::<RivqenError>();
}

type Row = (ErrorCode, &'static str, Category, FallbackHint);

/// errors.md section 2: code text, category, fallback hint.
const CATALOG: [Row; 18] = [
    (
        ErrorCode::ProtocolUnsupported,
        "PROTOCOL_UNSUPPORTED",
        Category::UnsupportedCapability,
        FallbackHint::NormalLoad,
    ),
    (
        ErrorCode::ManifestInvalid,
        "MANIFEST_INVALID",
        Category::Protocol,
        FallbackHint::NormalLoad,
    ),
    (
        ErrorCode::PatchInvalid,
        "PATCH_INVALID",
        Category::Protocol,
        FallbackHint::NormalLoad,
    ),
    (
        ErrorCode::IntegrityError,
        "INTEGRITY_ERROR",
        Category::Protocol,
        FallbackHint::NormalLoad,
    ),
    (
        ErrorCode::PatchContentRejected,
        "PATCH_CONTENT_REJECTED",
        Category::SecurityViolation,
        FallbackHint::NormalLoad,
    ),
    (
        ErrorCode::RqpMarkupLimit,
        "RQP_MARKUP_LIMIT",
        Category::Protocol,
        FallbackHint::NormalLoad,
    ),
    (
        ErrorCode::CacheCorrupted,
        "CACHE_CORRUPTED",
        Category::CacheCorruption,
        FallbackHint::NormalLoad,
    ),
    (
        ErrorCode::AuthPartitionChanged,
        "AUTH_PARTITION_CHANGED",
        Category::AuthChanged,
        FallbackHint::Stop,
    ),
    (
        ErrorCode::TlsError,
        "TLS_ERROR",
        Category::SecurityViolation,
        FallbackHint::Stop,
    ),
    (
        ErrorCode::OriginNotAllowed,
        "ORIGIN_NOT_ALLOWED",
        Category::SecurityViolation,
        FallbackHint::NormalLoad,
    ),
    (
        ErrorCode::WebviewCapabilityMissing,
        "WEBVIEW_CAPABILITY_MISSING",
        Category::UnsupportedCapability,
        FallbackHint::NormalLoad,
    ),
    (
        ErrorCode::TransportDisconnected,
        "TRANSPORT_DISCONNECTED",
        Category::Network,
        FallbackHint::RetryLater,
    ),
    (
        ErrorCode::MemoryBudgetExceeded,
        "MEMORY_BUDGET_EXCEEDED",
        Category::ResourceLimit,
        FallbackHint::NormalLoad,
    ),
    (
        ErrorCode::PatchTooLarge,
        "PATCH_TOO_LARGE",
        Category::ResourceLimit,
        FallbackHint::NormalLoad,
    ),
    (
        ErrorCode::DocumentTooLarge,
        "DOCUMENT_TOO_LARGE",
        Category::ResourceLimit,
        FallbackHint::NormalLoad,
    ),
    (
        ErrorCode::TimeoutFirstByte,
        "TIMEOUT_FIRST_BYTE",
        Category::Timeout,
        FallbackHint::NormalLoad,
    ),
    (
        ErrorCode::TimeoutTotal,
        "TIMEOUT_TOTAL",
        Category::Timeout,
        FallbackHint::NormalLoad,
    ),
    (
        ErrorCode::InternalError,
        "INTERNAL_ERROR",
        Category::Internal,
        FallbackHint::NormalLoad,
    ),
];

#[test]
fn model_error_catalog_matches_errors_md() {
    for (code, text, category, hint) in CATALOG {
        assert_eq!(code.as_str(), text);
        assert_eq!(code.to_string(), text);
        assert_eq!(code.category(), category, "{text}");
        assert_eq!(code.fallback(), hint, "{text}");
    }
}

#[test]
fn model_error_catalog_lists_every_code_once_with_unique_text() {
    let all: Vec<_> = ErrorCode::ALL.to_vec();
    let expected: Vec<_> = CATALOG.iter().map(|c| c.0).collect();
    assert_eq!(all, expected);
    let texts: HashSet<_> = ErrorCode::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(texts.len(), ErrorCode::ALL.len());
    for t in texts {
        assert!(t.bytes().all(|b| b.is_ascii_uppercase() || b == b'_'));
    }
}
