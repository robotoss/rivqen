// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The legacy session engine through its public API (WP-08 S2, T-01;
//! legacy client CL-05 to CL-37). The record functions are those of
//! `record.rs`; a visible entry comes from `record::commit`. More paths with
//! a visible entry are unit tests of `legacy::fsm`, and the fixture run
//! (`legacy_client_fixtures_pass`) checks the whole.
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

use rivqen_cache::key::{Partition, Protocol, VaryEntry};
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
use rivqen_session::legacy::record::{self, NewRecord, RequestValues, WriteBasis};
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

/// An entry under the key that `record::commit` did not write: the record
/// never shows it (CL-40).
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

    fn head(&mut self, id: SessionId, exchange: ExchangeId, fields: Vec<FieldLine>) -> Vec<Effect> {
        self.step(
            id,
            Input::ResponseHead {
                exchange,
                status: 200,
                fields,
            },
        )
    }

    fn chunk(&mut self, id: SessionId, exchange: ExchangeId, bytes: &[u8]) -> Vec<Effect> {
        self.step(
            id,
            Input::BodyChunk {
                exchange,
                bytes: bytes.to_vec(),
            },
        )
    }

    /// CL-35: a refresh with no entry in the store; returns the exchange of
    /// the new request.
    fn refresh(&mut self, id: SessionId) -> ExchangeId {
        assert!(matches!(
            self.step(id, Input::Refresh).as_slice(),
            [Effect::Lookup { .. }]
        ));
        assert_eq!(self.step(id, lookup(None, false)), vec![Effect::ReadCookie]);
        let effects = self.step(id, Input::Cookie(None));
        let [Effect::Send(spec)] = effects.as_slice() else {
            panic!("{effects:?}");
        };
        spec.exchange
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
    // The record finds an entry that the session cannot have written and
    // calls it damaged (CL-40).
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
    committed_entry_with(&[])
}

