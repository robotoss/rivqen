// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Resource limits: default values and safe ranges (resource-limits.md
//! sections 1 and 3).
//!
//! The defaults are initial test values; WP-19 sets them per device class.
//! [`Limit::range`] is the one table of safe ranges. `rivqen-core` checks a
//! configuration against it (`EngineConfig`, `Engine::new`).
//!
//! The limits that `rqp/1` fixes (`maxRqHaveBytes`, `maxRqCapabilitiesBytes`,
//! `maxRqNonceBytes`) are not here: they are constants of `rivqen-proto`,
//! because a different value on one side changes the result for valid
//! requests (resource-limits section 3, rule 1).

const KIB: u64 = 1024;
const MIB: u64 = 1024 * KIB;
const SECOND_MS: u64 = 1000;

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
            max_json_depth: 16,
            max_header_bytes: 16 * KIB,
            max_headers_total_bytes: 64 * KIB,
            max_active_sessions: 8,
            max_concurrent_prefetch: 2,
            timeout_connect_ms: 5 * SECOND_MS,
            timeout_first_byte_ms: 10 * SECOND_MS,
            timeout_total_ms: 30 * SECOND_MS,
            memory_l1_bytes: 8 * MIB,
            disk_l2_bytes: 64 * MIB,
        }
    }
}

impl ResourceLimits {
    /// The value of one limit, widened to `u64` (no arithmetic).
    #[must_use]
    pub fn get(&self, limit: Limit) -> u64 {
        match limit {
            Limit::MaxDocumentBytes => self.max_document_bytes,
            Limit::MaxCompressedRatio => u64::from(self.max_compressed_ratio),
            Limit::PrefixBufferBytes => self.prefix_buffer_bytes,
            Limit::MaxChunkBytes => self.max_chunk_bytes,
            Limit::MaxBlocks => u64::from(self.max_blocks),
            Limit::MaxBlockKeyBytesLegacy => u64::from(self.max_block_key_bytes_legacy),
            Limit::MaxBlockKeyBytesRqp => u64::from(self.max_block_key_bytes_rqp),
            Limit::MaxBlockBytes => self.max_block_bytes,
            Limit::MaxManifestBytes => self.max_manifest_bytes,
            Limit::MaxOpenElements => u64::from(self.max_open_elements),
            Limit::MaxPatchBytes => self.max_patch_bytes,
            Limit::MaxJsonDepth => u64::from(self.max_json_depth),
            Limit::MaxHeaderBytes => self.max_header_bytes,
            Limit::MaxHeadersTotalBytes => self.max_headers_total_bytes,
            Limit::MaxActiveSessions => u64::from(self.max_active_sessions),
            Limit::MaxConcurrentPrefetch => u64::from(self.max_concurrent_prefetch),
            Limit::TimeoutConnect => self.timeout_connect_ms,
            Limit::TimeoutFirstByte => self.timeout_first_byte_ms,
            Limit::TimeoutTotal => self.timeout_total_ms,
            Limit::MemoryL1Bytes => self.memory_l1_bytes,
            Limit::DiskL2Bytes => self.disk_l2_bytes,
        }
    }
}

/// An inclusive range of allowed values: `min <= value <= max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LimitRange {
    min: u64,
    max: u64,
}

impl LimitRange {
    /// The range `min..=max`. Only the table of [`Limit::range`] makes one, so
    /// `min <= max` holds (test `model_limits_ranges_are_ordered`).
    const fn new(min: u64, max: u64) -> Self {
        Self { min, max }
    }

    /// The smallest allowed value.
    #[must_use]
    pub const fn min(self) -> u64 {
        self.min
    }

    /// The largest allowed value.
    #[must_use]
    pub const fn max(self) -> u64 {
        self.max
    }

    /// True when `min <= value <= max`.
    #[must_use]
    pub const fn contains(self, value: u64) -> bool {
        self.min <= value && value <= self.max
    }

    /// True when the range has one value: the protocol fixes the limit, so a
    /// host cannot change it (resource-limits section 1.1).
    #[must_use]
    pub const fn is_fixed(self) -> bool {
        self.min == self.max
    }
}

/// One configurable limit of [`ResourceLimits`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Limit {
    /// `maxDocumentBytes`
    MaxDocumentBytes,
    /// `maxCompressedRatio`
    MaxCompressedRatio,
    /// `prefixBufferBytes`
    PrefixBufferBytes,
    /// `maxChunkBytes`
    MaxChunkBytes,
    /// `maxBlocks`
    MaxBlocks,
    /// `maxBlockKeyBytes` in legacy mode
    MaxBlockKeyBytesLegacy,
    /// `maxBlockKeyBytes` in RQP
    MaxBlockKeyBytesRqp,
    /// `maxBlockBytes`
    MaxBlockBytes,
    /// `maxManifestBytes`
    MaxManifestBytes,
    /// `maxOpenElements`
    MaxOpenElements,
    /// `maxPatchBytes`
    MaxPatchBytes,
    /// `maxJsonDepth`
    MaxJsonDepth,
    /// `maxHeaderBytes`, one response field line
    MaxHeaderBytes,
    /// `maxHeaderBytes`, all response field lines
    MaxHeadersTotalBytes,
    /// `maxActiveSessions`
    MaxActiveSessions,
    /// `maxConcurrentPrefetch`
    MaxConcurrentPrefetch,
    /// `timeoutConnect`, milliseconds
    TimeoutConnect,
    /// `timeoutFirstByte`, milliseconds
    TimeoutFirstByte,
    /// `timeoutTotal`, milliseconds
    TimeoutTotal,
    /// `memoryL1Bytes`
    MemoryL1Bytes,
    /// `diskL2Bytes`
    DiskL2Bytes,
}

