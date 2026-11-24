// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The legacy session engine through its public API (WP-08 S2, T-01;
//! legacy client CL-05 to CL-37). The record functions are those of
//! `record.rs` (stubs until the record lane is integrated: no entry is
//! visible, every write is a removal); the paths with a visible entry are
//! unit tests of `legacy::fsm`, and the fixture run (T-05) checks the whole.
//!
//! Every effect of every test passes `check_writes`: each cache write names
//! the request partition (DL-220, RSK-06).
#![cfg(feature = "legacy")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::arithmetic_side_effects
)]

use rivqen_cache::key::{Partition, Protocol};
use rivqen_cache::policy::{EntryPolicy, Expires};
use rivqen_cache::txn::{
    EntryMeta, EntryVersion, Epoch, SnapshotBody, StoredEntry, TxnOp, Validators, Writer,
};
use rivqen_model::http::FieldLine;
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_model::safe::{PageBytes, Secret};
use rivqen_model::time::{Instant, UnixTime};
use rivqen_proto_legacy::body::{BodyLimits, DATA_BODY_DEPTH};
use rivqen_session::legacy::engine::LegacyEngine;
use rivqen_session::legacy::record::{self, NewRecord, WriteBasis};
use rivqen_session::legacy::types::{
    DEFAULT_UNAVAILABLE_TIME_MS, Effect, ExchangeId, Input, InterceptId, LegacyConfig,
    LookupResult, Now, OpenContext, Presentation, SessionId, TransportError, VaryKey,
};
use rivqen_template::legacy::{SplitOutcome, etag_v1, split_v1, template_tag_v1};
use rivqen_template::limits::TemplateLimits;

const PARTITION: PartitionId = PartitionId::from_bytes([7; 32]);
const OTHER: PartitionId = PartitionId::from_bytes([8; 32]);
const PARTITION_EPOCH: Epoch = Epoch(5);
const SITE_EPOCH: Epoch = Epoch(6);
const URL: &str = "https://shop.example.com/catalog";
const WALL: u64 = 1_800_000_000;

fn page() -> String {
    "<html><head><title>Shop</title></head><body><!--sonicdiff-a-->1<!--sonicdiff-a-end--></body></html>"
        .to_owned()
}

fn line(name: &str, value: &str) -> FieldLine {
    FieldLine::new(name, value)
}

fn legacy_head(text: &str) -> Vec<FieldLine> {
    let SplitOutcome::Applied(split) =
        split_v1(text.as_bytes(), &TemplateLimits::default()).unwrap()
    else {
        panic!("no split");
    };
    vec![
        line("content-type", "text/html; charset=utf-8"),
        line("etag", etag_v1(text.as_bytes()).as_str()),
        line("template-tag", template_tag_v1(&split.template).as_str()),
        line("template-change", "true"),
        line("cache-offline", "true"),
    ]
}

fn config(presentation: Presentation) -> LegacyConfig {
    LegacyConfig {
        presentation,
        accept_diff: true,
        authenticated_cache: false,
        unavailable_time_ms: DEFAULT_UNAVAILABLE_TIME_MS,
        template: TemplateLimits::default(),
        body: BodyLimits::new(1 << 20, DATA_BODY_DEPTH),
    }
}

fn context(url: &str, partition: PartitionId, network_valid: bool) -> OpenContext {
    OpenContext {
        url: rivqen_url::parse(url).unwrap(),
        partition,
        anonymous: false,
        network_valid,
        platform_fields: vec![line("accept-encoding", "gzip")],
        vary_key: VaryKey(Secret::new([1; 32])),
    }
}

/// An entry under the key; the record stub never shows it.
fn stored() -> StoredEntry {
    StoredEntry {
        meta: EntryMeta {
            key: CacheKey::from_bytes([2; 32]),
            partition: Partition::Scoped(PARTITION),
            host: "shop.example.com".to_owned(),
            protocol: Protocol::Legacy,
            vary: Vec::new(),
            validators: Validators::default(),
            policy: EntryPolicy {
                cache_control: rivqen_cache::header::CacheControl::default(),
                expires: Expires::Absent,
                date: None,
                age: None,
                request_time: UnixTime(WALL),
                response_time: UnixTime(WALL),
                ttl_seconds: None,
                offline: false,
            },
            stored_at: UnixTime(WALL),
            last_used_at: UnixTime(WALL),
        },
        version: EntryVersion(3),
        body: SnapshotBody {
            page: PageBytes::new(page().into_bytes()),
            template: None,
            data: Vec::new(),
            fields: Vec::new(),
        },
    }
}

fn lookup(entry: Option<StoredEntry>, disabled: bool) -> Input {
    Input::Lookup(Box::new(LookupResult {
        entry,
        disabled,
        partition_epoch: PARTITION_EPOCH,
        site_epoch: SITE_EPOCH,
    }))
}

/// DL-220: every cache write of a session names its request partition and
/// the epochs of its lookup.
fn check_writes(partition: PartitionId, effects: &[Effect]) {
    for effect in effects {
        let Effect::Cache(txn) = effect else {
            continue;
        };
        assert_eq!(txn.pre.writer, Writer::request(partition, PARTITION_EPOCH));
        assert_eq!(txn.pre.partition_epoch, PARTITION_EPOCH);
        assert_eq!(txn.pre.site_epoch, SITE_EPOCH);
        let target = match &txn.op {
            TxnOp::Replace { meta, .. } => meta.partition,
            TxnOp::UpdateFields { partition, .. }
            | TxnOp::Remove { partition, .. }
            | TxnOp::DisableKey { partition, .. } => *partition,
            other => panic!("a session never writes {other:?}"),
        };
        assert_eq!(target, Partition::Scoped(partition));
    }
}

struct Run {
    engine: LegacyEngine,
    mono: u64,
    partitions: Vec<(SessionId, PartitionId)>,
}

impl Run {
    fn new(presentation: Presentation) -> Self {
        Self {
            engine: LegacyEngine::new(config(presentation)),
            mono: 0,
            partitions: Vec::new(),
        }
    }

    fn now(&self) -> Now {
        Now {
            mono: Instant(self.mono),
            wall: UnixTime(WALL + self.mono / 1000),
        }
    }

    fn open(&mut self, context: OpenContext) -> (SessionId, Vec<Effect>) {
        let partition = context.partition;
        let (id, effects) = self.engine.open(self.now(), context);
        self.partitions.push((id, partition));
        check_writes(partition, &effects);
        (id, effects)
    }

