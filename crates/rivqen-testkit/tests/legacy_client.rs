// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! Tests of the runner of the canonical legacy client fixtures (WP-08 S2,
//! T-03; DL-110, DL-118, DL-216; `fixtures/legacy/README.md` "Client
//! fixtures").
#![cfg(feature = "legacy")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::assert_is_empty
)]

use std::collections::BTreeMap;

use rivqen_cache::key::{Partition, Protocol, VaryDigest, VaryEntry};
use rivqen_cache::txn::{
    CacheTransaction, EntryMeta, Epoch, Expect, Preconditions, SnapshotBody, TxnOp, Validators,
    Writer,
};
use rivqen_model::http::FieldLine;
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_model::safe::PageBytes;
use rivqen_model::time::UnixTime;
use rivqen_proto_legacy::message::ResultCode;
use rivqen_session::legacy::record;
use rivqen_session::legacy::types::{
    Effect, ExchangeId, Input, Now, OpenContext, RequestSpec, SessionId,
};
use rivqen_testkit::exchange;
use rivqen_testkit::fixtures::load_json;
use rivqen_testkit::fixtures_root;
use rivqen_testkit::legacy_client::{
    Action, ClientFixture, CookieCall, Difference, InterceptView, Projection, ResultView,
    SessionEngine, compare, expected_client, fixture_ids, is_canonical_id, load, partition_id, run,
    run_with,
};
use rivqen_testkit::store::contract::sample_policy;
use serde_json::Value;

fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    rivqen_integrity::sha256(bytes)
        .as_bytes()
        .iter()
        .fold(String::new(), |mut out, b| {
            write!(out, "{b:02x}").unwrap();
            out
        })
}

// --- the fixture list and the expected projections ---

#[test]
fn legacy_client_ids_are_the_canonical_client_fixtures() {
    let ids = fixture_ids().unwrap();
    let stored: Vec<String> = exchange::load_all()
        .unwrap()
        .into_iter()
        .map(|e| e.id)
        .collect();
    // Every canonical client fixture (kind client, profile legacy-canonical)
    // has a stored exchanges.json, and the directory list gives the same ids.
    assert_eq!(ids, stored);
    assert!(!ids.is_empty());
    let quick = ids.iter().filter(|id| id.ends_with("-Q")).count();
    let standard = ids.iter().filter(|id| id.ends_with("-S")).count();
    assert_eq!(quick + standard, ids.len());
    assert_eq!(
        quick, standard,
        "every fixture has its Quick and Standard form"
    );
}

#[test]
fn legacy_client_ids_never_leave_the_fixture_root() {
    assert!(is_canonical_id("FX-LEG-CL-CS04-Q"));
    for id in [
        "FX-LEG-CL-CS04-Q--ANDROID",
        "FX-LEG-CL-",
        "FX-LEG-CL-../x",
        "FX-LEG-CL-a",
        "FX-LEG-CS04",
        "FX-LEG-CL-A/B",
        "",
    ] {
        assert!(!is_canonical_id(id), "{id}");
        assert!(load(id).is_err(), "{id}");
    }
}

#[test]
fn legacy_client_expected_parses() {
    for id in fixture_ids().unwrap() {
        let fixture = load(&id).unwrap_or_else(|e| panic!("{id}: {e}"));
        // The parsed projection gives the same JSON value again: nothing of
        // the `client` member is lost or added.
        let path = fixtures_root()
            .join("legacy")
            .join(&id)
            .join("expected.json");
        let original = load_json(&path).unwrap();
        let written: Value = serde_json::from_str(&fixture.expected.to_json_text()).unwrap();
        assert_eq!(written, original["client"], "{id}");
        assert!(fixture.expected.problems.is_empty());
        assert_eq!(
            compare(&fixture.expected, &fixture.expected),
            vec![],
            "{id}"
        );
        // The schedule gives one session entry per create_session step.
        assert_eq!(
            fixture.expected.sessions.len(),
            fixture
                .case
                .schedule
                .iter()
                .filter(|s| matches!(s, rivqen_testkit::legacy_client::Step::CreateSession))
                .count(),
            "{id}"
        );
    }
}

