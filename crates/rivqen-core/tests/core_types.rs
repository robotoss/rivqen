// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! No public type of the facade prints a planted marker or a configured value
//! (ERR-06, errors-logging section 4; the pattern of rivqen-model
//! `model_debug`). The placeholder payloads of later work packages have no public constructor; their
//! `Debug` is checked inside the crate (`core_action_placeholders_*`,
//! `core_event_placeholders_*`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use rivqen_cache::key::Partition;
use rivqen_cache::memory::PressureLevel;
use rivqen_cache::txn::{
    CacheTransaction, EntryVersion, Epoch, Expect, Preconditions, StoredEntry, TxnOp,
};
use rivqen_core::action::{Action, CacheKey, CacheTxn, DocumentRef, Outcome};
use rivqen_core::config::{ConfigError, Engine, EngineConfig, Violation};
use rivqen_core::event::{Event, Snapshot};
use rivqen_model::ids::CacheKey as ModelCacheKey;
use rivqen_model::limits::{Limit, ResourceLimits};
use rivqen_model::time::Instant;
use rivqen_testkit::store::contract::sample_entry;

const MARKER: &str = "PLANTEDMARKER";
/// A configured value; it must not appear in an error text.
const PLANTED_VALUE: u64 = 4_242_424_242;

fn assert_clean(what: &str, text: &str) {
    assert!(!text.contains(MARKER), "{what} leaks the marker: {text}");
    assert!(
        !text.contains(&PLANTED_VALUE.to_string()),
        "{what} leaks the configured value: {text}"
    );
}

#[test]
fn core_types_cache_txn_debug_names_the_kind() {
    let key = ModelCacheKey::from_bytes([9; 32]);
    let (meta, body) = sample_entry(Partition::Public, key, "example.com", "tag", 7);
    let pre = Preconditions {
        entry: Expect::Any,
        partition_epoch: Epoch(0),
        site_epoch: Epoch(0),
    };
    let partition = Partition::Public;
    let ops = [
        (
            "Replace",
            TxnOp::Replace {
                meta: Box::new(meta.clone()),
                body: Box::new(body),
            },
        ),
        (
            "UpdateFields",
            TxnOp::UpdateFields {
                partition,
                key,
                fields: Vec::new(),
                policy: meta.policy,
            },
        ),
        ("Remove", TxnOp::Remove { partition, key }),
        (
            "DisableKey",
            TxnOp::DisableKey {
                partition,
                key,
                until: rivqen_model::time::UnixTime(1),
            },
        ),
        ("RevokePartition", TxnOp::RevokePartition { partition }),
        ("ClearSite", TxnOp::ClearSite { domain: None }),
    ];
    for (kind, op) in ops {
        let text = format!("{:?}", CacheTxn::new(CacheTransaction { op, pre }));
        assert_eq!(text, format!("CacheTxn {{ op: {kind}, .. }}"));
    }
}

#[test]
fn core_types_debug_has_no_content() {
    let key = ModelCacheKey::from_bytes([9; 32]);
    let (meta, body) = sample_entry(Partition::Public, key, MARKER, MARKER, 7);
    let pre = Preconditions {
        entry: Expect::Any,
        partition_epoch: Epoch(0),
        site_epoch: Epoch(0),
    };
    let txn = |op| CacheTxn::new(CacheTransaction { op, pre });
    let ops = [
        TxnOp::Replace {
            meta: Box::new(meta.clone()),
            body: Box::new(body.clone()),
        },
        TxnOp::UpdateFields {
            partition: Partition::Public,
            key,
            fields: vec![rivqen_model::http::FieldLine::new("x-planted", MARKER)],
            policy: meta.policy,
        },
        TxnOp::ClearSite {
            domain: Some(MARKER.to_owned()),
        },
        TxnOp::Remove {
            partition: Partition::Public,
            key,
        },
    ];
    let snapshot = Snapshot::new(StoredEntry {
        meta,
        version: EntryVersion(1),
        body,
    });
    let mut cache_debug: Vec<(&str, String)> = vec![
        ("Snapshot", format!("{snapshot:?}")),
        (
            "Event",
            format!("{:?}", Event::CacheResult(Some(snapshot.clone()))),
        ),
        (
            "Event",
            format!("{:?}", Event::MemoryPressure(PressureLevel::Moderate)),
        ),
        (
            "Action",
            format!(
                "{:?}",
                Action::ShowCachedDocument(DocumentRef::new(
                    CacheKey::new(Partition::Public, key),
                    EntryVersion(1)
                ))
            ),
        ),
        (
            "Action",
            format!(
                "{:?}",
                Action::InvalidateCache(CacheKey::new(Partition::Public, key))
            ),
        ),
    ];
    for op in ops {
        cache_debug.push(("Action", format!("{:?}", Action::CommitCache(txn(op)))));
    }
    for (what, text) in &cache_debug {
        assert_clean(what, text);
    }
    assert!(cache_debug[0].1.contains("size_bytes"));
}

#[test]
fn core_types_debug_has_no_markers() {
    let engine = Engine::new(EngineConfig::default()).unwrap();
    let limits = ResourceLimits {
        timeout_total_ms: PLANTED_VALUE,
        ..ResourceLimits::default()
    };
    let err = Engine::new(EngineConfig::new(limits)).unwrap_err();
    let cause = *ConfigError::of(&err).unwrap();

    let mut debug: Vec<(&str, String)> = vec![
        ("Engine", format!("{engine:?}")),
        ("EngineConfig", format!("{:?}", EngineConfig::default())),
        ("RivqenError", format!("{err:?}")),
        ("ConfigError", format!("{cause:?}")),
        (
            "Violation",
            format!("{:?}", Violation::Above(Limit::TimeoutTotal)),
        ),
        ("Outcome", format!("{:?}", Outcome::FirstLoad)),
        ("Outcome", format!("{:?}", Outcome::CacheHitNotModified)),
        ("Outcome", format!("{:?}", Outcome::DataUpdated)),
        ("Outcome", format!("{:?}", Outcome::TemplateChanged)),
        ("Outcome", format!("{:?}", Outcome::ServedStale)),
    ];
    for event in [
        Event::WebViewReady,
        Event::CacheResult(None),
        Event::HttpCompleted,
        Event::BridgeReady,
        Event::PageRendered,
        Event::NavigationCancelled,
        Event::PartitionChanged,
        Event::Tick(Instant(1)),
    ] {
        debug.push(("Event", format!("{event:?}")));
    }
    for (what, text) in &debug {
        assert_clean(what, text);
    }

    let display = [
        ("RivqenError", err.to_string()),
        ("ConfigError", cause.to_string()),
    ];
    for (what, text) in &display {
        assert_clean(what, text);
    }
    assert_eq!(cause.limit(), Limit::TimeoutTotal);
}