    fn step(&mut self, id: SessionId, input: Input) -> Vec<Effect> {
        let effects = self.engine.handle(self.now(), id, input);
        let partition = self
            .partitions
            .iter()
            .find(|(session, _)| *session == id)
            .map_or(PARTITION, |(_, partition)| *partition);
        check_writes(partition, &effects);
        effects
    }

    /// Open a session at `URL` and send its request.
    fn start(&mut self, entry: Option<StoredEntry>) -> (SessionId, ExchangeId, Vec<Effect>) {
        let (id, mut effects) = self.open(context(URL, PARTITION, true));
        effects.extend(self.step(id, lookup(entry, false)));
        effects.extend(self.step(id, Input::Cookie(None)));
        let exchange = sent(&effects).expect("a request");
        (id, exchange, effects)
    }

    fn client(&mut self, id: SessionId) -> Vec<Effect> {
        let mut effects = self.step(id, Input::BindClient);
        effects.extend(self.step(id, Input::ClientReady));
        effects
    }

    fn respond(
        &mut self,
        id: SessionId,
        exchange: ExchangeId,
        status: u16,
        fields: Vec<FieldLine>,
        body: &[u8],
    ) -> Vec<Effect> {
        let mut effects = self.step(
            id,
            Input::ResponseHead {
                exchange,
                status,
                fields,
            },
        );
        if !body.is_empty() {
            effects.extend(self.step(
                id,
                Input::BodyChunk {
                    exchange,
                    bytes: body.to_vec(),
                },
            ));
        }
        effects.extend(self.step(id, Input::BodyEnd { exchange }));
        effects
    }
}

fn sent(effects: &[Effect]) -> Option<ExchangeId> {
    effects.iter().find_map(|effect| match effect {
        Effect::Send(spec) => Some(spec.exchange),
        _ => None,
    })
}

fn results(effects: &[Effect]) -> Vec<(i32, i32)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Result { src, final_code } => Some((src.value(), final_code.value())),
            _ => None,
        })
        .collect()
}

fn count(effects: &[Effect], wanted: &Effect) -> usize {
    effects.iter().filter(|effect| *effect == wanted).count()
}

fn writes(effects: &[Effect]) -> usize {
    effects
        .iter()
        .filter(|effect| matches!(effect, Effect::Cache(_)))
        .count()
}

#[derive(Debug, PartialEq, Eq)]
enum Answer {
    Null,
    Open(Vec<u8>),
    Doc(Vec<u8>, bool),
}

/// What `intercept` got in `effects`.
fn answer(effects: &[Effect], intercept: InterceptId) -> Option<Answer> {
    let mut found = None;
    let mut bytes = Vec::new();
    for effect in effects {
        match effect {
            Effect::AnswerNull(id) if *id == intercept => found = Some(Answer::Null),
            Effect::AnswerDocument {
                intercept: id,
                head,
            } if *id == intercept => {
                assert_eq!(head.status, 200);
                found = Some(Answer::Open(Vec::new()));
            }
            Effect::StreamChunk {
                intercept: id,
                bytes: chunk,
            } if *id == intercept => {
                bytes.extend_from_slice(chunk);
                found = Some(Answer::Open(bytes.clone()));
            }
            Effect::StreamEnd {
                intercept: id,
                complete,
            } if *id == intercept => found = Some(Answer::Doc(bytes.clone(), *complete)),
            _ => {}
        }
    }
    found
}

// --- open (CL-05, CL-07 to CL-11) ---

#[test]
fn legacy_engine_open_gives_lookup_then_cookie_then_send() {
    let mut run = Run::new(Presentation::Quick);
    let (id, effects) = run.open(context(
        "https://shop.example.com/catalog?b=2#top",
        PARTITION,
        true,
    ));
    let [Effect::Lookup { partition, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(*partition, Partition::Scoped(PARTITION));
    assert_eq!(run.step(id, lookup(None, false)), vec![Effect::ReadCookie]);
    let effects = run.step(id, Input::Cookie(Some("sid=1".to_owned())));
    let [Effect::Send(spec)] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(spec.target, "/catalog?b=2");
    assert_eq!(
        spec.fields,
        vec![line("accept-diff", "true"), line("cookie", "sid=1")]
    );
    // An answer that the session does not wait for changes nothing.
    assert_eq!(run.step(id, Input::Cookie(None)), Vec::new());
    assert_eq!(run.step(id, lookup(None, true)), Vec::new());
}

#[test]
fn legacy_engine_open_accept_diff_off_and_empty_cookie() {
    let mut config = config(Presentation::Standard);
    config.accept_diff = false;
    let mut engine = LegacyEngine::new(config);
    let now = Now {
        mono: Instant(0),
        wall: UnixTime(WALL),
    };
    let (id, _) = engine.open(now, context(URL, PARTITION, true));
    let _ = engine.handle(now, id, lookup(None, false));
    let effects = engine.handle(now, id, Input::Cookie(Some(String::new())));
    let [Effect::Send(spec)] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(spec.fields, vec![line("accept-diff", "false")]);
}

#[test]
fn legacy_engine_open_second_session_for_a_key_does_not_open() {
    let mut run = Run::new(Presentation::Standard);
    let (first, _) = run.open(context(URL, PARTITION, true));
    let (second, effects) = run.open(context(URL, PARTITION, true));
    assert_ne!(first, second);
    assert_eq!(effects, vec![Effect::Closed]);
    assert_eq!(run.step(second, Input::BindClient), Vec::new());
    // Another partition or another URL opens.
    let (_, effects) = run.open(context(URL, OTHER, true));
    assert!(matches!(effects.as_slice(), [Effect::Lookup { .. }]));
    let (_, effects) = run.open(context("https://shop.example.com/other", PARTITION, true));
    assert!(matches!(effects.as_slice(), [Effect::Lookup { .. }]));
    // The fragment is not part of the key.
    let (_, effects) = run.open(context(&format!("{URL}#x"), PARTITION, true));
    assert_eq!(effects, vec![Effect::Closed]);
    // After the destroy the key is free.
    let effects = run.step(first, Input::Destroy);
    assert_eq!(
        effects,
        vec![
            Effect::Result {
                src: rivqen_proto_legacy::message::ResultCode::NoResult,
                final_code: rivqen_proto_legacy::message::ResultCode::NoResult,
            },
            Effect::Closed
        ]
    );
    let (_, effects) = run.open(context(URL, PARTITION, true));
    assert!(matches!(effects.as_slice(), [Effect::Lookup { .. }]));
}

#[test]
fn legacy_engine_open_disabled_key_closes_with_no_other_effect() {
    let mut run = Run::new(Presentation::Quick);
    let (id, _) = run.open(context(URL, PARTITION, true));
    assert_eq!(
        run.step(id, lookup(Some(stored()), true)),
        vec![Effect::Closed]
    );
    assert_eq!(run.step(id, Input::ClientReady), Vec::new());
    assert_eq!(run.step(id, Input::Destroy), Vec::new());
    // The key is free: the session did not open.
    let (_, effects) = run.open(context(URL, PARTITION, true));
    assert!(matches!(effects.as_slice(), [Effect::Lookup { .. }]));
}

#[test]
fn legacy_engine_open_without_network_sends_nothing_and_loads_normally() {
    for presentation in [Presentation::Quick, Presentation::Standard] {
        let mut run = Run::new(presentation);
        let (id, _) = run.open(context(URL, PARTITION, false));
        let mut effects = run.step(id, lookup(None, false));
        assert_eq!(effects, Vec::new());
        effects = run.client(id);
        assert_eq!(effects, vec![Effect::LoadUrl]);
        effects = run.step(id, Input::Intercept(InterceptId(1)));
        assert_eq!(effects, vec![Effect::AnswerNull(InterceptId(1))]);
        // Refresh keeps the network state of the open (CL-10).
        effects = run.step(id, Input::Refresh);
        assert!(matches!(effects.as_slice(), [Effect::Lookup { .. }]));
        effects = run.step(id, lookup(None, false));
        assert!(!effects.contains(&Effect::ReadCookie));
        assert_eq!(sent(&effects), None);
        effects = run.step(id, Input::Destroy);
        assert_eq!(results(&effects), vec![(-1, -1)]);
    }
}

#[test]
fn legacy_engine_open_damaged_entry_is_removed() {
    let mut run = Run::new(Presentation::Quick);
    let (_, _, effects) = run.start(Some(stored()));
    // The record stub sees no visible entry and calls it damaged (CL-40).
    let removals: Vec<_> = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Cache(txn) => Some(txn),
            _ => None,
        })
        .collect();
    assert_eq!(removals.len(), 1);
    assert!(matches!(removals[0].op, TxnOp::Remove { .. }));
    assert_eq!(
        removals[0].pre.entry,
        rivqen_cache::txn::Expect::Version(EntryVersion(3))
    );
}

