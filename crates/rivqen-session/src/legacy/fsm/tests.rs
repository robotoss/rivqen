// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Unit tests of the legacy session with a fake record port: the paths with
//! a visible entry (CL-17, CL-27 to CL-31, CL-40) and the content of the
//! commits. `record.rs` is a stub until the record lane is integrated (no
//! entry is visible, every write is a removal), so these paths need fakes
//! here; the fixture run (T-05) checks them with the real record.
//!
//! Every effect of every test passes `Run::check_writes`: each cache write
//! names the request partition and the epochs of the lookup (DL-220,
//! RSK-06).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::too_many_lines
)]

use proptest::prelude::*;
use rivqen_cache::header::{CacheControl, Vary, parse_vary};
use rivqen_cache::key::{Partition, Protocol, VaryEntry};
use rivqen_cache::policy::{EntryPolicy, Expires};
use rivqen_cache::txn::{
    CacheTransaction, EntryMeta, EntryVersion, Epoch, Expect, SnapshotBody, StoredEntry, TxnOp,
    Validators, Writer,
};
use rivqen_model::http::FieldLine;
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_model::safe::{PageBytes, Secret};
use rivqen_model::time::{Instant, UnixTime};
use rivqen_proto_legacy::body::{BodyLimits, DATA_BODY_DEPTH};
use rivqen_template::legacy::{
    DataBlock, SplitOutcome, etag_v1, render_v1, split_v1, template_tag_v1,
};
use rivqen_template::limits::TemplateLimits;

use super::port::RecordPort;
use crate::legacy::engine::LegacyEngine;
use crate::legacy::record::{LegacyRecord, NewRecord, NotVisible, RequestValues, WriteBasis};
use crate::legacy::types::{
    DEFAULT_UNAVAILABLE_TIME_MS, Effect, ExchangeId, Input, InterceptId, LegacyConfig,
    LookupResult, Now, OpenContext, Presentation, SessionId, TransportError, VaryKey,
};

const PARTITION: PartitionId = PartitionId::from_bytes([7; 32]);
const PARTITION_EPOCH: Epoch = Epoch(3);
const SITE_EPOCH: Epoch = Epoch(4);
const STORED_VERSION: EntryVersion = EntryVersion(11);
const URL: &str = "https://shop.example.com/catalog?page=2";
const WALL: u64 = 1_800_000_000;

// --- pages ---

fn page(price: &str) -> String {
    format!(
        "<html><head><title>Shop</title></head><body><p><!--sonicdiff-price-->{price}<!--sonicdiff-price-end--></p></body></html>"
    )
}

fn split(text: &str) -> (String, Vec<DataBlock>) {
    match split_v1(text.as_bytes(), &TemplateLimits::default()).unwrap() {
        SplitOutcome::Applied(split) => (split.template, split.data),
        SplitOutcome::NotApplied(reason) => panic!("{reason:?}"),
    }
}

fn line(name: &str, value: &str) -> FieldLine {
    FieldLine::new(name, value)
}

/// The head of a legacy `200` for `text`.
fn legacy_head(text: &str, template_change: &str, offline: &str) -> Vec<FieldLine> {
    let (template, _) = split(text);
    vec![
        line("Content-Type", "text/html; charset=utf-8"),
        line("ETag", etag_v1(text.as_bytes()).as_str()),
        line("template-tag", template_tag_v1(&template).as_str()),
        line("template-change", template_change),
        line("cache-offline", offline),
        line("x-page", "1"),
    ]
}

/// A data update body that rebuilds `new` from the template of `old`.
fn data_body(old: &str, new: &str) -> (Vec<u8>, String) {
    let (template, _) = split(old);
    let (_, data) = split(new);
    let rebuilt = render_v1(&template, &data, &TemplateLimits::default()).unwrap();
    let sha = etag_v1(rebuilt.as_bytes());
    let members: Vec<String> = data
        .iter()
        .map(|block| {
            format!(
                "{}:{}",
                serde_json::to_string(&block.key).unwrap(),
                serde_json::to_string(&block.value).unwrap()
            )
        })
        .collect();
    let body = format!(
        "{{\"data\":{{{}}},\"html-sha1\":\"{}\"}}",
        members.join(","),
        sha.as_str()
    );
    (body.into_bytes(), sha.as_str().to_owned())
}

/// The head of a data update of `old` to `new`.
fn data_head(old: &str, new: &str, offline: &str) -> Vec<FieldLine> {
    let (template, _) = split(old);
    let (_, sha) = data_body(old, new);
    vec![
        line("content-type", "text/html; charset=utf-8"),
        line("etag", &sha),
        line("template-tag", template_tag_v1(&template).as_str()),
        line("template-change", "false"),
        line("cache-offline", offline),
        line("x-data", "2"),
    ]
}

// --- the fake record ---

fn policy() -> EntryPolicy {
    EntryPolicy {
        cache_control: CacheControl::default(),
        expires: Expires::Absent,
        date: None,
        age: None,
        request_time: UnixTime(WALL),
        response_time: UnixTime(WALL),
        ttl_seconds: None,
        offline: false,
    }
}

/// A stored entry of `text`; the host marks the fake answer of the record:
/// `damaged` and `other` are not visible.
fn stored(text: &str, host: &str) -> StoredEntry {
    let (template, data) = split(text);
    StoredEntry {
        meta: EntryMeta {
            key: CacheKey::from_bytes([1; 32]),
            partition: Partition::Scoped(PARTITION),
            host: host.to_owned(),
            protocol: Protocol::Legacy,
            vary: Vec::new(),
            validators: Validators::default(),
            policy: policy(),
            stored_at: UnixTime(WALL),
            last_used_at: UnixTime(WALL),
        },
        version: STORED_VERSION,
        body: SnapshotBody {
            page: PageBytes::new(text.as_bytes().to_vec()),
            template: Some(PageBytes::new(template.into_bytes())),
            data: data
                .into_iter()
                .map(|block| (block.key, PageBytes::new(block.value.into_bytes())))
                .collect(),
            fields: vec![
                line("x-stored", "s"),
                line("content-type", "text/html; charset=utf-8"),
            ],
        },
    }
}