/// [`committed_entry`] with the `Vary` entries `vary`.
fn committed_entry_with(vary: &[VaryEntry]) -> StoredEntry {
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
            vary,
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
    // One write: record::disable (CL-19).
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
        assert_eq!(
            run.client(id),
            vec![Effect::LoadUrl],
            "{} bytes",
            body.len()
        );
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
fn legacy_engine_stop_stopped_sessions_do_not_count_in_the_session_bound() {
    // DL-229 (h): 256 sessions that `partition_changed` stopped and the app
    // never destroyed do not block a new session; each still holds its key
    // until its `Destroy` (CL-07).
    let mut run = Run::new(Presentation::Quick);
    let mut first = None;
    for n in 0..256 {
        let (id, effects) = run.open(context(&format!("{URL}/{n}"), PARTITION, true));
        assert!(matches!(effects.as_slice(), [Effect::Lookup { .. }]), "{n}");
        first.get_or_insert(id);
    }
    let (_, effects) = run.open(context(&format!("{URL}/256"), PARTITION, true));
    assert_eq!(effects, vec![Effect::Closed], "256 open sessions");
    let now = run.now();
    let _ = run.engine.partition_changed(now, PARTITION);
    let (_, effects) = run.open(context(&format!("{URL}/256"), PARTITION, true));
    assert!(matches!(effects.as_slice(), [Effect::Lookup { .. }]));
    // A stopped session holds its key until its `Destroy`.
    let (_, effects) = run.open(context(&format!("{URL}/0"), PARTITION, true));
    assert_eq!(effects, vec![Effect::Closed]);
    assert_eq!(run.step(first.unwrap(), Input::Destroy), Vec::new());
    let (_, effects) = run.open(context(&format!("{URL}/0"), PARTITION, true));
    assert!(matches!(effects.as_slice(), [Effect::Lookup { .. }]));
}

#[test]
fn legacy_engine_stop_full_set_of_held_keys_fails_closed() {
    // DL-229 (h): at 1024 keys of stopped sessions that are not destroyed,
    // a new session does not open (`Closed`) until a `Destroy`.
    let mut run = Run::new(Presentation::Quick);
    let mut stopped = Vec::new();
    for round in 0..4 {
        for n in 0..256 {
            let (id, effects) = run.open(context(&format!("{URL}/{round}/{n}"), PARTITION, true));
            assert!(matches!(effects.as_slice(), [Effect::Lookup { .. }]));
            stopped.push(id);
        }
        let now = run.now();
        let _ = run.engine.partition_changed(now, PARTITION);
    }
    let (_, effects) = run.open(context(&format!("{URL}/x"), OTHER, true));
    assert_eq!(effects, vec![Effect::Closed]);
    assert_eq!(run.step(stopped[0], Input::Destroy), Vec::new());
    let (_, effects) = run.open(context(&format!("{URL}/x"), OTHER, true));
    assert!(matches!(effects.as_slice(), [Effect::Lookup { .. }]));
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

// --- delivery of a body while it arrives (CL-20 items 4 and 7) ---

/// A page with another template than [`page`].
fn other_page() -> String {
    "<html><head><title>New</title></head><body><!--sonicdiff-a-->2<!--sonicdiff-a-end--></body></html>"
        .to_owned()
}

fn stream_chunk(intercept: u64, bytes: &[u8]) -> Effect {
    Effect::StreamChunk {
        intercept: InterceptId(intercept),
        bytes: bytes.to_vec(),
    }
}

fn stream_end(intercept: u64, complete: bool) -> Effect {
    Effect::StreamEnd {
        intercept: InterceptId(intercept),
        complete,
    }
}

#[test]
fn legacy_engine_delivery_stream_gets_each_chunk_as_it_arrives() {
    // CL-20 item 4: a stream gets the body bytes while they arrive, not
    // only at the end of the body.
    let text = page();
    let bytes = text.as_bytes();
    let mut run = Run::new(Presentation::Standard);
    let (id, exchange, _) = run.start(None);
    assert_eq!(run.client(id), vec![Effect::LoadUrl]);
    assert_eq!(run.step(id, Input::Intercept(InterceptId(1))), Vec::new());
    assert_eq!(run.head(id, exchange, legacy_head(&text)), Vec::new());
    let effects = run.chunk(id, exchange, &bytes[..10]);
    assert_eq!(
        answer(&effects, InterceptId(1)),
        Some(Answer::Open(bytes[..10].to_vec()))
    );
    assert_eq!(
        run.chunk(id, exchange, &bytes[10..20]),
        vec![stream_chunk(1, &bytes[10..20])]
    );
    assert_eq!(
        run.chunk(id, exchange, &bytes[20..]),
        vec![stream_chunk(1, &bytes[20..])]
    );
    let effects = run.step(id, Input::BodyEnd { exchange });
    assert_eq!(
        stream_of(&effects, InterceptId(1)),
        vec![stream_end(1, true)]
    );
}

#[test]
fn legacy_engine_delivery_no_web_view_call_with_only_bind_or_only_ready() {
    // CL-20 item 7: with only `bind_client` or only `client_ready`, a cut
    // stream (CL-33) gives no `load_url`; the second call decides.
    let text = page();
    for (first, second) in [
        (Input::BindClient, Input::ClientReady),
        (Input::ClientReady, Input::BindClient),
    ] {
        let mut run = Run::new(Presentation::Standard);
        let (id, exchange, _) = run.start(None);
        assert_eq!(run.step(id, first), Vec::new());
        assert_eq!(run.step(id, Input::Intercept(InterceptId(1))), Vec::new());
        assert_eq!(run.head(id, exchange, legacy_head(&text)), Vec::new());
        let effects = run.chunk(id, exchange, &text.as_bytes()[..10]);
        assert_eq!(
            answer(&effects, InterceptId(1)),
            Some(Answer::Open(text.as_bytes()[..10].to_vec()))
        );
        let effects = run.step(
            id,
            Input::TransportFailed {
                exchange,
                error: TransportError::Read,
            },
        );
        assert_eq!(effects, vec![stream_end(1, false)]);
        assert_eq!(run.step(id, second), vec![Effect::LoadUrl]);
        assert_eq!(
            run.step(id, Input::Intercept(InterceptId(2))),
            vec![Effect::AnswerNull(InterceptId(2))]
        );
    }
}

#[test]
fn legacy_engine_delivery_template_change_reload_waits_for_client_ready() {
    // CL-20 item 7 with CL-31: the cached page went to an intercept before
    // `client_ready`; the reload of the new document waits for the client.
    let new = other_page();
    for presentation in [Presentation::Quick, Presentation::Standard] {
        let mut run = Run::new(presentation);
        let (id, exchange, _) = run.start(Some(committed_entry()));
        assert_eq!(run.step(id, Input::BindClient), Vec::new());
        let effects = run.step(id, Input::Intercept(InterceptId(1)));
        assert_eq!(
            answer(&effects, InterceptId(1)),
            Some(Answer::Doc(page().into_bytes(), true))
        );
        let effects = run.respond(id, exchange, 200, legacy_head(&new), new.as_bytes());
        assert_eq!(results(&effects), vec![(2000, 2000)]);
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::LoadUrl | Effect::LoadData { .. })),
            "{presentation:?}: {effects:?}"
        );
        let effects = run.step(id, Input::ClientReady);
        if presentation == Presentation::Quick {
            let [Effect::LoadData { body, .. }] = effects.as_slice() else {
                panic!("{effects:?}");
            };
            assert_eq!(body.as_slice(), new.as_bytes());
        } else {
            assert_eq!(effects, vec![Effect::LoadUrl]);
            let effects = run.step(id, Input::Intercept(InterceptId(2)));
            assert_eq!(
                answer(&effects, InterceptId(2)),
                Some(Answer::Doc(new.clone().into_bytes(), true))
            );
        }
    }
}