/// The entry that a commit of `page()` for `URL` in `PARTITION` stores.
fn committed_entry() -> StoredEntry {
    let text = page();
    let SplitOutcome::Applied(split) =
        split_v1(text.as_bytes(), &TemplateLimits::default()).unwrap()
    else {
        panic!("no split");
    };
    let basis = WriteBasis {
        partition: PARTITION,
        key: record::entry_key(PARTITION, &rivqen_url::parse(URL).unwrap()),
        partition_epoch: PARTITION_EPOCH,
        site_epoch: SITE_EPOCH,
        expect: rivqen_cache::txn::Expect::Absent,
    };
    let txn = record::commit(
        &basis,
        &NewRecord {
            host: "shop.example.com",
            vary: &[],
            page: &text,
            template: &split.template,
            data: &split.data,
            fields: &[line("content-type", "text/html; charset=utf-8")],
            etag: etag_v1(text.as_bytes()),
            template_tag: template_tag_v1(&split.template),
            response_fields: &[],
            request_time: UnixTime(WALL),
            response_time: UnixTime(WALL),
        },
    );
    let TxnOp::Replace { meta, body } = txn.op else {
        panic!("a commit is a replace");
    };
    StoredEntry {
        meta: *meta,
        version: EntryVersion(4),
        body: *body,
    }
}

#[test]
fn legacy_engine_open_entry_of_another_partition_or_key_is_damaged() {
    // DL-229 (f): the session uses only an entry of its own partition and
    // key; another one is damage, removed, and the request has no
    // validators.
    let mut run = Run::new(Presentation::Quick);
    let (_, _, effects) = run.start(Some(committed_entry()));
    assert_eq!(writes(&effects), 0);
    let Some(Effect::Send(spec)) = effects.last() else {
        panic!("{effects:?}");
    };
    assert_eq!(spec.fields.len(), 3, "the validators of the visible entry");

    let mut other_partition = committed_entry();
    other_partition.meta.partition = Partition::Scoped(OTHER);
    let mut other_key = committed_entry();
    other_key.meta.key = CacheKey::from_bytes([9; 32]);
    for entry in [other_partition, other_key] {
        let mut run = Run::new(Presentation::Quick);
        let (_, _, effects) = run.start(Some(entry));
        let removals: Vec<_> = effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Cache(txn) => Some(txn),
                _ => None,
            })
            .collect();
        assert_eq!(removals.len(), 1);
        assert!(matches!(removals[0].op, TxnOp::Remove { .. }));
        let Some(Effect::Send(spec)) = effects.last() else {
            panic!("{effects:?}");
        };
        assert_eq!(spec.fields, vec![line("accept-diff", "true")]);
    }
}

#[test]
fn legacy_engine_open_cookie_with_a_control_character_is_a_normal_load() {
    let mut run = Run::new(Presentation::Quick);
    let (id, _) = run.open(context(URL, PARTITION, true));
    let _ = run.step(id, lookup(None, false));
    let effects = run.step(id, Input::Cookie(Some("a=1\r\nx: y".to_owned())));
    assert_eq!(sent(&effects), None);
    assert_eq!(run.client(id), vec![Effect::LoadUrl]);
    let effects = run.step(id, Input::Intercept(InterceptId(1)));
    assert_eq!(effects, vec![Effect::AnswerNull(InterceptId(1))]);
}

// --- classification (CL-11, CL-13 to CL-19, CL-32, CL-44) ---

#[test]
fn legacy_engine_classify_cookies_go_first_for_every_status() {
    for status in [200, 304, 302, 404, 500] {
        let mut run = Run::new(Presentation::Quick);
        let (id, exchange, _) = run.start(None);
        let effects = run.step(
            id,
            Input::ResponseHead {
                exchange,
                status,
                fields: vec![
                    line("Set-Cookie", "a=1"),
                    line("x", "y"),
                    line("set-cookie2", "b=2"),
                ],
            },
        );
        assert_eq!(
            effects.first(),
            Some(&Effect::StoreCookies(vec![
                "a=1".to_owned(),
                "b=2".to_owned()
            ])),
            "{status}"
        );
    }
}