fn fake_check_visible(
    entry: &StoredEntry,
    _partition: PartitionId,
    _cache_key: CacheKey,
    _values: &RequestValues<'_>,
    _key: &VaryKey,
) -> Result<LegacyRecord, NotVisible> {
    match entry.meta.host.as_str() {
        "damaged" => return Err(NotVisible::Damaged),
        "other" => return Err(NotVisible::OtherVariant),
        _ => {}
    }
    let text = |bytes: &PageBytes| String::from_utf8(bytes.as_slice().to_vec()).unwrap();
    let page = text(&entry.body.page);
    let template = text(entry.body.template.as_ref().unwrap());
    Ok(LegacyRecord {
        version: entry.version,
        etag: etag_v1(page.as_bytes()),
        template_tag: template_tag_v1(&template),
        page,
        template,
        data: entry
            .body
            .data
            .iter()
            .map(|(key, value)| DataBlock {
                key: key.clone(),
                value: text(value),
            })
            .collect(),
        fields: entry.body.fields.clone(),
        vary: entry.meta.vary.clone(),
    })
}

fn fake_vary_entries(
    fields: &[FieldLine],
    _values: &RequestValues<'_>,
    _key: &VaryKey,
) -> Option<Vec<VaryEntry>> {
    match parse_vary(fields) {
        Vary::Any => None,
        Vary::None => Some(Vec::new()),
        Vary::Names(names) => Some(
            names
                .into_iter()
                .map(|name| VaryEntry { name, value: None })
                .collect(),
        ),
    }
}

fn fake_commit(basis: &WriteBasis, record: &NewRecord<'_>) -> CacheTransaction {
    CacheTransaction {
        op: TxnOp::Replace {
            meta: Box::new(EntryMeta {
                key: basis.key,
                partition: Partition::Scoped(basis.partition),
                host: record.host.to_owned(),
                protocol: Protocol::Legacy,
                vary: record.vary.to_vec(),
                validators: Validators {
                    etag: Some(record.etag.as_str().to_owned()),
                    template_tag: Some(record.template_tag.as_str().to_owned()),
                    rqp: None,
                },
                policy: policy(),
                stored_at: record.response_time,
                last_used_at: record.response_time,
            }),
            body: Box::new(SnapshotBody {
                page: PageBytes::new(record.page.as_bytes().to_vec()),
                template: Some(PageBytes::new(record.template.as_bytes().to_vec())),
                data: record
                    .data
                    .iter()
                    .map(|block| {
                        (
                            block.key.clone(),
                            PageBytes::new(block.value.as_bytes().to_vec()),
                        )
                    })
                    .collect(),
                fields: record.fields.to_vec(),
            }),
        },
        pre: basis.preconditions(),
    }
}

const FAKE: RecordPort = RecordPort {
    check_visible: fake_check_visible,
    vary_entries: fake_vary_entries,
    commit: fake_commit,
};

// --- the run ---

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

fn context(network_valid: bool) -> OpenContext {
    OpenContext {
        url: rivqen_url::parse(URL).unwrap(),
        partition: PARTITION,
        anonymous: false,
        network_valid,
        platform_fields: vec![line("accept-encoding", "gzip")],
        vary_key: VaryKey(Secret::new([9; 32])),
    }
}

struct Run {
    engine: LegacyEngine,
    id: SessionId,
    mono: u64,
}

/// Every cache write of a session names the request partition and the
/// epochs of its lookup (DL-220).
fn check_writes(effects: &[Effect]) {
    for effect in effects {
        let Effect::Cache(txn) = effect else {
            continue;
        };
        assert_eq!(
            txn.pre.writer,
            Writer::request(PARTITION, PARTITION_EPOCH),
            "{txn:?}"
        );
        assert_eq!(txn.pre.partition_epoch, PARTITION_EPOCH);
        assert_eq!(txn.pre.site_epoch, SITE_EPOCH);
        let target = match &txn.op {
            TxnOp::Replace { meta, .. } => meta.partition,
            TxnOp::UpdateFields { partition, .. }
            | TxnOp::Remove { partition, .. }
            | TxnOp::DisableKey { partition, .. } => *partition,
            other => panic!("a session never writes {other:?}"),
        };
        assert_eq!(target, Partition::Scoped(PARTITION));
    }
}

impl Run {
    fn now(&self) -> Now {
        Now {
            mono: Instant(self.mono),
            wall: UnixTime(WALL + self.mono / 1000),
        }
    }

    /// Open a session and answer the lookup with `entry`; with a valid
    /// network also the cookie read. Returns the run, the effects after the
    /// lookup, and the exchange of the request (if any).
    fn start(
        presentation: Presentation,
        entry: Option<StoredEntry>,
        network_valid: bool,
    ) -> (Self, Vec<Effect>, Option<ExchangeId>) {
        let mut run = Self {
            engine: LegacyEngine::with_port(config(presentation), FAKE),
            id: SessionId(0),
            mono: 0,
        };
        let (id, opened) = run.engine.open(run.now(), context(network_valid));
        run.id = id;
        assert!(matches!(opened.as_slice(), [Effect::Lookup { .. }]));
        let mut effects = run.step(Input::Lookup(Box::new(LookupResult {
            entry,
            disabled: false,
            partition_epoch: PARTITION_EPOCH,
            site_epoch: SITE_EPOCH,
        })));
        if network_valid {
            assert_eq!(effects, vec![Effect::ReadCookie]);
            effects = run.step(Input::Cookie(None));
        }
        let exchange = sent(&effects);
        (run, effects, exchange)
    }

    fn step(&mut self, input: Input) -> Vec<Effect> {
        let effects = self.engine.handle(self.now(), self.id, input);
        check_writes(&effects);
        effects
    }

    fn head(&mut self, exchange: ExchangeId, status: u16, fields: Vec<FieldLine>) -> Vec<Effect> {
        self.step(Input::ResponseHead {
            exchange,
            status,
            fields,
        })
    }

    fn chunk(&mut self, exchange: ExchangeId, bytes: &[u8]) -> Vec<Effect> {
        self.step(Input::BodyChunk {
            exchange,
            bytes: bytes.to_vec(),
        })
    }

    fn end(&mut self, exchange: ExchangeId) -> Vec<Effect> {
        self.step(Input::BodyEnd { exchange })
    }

    /// Head, one chunk and the end.
    fn respond(
        &mut self,
        exchange: ExchangeId,
        status: u16,
        fields: Vec<FieldLine>,
        body: &[u8],
    ) -> Vec<Effect> {
        let mut effects = self.head(exchange, status, fields);
        if !body.is_empty() {
            effects.extend(self.chunk(exchange, body));
        }
        effects.extend(self.end(exchange));
        effects
    }