impl Limit {
    /// Every limit, in the order of resource-limits section 1.
    pub const ALL: [Self; 21] = [
        Self::MaxDocumentBytes,
        Self::MaxCompressedRatio,
        Self::PrefixBufferBytes,
        Self::MaxChunkBytes,
        Self::MaxBlocks,
        Self::MaxBlockKeyBytesLegacy,
        Self::MaxBlockKeyBytesRqp,
        Self::MaxBlockBytes,
        Self::MaxManifestBytes,
        Self::MaxOpenElements,
        Self::MaxPatchBytes,
        Self::MaxJsonDepth,
        Self::MaxHeaderBytes,
        Self::MaxHeadersTotalBytes,
        Self::MaxActiveSessions,
        Self::MaxConcurrentPrefetch,
        Self::TimeoutConnect,
        Self::TimeoutFirstByte,
        Self::TimeoutTotal,
        Self::MemoryL1Bytes,
        Self::DiskL2Bytes,
    ];

    /// Pairs `(a, b)` where a valid configuration has `a <= b`
    /// (resource-limits section 1.1). The ranges alone already give
    /// `maxChunkBytes <= prefixBufferBytes`, `maxBlockBytes <=
    /// maxDocumentBytes` and `maxManifestBytes <= maxDocumentBytes`.
    pub const NOT_ABOVE: [(Self, Self); 4] = [
        // manifest-patch 1.5: a patch larger than a full document has no use.
        (Self::MaxPatchBytes, Self::MaxDocumentBytes),
        // One field line is part of the whole head.
        (Self::MaxHeaderBytes, Self::MaxHeadersTotalBytes),
        // The connection and the first byte are parts of the whole load.
        (Self::TimeoutConnect, Self::TimeoutTotal),
        (Self::TimeoutFirstByte, Self::TimeoutTotal),
    ];

    /// The stable name: the name of resource-limits section 1, with a suffix
    /// where one limit there has two values.
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.spec().0
    }

    /// The safe range (resource-limits section 1.1).
    #[must_use]
    pub const fn range(self) -> LimitRange {
        self.spec().1
    }

    /// The table of names and safe ranges. Reasons per row: resource-limits
    /// section 1.1. A range with `min == max` is a value that a protocol rule
    /// of `rqp/1` fixes (markup M-15, M-26; manifest-patch 1.8), or the
    /// legacy name limit of legacy wire C-07 (DL-169).
    const fn spec(self) -> (&'static str, LimitRange) {
        match self {
            Self::MaxDocumentBytes => ("maxDocumentBytes", LimitRange::new(MIB, 32 * MIB)),
            Self::MaxCompressedRatio => ("maxCompressedRatio", LimitRange::new(10, 100)),
            Self::PrefixBufferBytes => ("prefixBufferBytes", LimitRange::new(64 * KIB, 2 * MIB)),
            Self::MaxChunkBytes => ("maxChunkBytes", LimitRange::new(8 * KIB, 64 * KIB)),
            Self::MaxBlocks => ("maxBlocks", LimitRange::new(256, 256)),
            Self::MaxBlockKeyBytesLegacy => ("maxBlockKeyBytesLegacy", LimitRange::new(128, 128)),
            Self::MaxBlockKeyBytesRqp => ("maxBlockKeyBytesRqp", LimitRange::new(64, 64)),
            Self::MaxBlockBytes => ("maxBlockBytes", LimitRange::new(MIB, MIB)),
            Self::MaxManifestBytes => ("maxManifestBytes", LimitRange::new(64 * KIB, 64 * KIB)),
            Self::MaxOpenElements => ("maxOpenElements", LimitRange::new(256, 256)),
            Self::MaxPatchBytes => ("maxPatchBytes", LimitRange::new(KIB, 5 * MIB)),
            Self::MaxJsonDepth => ("maxJsonDepth", LimitRange::new(3, 16)),
            Self::MaxHeaderBytes => ("maxHeaderBytes", LimitRange::new(8 * KIB, 64 * KIB)),
            Self::MaxHeadersTotalBytes => {
                ("maxHeadersTotalBytes", LimitRange::new(8 * KIB, 256 * KIB))
            }
            Self::MaxActiveSessions => ("maxActiveSessions", LimitRange::new(1, 16)),
            Self::MaxConcurrentPrefetch => ("maxConcurrentPrefetch", LimitRange::new(1, 8)),
            Self::TimeoutConnect => ("timeoutConnect", LimitRange::new(SECOND_MS, 30 * SECOND_MS)),
            Self::TimeoutFirstByte => (
                "timeoutFirstByte",
                LimitRange::new(SECOND_MS, 60 * SECOND_MS),
            ),
            Self::TimeoutTotal => ("timeoutTotal", LimitRange::new(SECOND_MS, 120 * SECOND_MS)),
            Self::MemoryL1Bytes => ("memoryL1Bytes", LimitRange::new(8 * MIB, 32 * MIB)),
            Self::DiskL2Bytes => ("diskL2Bytes", LimitRange::new(64 * MIB, 256 * MIB)),
        }
    }
}
