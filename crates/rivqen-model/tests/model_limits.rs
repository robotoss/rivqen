// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! Default limit tests.

use rivqen_model::limits::ResourceLimits;

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
    assert_eq!(l.max_rq_have_bytes, 4096);
    assert_eq!(l.max_rq_capabilities_bytes, 1024);
    assert_eq!(l.max_rq_nonce_bytes, 128);
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