    fn client(&mut self) -> Vec<Effect> {
        let mut effects = self.step(Input::BindClient);
        effects.extend(self.step(Input::ClientReady));
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

fn messages(effects: &[Effect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Message(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn count(effects: &[Effect], wanted: &Effect) -> usize {
    effects.iter().filter(|effect| *effect == wanted).count()
}

/// The document that `intercept` got: bytes and `complete`, or `None` for
/// no answer; `Some((empty, false))`-like answers come from streams.
fn answer(effects: &[Effect], intercept: InterceptId) -> Option<Answer> {
    let mut found = None;
    let mut bytes = Vec::new();
    for effect in effects {
        match effect {
            Effect::AnswerNull(id) if *id == intercept => found = Some(Answer::Null),
            Effect::AnswerDocument { intercept: id, .. } if *id == intercept => {
                found = Some(Answer::Open);
            }
            Effect::StreamChunk {
                intercept: id,
                bytes: b,
            } if *id == intercept => {
                bytes.extend_from_slice(b);
            }
            Effect::StreamEnd {
                intercept: id,
                complete,
            } if *id == intercept => {
                found = Some(Answer::Doc(bytes.clone(), *complete));
            }
            _ => {}
        }
    }
    found
}

#[derive(Debug, PartialEq, Eq)]
enum Answer {
    Null,
    Open,
    Doc(Vec<u8>, bool),
}

fn writes(effects: &[Effect]) -> Vec<&CacheTransaction> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Cache(txn) => Some(txn.as_ref()),
            _ => None,
        })
        .collect()
}

fn is_remove(txn: &CacheTransaction, expect: Expect) -> bool {
    matches!(txn.op, TxnOp::Remove { .. }) && txn.pre.entry == expect
}

fn replaced_body(txn: &CacheTransaction) -> &SnapshotBody {
    match &txn.op {
        TxnOp::Replace { body, .. } => body,
        other => panic!("not a commit: {other:?}"),
    }
}

// --- open (CL-05 to CL-12) ---

#[test]
fn legacy_engine_open_sends_the_validators_of_a_visible_entry() {
    let old = page("10");
    let (_, effects, _) = Run::start(Presentation::Quick, Some(stored(&old, "h")), true);
    let Some(Effect::Send(spec)) = effects.first() else {
        panic!("{effects:?}");
    };
    let (template, _) = split(&old);
    assert_eq!(spec.target, "/catalog?page=2");
    assert_eq!(
        spec.fields,
        vec![
            line("accept-diff", "true"),
            line("if-none-match", etag_v1(old.as_bytes()).as_str()),
            line("template-tag", template_tag_v1(&template).as_str()),
        ]
    );
}

#[test]
fn legacy_engine_open_other_variant_sends_no_validators_and_commit_replaces_it() {
    let text = page("10");
    let (mut run, effects, exchange) =
        Run::start(Presentation::Quick, Some(stored(&text, "other")), true);
    let Some(Effect::Send(spec)) = effects.first() else {
        panic!("{effects:?}");
    };
    assert_eq!(spec.fields, vec![line("accept-diff", "true")]);
    let effects = run.respond(
        exchange.unwrap(),
        200,
        legacy_head(&text, "true", "true"),
        text.as_bytes(),
    );
    let writes = writes(&effects);
    assert_eq!(writes.len(), 1);
    // H-160: a commit for other values replaces the entry it saw.
    assert_eq!(writes[0].pre.entry, Expect::Version(STORED_VERSION));
    assert!(matches!(writes[0].op, TxnOp::Replace { .. }));
}

#[test]
fn legacy_engine_open_damaged_entry_is_removed_and_the_session_runs_without_it() {
    let text = page("10");
    let (_, effects, exchange) =
        Run::start(Presentation::Quick, Some(stored(&text, "damaged")), true);
    assert!(exchange.is_some());
    let writes = writes(&effects);
    assert_eq!(writes.len(), 1);
    assert!(is_remove(writes[0], Expect::Version(STORED_VERSION)));
    let Some(Effect::Send(spec)) = effects.last() else {
        panic!("{effects:?}");
    };
    assert_eq!(spec.fields, vec![line("accept-diff", "true")]);
}

#[test]
fn legacy_engine_open_without_network_shows_the_cached_page() {
    let text = page("10");
    let (mut run, effects, exchange) =
        Run::start(Presentation::Quick, Some(stored(&text, "h")), false);
    assert_eq!(effects, Vec::new());
    assert_eq!(exchange, None);
    let effects = run.client();
    let Some(Effect::LoadData { head, body }) = effects.first() else {
        panic!("{effects:?}");
    };
    assert_eq!(body.as_slice(), text.as_bytes());
    // CL-21: the stored fields, lower case, sorted by name.
    assert_eq!(
        head.fields,
        vec![
            line("content-type", "text/html; charset=utf-8"),
            line("x-stored", "s")
        ]
    );
    let effects = run.step(Input::Destroy);
    assert_eq!(results(&effects), vec![(-1, -1)]);
}

/// A stored entry whose page fields hold lines that never go to the web
/// view: `Set-Cookie`, a hop-by-hop field, a line with a control byte, a
/// name that is not a token.
fn stored_with_unsafe_fields(text: &str) -> StoredEntry {
    let mut entry = stored(text, "h");
    entry.body.fields = vec![
        line("X-Stored", "s"),
        line("Set-Cookie", "sid=other-account"),
        line("transfer-encoding", "chunked"),
        line("x-bad", "a\u{1}b"),
        line("bad name", "v"),
        line("content-type", "text/html; charset=utf-8"),
    ];
    entry
}

/// The clean page fields of [`stored_with_unsafe_fields`].
fn clean_fields() -> Vec<FieldLine> {
    vec![
        line("content-type", "text/html; charset=utf-8"),
        line("x-stored", "s"),
    ]
}

#[test]
fn legacy_engine_open_cached_document_fields_pass_the_filter() {
    // DL-229 (d): the stored lines pass the filter of DL-226 (b) and CL-21
    // before they reach `DocumentHead`.
    let text = page("10");
    let (mut run, _, _) = Run::start(
        Presentation::Quick,
        Some(stored_with_unsafe_fields(&text)),
        false,
    );
    let effects = run.client();
    let Some(Effect::LoadData { head, .. }) = effects.first() else {
        panic!("{effects:?}");
    };
    assert_eq!(head.fields, clean_fields());

    // Standard: the intercept gets the same fields.
    let (mut run, _, _) = Run::start(
        Presentation::Standard,
        Some(stored_with_unsafe_fields(&text)),
        false,
    );
    let _ = run.client();
    let effects = run.step(Input::Intercept(InterceptId(1)));
    let Some(Effect::AnswerDocument { head, .. }) = effects.first() else {
        panic!("{effects:?}");
    };
    assert_eq!(head.fields, clean_fields());
}

#[test]
fn legacy_engine_classify_304_field_update_never_keeps_an_unsafe_stored_line() {
    // DL-229 (d): the field update of CL-17 merges the filtered lines, so
    // the commit and the cached document hold none of the unsafe lines.
    let text = page("10");
    let (mut run, _, exchange) = Run::start(
        Presentation::Quick,
        Some(stored_with_unsafe_fields(&text)),
        true,
    );
    let effects = run.respond(
        exchange.unwrap(),
        304,
        vec![line("cache-offline", "true"), line("x-new", "n")],
        b"",
    );
    let writes = writes(&effects);
    assert_eq!(writes.len(), 1);
    let merged = vec![
        line("content-type", "text/html; charset=utf-8"),
        line("x-new", "n"),
        line("x-stored", "s"),
    ];
    assert_eq!(replaced_body(writes[0]).fields, merged);
    let effects = run.client();
    let Some(Effect::LoadData { head, .. }) = effects.first() else {
        panic!("{effects:?}");
    };
    assert_eq!(head.fields, merged);
}

// --- classification (CL-13 to CL-19, CL-44) ---

#[test]
fn legacy_engine_classify_304_updates_the_page_fields() {
    let text = page("10");
    let (mut run, _, exchange) = Run::start(Presentation::Quick, Some(stored(&text, "h")), true);
    let exchange = exchange.unwrap();
    let effects = run.respond(
        exchange,
        304,
        vec![
            line("cache-offline", "true"),
            line("X-Stored", "new"),
            line("set-cookie", "a=b"),
        ],
        b"",
    );
    // CL-11 item 2: the cookie first.
    assert_eq!(effects[0], Effect::StoreCookies(vec!["a=b".to_owned()]));
    // The Set-Cookie makes the exchange credentialed: no field update, the
    // entry is removed (DL-132).
    assert!(is_remove(
        writes(&effects)[0],
        Expect::Version(STORED_VERSION)
    ));
    assert_eq!(results(&effects), vec![(304, 304)]);

    let (mut run, _, exchange) = Run::start(Presentation::Quick, Some(stored(&text, "h")), true);
    let effects = run.respond(
        exchange.unwrap(),
        304,
        vec![line("cache-offline", "true"), line("X-Stored", "new")],
        b"",
    );
    let writes = writes(&effects);
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].pre.entry, Expect::Version(STORED_VERSION));
    let body = replaced_body(writes[0]);
    assert_eq!(body.page.as_slice(), text.as_bytes());
    assert_eq!(
        body.fields,
        vec![
            line("content-type", "text/html; charset=utf-8"),
            line("x-stored", "new")
        ]
    );
    assert_eq!(results(&effects), vec![(304, 304)]);
    // The cached page carries the merged fields.
    let effects = run.client();
    let Some(Effect::LoadData { head, .. }) = effects.first() else {
        panic!("{effects:?}");
    };
    assert_eq!(head.fields[1], line("x-stored", "new"));
}