fn cookie_head(exchange: ExchangeId, value: &str) -> Input {
    Input::ResponseHead {
        exchange,
        status: 500,
        fields: vec![line("set-cookie", value)],
    }
}

fn stored_cookies(value: &str) -> Effect {
    Effect::StoreCookies(vec![value.to_owned()])
}

#[test]
fn legacy_engine_classify_cookies_once_per_exchange() {
    // DL-229 (e): a second head of an exchange gives no effect.
    let mut run = Run::new(Presentation::Quick);
    let (id, exchange, _) = run.start(None);
    let effects = run.step(id, cookie_head(exchange, "a=1"));
    assert_eq!(count(&effects, &stored_cookies("a=1")), 1);
    assert_eq!(run.step(id, cookie_head(exchange, "b=2")), Vec::new());

    // A head after the end of the exchange gives no effect.
    let mut run = Run::new(Presentation::Quick);
    let (id, exchange, _) = run.start(None);
    let _ = run.step(
        id,
        Input::TransportFailed {
            exchange,
            error: TransportError::Read,
        },
    );
    assert_eq!(run.step(id, cookie_head(exchange, "a=1")), Vec::new());
}

#[test]
fn legacy_engine_classify_cookies_of_an_older_generation_once() {
    // CL-11 item 2: a head of an older generation stores its cookies once,
    // also after the session dropped that exchange (DL-229 (e)).
    let mut run = Run::new(Presentation::Quick);
    let (id, first, _) = run.start(None);
    let _ = run.step(id, Input::Refresh);
    let _ = run.step(id, lookup(None, false));
    let second = sent(&run.step(id, Input::Cookie(None))).unwrap();
    assert_ne!(first, second);
    assert_eq!(
        run.step(id, cookie_head(first, "old=1")),
        vec![stored_cookies("old=1")]
    );
    assert_eq!(run.step(id, cookie_head(first, "old=2")), Vec::new());
    // The current exchange still stores its own cookies.
    let effects = run.step(id, cookie_head(second, "new=1"));
    assert_eq!(effects.first(), Some(&stored_cookies("new=1")));
}

#[test]
fn legacy_engine_classify_cookies_of_dropped_exchanges_are_bounded() {
    // The session remembers at most 16 dropped exchanges whose head is due;
    // the oldest one is forgotten first.
    let mut run = Run::new(Presentation::Quick);
    let (id, first, _) = run.start(None);
    let mut exchanges = vec![first];
    for _ in 0..17 {
        let _ = run.step(id, Input::Refresh);
        let _ = run.step(id, lookup(None, false));
        exchanges.push(sent(&run.step(id, Input::Cookie(None))).unwrap());
    }
    // 17 exchanges were dropped before their head; the first one is gone.
    assert_eq!(run.step(id, cookie_head(exchanges[0], "a=0")), Vec::new());
    for exchange in &exchanges[1..17] {
        assert_eq!(
            run.step(id, cookie_head(*exchange, "a=1")),
            vec![stored_cookies("a=1")]
        );
    }
}

#[test]
fn legacy_engine_classify_normal_loads_without_an_entry() {
    let text = page();
    let mut not_legacy = legacy_head(&text);
    not_legacy.retain(|line| line.name != "cache-offline");
    let mut two_types = legacy_head(&text);
    two_types.push(line("Content-Type", "text/html"));
    let mut latin = legacy_head(&text);
    latin[0] = line("content-type", "text/html; charset=ISO-8859-1");
    let mut bad_key = legacy_head(&text);
    bad_key.push(line("sonic-etag-key", "content-security-policy"));
    let mut http = legacy_head(&text);
    http[4] = line("cache-offline", "http");
    for (status, fields) in [
        (304, vec![]),
        (302, vec![line("location", "/x")]),
        (404, vec![]),
        (500, vec![]),
        (200, not_legacy),
        (200, two_types),
        (200, latin),
        (200, bad_key),
        (200, http),
    ] {
        let mut run = Run::new(Presentation::Standard);
        let (id, exchange, _) = run.start(None);
        let mut effects = run.step(id, Input::Intercept(InterceptId(9)));
        assert_eq!(effects, Vec::new(), "the intercept waits for the head");
        effects = run.respond(id, exchange, status, fields, text.as_bytes());
        assert_eq!(
            answer(&effects, InterceptId(9)),
            Some(Answer::Null),
            "{status}"
        );
        assert_eq!(results(&effects), Vec::<(i32, i32)>::new());
        effects = run.client(id);
        assert_eq!(effects, vec![Effect::LoadUrl]);
        effects = run.step(id, Input::Destroy);
        assert_eq!(results(&effects), vec![(-1, -1)]);
    }
}

#[test]
fn legacy_engine_classify_http_disables_the_key() {
    let text = page();
    let mut fields = legacy_head(&text);
    fields[4] = line("cache-offline", "HTTP");
    fields.push(line("cache-control", "no-store"));
    let mut run = Run::new(Presentation::Quick);
    let (id, exchange, _) = run.start(None);
    let effects = run.respond(id, exchange, 200, fields, text.as_bytes());
    // One write (record::disable; the stub gives a removal until the
    // record lane is integrated).
    assert_eq!(writes(&effects), 1);
    assert_eq!(results(&effects), Vec::<(i32, i32)>::new());
}

#[test]
fn legacy_engine_classify_empty_body_is_a_normal_load() {
    // CL-32: the waiting intercept gets null, also after the head.
    let text = page();
    let mut run = Run::new(Presentation::Standard);
    let (id, exchange, _) = run.start(None);
    let mut effects = run.client(id);
    effects.extend(run.step(id, Input::Intercept(InterceptId(1))));
    effects.extend(run.respond(id, exchange, 200, legacy_head(&text), b""));
    assert_eq!(answer(&effects, InterceptId(1)), Some(Answer::Null));
    assert_eq!(results(&effects), Vec::<(i32, i32)>::new());
}

