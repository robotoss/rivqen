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
const CATALOG: [Row; 20] = [
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
    (
        ErrorCode::ConfigInvalid,
        "CONFIG_INVALID",
        Category::Internal,
        FallbackHint::NormalLoad,
    ),
    (
        ErrorCode::StorageUnavailable,
        "STORAGE_UNAVAILABLE",
        Category::PlatformConstraint,
        FallbackHint::NormalLoad,
    ),
];

#[test]
fn model_error_catalog_matches_table_of_this_file() {
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

// ---------------------------------------------------------------------------
// The catalog test reads the page (DL-179)

/// Codes of the page that are not in the enum (errors.md section 4): two
/// reserved codes, a notice and a server code.
const NOT_IN_ENUM: [&str; 4] = [
    "PATCH_BASE_MISMATCH",
    "TEMPLATE_MISMATCH",
    "LEGACY_MODE_USED",
    "SERVER_PASSTHROUGH",
];

fn page_text() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/engineering/core/errors.md");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The lines of a section: after the heading that starts with `heading`, up
/// to the next `## ` heading.
fn section<'a>(page: &'a str, heading: &str) -> Vec<&'a str> {
    page.lines()
        .skip_while(|l| !l.starts_with(heading))
        .skip(1)
        .take_while(|l| !l.starts_with("## "))
        .collect()
}

/// The texts between backticks in a cell.
fn ticks(cell: &str) -> Vec<&str> {
    cell.split('`').skip(1).step_by(2).collect()
}

/// One row of section 2: its codes, its category and the explicit hint.
struct PageRow {
    codes: Vec<String>,
    category: String,
    hint: Option<String>,
}

fn page_rows(page: &str) -> Vec<PageRow> {
    section(page, "## 2. Error codes")
        .into_iter()
        .filter(|l| l.starts_with("| `"))
        .map(|line| {
            let mut cells = line.trim_start_matches('|').splitn(3, '|');
            let codes = cells.next().unwrap();
            let category = cells
                .next()
                .unwrap_or_else(|| panic!("no category: {line}"));
            let behavior = cells
                .next()
                .unwrap_or_else(|| panic!("no behavior: {line}"));
            let hint = behavior
                .split("Hint: `")
                .nth(1)
                .and_then(|t| t.split('`').next())
                .map(str::to_owned);
            PageRow {
                codes: ticks(codes).into_iter().map(str::to_owned).collect(),
                category: category.trim().trim_matches('`').to_owned(),
                hint,
            }
        })
        .collect()
}

/// The hint of each code that the table of section 4 gives.
fn page_hints(page: &str, all: &[ErrorCode]) -> (Vec<(String, String)>, Option<String>) {
    const HINTS: [&str; 5] = [
        "Stop",
        "RetryLater",
        "NormalLoad",
        "None",
        "ShowCachedIfAllowed",
    ];
    let mut count_problem = None;
    let mut named: Vec<(String, String)> = Vec::new();
    let mut other = None;
    for line in section(page, "## 4. Implementation") {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        let (Some(hint), Some(codes)) = (cells.get(1), cells.get(2)) else {
            continue;
        };
        let hint = hint.trim_matches('`');
        if !HINTS.contains(&hint) {
            continue;
        }
        if codes.starts_with("all other") {
            other = Some((hint.to_owned(), (*codes).to_owned()));
        } else {
            for code in ticks(codes) {
                named.push((code.to_owned(), hint.to_owned()));
            }
        }
    }
    if let Some((hint, text)) = other {
        let rest: Vec<&str> = all
            .iter()
            .map(|c| c.as_str())
            .filter(|c| !named.iter().any(|(n, _)| n == c))
            .collect();
        let count: usize = text
            .split_whitespace()
            .find_map(|w| w.parse().ok())
            .unwrap_or_else(|| panic!("no count in {text:?}"));
        if rest.len() != count {
            count_problem = Some(format!(
                "hint table: {text:?}, the code list has {}",
                rest.len()
            ));
        }
        named.extend(rest.into_iter().map(|c| (c.to_owned(), hint.clone())));
    }
    (named, count_problem)
}