#[test]
fn legacy_engine_classify_304_cache_rules_remove_or_disable() {
    let text = page("10");
    for (fields, disabled) in [
        (vec![line("cache-offline", "false")], false),
        (vec![], false),
        (
            vec![
                line("cache-offline", "true"),
                line("cache-control", "no-store"),
            ],
            false,
        ),
        (
            vec![line("cache-offline", "true"), line("vary", "*")],
            false,
        ),
        (vec![line("cache-offline", "HTTP")], true),
    ] {
        let (mut run, _, exchange) =
            Run::start(Presentation::Standard, Some(stored(&text, "h")), true);
        let effects = run.respond(exchange.unwrap(), 304, fields, b"");
        let writes = writes(&effects);
        assert_eq!(writes.len(), 1, "{effects:?}");
        if disabled {
            // CL-19: normal load, no result (the stub of record::disable is
            // a removal until the record lane is integrated).
            assert_eq!(results(&effects), Vec::<(i32, i32)>::new());
        } else {
            assert!(is_remove(writes[0], Expect::Version(STORED_VERSION)));
            assert_eq!(results(&effects), vec![(304, 304)]);
        }
    }
}

#[test]
fn legacy_engine_classify_304_with_an_unsafe_head_is_a_normal_load() {
    // DL-226: the head test runs before CL-17 too.
    let text = page("10");
    for unsafe_field in [
        line("sonic-etag-key", "isReload"),
        line("connection", "content-security-policy"),
        line("x-bad", "a\u{1}b"),
    ] {
        let (mut run, _, exchange) =
            Run::start(Presentation::Quick, Some(stored(&text, "h")), true);
        let mut effects = run.client();
        assert!(matches!(effects[0], Effect::LoadData { .. }));
        effects = run.respond(
            exchange.unwrap(),
            304,
            vec![line("cache-offline", "true"), unsafe_field],
            b"",
        );
        assert!(is_remove(
            writes(&effects)[0],
            Expect::Version(STORED_VERSION)
        ));
        // The cached page was delivered: load_url (CL-14).
        assert!(effects.contains(&Effect::LoadUrl));
        assert_eq!(results(&effects), Vec::<(i32, i32)>::new());
        let effects = run.step(Input::Intercept(InterceptId(1)));
        assert_eq!(answer(&effects, InterceptId(1)), Some(Answer::Null));
    }
}

#[test]
fn legacy_engine_classify_200_with_the_stored_etag_is_not_modified() {
    let text = page("10");
    let (mut run, _, exchange) = Run::start(Presentation::Quick, Some(stored(&text, "h")), true);
    let fields = vec![
        line(
            "etag",
            &format!("W/\"{}\"", etag_v1(text.as_bytes()).as_str()),
        ),
        line("cache-offline", "true"),
    ];
    let effects = run.respond(exchange.unwrap(), 200, fields, text.as_bytes());
    assert_eq!(results(&effects), vec![(304, 304)]);
}

#[test]
fn legacy_engine_classify_errors_keep_or_remove_the_entry() {
    let text = page("10");
    // CL-18 item 1: 5xx, 408, 429 keep the cached page and the entry.
    for status in [500, 503, 408, 429] {
        let (mut run, _, exchange) =
            Run::start(Presentation::Quick, Some(stored(&text, "h")), true);
        let effects = run.respond(exchange.unwrap(), status, vec![], b"x");
        assert_eq!(writes(&effects).len(), 0);
        let effects = run.client();
        assert!(matches!(effects[0], Effect::LoadData { .. }), "{status}");
    }
    // CL-18 item 2, CL-44: the entry removed, a normal load.
    for status in [401, 404, 410, 301, 302] {
        let (mut run, _, exchange) =
            Run::start(Presentation::Quick, Some(stored(&text, "h")), true);
        let mut effects = run.client();
        effects.extend(run.respond(exchange.unwrap(), status, vec![], b""));
        assert!(is_remove(
            writes(&effects)[0],
            Expect::Version(STORED_VERSION)
        ));
        assert_eq!(count(&effects, &Effect::LoadUrl), 1, "{status}");
    }
}