#[test]
fn legacy_client_expected_rejects_an_unknown_member() {
    let path = fixtures_root()
        .join("legacy")
        .join("FX-LEG-CL-CS04-Q")
        .join("expected.json");
    let mut value = load_json(&path).unwrap();
    value["client"]["phases"][0]["actions"][0]["extra"] = Value::Bool(true);
    let error = expected_client(&value, "FX-LEG-CL-CS04-Q").unwrap_err();
    assert!(error.contains("unknown member `extra`"), "{error}");
    let value = load_json(&path).unwrap();
    assert!(expected_client(&value, "FX-LEG-CL-CS04-S").is_err());
}

// --- the comparison ---

fn cs04() -> ClientFixture {
    load("FX-LEG-CL-CS04-Q").unwrap()
}

fn one(expected: &Projection, actual: &Projection) -> Difference {
    let differences = compare(expected, actual);
    assert_eq!(differences.len(), 1, "{differences:#?}");
    differences.into_iter().next().unwrap()
}

#[test]
fn legacy_client_compare_an_action_member() {
    let expected = cs04().expected;
    let mut actual = expected.clone();
    let Action::LoadData { body_sha256, .. } = &mut actual.phases[0].actions[0] else {
        panic!("CS04-Q starts with load_data");
    };
    *body_sha256 = "00".to_owned();
    let d = one(&expected, &actual);
    assert_eq!(
        (d.phase, d.member.as_str(), d.index, d.path.as_str()),
        (Some(1), "actions", Some(0), "body_sha256")
    );
    assert_eq!(d.actual.as_deref(), Some("\"00\""));
    // Only the differing values, never the rest of the entry.
    let text = d.to_string();
    assert!(
        text.starts_with("phase 1 actions[0].body_sha256: expected \""),
        "{text}"
    );
    assert!(!text.contains("content-type"), "{text}");
}

#[test]
fn legacy_client_compare_the_action_order() {
    let expected = cs04().expected;
    let mut actual = expected.clone();
    let action = actual.phases[1].actions.remove(0);
    actual.phases[3].actions.insert(0, action);
    // A moved action: missing in one phase, and the order of the other
    // differs; the comparison is in exact order.
    let differences = compare(&expected, &actual);
    assert!(
        differences
            .iter()
            .any(|d| d.phase == Some(2) && d.member == "actions" && d.actual.is_none())
    );
    assert!(
        differences
            .iter()
            .any(|d| d.phase == Some(4) && d.member == "actions")
    );
}

#[test]
fn legacy_client_compare_a_cookie_as_a_multiset() {
    let expected = cs04().expected;
    let mut actual = expected.clone();
    // The order of a multiset does not count.
    actual.phases[0].cookies.reverse();
    actual.phases[0].cookies.push(CookieCall::Set {
        url: "https://shop.example.com/catalog".to_owned(),
        values: vec!["a=1".to_owned()],
    });
    let d = one(&expected, &actual);
    assert_eq!(
        (d.phase, d.member.as_str(), d.expected),
        (Some(1), "cookies", None)
    );
    let mut actual = expected.clone();
    let CookieCall::Get { value, .. } = &mut actual.phases[0].cookies[0] else {
        panic!("a cookie_get");
    };
    *value = Some("session=rq-test-cookie-9".to_owned());
    let d = one(&expected, &actual);
    assert_eq!(
        (d.member.as_str(), d.index, d.path.as_str()),
        ("cookies", Some(0), "value")
    );
    assert_eq!(d.expected.as_deref(), Some("null"));
}

#[test]
fn legacy_client_compare_a_result_and_an_intercept() {
    let expected = cs04().expected;
    let mut actual = expected.clone();
    actual.phases[3].results[0].final_code = 304;
    let d = one(&expected, &actual);
    assert_eq!(
        (d.phase, d.member.as_str(), d.path.as_str()),
        (Some(4), "results", "final_code")
    );
    let mut actual = expected.clone();
    actual.phases[0].intercepts[0] = InterceptView::Null { session: 2 };
    let d = one(&expected, &actual);
    assert_eq!(
        (d.member.as_str(), d.path.as_str()),
        ("intercepts", "session")
    );
    let mut actual = expected.clone();
    actual.phases[1].results.push(ResultView {
        session: 1,
        src_code: -1,
        final_code: -1,
    });
    let d = one(&expected, &actual);
    assert_eq!((d.member.as_str(), d.expected.is_none()), ("results", true));
}