#[test]
fn legacy_engine_classify_body_that_is_not_utf8_is_a_normal_load() {
    let text = page();
    // Without a stream: null, no result.
    let mut run = Run::new(Presentation::Standard);
    let (id, exchange, _) = run.start(None);
    let mut effects = run.step(id, Input::Intercept(InterceptId(1)));
    effects.extend(run.respond(id, exchange, 200, legacy_head(&text), b"\xff\xfe"));
    assert_eq!(answer(&effects, InterceptId(1)), Some(Answer::Null));
    assert_eq!(results(&effects), Vec::<(i32, i32)>::new());
    // A stream gets only the valid prefix, then ends incomplete; then a
    // normal load (CL-16 found at the body, as CL-33).
    let mut run = Run::new(Presentation::Standard);
    let (id, exchange, _) = run.start(None);
    let mut effects = run.client(id);
    effects.extend(run.step(id, Input::Intercept(InterceptId(2))));
    effects.extend(run.step(
        id,
        Input::ResponseHead {
            exchange,
            status: 200,
            fields: legacy_head(&text),
        },
    ));
    effects.extend(run.step(
        id,
        Input::BodyChunk {
            exchange,
            bytes: b"ab\xc3".to_vec(),
        },
    ));
    assert_eq!(
        answer(&effects, InterceptId(2)),
        Some(Answer::Open(b"ab".to_vec()))
    );
    effects.extend(run.step(
        id,
        Input::BodyChunk {
            exchange,
            bytes: b"(".to_vec(),
        },
    ));
    // DL-229 (b): the incomplete tail and the invalid byte never reach the
    // stream.
    assert_eq!(
        answer(&effects, InterceptId(2)),
        Some(Answer::Doc(b"ab".to_vec(), false))
    );
    assert_eq!(count(&effects, &Effect::LoadUrl), 2);
    assert_eq!(run.step(id, Input::BodyEnd { exchange }), Vec::new());
}

/// The stream effects of `intercept` in `effects`, in order.
fn stream_of(effects: &[Effect], intercept: InterceptId) -> Vec<Effect> {
    effects
        .iter()
        .filter(|effect| {
            matches!(effect,
                Effect::StreamChunk { intercept: id, .. } | Effect::StreamEnd { intercept: id, .. }
                    if *id == intercept)
        })
        .cloned()
        .collect()
}

#[test]
fn legacy_engine_classify_failed_body_streams_only_the_valid_prefix() {
    // DL-229 (b): a chunk with an invalid byte, a cut and a body over the
    // limit give a stream only the bytes of the valid UTF-8 prefix: never
    // the invalid bytes and never an incomplete tail.
    let text = page();
    // Over every document limit (the cap is 32 MiB).
    let over = vec![b'a'; 33 << 20];
    let cases: [(&[&[u8]], bool); 4] = [
        (&[b"\xff<script>x</script>"], false),
        (&[b"\xc3", b"(<script>x</script>"], false),
        (&[b"\xc3"], true),
        (&[b"\xc3", &over], false),
    ];
    for (chunks, cut) in cases {
        let mut run = Run::new(Presentation::Standard);
        let (id, exchange, _) = run.start(None);
        let mut effects = run.client(id);
        effects.extend(run.step(id, Input::Intercept(InterceptId(1))));
        effects.extend(run.step(
            id,
            Input::ResponseHead {
                exchange,
                status: 200,
                fields: legacy_head(&text),
            },
        ));
        for bytes in [b"<html>".as_slice()].iter().chain(chunks) {
            effects.extend(run.step(
                id,
                Input::BodyChunk {
                    exchange,
                    bytes: bytes.to_vec(),
                },
            ));
        }
        if cut {
            effects.extend(run.step(
                id,
                Input::TransportFailed {
                    exchange,
                    error: TransportError::Read,
                },
            ));
        }
        assert_eq!(
            stream_of(&effects, InterceptId(1)),
            vec![
                Effect::StreamChunk {
                    intercept: InterceptId(1),
                    bytes: b"<html>".to_vec()
                },
                Effect::StreamEnd {
                    intercept: InterceptId(1),
                    complete: false
                }
            ],
            "cut: {cut}, chunks: {}",
            chunks.len()
        );
        assert_eq!(effects.last(), Some(&Effect::LoadUrl));
        assert_eq!(writes(&effects), 0);
    }
}

#[test]
fn legacy_engine_classify_old_generation_end_runs_the_end_checks_standard() {
    // DL-229 (c): a stream of an older generation whose body ends with an
    // incomplete UTF-8 sequence ends incomplete, then `load_url` (CL-16 as
    // CL-33); no cache effect (CL-35).
    let text = page();
    let mut run = Run::new(Presentation::Standard);
    let (id, exchange, _) = run.start(None);
    let mut effects = run.client(id);
    effects.extend(run.step(id, Input::Intercept(InterceptId(1))));
    effects.extend(run.step(
        id,
        Input::ResponseHead {
            exchange,
            status: 200,
            fields: legacy_head(&text),
        },
    ));
    effects.extend(run.step(
        id,
        Input::BodyChunk {
            exchange,
            bytes: b"<html>\xc3".to_vec(),
        },
    ));
    assert_eq!(
        answer(&effects, InterceptId(1)),
        Some(Answer::Open(b"<html>".to_vec()))
    );
    effects = run.step(id, Input::Refresh);
    assert!(matches!(effects.as_slice(), [Effect::Lookup { .. }]));
    effects = run.step(id, Input::BodyEnd { exchange });
    assert_eq!(
        effects,
        vec![
            Effect::StreamEnd {
                intercept: InterceptId(1),
                complete: false
            },
            Effect::LoadUrl
        ]
    );
    // The next intercept waits for the head of the current generation.
    assert_eq!(run.step(id, Input::Intercept(InterceptId(2))), Vec::new());
}

#[test]
fn legacy_engine_classify_old_generation_end_runs_the_end_checks_quick() {
    // DL-229 (c): an empty body (CL-32) or a body that is not UTF-8 (CL-16)
    // of an older generation is never a complete document: Quick gives no
    // `load_data` of it at `client_ready`.
    let text = page();
    for body in [b"".as_slice(), b"<html>\xc3"] {
        let mut run = Run::new(Presentation::Quick);
        let (id, exchange, _) = run.start(None);
        let mut effects = run.step(
            id,
            Input::ResponseHead {
                exchange,
                status: 200,
                fields: legacy_head(&text),
            },
        );
        if !body.is_empty() {
            effects.extend(run.step(
                id,
                Input::BodyChunk {
                    exchange,
                    bytes: body.to_vec(),
                },
            ));
        }
        assert_eq!(effects, Vec::new());
        let effects = run.step(id, Input::Refresh);
        assert!(matches!(effects.as_slice(), [Effect::Lookup { .. }]));
        assert_eq!(run.step(id, Input::BodyEnd { exchange }), Vec::new());
        assert_eq!(run.client(id), vec![Effect::LoadUrl], "{} bytes", body.len());
        assert_eq!(run.step(id, Input::Intercept(InterceptId(1))), Vec::new());
        // The current generation gives the document.
        let mut effects = run.step(id, lookup(None, false));
        effects.extend(run.step(id, Input::Cookie(None)));
        let second = sent(&effects).unwrap();
        effects = run.respond(id, second, 200, legacy_head(&text), text.as_bytes());
        assert_eq!(
            answer(&effects, InterceptId(1)),
            Some(Answer::Doc(text.as_bytes().to_vec(), true))
        );
    }
}