#[test]
fn legacy_engine_classify_transport_errors_before_the_head() {
    let text = page("10");
    // CL-18 item 1: the cached page stays, no reload.
    let (mut run, _, exchange) = Run::start(Presentation::Quick, Some(stored(&text, "h")), true);
    let mut effects = run.client();
    effects.extend(run.step(Input::TransportFailed {
        exchange: exchange.unwrap(),
        error: TransportError::Refused,
    }));
    assert_eq!(count(&effects, &Effect::LoadUrl), 0);
    assert_eq!(writes(&effects).len(), 0);
    // CL-18 item 3: TLS: a normal load also after the delivery; the entry
    // stays.
    let (mut run, _, exchange) = Run::start(Presentation::Quick, Some(stored(&text, "h")), true);
    let mut effects = run.client();
    effects.extend(run.step(Input::TransportFailed {
        exchange: exchange.unwrap(),
        error: TransportError::Tls,
    }));
    assert_eq!(count(&effects, &Effect::LoadUrl), 1);
    assert_eq!(writes(&effects).len(), 0);
}

// --- outcomes (CL-25 to CL-33) ---

#[test]
fn legacy_engine_outcome_first_load_commits_the_split_page() {
    let text = page("10");
    let (mut run, _, exchange) = Run::start(Presentation::Standard, None, true);
    let effects = run.respond(
        exchange.unwrap(),
        200,
        legacy_head(&text, "true", "true"),
        text.as_bytes(),
    );
    let writes = writes(&effects);
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].pre.entry, Expect::Absent);
    let TxnOp::Replace { meta, body } = &writes[0].op else {
        panic!();
    };
    let (template, data) = split(&text);
    assert_eq!(meta.host, "shop.example.com");
    assert_eq!(meta.stored_at, UnixTime(WALL));
    assert_eq!(body.page.as_slice(), text.as_bytes());
    assert_eq!(
        body.template.as_ref().unwrap().as_slice(),
        template.as_bytes()
    );
    assert_eq!(body.data.len(), data.len());
    assert_eq!(
        body.fields,
        vec![
            line("content-type", "text/html; charset=utf-8"),
            line("x-page", "1")
        ]
    );
    assert_eq!(results(&effects), vec![(1000, 304)]);
}

#[test]
fn legacy_engine_outcome_first_load_commit_conditions() {
    let text = page("10");
    let other = page("11");
    let cases: Vec<(Vec<FieldLine>, &[u8])> = vec![
        (legacy_head(&text, "true", "false"), text.as_bytes()),
        (legacy_head(&text, "true", "bogus"), text.as_bytes()),
        (
            [
                legacy_head(&text, "true", "true"),
                vec![line("cache-control", "max-age=5, no-store")],
            ]
            .concat(),
            text.as_bytes(),
        ),
        (
            [
                legacy_head(&text, "true", "true"),
                vec![line("vary", "accept, *")],
            ]
            .concat(),
            text.as_bytes(),
        ),
        (
            [
                legacy_head(&text, "true", "true"),
                vec![line("set-cookie", "a=b")],
            ]
            .concat(),
            text.as_bytes(),
        ),
        // The etag is not the SHA-1 of the page.
        (legacy_head(&other, "true", "true"), text.as_bytes()),
    ];
    for (fields, body) in cases {
        let (mut run, _, exchange) = Run::start(Presentation::Standard, None, true);
        let effects = run.respond(exchange.unwrap(), 200, fields, body);
        assert_eq!(writes(&effects).len(), 0, "{effects:?}");
        assert_eq!(results(&effects), vec![(1000, 304)]);
    }
    // `store` commits.
    let (mut run, _, exchange) = Run::start(Presentation::Standard, None, true);
    let effects = run.respond(
        exchange.unwrap(),
        200,
        legacy_head(&text, "true", "STORE"),
        text.as_bytes(),
    );
    assert_eq!(writes(&effects).len(), 1);
}

#[test]
fn legacy_engine_outcome_credentialed_commit_needs_the_app_and_an_account() {
    let text = page("10");
    let fields = [
        legacy_head(&text, "true", "true"),
        vec![line("set-cookie", "a=b")],
    ]
    .concat();
    for (authenticated_cache, anonymous, commits) in [
        (true, false, true),
        (true, true, false),
        (false, false, false),
    ] {
        let mut config = config(Presentation::Standard);
        config.authenticated_cache = authenticated_cache;
        let mut engine = LegacyEngine::with_port(config, FAKE);
        let mut context = context(true);
        context.anonymous = anonymous;
        let now = Now {
            mono: Instant(0),
            wall: UnixTime(WALL),
        };
        let (id, _) = engine.open(now, context);
        let _ = engine.handle(
            now,
            id,
            Input::Lookup(Box::new(LookupResult {
                entry: None,
                disabled: false,
                partition_epoch: PARTITION_EPOCH,
                site_epoch: SITE_EPOCH,
            })),
        );
        let effects = engine.handle(now, id, Input::Cookie(None));
        let exchange = sent(&effects).unwrap();
        let mut effects = engine.handle(
            now,
            id,
            Input::ResponseHead {
                exchange,
                status: 200,
                fields: fields.clone(),
            },
        );
        effects.extend(engine.handle(
            now,
            id,
            Input::BodyChunk {
                exchange,
                bytes: text.as_bytes().to_vec(),
            },
        ));
        effects.extend(engine.handle(now, id, Input::BodyEnd { exchange }));
        check_writes(&effects);
        assert_eq!(writes(&effects).len(), usize::from(commits));
    }
}