#[test]
fn legacy_client_compare_a_request_response() {
    let expected = cs04().expected;
    let mut actual = expected.clone();
    actual.phases[1].requests[0]
        .response
        .as_mut()
        .unwrap()
        .complete = false;
    let d = one(&expected, &actual);
    assert_eq!(
        (d.phase, d.member.as_str(), d.index, d.path.as_str()),
        (Some(2), "requests", Some(0), "response.complete")
    );
    let mut actual = expected.clone();
    actual.phases[1].requests[0].response = None;
    let differences = compare(&expected, &actual);
    assert!(
        differences
            .iter()
            .any(|d| d.path == "response.status" && d.actual.is_none())
    );
}

#[test]
fn legacy_client_compare_a_snapshot_record() {
    let expected = cs04().expected;
    let mut actual = expected.clone();
    actual.phases[3].snapshots[0].entries[0].etag = Some("x".to_owned());
    let d = one(&expected, &actual);
    assert_eq!(
        (d.phase, d.member.as_str(), d.index, d.path.as_str()),
        (Some(4), "snapshots", Some(0), "entries[0].etag")
    );
    // DL-118: data_sha256 by presence only.
    let mut actual = expected.clone();
    actual.phases[3].snapshots[0].entries[0].data_sha256 = Some("other".to_owned());
    assert_eq!(compare(&expected, &actual), vec![]);
    actual.phases[3].snapshots[0].entries[0].data_sha256 = None;
    let d = one(&expected, &actual);
    assert!(d.path.starts_with("entries[0].data_sha256"), "{d}");
    let mut actual = expected.clone();
    actual.phases[3].snapshots[0].entries.clear();
    let d = one(&expected, &actual);
    assert_eq!((d.path.as_str(), d.actual.is_none()), ("entries[0]", true));
}

#[test]
fn legacy_client_compare_sessions_phases_and_problems() {
    let expected = cs04().expected;
    let mut actual = expected.clone();
    actual.sessions[1].session = None;
    let differences = compare(&expected, &actual);
    assert!(
        differences
            .iter()
            .any(|d| d.member == "sessions" && d.path == "opened")
    );
    let mut actual = expected.clone();
    actual.phases[2].steps = Some((16, 17));
    let d = one(&expected, &actual);
    assert_eq!((d.phase, d.member.as_str()), (Some(3), "steps"));
    let mut actual = expected.clone();
    actual.phases.pop();
    let d = one(&expected, &actual);
    assert_eq!((d.member.as_str(), d.path.as_str()), ("phases", "length"));
    let mut actual = expected.clone();
    actual.problems.push("step 3: something".to_owned());
    let d = one(&expected, &actual);
    assert_eq!(
        (d.member.as_str(), d.actual.as_deref()),
        ("runner", Some("step 3: something"))
    );
}

#[test]
fn legacy_client_projection_debug_has_no_content() {
    let expected = cs04().expected;
    let text = format!("{expected:?}");
    assert!(!text.contains("sonicdiff"), "{text}");
    assert!(!text.contains("a974949d"), "{text}");
    assert!(!text.contains("shop.example.com"), "{text}");
}

// --- the driver on the engine of the base commit ---

#[test]
fn legacy_client_runs_all() {
    let ids = fixture_ids().unwrap();
    for id in &ids {
        let fixture = load(id).unwrap();
        let first = run(&fixture);
        let second = run(&fixture);
        assert_eq!(first, second, "{id}: the run is not deterministic");
        // The phases come from the schedule only: the same split as
        // expected.json for every engine.
        let steps = |p: &Projection| p.phases.iter().map(|p| p.steps).collect::<Vec<_>>();
        assert_eq!(steps(&first), steps(&fixture.expected), "{id}");
        assert_eq!(
            first.sessions.len(),
            fixture.expected.sessions.len(),
            "{id}"
        );
        for (s, e) in first.sessions.iter().zip(&fixture.expected.sessions) {
            assert_eq!(
                (s.step, &s.partition, &s.url),
                (e.step, &e.partition, &e.url),
                "{id}"
            );
        }
    }
}