// --- generations and their exchanges (CL-35) ---

#[test]
fn legacy_engine_delivery_after_a_refresh_the_old_document_is_still_given() {
    // CL-35: until the new generation has an outcome, the document of the
    // old generation is the current document, and the session keeps the
    // exchange that holds its body.
    let text = page();
    let mut run = Run::new(Presentation::Standard);
    let (id, first, _) = run.start(None);
    let effects = run.respond(id, first, 200, legacy_head(&text), text.as_bytes());
    assert_eq!(results(&effects), vec![(1000, 304)]);
    let _ = run.refresh(id);
    assert_eq!(run.client(id), vec![Effect::LoadUrl]);
    let effects = run.step(id, Input::Intercept(InterceptId(1)));
    assert_eq!(
        answer(&effects, InterceptId(1)),
        Some(Answer::Doc(text.into_bytes(), true))
    );
}

/// Generations 0 and 1 of a session with an open stream each: intercept 1
/// (before the client) streams the first load of generation 0, and
/// intercept 2 (after `load_url`) streams the first load of generation 1.
/// Both bodies are open with `<html>`.
fn two_open_streams(run: &mut Run) -> (SessionId, ExchangeId, ExchangeId) {
    let text = page();
    let (id, first, _) = run.start(None);
    assert_eq!(run.step(id, Input::Intercept(InterceptId(1))), Vec::new());
    assert_eq!(run.head(id, first, legacy_head(&text)), Vec::new());
    let effects = run.chunk(id, first, b"<html>");
    assert_eq!(
        answer(&effects, InterceptId(1)),
        Some(Answer::Open(b"<html>".to_vec()))
    );
    let second = run.refresh(id);
    assert_eq!(run.head(id, second, legacy_head(&text)), Vec::new());
    assert_eq!(run.client(id), vec![Effect::LoadUrl]);
    assert_eq!(run.step(id, Input::Intercept(InterceptId(2))), Vec::new());
    let effects = run.chunk(id, second, b"<html>");
    assert_eq!(
        answer(&effects, InterceptId(2)),
        Some(Answer::Open(b"<html>".to_vec()))
    );
    (id, first, second)
}

#[test]
fn legacy_engine_delivery_older_streams_go_on_while_an_intercept_waits() {
    // CL-35 with CL-20 item 4: the stream of an older generation gets the
    // rest of its body; an intercept that waits for the head of the current
    // generation gets no answer from an older body.
    let text = page();
    let mut run = Run::new(Presentation::Standard);
    let (id, first, second) = two_open_streams(&mut run);
    let third = run.refresh(id);
    // The body of generation 1 is not UTF-8 (CL-16): its stream ends
    // incomplete, then `load_url`; the session has no document now.
    assert_eq!(
        run.chunk(id, second, b"\xff"),
        vec![stream_end(2, false), Effect::LoadUrl]
    );
    assert_eq!(run.step(id, Input::Intercept(InterceptId(3))), Vec::new());
    // The body of generation 0 goes on: only its own stream gets it.
    assert_eq!(
        run.chunk(id, first, b"<body>"),
        vec![stream_chunk(1, b"<body>")]
    );
    assert_eq!(
        run.step(id, Input::BodyEnd { exchange: first }),
        vec![stream_end(1, true)]
    );
    // The first load of generation 2 answers intercept 3.
    let effects = run.respond(id, third, 200, legacy_head(&text), text.as_bytes());
    assert_eq!(
        answer(&effects, InterceptId(3)),
        Some(Answer::Doc(text.into_bytes(), true))
    );
}

