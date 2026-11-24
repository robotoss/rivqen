// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Properties of the legacy session engine (WP-08 S2, T-01): for random
//! sequences of opens, inputs and partition changes over several sessions,
//! the engine never panics, gives no effect for a session after its
//! `Closed`, never uses the timers of S3, names the request partition in
//! every session write (DL-220), and gives the same effects for the same
//! inputs.
#![cfg(feature = "legacy")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::arithmetic_side_effects
)]

use std::collections::BTreeSet;

use proptest::prelude::*;
use rivqen_cache::key::Partition;
use rivqen_cache::txn::{Epoch, TxnOp, Writer};
use rivqen_model::http::FieldLine;
use rivqen_model::ids::PartitionId;
use rivqen_model::safe::Secret;
use rivqen_model::time::{Instant, UnixTime};
use rivqen_proto_legacy::body::{BodyLimits, DATA_BODY_DEPTH};
use rivqen_session::legacy::engine::LegacyEngine;
use rivqen_session::legacy::types::{
    DEFAULT_UNAVAILABLE_TIME_MS, Effect, ExchangeId, Input, InterceptId, LegacyConfig,
    LookupResult, Now, OpenContext, Presentation, SessionId, TransportError, VaryKey,
};
use rivqen_template::legacy::{SplitOutcome, etag_v1, split_v1, template_tag_v1};
use rivqen_template::limits::TemplateLimits;

const PARTITIONS: [PartitionId; 2] = [
    PartitionId::from_bytes([1; 32]),
    PartitionId::from_bytes([2; 32]),
];
const URLS: [&str; 2] = [
    "https://shop.example.com/a",
    "https://shop.example.com/b?x=1",
];
const EPOCH: Epoch = Epoch(9);

fn page() -> &'static str {
    "<html><head><title>T</title></head><body><!--sonicdiff-k-->v<!--sonicdiff-k-end--></body></html>"
}

fn line(name: &str, value: &str) -> FieldLine {
    FieldLine::new(name, value)
}

fn legacy_head() -> Vec<FieldLine> {
    let SplitOutcome::Applied(split) =
        split_v1(page().as_bytes(), &TemplateLimits::default()).unwrap()
    else {
        panic!("no split");
    };
    vec![
        line("content-type", "text/html; charset=utf-8"),
        line("etag", etag_v1(page().as_bytes()).as_str()),
        line("template-tag", template_tag_v1(&split.template).as_str()),
        line("template-change", "true"),
        line("cache-offline", "true"),
    ]
}

#[derive(Debug, Clone)]
enum Step {
    Open {
        url: usize,
        partition: usize,
        network_valid: bool,
    },
    Input {
        session: u64,
        input: Input,
    },
    PartitionChanged(usize),
}

fn input() -> impl Strategy<Value = Input> {
    let exchange = (0_u64..4).prop_map(ExchangeId);
    let heads = vec![
        legacy_head(),
        vec![line("set-cookie", "a=1"), line("cache-offline", "http")],
        vec![line("connection", "x-frame-options")],
        vec![],
    ];
    let bodies = vec![page().as_bytes().to_vec(), b"\xf0\x9f".to_vec(), Vec::new()];
    prop_oneof![
        any::<bool>().prop_map(|disabled| Input::Lookup(Box::new(LookupResult {
            entry: None,
            disabled,
            partition_epoch: EPOCH,
            site_epoch: EPOCH,
        }))),
        prop::option::of("[a-z=;\r]{0,5}").prop_map(Input::Cookie),
        (
            exchange.clone(),
            prop::sample::select(vec![200_u16, 304, 301, 403, 503]),
            prop::sample::select(heads)
        )
            .prop_map(|(exchange, status, fields)| Input::ResponseHead {
                exchange,
                status,
                fields
            }),
        (exchange.clone(), prop::sample::select(bodies))
            .prop_map(|(exchange, bytes)| Input::BodyChunk { exchange, bytes }),
        exchange
            .clone()
            .prop_map(|exchange| Input::BodyEnd { exchange }),
        (
            exchange,
            prop::sample::select(vec![TransportError::Tls, TransportError::Read])
        )
            .prop_map(|(exchange, error)| Input::TransportFailed { exchange, error }),
        Just(Input::BindClient),
        Just(Input::ClientReady),
        (0_u64..3).prop_map(|id| Input::Intercept(InterceptId(id))),
        Just(Input::GetDiff),
        Just(Input::PageFinished),
        Just(Input::Refresh),
        Just(Input::Tick),
        Just(Input::Destroy),
    ]
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        1 => (0_usize..2, 0_usize..2, any::<bool>()).prop_map(|(url, partition, network_valid)| {
            Step::Open { url, partition, network_valid }
        }),
        8 => (0_u64..4, input()).prop_map(|(session, input)| Step::Input { session, input }),
        1 => (0_usize..2).prop_map(Step::PartitionChanged),
    ]
}