#[test]
#[ignore = "T-05 makes every canonical legacy client fixture pass"]
fn legacy_client_fixtures_pass() {
    let mut lines = Vec::new();
    let mut failing = 0_usize;
    let ids = fixture_ids().unwrap();
    for id in &ids {
        let fixture = load(id).unwrap();
        let differences = compare(&fixture.expected, &run(&fixture));
        if !differences.is_empty() {
            failing += 1;
        }
        for d in differences {
            lines.push(format!("{id}: {d}"));
        }
    }
    assert!(
        lines.is_empty(),
        "{failing} of {} canonical legacy client fixtures differ ({} differences):\n{}",
        ids.len(),
        lines.len(),
        lines.join("\n")
    );
}

// --- the driver with a scripted engine ---

/// A small scripted engine: it reads, asks for the cookie, sends, answers
/// every intercept with null, loads the URL at `client_ready` and after a
/// transport failure, gives a message at `get_diff`, sets the result at the
/// end of a body and closes at `destroy`. It logs every input with the
/// clock.
#[derive(Default)]
struct Script {
    next: u64,
    exchange: u64,
    sessions: BTreeMap<u64, Open>,
    log: Vec<(u64, String)>,
    refuse_open: bool,
    drop_accept_diff: bool,
    effect_after_close: bool,
    commit: Option<Commit>,
}

struct Open {
    partition: PartitionId,
    key: CacheKey,
    target: String,
    epochs: (Epoch, Epoch),
}

#[derive(Clone)]
struct Commit {
    vary: Vec<VaryEntry>,
    writer: Option<PartitionId>,
}

impl Script {
    fn kinds(&self, prefix: &str) -> Vec<(u64, String)> {
        self.log
            .iter()
            .filter(|(_, k)| k.starts_with(prefix))
            .cloned()
            .collect()
    }

    fn commit(open: &Open, commit: &Commit) -> Effect {
        let meta = EntryMeta {
            key: open.key,
            partition: Partition::Scoped(open.partition),
            host: "shop.example.com".to_owned(),
            protocol: Protocol::Legacy,
            vary: commit.vary.clone(),
            validators: Validators {
                etag: Some("e1".to_owned()),
                template_tag: Some("t1".to_owned()),
                rqp: None,
            },
            policy: sample_policy(0),
            stored_at: UnixTime(0),
            last_used_at: UnixTime(0),
        };
        let body = SnapshotBody {
            page: PageBytes::new(b"P".to_vec()),
            template: Some(PageBytes::new(b"T".to_vec())),
            data: vec![("{a}".to_owned(), PageBytes::new(b"x".to_vec()))],
            fields: vec![
                FieldLine::new("X-A", "1"),
                FieldLine::new("Content-Type", "text/html"),
            ],
        };
        let writer = commit.writer.unwrap_or(open.partition);
        Effect::Cache(Box::new(CacheTransaction {
            op: TxnOp::Replace {
                meta: Box::new(meta),
                body: Box::new(body),
            },
            pre: Preconditions {
                entry: Expect::Any,
                writer: Writer::request(writer, open.epochs.0),
                partition_epoch: open.epochs.0,
                site_epoch: open.epochs.1,
            },
        }))
    }
}

impl SessionEngine for Script {
    fn open(&mut self, now: Now, context: OpenContext) -> (SessionId, Vec<Effect>) {
        self.log.push((now.mono.0, "open".to_owned()));
        let id = self.next;
        self.next += 1;
        let key = record::entry_key(context.partition, &context.url);
        self.sessions.insert(
            id,
            Open {
                partition: context.partition,
                key,
                target: context.url.path().to_owned(),
                epochs: (Epoch(0), Epoch(0)),
            },
        );
        let lookup = Effect::Lookup {
            partition: Partition::Scoped(context.partition),
            key,
        };
        (SessionId(id), vec![lookup])
    }