#[test]
fn legacy_engine_outcome_data_update_after_the_delivery_sends_the_diff() {
    let old = page("10");
    let new = page("12");
    let (mut run, _, exchange) = Run::start(Presentation::Quick, Some(stored(&old, "h")), true);
    let mut effects = run.client();
    assert!(matches!(effects[0], Effect::LoadData { .. }));
    effects = run.step(Input::GetDiff);
    assert_eq!(effects, Vec::new());
    let (body, sha) = data_body(&old, &new);
    let effects = run.respond(exchange.unwrap(), 200, data_head(&old, &new, "true"), &body);
    assert_eq!(results(&effects), vec![(200, 200)]);
    let messages = messages(&effects);
    assert_eq!(messages.len(), 1);
    let expected_diff = serde_json::to_string(
        "{\"{price}\":\"<!--sonicdiff-price-->12<!--sonicdiff-price-end-->\"}",
    )
    .unwrap();
    assert!(
        messages[0].starts_with(&format!("{{\"result\":{expected_diff},\"code\":200,\"srcCode\":200,\"extra\":{{\"eTag\":\"{sha}\"")),
        "{}",
        messages[0]
    );
    assert!(messages[0].ends_with(",\"cache-offline\":\"true\",\"isReload\":\"false\"}}"));
    // The commit: the rebuilt page, the full server data, the stored
    // template, the page fields of the response.
    let writes = writes(&effects);
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].pre.entry, Expect::Version(STORED_VERSION));
    let stored_body = replaced_body(writes[0]);
    assert_eq!(stored_body.page.as_slice(), new.as_bytes());
    assert_eq!(stored_body.data.len(), 2);
    assert_eq!(
        stored_body.fields,
        vec![
            line("content-type", "text/html; charset=utf-8"),
            line("x-data", "2")
        ]
    );
    // CL-23 item 4: a later get_diff gets the same message again.
    let again = run.step(Input::GetDiff);
    assert_eq!(super::tests::messages(&again), messages);
}

#[test]
fn legacy_engine_outcome_data_update_before_the_delivery_rebuilds_the_document() {
    let old = page("10");
    let new = page("12");
    let (mut run, _, exchange) = Run::start(Presentation::Standard, Some(stored(&old, "h")), true);
    let (body, _) = data_body(&old, &new);
    let effects = run.respond(
        exchange.unwrap(),
        200,
        data_head(&old, &new, "false"),
        &body,
    );
    // CL-27: no commit for `false`; the entry is removed after the delivery.
    assert!(is_remove(
        writes(&effects)[0],
        Expect::Version(STORED_VERSION)
    ));
    assert_eq!(results(&effects), vec![(200, 304)]);
    let mut effects = run.client();
    assert_eq!(effects, vec![Effect::LoadUrl]);
    effects = run.step(Input::Intercept(InterceptId(4)));
    assert_eq!(
        answer(&effects, InterceptId(4)),
        Some(Answer::Doc(new.into_bytes(), true))
    );
}

#[test]
fn legacy_engine_outcome_data_update_store_gives_304_without_a_diff() {
    // An empty diff (CL-28) has the page of the entry, so its etag is the
    // stored etag: CL-13 item 2 takes it first (304 / 304).
    let old = page("10");
    for (new, offline, src) in [(page("12"), "store", 200), (page("10"), "true", 304)] {
        let (mut run, _, exchange) = Run::start(Presentation::Quick, Some(stored(&old, "h")), true);
        let mut effects = run.client();
        effects.extend(run.step(Input::GetDiff));
        let (body, _) = data_body(&old, &new);
        effects = run.respond(
            exchange.unwrap(),
            200,
            data_head(&old, &new, offline),
            &body,
        );
        assert_eq!(results(&effects), vec![(src, 304)]);
        let messages = messages(&effects);
        assert_eq!(messages.len(), 1);
        assert!(messages[0].starts_with(&format!("{{\"code\":304,\"srcCode\":{src},")));
        assert_eq!(writes(&effects).len(), 1);
        assert!(matches!(writes(&effects)[0].op, TxnOp::Replace { .. }));
    }
}

#[test]
fn legacy_engine_outcome_rebuild_check_failure_is_a_normal_load() {
    let old = page("10");
    let new = page("12");
    let (body, _) = data_body(&old, &new);
    let bad_sha = String::from_utf8(body.clone())
        .unwrap()
        .replace("\"html-sha1\":\"", "\"html-sha1\":\"0");
    let mut wrong_tag = data_head(&old, &new, "true");
    wrong_tag[2] = line("template-tag", "0000");
    for (fields, body) in [
        (data_head(&old, &new, "true"), bad_sha.into_bytes()),
        (wrong_tag, body.clone()),
        (data_head(&old, &new, "true"), b"{\"data\":[]}".to_vec()),
    ] {
        let (mut run, _, exchange) = Run::start(Presentation::Quick, Some(stored(&old, "h")), true);
        let mut effects = run.client();
        effects.extend(run.respond(exchange.unwrap(), 200, fields, &body));
        // CL-29: normal load (load_url after the cached page), the entry
        // removed, codes -1 / -1.
        assert!(is_remove(
            writes(&effects)[0],
            Expect::Version(STORED_VERSION)
        ));
        assert_eq!(count(&effects, &Effect::LoadUrl), 1);
        assert_eq!(results(&effects), Vec::<(i32, i32)>::new());
        let effects = run.step(Input::Destroy);
        assert_eq!(results(&effects), vec![(-1, -1)]);
    }
}

#[test]
fn legacy_engine_outcome_template_change_after_the_delivery_reloads() {
    let old = page("10");
    let new = "<html><head><title>New</title></head><body>x</body></html>".to_owned();
    for presentation in [Presentation::Quick, Presentation::Standard] {
        let (mut run, _, exchange) = Run::start(presentation, Some(stored(&old, "h")), true);
        let mut effects = run.client();
        if presentation == Presentation::Standard {
            effects.extend(run.step(Input::Intercept(InterceptId(1))));
            assert_eq!(
                answer(&effects, InterceptId(1)),
                Some(Answer::Doc(old.clone().into_bytes(), true))
            );
        }
        effects = run.step(Input::GetDiff);
        effects.extend(run.respond(
            exchange.unwrap(),
            200,
            legacy_head(&new, "true", "true"),
            new.as_bytes(),
        ));
        assert_eq!(results(&effects), vec![(2000, 2000)]);
        // The callback is dropped: no message until the next get_diff.
        assert_eq!(messages(&effects), Vec::<String>::new());
        match presentation {
            Presentation::Quick => {
                let Some(Effect::LoadData { body, .. }) = effects
                    .iter()
                    .find(|e| matches!(e, Effect::LoadData { .. }))
                else {
                    panic!("{effects:?}");
                };
                assert_eq!(body.as_slice(), new.as_bytes());
            }
            Presentation::Standard => {
                assert_eq!(count(&effects, &Effect::LoadUrl), 1);
                let effects = run.step(Input::Intercept(InterceptId(2)));
                assert_eq!(
                    answer(&effects, InterceptId(2)),
                    Some(Answer::Doc(new.clone().into_bytes(), true))
                );
            }
        }
        let commit = writes(&effects);
        assert_eq!(commit.len(), 1);
        assert_eq!(replaced_body(commit[0]).page.as_slice(), new.as_bytes());
        let effects = run.step(Input::GetDiff);
        assert_eq!(messages(&effects).len(), 1);
    }
}

