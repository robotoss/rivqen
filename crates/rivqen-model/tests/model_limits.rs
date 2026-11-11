// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! Default limit and safe range tests (resource-limits sections 1 and 1.1).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::HashSet;

use rivqen_model::limits::{Limit, ResourceLimits};

const KIB: u64 = 1024;
const MIB: u64 = 1024 * KIB;

#[test]
fn model_limits_default_has_the_initial_values() {
    let l = ResourceLimits::default();
    assert_eq!(l.max_document_bytes, 5 * 1024 * 1024);
    assert_eq!(l.max_compressed_ratio, 50);
    assert_eq!(l.prefix_buffer_bytes, 512 * 1024);
    assert_eq!(l.max_chunk_bytes, 64 * 1024);
    assert_eq!(l.max_blocks, 256);
    assert_eq!(l.max_block_key_bytes_legacy, 128);
    assert_eq!(l.max_block_key_bytes_rqp, 64);
    assert_eq!(l.max_block_bytes, 1024 * 1024);
    assert_eq!(l.max_manifest_bytes, 64 * 1024);
    assert_eq!(l.max_open_elements, 256);
    assert_eq!(l.max_patch_bytes, 256 * 1024);
    assert_eq!(l.max_json_depth, 16);
    assert_eq!(l.max_header_bytes, 16 * 1024);
    assert_eq!(l.max_headers_total_bytes, 64 * 1024);
    assert_eq!(l.max_active_sessions, 8);
    assert_eq!(l.max_concurrent_prefetch, 2);
    assert_eq!(l.timeout_connect_ms, 5_000);
    assert_eq!(l.timeout_first_byte_ms, 10_000);
    assert_eq!(l.timeout_total_ms, 30_000);
    assert_eq!(l.memory_l1_bytes, 8 * 1024 * 1024);
    assert_eq!(l.disk_l2_bytes, 64 * 1024 * 1024);
}

/// Every field has its own value, so a `get` arm that reads the wrong field
/// fails.
fn distinct() -> ResourceLimits {
    ResourceLimits {
        max_document_bytes: 101,
        max_compressed_ratio: 102,
        prefix_buffer_bytes: 103,
        max_chunk_bytes: 104,
        max_blocks: 105,
        max_block_key_bytes_legacy: 106,
        max_block_key_bytes_rqp: 107,
        max_block_bytes: 108,
        max_manifest_bytes: 109,
        max_open_elements: 110,
        max_patch_bytes: 111,
        max_json_depth: 112,
        max_header_bytes: 113,
        max_headers_total_bytes: 114,
        max_active_sessions: 115,
        max_concurrent_prefetch: 116,
        timeout_connect_ms: 117,
        timeout_first_byte_ms: 118,
        timeout_total_ms: 119,
        memory_l1_bytes: 120,
        disk_l2_bytes: 121,
    }
}

#[test]
fn model_limits_get_reads_the_field_of_each_limit() {
    let l = distinct();
    let got: Vec<u64> = Limit::ALL.iter().map(|&limit| l.get(limit)).collect();
    let want: Vec<u64> = (101..=121).collect();
    assert_eq!(got, want);
    let wide = ResourceLimits {
        max_blocks: u32::MAX,
        ..distinct()
    };
    assert_eq!(wide.get(Limit::MaxBlocks), u64::from(u32::MAX));
}

#[test]
fn model_limits_names_are_the_table_names() {
    let names: Vec<&str> = Limit::ALL.iter().map(|l| l.name()).collect();
    assert_eq!(
        names,
        [
            "maxDocumentBytes",
            "maxCompressedRatio",
            "prefixBufferBytes",
            "maxChunkBytes",
            "maxBlocks",
            "maxBlockKeyBytesLegacy",
            "maxBlockKeyBytesRqp",
            "maxBlockBytes",
            "maxManifestBytes",
            "maxOpenElements",
            "maxPatchBytes",
            "maxJsonDepth",
            "maxHeaderBytes",
            "maxHeadersTotalBytes",
            "maxActiveSessions",
            "maxConcurrentPrefetch",
            "timeoutConnect",
            "timeoutFirstByte",
            "timeoutTotal",
            "memoryL1Bytes",
            "diskL2Bytes",
        ]
    );
    let unique: HashSet<&str> = names.iter().copied().collect();
    assert_eq!(unique.len(), Limit::ALL.len());
}