#[test]
fn legacy_engine_delivery_four_exchanges_stay_open() {
    // The session keeps four exchanges (its bound): here two older
    // streams, the document of generation 2 and the current request. None
    // of them ends.
    let text = page();
    let mut run = Run::new(Presentation::Standard);
    let (id, first, second) = two_open_streams(&mut run);
    let third = run.refresh(id);
    assert_eq!(run.head(id, third, legacy_head(&text)), Vec::new());
    // `refresh` checks that the request of generation 3 is the only effect.
    let _ = run.refresh(id);
    assert_eq!(
        run.chunk(id, first, b"<body>"),
        vec![stream_chunk(1, b"<body>")]
    );
    assert_eq!(
        run.chunk(id, second, b"<body>"),
        vec![stream_chunk(2, b"<body>")]
    );
}

// --- codes (CL-22) ---

#[test]
fn legacy_engine_codes_first_load_after_a_refresh_is_a_new_document() {
    // CL-22, CL-35: after a refresh the document of the old generation
    // counts as delivered, so the first load of the new generation is
    // another document: 1000 / 1000.
    let text = page();
    let mut run = Run::new(Presentation::Standard);
    let (id, first, _) = run.start(None);
    let effects = run.respond(id, first, 200, legacy_head(&text), text.as_bytes());
    assert_eq!(results(&effects), vec![(1000, 304)]);
    let second = run.refresh(id);
    let effects = run.respond(id, second, 200, legacy_head(&text), text.as_bytes());
    assert_eq!(results(&effects), vec![(1000, 1000)]);
}

// --- body checks (CL-01, CL-16, CL-33) ---

#[test]
fn legacy_engine_classify_chunk_after_the_end_changes_nothing() {
    // CL-01: the body ends once; a later chunk is not part of the document.
    let text = page();
    let mut run = Run::new(Presentation::Standard);
    let (id, exchange, _) = run.start(None);
    let _ = run.respond(id, exchange, 200, legacy_head(&text), text.as_bytes());
    assert_eq!(
        run.chunk(id, exchange, b"<script>late</script>"),
        Vec::new()
    );
    assert_eq!(run.client(id), vec![Effect::LoadUrl]);
    let effects = run.step(id, Input::Intercept(InterceptId(1)));
    assert_eq!(
        answer(&effects, InterceptId(1)),
        Some(Answer::Doc(text.into_bytes(), true))
    );
}

#[test]
fn legacy_engine_classify_template_change_body_not_utf8_removes_the_entry() {
    // CL-16 found at the body is a normal load as CL-14: the entry is
    // removed and the next intercept gets `null`. (A cut body, CL-33,
    // keeps the cached page and the entry.)
    let new = other_page();
    let mut run = Run::new(Presentation::Standard);
    let (id, exchange, _) = run.start(Some(committed_entry()));
    let mut effects = run.head(id, exchange, legacy_head(&new));
    effects.extend(run.chunk(id, exchange, b"<html>\xff"));
    let [Effect::Cache(txn)] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert!(matches!(txn.op, TxnOp::Remove { .. }), "{txn:?}");
    assert_eq!(
        txn.pre.entry,
        rivqen_cache::txn::Expect::Version(EntryVersion(4))
    );
    assert_eq!(run.client(id), vec![Effect::LoadUrl]);
    assert_eq!(
        run.step(id, Input::Intercept(InterceptId(1))),
        vec![Effect::AnswerNull(InterceptId(1))]
    );
    let effects = run.step(id, Input::Destroy);
    assert_eq!(results(&effects), vec![(-1, -1)]);
}

// --- the `Vary` of the entry (CL-12, CL-17) ---

/// The `Vary` entries of `vary` for a request with the session fields
/// `session` and the platform fields and the vary key of [`context`].
fn vary_of(vary: &str, session: &[FieldLine]) -> Vec<VaryEntry> {
    let platform = [line("accept-encoding", "gzip")];
    record::vary_entries(
        &[line("vary", vary)],
        &RequestValues {
            session_fields: session,
            platform_fields: &platform,
        },
        &VaryKey(Secret::new([1; 32])),
    )
    .unwrap()
}

