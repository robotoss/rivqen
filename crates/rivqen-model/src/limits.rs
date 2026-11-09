// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Default resource limits (resource-limits.md section 1).
//!
//! The values are initial test values; WP-19 sets them per device class.
//! Configurable ranges and `EngineConfig` come with the core facade.

const KIB: u64 = 1024;
const MIB: u64 = 1024 * KIB;

/// Size, count, depth and time limits of the core. Sizes are in bytes, times
/// in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceLimits {
    /// `maxDocumentBytes`: decoded main document.
    pub max_document_bytes: u64,
    /// `maxCompressedRatio`: decoded bytes per encoded byte.
    pub max_compressed_ratio: u32,
    /// `prefixBufferBytes`: bridge stream prefix.
    pub prefix_buffer_bytes: u64,
    /// `maxChunkBytes`: one stream chunk.
    pub max_chunk_bytes: u64,
    /// `maxBlocks`: data blocks per page.
    pub max_blocks: u32,
    /// `maxBlockKeyBytes` in legacy mode.
    pub max_block_key_bytes_legacy: u32,
    /// `maxBlockKeyBytes` in RQP.
    pub max_block_key_bytes_rqp: u32,
    /// `maxBlockBytes`: one data block.
    pub max_block_bytes: u64,
    /// `maxManifestBytes`: text of the RQP manifest.
    pub max_manifest_bytes: u64,
    /// `maxOpenElements`: depth counter of the RQP markup analysis.
    pub max_open_elements: u32,
    /// `maxPatchBytes`: one RQP patch envelope.
    pub max_patch_bytes: u64,
    /// `maxRqHaveBytes`: fixed by `rqp/1`.
    pub max_rq_have_bytes: u32,
    /// `maxRqCapabilitiesBytes`: fixed by `rqp/1`.
    pub max_rq_capabilities_bytes: u32,
    /// `maxRqNonceBytes`: fixed by `rqp/1`.
    pub max_rq_nonce_bytes: u32,
    /// `maxJsonDepth`.
    pub max_json_depth: u32,
    /// `maxHeaderBytes`: one response header.
    pub max_header_bytes: u64,
    /// `maxHeaderBytes`: all response headers.
    pub max_headers_total_bytes: u64,
    /// `maxActiveSessions` per engine.
    pub max_active_sessions: u32,
    /// `maxConcurrentPrefetch`.
    pub max_concurrent_prefetch: u32,
    /// `timeoutConnect`.
    pub timeout_connect_ms: u64,
    /// `timeoutFirstByte`.
    pub timeout_first_byte_ms: u64,
    /// `timeoutTotal`.
    pub timeout_total_ms: u64,
    /// `memoryL1Bytes`: the lowest device class (8 MiB; the range is 8 to 32 MiB).
    pub memory_l1_bytes: u64,
    /// `diskL2Bytes`: the lowest device class (64 MiB; the range is 64 to 256 MiB).
    pub disk_l2_bytes: u64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_document_bytes: 5 * MIB,
            max_compressed_ratio: 50,
            prefix_buffer_bytes: 512 * KIB,
            max_chunk_bytes: 64 * KIB,
            max_blocks: 256,
            max_block_key_bytes_legacy: 128,
            max_block_key_bytes_rqp: 64,
            max_block_bytes: MIB,
            max_manifest_bytes: 64 * KIB,
            max_open_elements: 256,
            max_patch_bytes: 256 * KIB,
            max_rq_have_bytes: 4096,
            max_rq_capabilities_bytes: 1024,
            max_rq_nonce_bytes: 128,
            max_json_depth: 16,
            max_header_bytes: 16 * KIB,
            max_headers_total_bytes: 64 * KIB,
            max_active_sessions: 8,
            max_concurrent_prefetch: 2,
            timeout_connect_ms: 5_000,
            timeout_first_byte_ms: 10_000,
            timeout_total_ms: 30_000,
            memory_l1_bytes: 8 * MIB,
            disk_l2_bytes: 64 * MIB,
        }
    }
}