// --- delivery and codes (CL-20 to CL-24) ---

#[test]
fn legacy_engine_delivery_no_web_view_call_before_bind_and_ready() {
    let text = page();
    let mut run = Run::new(Presentation::Quick);
    let (id, exchange, _) = run.start(None);
    let mut effects = run.step(id, Input::BindClient);
    effects.extend(run.respond(id, exchange, 200, legacy_head(&text), text.as_bytes()));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::LoadUrl | Effect::LoadData { .. }))
    );
    // Quick: the complete document at client_ready; 1000 / 304 (CL-22).
    assert_eq!(results(&effects), vec![(1000, 304)]);
    effects = run.step(id, Input::ClientReady);
    let [Effect::LoadData { head, body }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(body.as_slice(), text.as_bytes());
    assert_eq!(
        head.fields,
        vec![line("content-type", "text/html; charset=utf-8")]
    );
    // CL-20 item 5: the intercept after load_data gets null.
    effects = run.step(id, Input::Intercept(InterceptId(1)));
    assert_eq!(effects, vec![Effect::AnswerNull(InterceptId(1))]);
    assert_eq!(run.step(id, Input::ClientReady), Vec::new());
}

#[test]
fn legacy_engine_delivery_quick_streams_a_document_that_is_not_complete() {
    // CL-20 items 2 and 4: Quick calls load_url when the document is not
    // complete at client_ready; the intercept waits for the head, then
    // streams; the codes are 1000 / 1000 (the first document was not
    // complete at the delivery).
    let text = page();
    let mut run = Run::new(Presentation::Quick);
    let (id, exchange, _) = run.start(None);
    let mut effects = run.client(id);
    assert_eq!(effects, vec![Effect::LoadUrl]);
    effects = run.step(id, Input::Intercept(InterceptId(1)));
    assert_eq!(effects, Vec::new());
    effects = run.step(
        id,
        Input::ResponseHead {
            exchange,
            status: 200,
            fields: legacy_head(&text),
        },
    );
    assert_eq!(
        effects,
        Vec::new(),
        "the intercept waits for the first byte"
    );
    let bytes = text.as_bytes();
    effects = run.step(
        id,
        Input::BodyChunk {
            exchange,
            bytes: bytes[..7].to_vec(),
        },
    );
    effects.extend(run.step(
        id,
        Input::BodyChunk {
            exchange,
            bytes: bytes[7..].to_vec(),
        },
    ));
    effects.extend(run.step(id, Input::BodyEnd { exchange }));
    assert_eq!(
        answer(&effects, InterceptId(1)),
        Some(Answer::Doc(bytes.to_vec(), true))
    );
    assert_eq!(results(&effects), vec![(1000, 1000)]);
    // A second intercept gets null.
    effects = run.step(id, Input::Intercept(InterceptId(2)));
    assert_eq!(effects, vec![Effect::AnswerNull(InterceptId(2))]);
}

#[test]
fn legacy_engine_delivery_standard_gives_a_complete_document_in_one_list() {
    let text = page();
    let mut run = Run::new(Presentation::Standard);
    let (id, exchange, _) = run.start(None);
    let mut effects = run.respond(id, exchange, 200, legacy_head(&text), text.as_bytes());
    effects.extend(run.client(id));
    assert_eq!(count(&effects, &Effect::LoadUrl), 1);
    let effects = run.step(id, Input::Intercept(InterceptId(5)));
    assert_eq!(effects.len(), 3);
    assert_eq!(
        answer(&effects, InterceptId(5)),
        Some(Answer::Doc(text.into_bytes(), true))
    );
}

#[test]
fn legacy_engine_delivery_body_over_the_limit_is_a_body_failure() {
    let text = page();
    let mut config = config(Presentation::Standard);
    config.template = TemplateLimits::new(16, 8, 1024, 64, 64);
    let mut engine = LegacyEngine::new(config);
    let now = Now {
        mono: Instant(0),
        wall: UnixTime(WALL),
    };
    let (id, _) = engine.open(now, context(URL, PARTITION, true));
    let _ = engine.handle(now, id, lookup(None, false));
    let exchange = sent(&engine.handle(now, id, Input::Cookie(None))).unwrap();
    let mut effects = engine.handle(now, id, Input::BindClient);
    effects.extend(engine.handle(now, id, Input::ClientReady));
    effects.extend(engine.handle(now, id, Input::Intercept(InterceptId(1))));
    effects.extend(engine.handle(
        now,
        id,
        Input::ResponseHead {
            exchange,
            status: 200,
            fields: legacy_head(&text),
        },
    ));
    effects.extend(engine.handle(
        now,
        id,
        Input::BodyChunk {
            exchange,
            bytes: text.as_bytes()[..10].to_vec(),
        },
    ));
    effects.extend(engine.handle(
        now,
        id,
        Input::BodyChunk {
            exchange,
            bytes: text.as_bytes()[10..].to_vec(),
        },
    ));
    assert_eq!(
        answer(&effects, InterceptId(1)),
        Some(Answer::Doc(text.as_bytes()[..10].to_vec(), false))
    );
    assert_eq!(count(&effects, &Effect::LoadUrl), 2);
    assert_eq!(
        engine.handle(now, id, Input::BodyEnd { exchange }),
        Vec::new()
    );
}

#[test]
fn legacy_engine_codes_message_after_get_diff_and_never_for_minus_one() {
    let text = page();
    let mut run = Run::new(Presentation::Quick);
    let (id, exchange, _) = run.start(None);
    assert_eq!(run.step(id, Input::GetDiff), Vec::new());
    let effects = run.respond(id, exchange, 200, legacy_head(&text), text.as_bytes());
    let messages: Vec<&Effect> = effects
        .iter()
        .filter(|effect| matches!(effect, Effect::Message(_)))
        .collect();
    let etag = etag_v1(text.as_bytes());
    let tag = &legacy_head(&text)[2].value;
    assert_eq!(
        messages,
        vec![&Effect::Message(format!(
            "{{\"code\":304,\"srcCode\":1000,\"extra\":{{\"eTag\":\"{}\",\"template-tag\":\"{tag}\",\"cache-offline\":\"true\",\"isReload\":\"false\"}}}}",
            etag.as_str()
        ))]
    );
    // A normal load: no message.
    let mut run = Run::new(Presentation::Quick);
    let (id, exchange, _) = run.start(None);
    let mut effects = run.step(id, Input::GetDiff);
    effects.extend(run.respond(id, exchange, 404, vec![], b""));
    effects.extend(run.step(id, Input::GetDiff));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Message(_)))
    );
}