#[test]
fn legacy_engine_open_vary_match_reads_the_cookie() {
    // CL-12: the `Vary` match at the open uses the cookie of the request:
    // the entry of the same cookie is visible (validators), the entry of
    // another cookie is another variant (no validators, no write).
    let vary = vary_of("cookie", &[line("cookie", "sid=1")]);
    assert!(vary[0].value.is_some());
    for (cookie, visible) in [("sid=1", true), ("sid=2", false)] {
        let mut run = Run::new(Presentation::Quick);
        let (id, _) = run.open(context(URL, PARTITION, true));
        assert_eq!(
            run.step(id, lookup(Some(committed_entry_with(&vary)), false)),
            vec![Effect::ReadCookie]
        );
        let effects = run.step(id, Input::Cookie(Some(cookie.to_owned())));
        let [Effect::Send(spec)] = effects.as_slice() else {
            panic!("{effects:?}");
        };
        assert_eq!(
            spec.fields.iter().any(|line| line.name == "if-none-match"),
            visible,
            "{cookie}"
        );
    }
}

/// The `Vary` entries of the one commit in `effects`.
fn committed_vary(effects: &[Effect]) -> Vec<VaryEntry> {
    let commits: Vec<&TxnOp> = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Cache(txn) => Some(&txn.op),
            _ => None,
        })
        .collect();
    let [TxnOp::Replace { meta, .. }] = commits.as_slice() else {
        panic!("{effects:?}");
    };
    meta.vary.clone()
}

#[test]
fn legacy_engine_classify_304_keeps_or_recomputes_the_vary_of_the_entry() {
    // CL-17 (DL-126): the field update of a `304` without `Vary` keeps the
    // `Vary` entries of the entry; a `Vary` field recomputes them from the
    // names of the response and the values of this request (CL-12).
    let encoding = vary_of("accept-encoding", &[]);
    assert!(encoding[0].value.is_some());
    let absent = vary_of("x-absent", &[]);
    assert_eq!(absent[0].value, None);
    let offline = line("cache-offline", "true");
    for (stored_vary, fields, expected) in [
        (encoding.clone(), vec![offline.clone()], encoding.clone()),
        (
            Vec::new(),
            vec![offline.clone(), line("Vary", "Accept-Encoding")],
            encoding.clone(),
        ),
        (
            encoding.clone(),
            vec![offline.clone(), line("vary", "x-absent")],
            absent.clone(),
        ),
    ] {
        let mut run = Run::new(Presentation::Quick);
        let (id, exchange, effects) = run.start(Some(committed_entry_with(&stored_vary)));
        let Some(Effect::Send(spec)) = effects.last() else {
            panic!("{effects:?}");
        };
        assert_eq!(spec.fields.len(), 3, "the entry is visible");
        let effects = run.respond(id, exchange, 304, fields, b"");
        assert_eq!(results(&effects), vec![(304, 304)]);
        assert_eq!(committed_vary(&effects), expected);
    }
}

// --- `Debug` of the engine state (DL-227) ---

#[test]
fn legacy_engine_debug_texts_of_a_session_and_a_held_key_are_exact() {
    // A session writes its flags and counts; a held key writes the length
    // of its URL. Neither writes the URL.
    let mut run = Run::new(Presentation::Standard);
    let (id, _, _) = run.start(None);
    let second = run.refresh(id);
    assert_eq!(run.client(id), vec![Effect::LoadUrl]);
    assert_eq!(run.step(id, Input::Intercept(InterceptId(1))), Vec::new());
    assert_eq!(run.head(id, second, legacy_head(&page())), Vec::new());
    let text = format!("{:?}", run.engine);
    assert!(
        text.contains(
            "Session { stopped: false, generation: 1, kind: Some(FirstLoad), exchanges: 1, waiting: 1, .. }"
        ),
        "{text}"
    );
    assert!(!text.contains("shop.example"), "{text}");
    let now = run.now();
    let _ = run.engine.partition_changed(now, PARTITION);
    let text = format!("{:?}", run.engine);
    let url_len = rivqen_url::parse(URL).unwrap().cache_url(&[]).len();
    assert!(
        text.contains(&format!("HeldKey {{ url_len: {url_len}, .. }}")),
        "{text}"
    );
    assert!(!text.contains("shop.example"), "{text}");
}