#[test]
fn legacy_engine_outcome_template_change_store_and_no_commit() {
    let old = page("10");
    let new = "<html><head><title>New</title></head><body>y</body></html>".to_owned();
    // CL-30: `store` after the delivery: no reload, final 304, commit.
    let (mut run, _, exchange) = Run::start(Presentation::Quick, Some(stored(&old, "h")), true);
    let effects = run.client();
    assert!(matches!(effects[0], Effect::LoadData { .. }));
    let effects = run.respond(
        exchange.unwrap(),
        200,
        legacy_head(&new, "true", "store"),
        new.as_bytes(),
    );
    assert_eq!(results(&effects), vec![(2000, 304)]);
    assert_eq!(count(&effects, &Effect::LoadUrl), 0);
    assert!(!effects.iter().any(|e| matches!(e, Effect::LoadData { .. })));
    assert!(matches!(writes(&effects)[0].op, TxnOp::Replace { .. }));
    // Before the delivery: the new document, 2000 / 304; `false`: the old
    // entry removed (DL-132).
    let (mut run, _, exchange) = Run::start(Presentation::Quick, Some(stored(&old, "h")), true);
    let effects = run.respond(
        exchange.unwrap(),
        200,
        legacy_head(&new, "1", "false"),
        new.as_bytes(),
    );
    assert_eq!(results(&effects), vec![(2000, 304)]);
    assert!(is_remove(
        writes(&effects)[0],
        Expect::Version(STORED_VERSION)
    ));
    let effects = run.client();
    let Some(Effect::LoadData { body, .. }) = effects.first() else {
        panic!("{effects:?}");
    };
    assert_eq!(body.as_slice(), new.as_bytes());
}

#[test]
fn legacy_engine_outcome_cut_with_an_entry_keeps_the_cached_page_except_tls() {
    let old = page("10");
    let new = "<html><head><title>New</title></head><body>z</body></html>".to_owned();
    for (error, reloads) in [(TransportError::Read, false), (TransportError::Tls, true)] {
        let (mut run, _, exchange) =
            Run::start(Presentation::Standard, Some(stored(&old, "h")), true);
        let exchange = exchange.unwrap();
        let mut effects = run.head(exchange, 200, legacy_head(&new, "true", "true"));
        effects.extend(run.chunk(exchange, &new.as_bytes()[..10]));
        effects.extend(run.step(Input::TransportFailed { exchange, error }));
        assert_eq!(writes(&effects).len(), 0);
        effects = run.client();
        assert_eq!(effects, vec![Effect::LoadUrl]);
        effects = run.step(Input::Intercept(InterceptId(3)));
        let expected = if reloads {
            Answer::Null
        } else {
            Answer::Doc(old.clone().into_bytes(), true)
        };
        assert_eq!(answer(&effects, InterceptId(3)), Some(expected));
        let effects = run.step(Input::Destroy);
        assert_eq!(results(&effects), vec![(-1, -1)]);
    }
}

// --- codes and messages (CL-22 to CL-24, CL-28, CL-35) ---

#[test]
fn legacy_engine_codes_old_diff_is_not_sent_and_the_page_reloads() {
    let old = page("10");
    let new = page("12");
    let (mut run, _, exchange) = Run::start(Presentation::Quick, Some(stored(&old, "h")), true);
    let mut effects = run.client();
    let (body, _) = data_body(&old, &new);
    effects.extend(run.respond(exchange.unwrap(), 200, data_head(&old, &new, "true"), &body));
    assert_eq!(results(&effects), vec![(200, 200)]);
    run.mono += 30_000;
    effects = run.step(Input::GetDiff);
    assert_eq!(messages(&effects).len(), 1, "30 000 ms is not older");
    run.mono += 1;
    // Sent once already: no later message, no reload.
    effects = run.step(Input::GetDiff);
    assert_eq!(effects, Vec::new());

    let (mut run, _, exchange) = Run::start(Presentation::Quick, Some(stored(&old, "h")), true);
    let mut effects = run.client();
    effects.extend(run.respond(exchange.unwrap(), 200, data_head(&old, &new, "true"), &body));
    run.mono += 30_001;
    effects = run.step(Input::GetDiff);
    // DL-130: never sent: final -1, a normal load.
    assert_eq!(results(&effects), vec![(200, -1)]);
    assert_eq!(effects.last(), Some(&Effect::LoadUrl));
    assert_eq!(messages(&effects), Vec::<String>::new());
    let effects = run.step(Input::Intercept(InterceptId(1)));
    assert_eq!(answer(&effects, InterceptId(1)), Some(Answer::Null));
    let effects = run.step(Input::Destroy);
    assert_eq!(results(&effects), vec![(200, -1)]);
}

#[test]
fn legacy_engine_codes_refresh_starts_a_generation_with_is_reload() {
    let old = page("10");
    let (mut run, _, exchange) = Run::start(Presentation::Quick, Some(stored(&old, "h")), true);
    let mut effects = run.client();
    let first = exchange.unwrap();
    effects.extend(run.respond(first, 304, vec![line("cache-offline", "true")], b""));
    assert_eq!(results(&effects), vec![(304, 304)]);
    // CL-35: a new lookup and a new cookie read; the validators again.
    effects = run.step(Input::Refresh);
    assert!(matches!(effects.as_slice(), [Effect::Lookup { .. }]));
    effects = run.step(Input::Lookup(Box::new(LookupResult {
        entry: Some(stored(&old, "h")),
        disabled: true, // ignored after the open
        partition_epoch: PARTITION_EPOCH,
        site_epoch: SITE_EPOCH,
    })));
    assert_eq!(effects, vec![Effect::ReadCookie]);
    effects = run.step(Input::Cookie(Some("sid=1".to_owned())));
    let Some(Effect::Send(spec)) = effects.first() else {
        panic!("{effects:?}");
    };
    assert_eq!(spec.fields.len(), 4);
    let second = spec.exchange;
    assert_ne!(first, second);
    // A second head of the old exchange (it had its `304`): no effect, also
    // no cookie (DL-229 (e)); `legacy_engine_classify_cookies_of_an_older_
    // generation_once` tests the first head of an older generation.
    effects = run.head(first, 200, vec![line("Set-Cookie", "old=1")]);
    assert_eq!(effects, Vec::new());
    effects = run.step(Input::GetDiff);
    assert_eq!(effects, Vec::new(), "the result was reset");
    effects = run.respond(second, 304, vec![line("cache-offline", "true")], b"");
    assert_eq!(results(&effects), vec![(304, 304)]);
    let messages = messages(&effects);
    assert_eq!(
        messages,
        vec!["{\"code\":304,\"srcCode\":304,\"extra\":{\"cache-offline\":\"true\",\"isReload\":\"true\"}}".to_owned()]
    );
}