#[test]
fn model_limits_ranges_are_the_table_of_section_1_1() {
    let ranges: Vec<(u64, u64)> = Limit::ALL
        .iter()
        .map(|l| (l.range().min(), l.range().max()))
        .collect();
    assert_eq!(
        ranges,
        [
            (MIB, 32 * MIB),
            (10, 100),
            (64 * KIB, 2 * MIB),
            (8 * KIB, 64 * KIB),
            (256, 256),
            (128, 1024),
            (64, 64),
            (MIB, MIB),
            (64 * KIB, 64 * KIB),
            (256, 256),
            (KIB, 5 * MIB),
            (3, 16),
            (8 * KIB, 64 * KIB),
            (8 * KIB, 256 * KIB),
            (1, 16),
            (1, 8),
            (1_000, 30_000),
            (1_000, 60_000),
            (1_000, 120_000),
            (8 * MIB, 32 * MIB),
            (64 * MIB, 256 * MIB),
        ]
    );
}

#[test]
fn model_limits_ranges_are_ordered() {
    for limit in Limit::ALL {
        let r = limit.range();
        assert!(r.min() <= r.max(), "{}", limit.name());
        assert!(
            r.min() > 0,
            "{}: zero would disable the feature",
            limit.name()
        );
    }
}

#[test]
fn model_limits_defaults_are_within_their_ranges() {
    let l = ResourceLimits::default();
    for limit in Limit::ALL {
        assert!(limit.range().contains(l.get(limit)), "{}", limit.name());
    }
    for (a, b) in Limit::NOT_ABOVE {
        assert!(l.get(a) <= l.get(b), "{} > {}", a.name(), b.name());
    }
}

#[test]
fn model_limits_range_contains_both_ends_only() {
    let r = Limit::MaxJsonDepth.range();
    assert!(!r.contains(2));
    assert!(r.contains(3));
    assert!(r.contains(16));
    assert!(!r.contains(17));
    assert!(!r.contains(0));
    assert!(!r.contains(u64::MAX));
}

#[test]
fn model_limits_fixed_ranges_are_the_protocol_values() {
    let fixed: Vec<&str> = Limit::ALL
        .iter()
        .filter(|l| l.range().is_fixed())
        .map(|l| l.name())
        .collect();
    assert_eq!(
        fixed,
        [
            "maxBlocks",
            "maxBlockKeyBytesRqp",
            "maxBlockBytes",
            "maxManifestBytes",
            "maxOpenElements",
        ]
    );
}

#[test]
fn model_limits_ranges_imply_the_unlisted_relations() {
    // Section 1.1: these pairs hold for any values in range, so they are not
    // in NOT_ABOVE.
    let below = |a: Limit, b: Limit| a.range().max() <= b.range().min();
    assert!(below(Limit::MaxChunkBytes, Limit::PrefixBufferBytes));
    assert!(below(Limit::MaxBlockBytes, Limit::MaxDocumentBytes));
    assert!(below(Limit::MaxManifestBytes, Limit::MaxDocumentBytes));
}

#[test]
fn model_limits_not_above_pairs() {
    let names: Vec<(&str, &str)> = Limit::NOT_ABOVE
        .iter()
        .map(|(a, b)| (a.name(), b.name()))
        .collect();
    assert_eq!(
        names,
        [
            ("maxPatchBytes", "maxDocumentBytes"),
            ("maxHeaderBytes", "maxHeadersTotalBytes"),
            ("timeoutConnect", "timeoutTotal"),
            ("timeoutFirstByte", "timeoutTotal"),
        ]
    );
}