fn config(quick: bool) -> LegacyConfig {
    LegacyConfig {
        presentation: if quick {
            Presentation::Quick
        } else {
            Presentation::Standard
        },
        accept_diff: true,
        authenticated_cache: false,
        unavailable_time_ms: DEFAULT_UNAVAILABLE_TIME_MS,
        template: TemplateLimits::default(),
        body: BodyLimits::new(4096, DATA_BODY_DEPTH),
    }
}

/// Checks of one list of effects of `session` (of `partition`).
fn check(
    session: SessionId,
    partition: Option<PartitionId>,
    effects: &[Effect],
    closed: &mut BTreeSet<SessionId>,
) {
    if closed.contains(&session) {
        assert_eq!(effects, &[] as &[Effect], "an effect after Closed");
    }
    for (index, effect) in effects.iter().enumerate() {
        assert!(!matches!(effect, Effect::ScheduleTimer(_)));
        if *effect == Effect::Closed {
            assert_eq!(index + 1, effects.len(), "Closed is the last effect");
            closed.insert(session);
        }
        if let Effect::Cache(txn) = effect {
            let partition = partition.unwrap();
            assert_eq!(txn.pre.writer, Writer::request(partition, EPOCH));
            assert!(!matches!(
                txn.op,
                TxnOp::RevokePartition { .. } | TxnOp::ClearSite { .. }
            ));
            if let TxnOp::Remove {
                partition: target, ..
            }
            | TxnOp::DisableKey {
                partition: target, ..
            } = txn.op
            {
                assert_eq!(target, Partition::Scoped(partition));
            }
        }
    }
}

/// Run `steps`; return every effect in order.
fn run(quick: bool, steps: &[Step]) -> Vec<(Option<SessionId>, Effect)> {
    let mut engine = LegacyEngine::new(config(quick));
    let mut partitions: Vec<(SessionId, PartitionId)> = Vec::new();
    let mut closed = BTreeSet::new();
    let mut all = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        let now = Now {
            mono: Instant(u64::try_from(index).unwrap() * 997),
            wall: UnixTime(1_800_000_000 + u64::try_from(index).unwrap()),
        };
        match step {
            Step::Open {
                url,
                partition,
                network_valid,
            } => {
                let context = OpenContext {
                    url: rivqen_url::parse(URLS[*url]).unwrap(),
                    partition: PARTITIONS[*partition],
                    anonymous: false,
                    network_valid: *network_valid,
                    platform_fields: Vec::new(),
                    vary_key: VaryKey(Secret::new([3; 32])),
                };
                let (id, effects) = engine.open(now, context);
                assert!(!closed.contains(&id) && partitions.iter().all(|(s, _)| *s != id));
                partitions.push((id, PARTITIONS[*partition]));
                check(id, Some(PARTITIONS[*partition]), &effects, &mut closed);
                all.extend(effects.into_iter().map(|effect| (Some(id), effect)));
            }
            Step::Input { session, input } => {
                let id = SessionId(*session);
                let partition = partitions.iter().find(|(s, _)| *s == id).map(|(_, p)| *p);
                let effects = engine.handle(now, id, input.clone());
                if partition.is_none() {
                    assert_eq!(effects, Vec::new(), "an unknown session");
                }
                check(id, partition, &effects, &mut closed);
                all.extend(effects.into_iter().map(|effect| (Some(id), effect)));
            }
            Step::PartitionChanged(partition) => {
                let old = PARTITIONS[*partition];
                let effects = engine.partition_changed(now, old);
                let (named, revoke) = &effects[0];
                let Effect::Cache(txn) = revoke else {
                    panic!("{revoke:?}");
                };
                assert_eq!(
                    txn.op,
                    TxnOp::RevokePartition {
                        partition: Partition::Scoped(old)
                    }
                );
                if let Some(first) = named {
                    assert!(!closed.contains(first));
                }
                let mut sessions: Vec<SessionId> =
                    effects[1..].iter().filter_map(|(s, _)| *s).collect();
                sessions.dedup();
                for id in sessions {
                    let mine: Vec<Effect> = effects[1..]
                        .iter()
                        .filter(|(s, _)| *s == Some(id))
                        .map(|(_, e)| e.clone())
                        .collect();
                    let partition = partitions.iter().find(|(s, _)| *s == id).map(|(_, p)| *p);
                    assert_eq!(partition, Some(old));
                    assert_eq!(mine.last(), Some(&Effect::Closed));
                    check(id, partition, &mine, &mut closed);
                }
                all.extend(effects);
            }
        }
    }
    all
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn legacy_engine_prop_no_panic_no_effect_after_closed_and_deterministic(
        quick in any::<bool>(),
        steps in prop::collection::vec(step(), 0..60),
    ) {
        let first = run(quick, &steps);
        let second = run(quick, &steps);
        prop_assert_eq!(first, second);
    }
}