// --- outcomes (CL-33) ---

#[test]
fn legacy_engine_outcome_cut_stream_ends_incomplete_then_load_url() {
    let text = page();
    for error in [TransportError::Read, TransportError::Connection] {
        let mut run = Run::new(Presentation::Standard);
        let (id, exchange, _) = run.start(None);
        let mut effects = run.client(id);
        effects.extend(run.step(id, Input::Intercept(InterceptId(1))));
        effects.extend(run.step(
            id,
            Input::ResponseHead {
                exchange,
                status: 200,
                fields: legacy_head(&text),
            },
        ));
        effects.extend(run.step(
            id,
            Input::BodyChunk {
                exchange,
                bytes: text.as_bytes()[..20].to_vec(),
            },
        ));
        effects.extend(run.step(id, Input::TransportFailed { exchange, error }));
        assert_eq!(
            answer(&effects, InterceptId(1)),
            Some(Answer::Doc(text.as_bytes()[..20].to_vec(), false))
        );
        assert_eq!(count(&effects, &Effect::LoadUrl), 2);
        assert_eq!(writes(&effects), 0);
        effects = run.step(id, Input::Intercept(InterceptId(2)));
        assert_eq!(effects, vec![Effect::AnswerNull(InterceptId(2))]);
        effects = run.step(id, Input::Destroy);
        assert_eq!(results(&effects), vec![(-1, -1)]);
    }
}

#[test]
fn legacy_engine_outcome_transport_failure_before_the_head_without_an_entry() {
    for error in [
        TransportError::Refused,
        TransportError::Connection,
        TransportError::Tls,
        TransportError::Read,
    ] {
        let mut run = Run::new(Presentation::Quick);
        let (id, exchange, _) = run.start(None);
        let mut effects = run.step(id, Input::Intercept(InterceptId(1)));
        effects.extend(run.step(id, Input::TransportFailed { exchange, error }));
        assert_eq!(effects, vec![Effect::AnswerNull(InterceptId(1))]);
        assert_eq!(run.client(id), vec![Effect::LoadUrl]);
    }
}

// --- stop (CL-06, CL-34, CL-45) ---

#[test]
fn legacy_engine_stop_destroy_answers_the_waiting_intercept_and_closes() {
    let mut run = Run::new(Presentation::Standard);
    let (id, exchange, _) = run.start(None);
    let _ = run.client(id);
    let _ = run.step(id, Input::Intercept(InterceptId(1)));
    let effects = run.step(id, Input::Destroy);
    assert_eq!(
        effects,
        vec![
            Effect::Result {
                src: rivqen_proto_legacy::message::ResultCode::NoResult,
                final_code: rivqen_proto_legacy::message::ResultCode::NoResult,
            },
            Effect::AnswerNull(InterceptId(1)),
            Effect::Closed
        ]
    );
    // No effect after Closed, also for a late response (CL-11 item 4).
    for input in [
        Input::ResponseHead {
            exchange,
            status: 200,
            fields: vec![line("set-cookie", "a=1")],
        },
        Input::Intercept(InterceptId(2)),
        Input::GetDiff,
        Input::Refresh,
        Input::Destroy,
    ] {
        assert_eq!(run.step(id, input), Vec::new());
    }
}

#[test]
fn legacy_engine_stop_unknown_and_stale_inputs_give_no_effect() {
    let mut run = Run::new(Presentation::Quick);
    assert_eq!(run.step(SessionId(77), Input::ClientReady), Vec::new());
    let (id, exchange, _) = run.start(None);
    let unknown = ExchangeId(exchange.0 + 1);
    for input in [
        Input::ResponseHead {
            exchange: unknown,
            status: 200,
            fields: vec![line("set-cookie", "a=1")],
        },
        Input::BodyChunk {
            exchange: unknown,
            bytes: b"x".to_vec(),
        },
        Input::BodyEnd { exchange: unknown },
        Input::TransportFailed {
            exchange: unknown,
            error: TransportError::Read,
        },
        Input::BodyChunk {
            exchange,
            bytes: b"before the head".to_vec(),
        },
        Input::Tick,
        Input::PageFinished,
    ] {
        assert_eq!(run.step(id, input), Vec::new());
    }
}

#[test]
fn legacy_engine_stop_partition_changed_revokes_and_stops_the_sessions() {
    let text = page();
    let mut run = Run::new(Presentation::Standard);
    let (first, exchange, _) = run.start(None);
    let (other, _) = run.open(context("https://shop.example.com/b", OTHER, true));
    let (second, _) = run.open(context("https://shop.example.com/c", PARTITION, true));
    let _ = run.client(first);
    let _ = run.step(first, Input::Intercept(InterceptId(1)));
    let _ = run.step(
        first,
        Input::ResponseHead {
            exchange,
            status: 200,
            fields: legacy_head(&text),
        },
    );
    let streamed = run.step(
        first,
        Input::BodyChunk {
            exchange,
            bytes: b"<html>".to_vec(),
        },
    );
    assert_eq!(
        answer(&streamed, InterceptId(1)),
        Some(Answer::Open(b"<html>".to_vec()))
    );
    let now = run.now();
    let effects = run.engine.partition_changed(now, PARTITION);
    let (Some(named), Effect::Cache(txn)) = &effects[0] else {
        panic!("{effects:?}");
    };
    assert_eq!(*named, first);
    assert_eq!(
        txn.op,
        TxnOp::RevokePartition {
            partition: Partition::Scoped(PARTITION)
        }
    );
    assert_eq!(txn.pre.writer, Writer::Maintenance);
    let of = |id: SessionId| -> Vec<Effect> {
        effects
            .iter()
            .filter(|(session, _)| *session == Some(id))
            .map(|(_, effect)| effect.clone())
            .collect()
    };
    let first_effects = of(first);
    assert_eq!(
        answer(&first_effects, InterceptId(1)),
        Some(Answer::Doc(Vec::new(), false))
    );
    assert_eq!(first_effects.last(), Some(&Effect::Closed));
    assert_eq!(of(second).last(), Some(&Effect::Closed));
    assert_eq!(of(other), Vec::new());
    assert_eq!(
        effects
            .iter()
            .filter(|(_, effect)| matches!(effect, Effect::Cache(_)))
            .count(),
        1
    );
    // Nothing after the stop; the key stays taken until the destroy (CL-07).
    assert_eq!(
        run.step(
            first,
            Input::BodyChunk {
                exchange,
                bytes: b"x".to_vec()
            }
        ),
        Vec::new()
    );
    let (_, effects) = run.open(context(URL, PARTITION, true));
    assert_eq!(effects, vec![Effect::Closed]);
    assert_eq!(run.step(first, Input::Destroy), Vec::new());
    let (_, effects) = run.open(context(URL, PARTITION, true));
    assert!(matches!(effects.as_slice(), [Effect::Lookup { .. }]));
    // The other partition still works.
    assert_eq!(
        run.step(other, lookup(None, false)),
        vec![Effect::ReadCookie]
    );
}

