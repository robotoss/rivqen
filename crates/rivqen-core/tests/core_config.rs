// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! Engine configuration check (resource-limits sections 1.1 and 3).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::error::Error as _;

use proptest::prelude::*;
use rivqen_core::config::{CONFIG_OP, ConfigError, Engine, EngineConfig, Violation};
use rivqen_model::context::RivqenError;
use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_model::limits::{Limit, ResourceLimits};

/// Sets one limit. A value that does not fit a `u32` field saturates to
/// `u32::MAX` (still out of every range of a `u32` field).
fn set(l: &mut ResourceLimits, limit: Limit, value: u64) {
    let small = u32::try_from(value).unwrap_or(u32::MAX);
    match limit {
        Limit::MaxDocumentBytes => l.max_document_bytes = value,
        Limit::MaxCompressedRatio => l.max_compressed_ratio = small,
        Limit::PrefixBufferBytes => l.prefix_buffer_bytes = value,
        Limit::MaxChunkBytes => l.max_chunk_bytes = value,
        Limit::MaxBlocks => l.max_blocks = small,
        Limit::MaxBlockKeyBytesLegacy => l.max_block_key_bytes_legacy = small,
        Limit::MaxBlockKeyBytesRqp => l.max_block_key_bytes_rqp = small,
        Limit::MaxBlockBytes => l.max_block_bytes = value,
        Limit::MaxManifestBytes => l.max_manifest_bytes = value,
        Limit::MaxOpenElements => l.max_open_elements = small,
        Limit::MaxPatchBytes => l.max_patch_bytes = value,
        Limit::MaxJsonDepth => l.max_json_depth = small,
        Limit::MaxHeaderBytes => l.max_header_bytes = value,
        Limit::MaxHeadersTotalBytes => l.max_headers_total_bytes = value,
        Limit::MaxActiveSessions => l.max_active_sessions = small,
        Limit::MaxConcurrentPrefetch => l.max_concurrent_prefetch = small,
        Limit::TimeoutConnect => l.timeout_connect_ms = value,
        Limit::TimeoutFirstByte => l.timeout_first_byte_ms = value,
        Limit::TimeoutTotal => l.timeout_total_ms = value,
        Limit::MemoryL1Bytes => l.memory_l1_bytes = value,
        Limit::DiskL2Bytes => l.disk_l2_bytes = value,
        _ => panic!("new limit {}: extend the test", limit.name()),
    }
}

/// The defaults with the other side of each `NOT_ABOVE` pair at its maximum,
/// so that one limit can take any value of its range.
fn roomy() -> ResourceLimits {
    let mut l = ResourceLimits::default();
    for (_, bound) in Limit::NOT_ABOVE {
        set(&mut l, bound, bound.range().max());
    }
    l
}

fn rejected(limits: ResourceLimits) -> (RivqenError, ConfigError) {
    let err = Engine::new(EngineConfig::new(limits)).expect_err("must be rejected");
    let cause = *ConfigError::of(&err).expect("cause is a ConfigError");
    assert_eq!(err.code(), ErrorCode::ConfigInvalid);
    assert_eq!(err.context().op, CONFIG_OP);
    assert_eq!(cause.code(), ErrorCode::ConfigInvalid);
    assert_eq!(EngineConfig::new(limits).check(), Err(cause));
    (err, cause)
}

#[test]
fn core_config_defaults_pass() {
    let engine = Engine::new(EngineConfig::default()).unwrap();
    assert_eq!(engine.config().limits, ResourceLimits::default());
    assert_eq!(EngineConfig::default().check(), Ok(()));
    assert_eq!(
        EngineConfig::new(ResourceLimits::default()),
        EngineConfig::default()
    );
}

#[test]
fn core_config_both_ends_of_each_range_pass() {
    for limit in Limit::ALL {
        for value in [limit.range().min(), limit.range().max()] {
            let mut l = roomy();
            set(&mut l, limit, value);
            // Keep the pairs: a small bound side needs a small limit side.
            for (a, b) in Limit::NOT_ABOVE {
                if b == limit {
                    set(&mut l, a, a.range().min());
                }
            }
            let engine = Engine::new(EngineConfig::new(l));
            assert!(engine.is_ok(), "{} = {value}", limit.name());
            assert_eq!(engine.unwrap().config().limits, l);
        }
    }
}

#[test]
fn core_config_out_of_range_rejected() {
    for limit in Limit::ALL {
        let range = limit.range();
        let below = [range.min() - 1, 0];
        let above = [range.max() + 1, u64::MAX];
        for (values, violation) in [(below, Violation::BelowMin), (above, Violation::AboveMax)] {
            for value in values {
                let mut l = roomy();
                set(&mut l, limit, value);
                let (err, cause) = rejected(l);
                assert_eq!(cause.limit(), limit, "{} = {value}", limit.name());
                assert_eq!(cause.violation(), violation, "{} = {value}", limit.name());
                assert!(err.source().is_some());
            }
        }
    }
}

