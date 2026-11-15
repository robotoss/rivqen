// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The contract types of the cache engine (WP-07 S1): error mapping, sizes
//! and `Debug` texts that hold no URL, key or content.

use rivqen_cache::driver::{StoreError, StoreErrorKind};
use rivqen_cache::key::{KeyParts, Partition, Protocol, VaryDigest, VaryEntry};
use rivqen_cache::txn::{CommitStep, SnapshotBody};
use rivqen_model::error::{Category, ErrorCode, FallbackHint, HasErrorCode};
use rivqen_model::http::FieldLine;
use rivqen_model::ids::PartitionId;
use rivqen_model::safe::PageBytes;

const MARKER: &str = "s3cr3t-marker";

#[test]
fn cache_store_error_maps_to_the_catalog() {
    let unavailable = StoreError::new(StoreErrorKind::Unavailable, "apply");
    assert_eq!(unavailable.code(), ErrorCode::StorageUnavailable);
    assert_eq!(unavailable.code().category(), Category::PlatformConstraint);
    assert_eq!(unavailable.code().fallback(), FallbackHint::NormalLoad);
    assert_eq!(unavailable.kind(), StoreErrorKind::Unavailable);
    assert_eq!(unavailable.op(), "apply");
    assert_eq!(unavailable.to_string(), "STORAGE_UNAVAILABLE in apply");

    let corrupted = StoreError::new(StoreErrorKind::Corrupted, "read");
    assert_eq!(corrupted.code(), ErrorCode::CacheCorrupted);
    assert_eq!(corrupted.to_string(), "CACHE_CORRUPTED in read");
}

#[test]
fn cache_snapshot_body_counts_every_part() {
    let body = SnapshotBody {
        page: PageBytes::new(vec![0; 100]),
        template: Some(PageBytes::new(vec![0; 40])),
        data: vec![("{a}".to_owned(), PageBytes::new(vec![0; 7]))],
        fields: vec![FieldLine::new("content-type", "text/html")],
    };
    // 100 + 40 + (3 + 7) + (12 + 9)
    assert_eq!(body.size_bytes(), 171);

    let page_only = SnapshotBody {
        page: PageBytes::new(vec![0; 5]),
        template: None,
        data: Vec::new(),
        fields: Vec::new(),
    };
    assert_eq!(page_only.size_bytes(), 5);
}

#[test]
fn cache_debug_texts_hold_no_content() {
    let body = SnapshotBody {
        page: PageBytes::new(MARKER.as_bytes().to_vec()),
        template: Some(PageBytes::new(MARKER.as_bytes().to_vec())),
        data: vec![(
            MARKER.to_owned(),
            PageBytes::new(MARKER.as_bytes().to_vec()),
        )],
        fields: vec![FieldLine::new("x-test", MARKER)],
    };
    let text = format!("{body:?}");
    assert!(!text.contains(MARKER), "{text}");
    assert!(text.contains("size_bytes"), "{text}");

    let digest = VaryDigest::from_bytes([0xAB; 32]);
    assert_eq!(format!("{digest:?}"), "VaryDigest(..)");
    assert_eq!(digest.as_bytes(), &[0xAB; 32]);

    let vary = [VaryEntry {
        name: "cookie".to_owned(),
        value: Some(digest),
    }];
    let parts = KeyParts {
        partition: Partition::Scoped(PartitionId::from_bytes([1; 32])),
        origin: "https://shop.example.com",
        canonical_url: "https://shop.example.com/cart?token=s3cr3t-marker",
        vary: &vary,
        protocol: Protocol::Rqp,
        locale: None,
    };
    let text = format!("{parts:?}");
    assert!(!text.contains(MARKER), "{text}");
    assert!(!text.contains("shop.example.com"), "{text}");
    assert!(!text.to_ascii_lowercase().contains("abab"), "{text}");
}

#[test]
fn cache_commit_steps_are_in_the_order_of_storage_section_2() {
    assert_eq!(
        CommitStep::ALL,
        [
            CommitStep::WriteTemp,
            CommitStep::SyncTemp,
            CommitStep::Promote,
            CommitStep::CommitMeta,
            CommitStep::RemoveOld,
        ]
    );
}