fn lookup_at(partition_epoch: Epoch, site_epoch: Epoch) -> Input {
    Input::Lookup(Box::new(LookupResult {
        entry: None,
        disabled: false,
        partition_epoch,
        site_epoch,
    }))
}

#[test]
fn legacy_engine_stop_refresh_lookup_with_other_epochs_stops_without_a_write() {
    // DL-229 (a), RSK-06: a revoke or a site clear that the engine did not
    // see is never healed by a refresh. `Run::step` checks that every write
    // names the epochs of the opening lookup.
    let text = page();
    for (partition_epoch, site_epoch) in [
        (Epoch(9), SITE_EPOCH),
        (PARTITION_EPOCH, Epoch(9)),
        (Epoch(9), Epoch(9)),
    ] {
        let mut run = Run::new(Presentation::Standard);
        let (id, first, _) = run.start(None);
        let mut effects = run.client(id);
        effects.extend(run.respond(id, first, 200, legacy_head(&text), text.as_bytes()));
        assert_eq!(writes(&effects), 1, "the first load commits");
        effects = run.step(id, Input::Refresh);
        assert!(matches!(effects.as_slice(), [Effect::Lookup { .. }]));
        effects = run.step(id, lookup_at(partition_epoch, site_epoch));
        assert_eq!(
            effects,
            vec![
                Effect::Result {
                    src: rivqen_proto_legacy::message::ResultCode::NoResult,
                    final_code: rivqen_proto_legacy::message::ResultCode::NoResult,
                },
                Effect::Closed
            ]
        );
        // No cookie read, no request, no write after the stop.
        assert_eq!(run.step(id, Input::Cookie(None)), Vec::new());
        assert_eq!(
            run.respond(id, ExchangeId(2), 200, legacy_head(&text), text.as_bytes()),
            Vec::new()
        );
        // The key stays taken until the destroy (CL-07).
        let (_, effects) = run.open(context(URL, PARTITION, true));
        assert_eq!(effects, vec![Effect::Closed]);
        assert_eq!(run.step(id, Input::Destroy), Vec::new());
        let (_, effects) = run.open(context(URL, PARTITION, true));
        assert!(matches!(effects.as_slice(), [Effect::Lookup { .. }]));
    }
}

#[test]
fn legacy_engine_stop_refresh_with_other_epochs_ends_an_old_stream() {
    // CL-06: an open stream ends incomplete, then `Closed`.
    let text = page();
    let mut run = Run::new(Presentation::Standard);
    let (id, exchange, _) = run.start(None);
    let mut effects = run.client(id);
    effects.extend(run.step(id, Input::Intercept(InterceptId(1))));
    effects.extend(run.step(
        id,
        Input::ResponseHead {
            exchange,
            status: 200,
            fields: legacy_head(&text),
        },
    ));
    effects.extend(run.step(
        id,
        Input::BodyChunk {
            exchange,
            bytes: b"<html>".to_vec(),
        },
    ));
    assert_eq!(
        answer(&effects, InterceptId(1)),
        Some(Answer::Open(b"<html>".to_vec()))
    );
    let _ = run.step(id, Input::Refresh);
    let effects = run.step(id, lookup_at(Epoch(9), SITE_EPOCH));
    assert_eq!(
        effects,
        vec![
            Effect::Result {
                src: rivqen_proto_legacy::message::ResultCode::NoResult,
                final_code: rivqen_proto_legacy::message::ResultCode::NoResult,
            },
            Effect::StreamEnd {
                intercept: InterceptId(1),
                complete: false
            },
            Effect::Closed
        ]
    );
    assert_eq!(run.step(id, Input::BodyEnd { exchange }), Vec::new());
}

#[test]
fn legacy_engine_stop_refresh_lookup_with_the_same_epochs_goes_on() {
    let mut run = Run::new(Presentation::Quick);
    let (id, _, _) = run.start(None);
    assert!(matches!(
        run.step(id, Input::Refresh).as_slice(),
        [Effect::Lookup { .. }]
    ));
    assert_eq!(
        run.step(id, lookup_at(PARTITION_EPOCH, SITE_EPOCH)),
        vec![Effect::ReadCookie]
    );
    assert!(sent(&run.step(id, Input::Cookie(None))).is_some());
}

#[test]
fn legacy_engine_stop_partition_changed_without_a_session() {
    let mut engine = LegacyEngine::new(config(Presentation::Quick));
    let now = Now {
        mono: Instant(0),
        wall: UnixTime(WALL),
    };
    let effects = engine.partition_changed(now, PARTITION);
    assert_eq!(effects.len(), 1);
    assert!(matches!(
        &effects[0],
        (None, Effect::Cache(txn)) if txn.op == TxnOp::RevokePartition { partition: Partition::Scoped(PARTITION) }
    ));
}

#[test]
fn legacy_engine_writes_name_the_request_partition() {
    // Writes of two partitions in one engine: each names its own (the
    // check is in `Run::step`), and the epochs of its own lookup.
    let mut run = Run::new(Presentation::Quick);
    let mut total = 0;
    for partition in [PARTITION, OTHER] {
        let (id, _) = run.open(context(URL, partition, true));
        let mut effects = run.step(id, lookup(Some(stored()), false));
        effects.extend(run.step(id, Input::Cookie(None)));
        let exchange = sent(&effects).unwrap();
        let mut http = legacy_head(&page());
        http[4] = line("cache-offline", "http");
        effects.extend(run.respond(id, exchange, 200, http, page().as_bytes()));
        total += writes(&effects);
    }
    assert_eq!(total, 4);
}