    fn handle(&mut self, now: Now, session: SessionId, input: Input) -> Vec<Effect> {
        self.log.push((now.mono.0, format!("{input:?}")));
        match input {
            Input::Lookup(result) => {
                if let Some(open) = self.sessions.get_mut(&session.0) {
                    open.epochs = (result.partition_epoch, result.site_epoch);
                }
                if self.refuse_open || result.disabled {
                    vec![Effect::Closed]
                } else {
                    vec![Effect::ReadCookie]
                }
            }
            Input::Cookie(cookie) => {
                self.exchange += 1;
                let mut fields = Vec::new();
                if !self.drop_accept_diff {
                    fields.push(FieldLine::new("accept-diff", "true"));
                }
                if let Some(cookie) = cookie.filter(|c| !c.is_empty()) {
                    fields.push(FieldLine::new("cookie", cookie));
                }
                let target = self.sessions[&session.0].target.clone();
                vec![Effect::Send(RequestSpec {
                    exchange: ExchangeId(self.exchange),
                    target,
                    fields,
                })]
            }
            Input::ResponseHead { fields, .. } => {
                let values: Vec<String> = fields
                    .iter()
                    .filter(|f| f.is("set-cookie"))
                    .map(|f| f.value.clone())
                    .collect();
                if values.is_empty() {
                    vec![]
                } else {
                    vec![Effect::StoreCookies(values)]
                }
            }
            Input::BodyEnd { .. } => {
                let mut out = vec![Effect::Result {
                    src: ResultCode::FirstLoad,
                    final_code: ResultCode::FirstLoad,
                }];
                if let Some(commit) = &self.commit {
                    out.push(Self::commit(&self.sessions[&session.0], commit));
                }
                out
            }
            Input::TransportFailed { .. } | Input::ClientReady => vec![Effect::LoadUrl],
            Input::Intercept(id) => vec![Effect::AnswerNull(id)],
            Input::GetDiff => vec![Effect::Message("{\"code\":304}".to_owned())],
            Input::Destroy if self.effect_after_close => vec![Effect::Closed, Effect::LoadUrl],
            Input::Destroy => vec![Effect::Closed],
            _ => vec![],
        }
    }

    fn partition_changed(
        &mut self,
        _now: Now,
        _old: PartitionId,
    ) -> Vec<(Option<SessionId>, Effect)> {
        vec![]
    }
}

#[test]
fn legacy_client_driver_holds_releases_and_cuts_as_the_model() {
    // Schedule: cut 128, hold at the head, create_session (step 3), …,
    // wait proxy.hold, settle 300, release, wait proxy.response_end, …
    let fixture = load("FX-LEG-CL-INTERRUPTED-STREAM-S").unwrap();
    let mut engine = Script::default();
    let p = run_with(&fixture, &mut engine);
    assert_eq!(p.problems, Vec::<String>::new());
    // The head waits for the release (clock 300); 128 body bytes, then the
    // cut: a read error after the head.
    let heads = engine.kinds("ResponseHead");
    assert_eq!(heads.len(), 1);
    assert_eq!(heads[0].0, 300);
    let chunks = engine.kinds("BodyChunk");
    assert_eq!(chunks.len(), 1);
    assert!(chunks[0].1.contains("len: 128"), "{chunks:?}");
    let failed = engine.kinds("TransportFailed");
    assert_eq!(failed.len(), 1);
    assert!(failed[0].1.contains("Read"), "{failed:?}");
    assert!(engine.kinds("BodyEnd").is_empty());
    // The request is in the phase of its send; its response shows the cut.
    assert_eq!(p.phases.len(), 3);
    let request = &p.phases[0].requests[0];
    assert_eq!(request.exchange, 1);
    assert_eq!(
        request.fields,
        vec![("accept-diff".to_owned(), "true".to_owned())]
    );
    let response = request.response.as_ref().unwrap();
    assert_eq!((response.status, response.complete), (200, false));
    assert_eq!(p.phases[0].requests, fixture.expected.phases[0].requests);
    assert_eq!(p.phases[0].cookies, fixture.expected.phases[0].cookies);
    // Phase 2 has the load_url of the transport failure; phase 3 the
    // message and the codes of the destroy step (no result: -1 / -1).
    assert!(matches!(
        p.phases[1].actions[..],
        [Action::LoadUrl { session: 1, .. }]
    ));
    assert!(matches!(
        p.phases[2].actions[..],
        [Action::Message { session: 1, .. }]
    ));
    assert_eq!(p.phases[2].results, fixture.expected.phases[2].results);
    assert_eq!(
        p.phases[0].intercepts,
        vec![InterceptView::Null { session: 1 }]
    );
    assert_eq!(p.phases[2].snapshots.len(), 1);
    assert!(p.phases[2].snapshots[0].entries.is_empty());
    assert_eq!(p.sessions, fixture.expected.sessions);
}