/// The differences between the page and the codes; each names the code.
fn catalog_problems(page: &str, all: &[ErrorCode]) -> Vec<String> {
    let mut problems = Vec::new();
    let rows = page_rows(page);
    let (hints, count_problem) = page_hints(page, all);
    problems.extend(count_problem);
    let mut seen: Vec<&str> = Vec::new();
    for row in &rows {
        for code in &row.codes {
            seen.push(code);
            if NOT_IN_ENUM.contains(&code.as_str()) {
                continue;
            }
            let Some(found) = all.iter().find(|c| c.as_str() == code) else {
                problems.push(format!("{code}: on the page, not in ErrorCode::ALL"));
                continue;
            };
            let category = format!("{:?}", found.category());
            if row.category != category {
                problems.push(format!(
                    "{code}: category {} on the page, {category} in code",
                    row.category
                ));
            }
            let hint = format!("{:?}", found.fallback());
            if let Some(explicit) = row.hint.as_ref().filter(|h| **h != hint) {
                problems.push(format!(
                    "{code}: hint {explicit} in the row, {hint} in code"
                ));
            }
            match hints.iter().find(|(c, _)| c == code) {
                Some((_, table)) if *table == hint => {}
                Some((_, table)) => problems.push(format!(
                    "{code}: hint {table} in the hint table, {hint} in code"
                )),
                None => problems.push(format!("{code}: not in the hint table")),
            }
        }
    }
    for code in all {
        if !seen.contains(&code.as_str()) {
            problems.push(format!(
                "{}: in ErrorCode::ALL, no row on the page",
                code.as_str()
            ));
        }
    }
    for code in NOT_IN_ENUM {
        if !seen.contains(&code) {
            problems.push(format!("{code}: reserved code has no row on the page"));
        }
    }
    problems
}

#[test]
fn model_error_catalog_matches_errors_md() {
    let problems = catalog_problems(&page_text(), &ErrorCode::ALL);
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn model_error_catalog_check_names_a_code_missing_from_the_page() {
    let page = page_text();
    let edited: String = page
        .lines()
        .filter(|l| !l.starts_with("| `TLS_ERROR`"))
        .collect::<Vec<_>>()
        .join("\n");
    let problems = catalog_problems(&edited, &ErrorCode::ALL);
    assert!(
        problems.iter().any(|p| p.starts_with("TLS_ERROR:")),
        "{problems:?}"
    );
}

#[test]
fn model_error_catalog_check_names_a_code_missing_from_the_enum() {
    let all: Vec<ErrorCode> = ErrorCode::ALL
        .iter()
        .copied()
        .filter(|c| *c != ErrorCode::TimeoutTotal)
        .collect();
    let problems = catalog_problems(&page_text(), &all);
    assert!(
        problems.iter().any(|p| p.starts_with("TIMEOUT_TOTAL:")),
        "{problems:?}"
    );
}

#[test]
fn model_error_catalog_check_names_a_changed_category() {
    let edited = page_text().replace(
        "| `TIMEOUT_FIRST_BYTE` / `TIMEOUT_TOTAL` | Timeout |",
        "| `TIMEOUT_FIRST_BYTE` / `TIMEOUT_TOTAL` | Network |",
    );
    let problems = catalog_problems(&edited, &ErrorCode::ALL);
    assert!(
        problems
            .iter()
            .any(|p| p.starts_with("TIMEOUT_TOTAL: category")),
        "{problems:?}"
    );
}

#[test]
fn model_error_catalog_check_names_a_changed_hint() {
    let edited = page_text().replace("| `RetryLater` |", "| `Stop` |");
    let problems = catalog_problems(&edited, &ErrorCode::ALL);
    assert!(
        problems
            .iter()
            .any(|p| p.starts_with("TRANSPORT_DISCONNECTED: hint")),
        "{problems:?}"
    );
}