#[test]
fn legacy_engine_codes_refresh_data_update_sends_a_diff() {
    // CS-29-like: the document of the old generation counts as delivered.
    let old = page("10");
    let new = page("12");
    let (mut run, _, exchange) = Run::start(Presentation::Standard, Some(stored(&old, "h")), true);
    let mut effects = run.respond(
        exchange.unwrap(),
        304,
        vec![line("cache-offline", "true")],
        b"",
    );
    effects.extend(run.step(Input::Refresh));
    effects.extend(run.step(Input::Lookup(Box::new(LookupResult {
        entry: Some(stored(&old, "h")),
        disabled: false,
        partition_epoch: PARTITION_EPOCH,
        site_epoch: SITE_EPOCH,
    }))));
    effects.extend(run.step(Input::Cookie(None)));
    let second = sent(&effects).unwrap();
    let (body, _) = data_body(&old, &new);
    effects = run.respond(second, 200, data_head(&old, &new, "true"), &body);
    assert_eq!(results(&effects), vec![(200, 200)]);
}

// --- stop ---

#[test]
fn legacy_engine_stop_destroy_after_a_commit_keeps_it_and_answers_nothing_later() {
    let text = page("10");
    let (mut run, _, exchange) = Run::start(Presentation::Standard, None, true);
    let exchange = exchange.unwrap();
    let mut effects = run.client();
    effects.extend(run.step(Input::Intercept(InterceptId(1))));
    effects.extend(run.head(exchange, 200, legacy_head(&text, "true", "true")));
    effects.extend(run.chunk(exchange, &text.as_bytes()[..5]));
    assert_eq!(answer(&effects, InterceptId(1)), Some(Answer::Open));
    effects.extend(run.step(Input::Destroy));
    assert_eq!(
        answer(&effects, InterceptId(1)),
        Some(Answer::Doc(text.as_bytes()[..5].to_vec(), false))
    );
    assert_eq!(effects.last(), Some(&Effect::Closed));
    assert_eq!(run.chunk(exchange, &text.as_bytes()[5..]), Vec::new());
    assert_eq!(run.end(exchange), Vec::new());
}

// --- properties ---

fn arbitrary_input() -> impl Strategy<Value = Input> {
    let text = page("10");
    let new = page("12");
    let (data, _) = data_body(&text, &new);
    let heads = vec![
        legacy_head(&text, "true", "true"),
        legacy_head(&new, "false", "store"),
        data_head(&text, &new, "true"),
        vec![line("cache-offline", "true"), line("set-cookie", "a=1")],
        vec![],
    ];
    let bodies = vec![
        text.clone().into_bytes(),
        data,
        b"\xff".to_vec(),
        Vec::new(),
    ];
    let exchange = (0_u64..4).prop_map(ExchangeId);
    prop_oneof![
        (any::<bool>(), any::<bool>(), 0_u8..3).prop_map(move |(entry, disabled, host)| {
            let host = ["h", "other", "damaged"][usize::from(host)];
            Input::Lookup(Box::new(LookupResult {
                entry: entry.then(|| stored(&page("10"), host)),
                disabled,
                partition_epoch: PARTITION_EPOCH,
                site_epoch: SITE_EPOCH,
            }))
        }),
        prop::option::of("[a-z=;\u{1}]{0,6}").prop_map(Input::Cookie),
        (
            exchange.clone(),
            prop::sample::select(vec![200_u16, 304, 302, 404, 500]),
            prop::sample::select(heads)
        )
            .prop_map(|(exchange, status, fields)| Input::ResponseHead {
                exchange,
                status,
                fields
            }),
        (exchange.clone(), prop::sample::select(bodies), 0_usize..200).prop_map(
            |(exchange, bytes, cut)| Input::BodyChunk {
                exchange,
                bytes: bytes[..cut.min(bytes.len())].to_vec()
            }
        ),
        exchange
            .clone()
            .prop_map(|exchange| Input::BodyEnd { exchange }),
        (
            exchange,
            prop::sample::select(vec![
                TransportError::Refused,
                TransportError::Tls,
                TransportError::Read
            ])
        )
            .prop_map(|(exchange, error)| Input::TransportFailed { exchange, error }),
        Just(Input::BindClient),
        Just(Input::ClientReady),
        (0_u64..4).prop_map(|id| Input::Intercept(InterceptId(id))),
        Just(Input::GetDiff),
        Just(Input::PageFinished),
        Just(Input::Refresh),
        Just(Input::Tick),
        Just(Input::Destroy),
    ]
}

fn run_inputs(presentation: Presentation, inputs: &[(u16, Input)]) -> Vec<Vec<Effect>> {
    let mut engine = LegacyEngine::with_port(config(presentation), FAKE);
    let mut now = Now {
        mono: Instant(0),
        wall: UnixTime(WALL),
    };
    let (id, first) = engine.open(now, context(true));
    let mut all = vec![first];
    let mut closed = false;
    for (advance, input) in inputs {
        now.mono = Instant(now.mono.0 + u64::from(*advance));
        let effects = engine.handle(now, id, input.clone());
        check_writes(&effects);
        if closed {
            assert_eq!(effects, Vec::new(), "an effect after Closed");
        }
        if effects.contains(&Effect::Closed) {
            assert_eq!(effects.last(), Some(&Effect::Closed));
            closed = true;
        }
        assert!(
            !effects.contains(&Effect::ScheduleTimer(Instant(0)))
                && !effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::ScheduleTimer(_)))
        );
        all.push(effects);
    }
    all
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn legacy_engine_prop_fake_record_no_panic_no_effect_after_closed_deterministic(
        quick in any::<bool>(),
        inputs in prop::collection::vec((0_u16..20_000, arbitrary_input()), 0..40),
    ) {
        let presentation = if quick { Presentation::Quick } else { Presentation::Standard };
        let first = run_inputs(presentation, &inputs);
        let second = run_inputs(presentation, &inputs);
        prop_assert_eq!(first, second);
    }
}
