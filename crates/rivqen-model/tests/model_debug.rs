// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! No public type prints a planted secret or content marker (ERR-06).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::fmt;

use rivqen_model::context::{Context, RivqenError};
use rivqen_model::error::{Category, ErrorCode, FallbackHint};
use rivqen_model::http::FieldLine;
use rivqen_model::ids::{Generation, PartitionId, RequestId, Revision, SessionId, ValueError};
use rivqen_model::limits::ResourceLimits;
use rivqen_model::safe::{PageBytes, SafeUrl, Scheme, Secret};
use rivqen_model::time::Instant;

const MARKER: &str = "PLANTEDMARKER";

#[derive(Debug)]
struct MarkedCause;
impl fmt::Display for MarkedCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(MARKER)
    }
}
impl std::error::Error for MarkedCause {}

fn assert_clean(what: &str, text: &str) {
    assert!(!text.contains(MARKER), "{what} leaks the marker: {text}");
    assert!(
        !text.to_lowercase().contains("eeee"),
        "{what} leaks id bytes: {text}"
    );
}

#[test]
fn model_debug_has_no_marker() {
    let mut sid = [0xee_u8; 16];
    sid[..4].copy_from_slice(&[1, 2, 3, 4]);
    let mut pid = [0xee_u8; 32];
    pid[..4].copy_from_slice(&[1, 2, 3, 4]);
    let session = SessionId::from_bytes(sid);
    let url = SafeUrl::new(Scheme::Https, "a.test", None, "/p")
        .unwrap()
        .with_query(Secret::new(format!("t={MARKER}")))
        .unwrap();
    let ctx = Context::new("op")
        .with_session(session)
        .with_generation(Generation::new(1))
        .with_request(RequestId::new(2))
        .with_bytes(3)
        .with_state("S");
    let err = RivqenError::new(ErrorCode::InternalError, ctx.clone()).with_source(MarkedCause);
    let value_err: ValueError = Revision::new(&format!("{MARKER} bad")).unwrap_err();

    let debug: Vec<(&str, String)> = vec![
        ("SessionId", format!("{session:?}")),
        ("PartitionId", format!("{:?}", PartitionId::from_bytes(pid))),
        ("Generation", format!("{:?}", Generation::new(1))),
        ("RequestId", format!("{:?}", RequestId::new(2))),
        (
            "Revision",
            format!("{:?}", Revision::new("r1.abc").unwrap()),
        ),
        ("Secret", format!("{:?}", Secret::new(MARKER.to_owned()))),
        (
            "Secret<bytes>",
            format!("{:?}", Secret::new(MARKER.as_bytes().to_vec())),
        ),
        ("SafeUrl", format!("{url:?}")),
        (
            "PageBytes",
            format!("{:?}", PageBytes::new(MARKER.as_bytes().to_vec())),
        ),
        ("Scheme", format!("{:?}", Scheme::Https)),
        ("Context", format!("{ctx:?}")),
        ("RivqenError", format!("{err:?}")),
        ("ValueError", format!("{value_err:?}")),
        (
            "FieldLine",
            format!("{:?}", FieldLine::new("Cookie", MARKER)),
        ),
        ("Instant", format!("{:?}", Instant(1))),
        ("ErrorCode", format!("{:?}", ErrorCode::TlsError)),
        ("Category", format!("{:?}", Category::Network)),
        ("FallbackHint", format!("{:?}", FallbackHint::Stop)),
        ("ResourceLimits", format!("{:?}", ResourceLimits::default())),
    ];
    for (what, text) in &debug {
        assert_clean(what, text);
    }

    let display: Vec<(&str, String)> = vec![
        ("SafeUrl", url.to_string()),
        ("RivqenError", err.to_string()),
        ("ValueError", value_err.to_string()),
        ("ErrorCode", ErrorCode::TlsError.to_string()),
        ("Revision", Revision::new("r1.abc").unwrap().to_string()),
    ];
    for (what, text) in &display {
        assert_clean(what, text);
    }
}