#[test]
fn legacy_client_driver_reports_what_it_cannot_run() {
    let fixture = load("FX-LEG-CL-INTERRUPTED-STREAM-S").unwrap();
    // A session that gives Lookup and Closed only did not open (CL-07).
    let mut engine = Script {
        refuse_open: true,
        ..Script::default()
    };
    let p = run_with(&fixture, &mut engine);
    assert_eq!(p.sessions[0].session, None);
    let all = p.problems.join("\n");
    assert!(all.contains("no such session opened"), "{all}");
    assert!(all.contains("wait for proxy.hold"), "{all}");
    assert!(
        all.contains("stored exchanges that no request used: 1"),
        "{all}"
    );
    // Another request than the stored one.
    let mut engine = Script {
        drop_accept_diff: true,
        ..Script::default()
    };
    let p = run_with(&fixture, &mut engine);
    assert!(
        p.problems.iter().any(|t| t.contains("the request differs")),
        "{:?}",
        p.problems
    );
    // An effect after Closed.
    let mut engine = Script {
        effect_after_close: true,
        ..Script::default()
    };
    let p = run_with(&fixture, &mut engine);
    assert!(
        p.problems
            .iter()
            .any(|t| t.contains("LoadUrl after Closed")),
        "{:?}",
        p.problems
    );
    assert!(!compare(&fixture.expected, &p).is_empty());
}

#[test]
fn legacy_client_driver_maps_the_store_to_records() {
    let fixture = load("FX-LEG-CL-ACTION-FIELDS-Q").unwrap();
    let mut engine = Script {
        commit: Some(Commit {
            vary: vec![VaryEntry {
                name: "accept-language".to_owned(),
                value: None,
            }],
            writer: None,
        }),
        ..Script::default()
    };
    let p = run_with(&fixture, &mut engine);
    assert_eq!(p.problems, Vec::<String>::new());
    let snapshot = p
        .phases
        .iter()
        .flat_map(|phase| &phase.snapshots)
        .next()
        .unwrap();
    assert_eq!(snapshot.entries.len(), 1);
    let r = &snapshot.entries[0];
    assert_eq!(r.partition.as_deref(), Some("rq-test-account"));
    assert_eq!(r.url, "https://shop.example.com/catalog");
    assert_eq!(r.vary, vec![("accept-language".to_owned(), None)]);
    assert_eq!(
        (r.etag.as_deref(), r.template_tag.as_deref()),
        (Some("e1"), Some("t1"))
    );
    assert_eq!(r.page_sha256.as_deref(), Some(sha256_hex(b"P").as_str()));
    assert_eq!(
        r.template_sha256.as_deref(),
        Some(sha256_hex(b"T").as_str())
    );
    assert_eq!(
        r.data_sha256.as_deref(),
        Some(sha256_hex(br#"{"{a}":"x"}"#).as_str())
    );
    assert_eq!(
        r.fields,
        vec![
            ("content-type".to_owned(), "text/html".to_owned()),
            ("x-a".to_owned(), "1".to_owned()),
        ]
    );
    // The partition ids of the runner are distinct and stable.
    assert_ne!(partition_id(Some("rq-test-account")), partition_id(None));
}

#[test]
fn legacy_client_driver_reports_a_vary_digest_and_a_foreign_writer() {
    let fixture = load("FX-LEG-CL-ACTION-FIELDS-Q").unwrap();
    // DL-127: a stored digest cannot be shown as the value: never a silent
    // pass.
    let mut engine = Script {
        commit: Some(Commit {
            vary: vec![VaryEntry {
                name: "cookie".to_owned(),
                value: Some(VaryDigest::from_bytes([1; 32])),
            }],
            writer: None,
        }),
        ..Script::default()
    };
    let p = run_with(&fixture, &mut engine);
    assert!(
        p.problems.iter().any(|t| t.contains("DL-127")),
        "{:?}",
        p.problems
    );
    assert!(!compare(&fixture.expected, &p).is_empty());
    // DL-220: a write that names another request partition.
    let mut engine = Script {
        commit: Some(Commit {
            vary: vec![],
            writer: Some(partition_id(Some("rq-test-account-2"))),
        }),
        ..Script::default()
    };
    let p = run_with(&fixture, &mut engine);
    assert!(
        p.problems.iter().any(|t| t.contains("DL-220")),
        "{:?}",
        p.problems
    );
}