#[test]
fn core_config_pairs_rejected_above_their_bound() {
    for (a, b) in Limit::NOT_ABOVE {
        let mut l = roomy();
        // Equal passes.
        let shared = a.range().max().min(b.range().max()).max(b.range().min());
        set(&mut l, b, shared);
        set(&mut l, a, shared);
        assert_eq!(
            EngineConfig::new(l).check(),
            Ok(()),
            "{} = {}",
            a.name(),
            b.name()
        );
        // One above fails, both values in range.
        set(&mut l, b, shared - 1);
        assert!(b.range().contains(shared - 1));
        let (_, cause) = rejected(l);
        assert_eq!(cause.limit(), a);
        assert_eq!(cause.violation(), Violation::Above(b));
    }
}

#[test]
fn core_config_first_failure_in_table_order_wins() {
    let mut l = ResourceLimits::default();
    set(&mut l, Limit::DiskL2Bytes, 0);
    set(&mut l, Limit::MaxCompressedRatio, 0);
    let (_, cause) = rejected(l);
    assert_eq!(cause.limit(), Limit::MaxCompressedRatio);
    // A range failure comes before a pair failure.
    let mut l = ResourceLimits::default();
    set(&mut l, Limit::TimeoutConnect, 20_000);
    set(&mut l, Limit::TimeoutTotal, 10_000);
    set(&mut l, Limit::DiskL2Bytes, u64::MAX);
    let (_, cause) = rejected(l);
    assert_eq!(cause.limit(), Limit::DiskL2Bytes);
}

#[test]
fn core_config_error_names_the_limit_never_the_value() {
    let value = 987_654_321_u64;
    let mut l = roomy();
    set(&mut l, Limit::MaxDocumentBytes, value);
    let (err, cause) = rejected(l);
    let texts = [
        err.to_string(),
        format!("{err:?}"),
        cause.to_string(),
        format!("{cause:?}"),
    ];
    for text in &texts {
        assert!(!text.contains("987654321"), "value in {text}");
        assert!(!text.contains("987_654_321"), "value in {text}");
    }
    assert_eq!(err.to_string(), "CONFIG_INVALID in engine_new");
    assert_eq!(
        cause.to_string(),
        "maxDocumentBytes is above its safe range"
    );

    let mut l = roomy();
    set(&mut l, Limit::MaxJsonDepth, 2);
    assert_eq!(
        rejected(l).1.to_string(),
        "maxJsonDepth is below its safe range"
    );

    let mut l = ResourceLimits::default();
    set(&mut l, Limit::MaxHeaderBytes, 32 * 1024);
    set(&mut l, Limit::MaxHeadersTotalBytes, 16 * 1024);
    assert_eq!(
        rejected(l).1.to_string(),
        "maxHeaderBytes is above maxHeadersTotalBytes"
    );
}

#[test]
fn core_config_of_ignores_other_errors() {
    use rivqen_model::context::Context;
    let plain = RivqenError::new(ErrorCode::ConfigInvalid, Context::new(CONFIG_OP));
    assert!(ConfigError::of(&plain).is_none());
}

#[test]
fn core_engine_is_send_and_sync() {
    fn check<T: Send + Sync>() {}
    check::<Engine>();
    check::<EngineConfig>();
}

/// The independent oracle: every value in its range and every pair kept.
fn valid(l: &ResourceLimits) -> bool {
    Limit::ALL.iter().all(|&k| k.range().contains(l.get(k)))
        && Limit::NOT_ABOVE.iter().all(|&(a, b)| l.get(a) <= l.get(b))
}

/// Any `u64`, with a bias to the ends of the ranges.
fn value() -> impl Strategy<Value = u64> {
    let edges: Vec<u64> = Limit::ALL
        .iter()
        .flat_map(|l| {
            let r = l.range();
            [r.min() - 1, r.min(), r.max(), r.max() + 1]
        })
        .chain([0, u64::from(u32::MAX), u64::MAX])
        .collect();
    prop_oneof![any::<u64>(), proptest::sample::select(edges)]
}

proptest! {
    #[test]
    fn core_config_prop_any_values(values in proptest::collection::vec(value(), 21)) {
        let mut l = ResourceLimits::default();
        for (&limit, &v) in Limit::ALL.iter().zip(&values) {
            set(&mut l, limit, v);
        }
        match Engine::new(EngineConfig::new(l)) {
            Ok(engine) => {
                prop_assert!(valid(&l));
                prop_assert_eq!(engine.config().limits, l);
            }
            Err(err) => {
                prop_assert!(!valid(&l));
                prop_assert_eq!(err.code(), ErrorCode::ConfigInvalid);
                let cause = ConfigError::of(&err).copied();
                prop_assert!(cause.is_some());
                let cause = cause.unwrap();
                let v = l.get(cause.limit());
                let r = cause.limit().range();
                match cause.violation() {
                    Violation::BelowMin => prop_assert!(v < r.min()),
                    Violation::AboveMax => prop_assert!(v > r.max()),
                    Violation::Above(b) => prop_assert!(v > l.get(b)),
                    _ => prop_assert!(false, "unknown violation"),
                }
            }
        }
    }
}
