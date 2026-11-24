// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The driver: it runs the steps of a case on a session engine and executes
//! every effect on a fake runtime, a fake network (the stored exchanges of
//! `exchanges.json`), a fake web view and the fake store, as the model of
//! `tools/legacy-fixtures/src/client-engine.mjs` runs a case (CL-01 to
//! CL-04).
//!
//! Step semantics (CL-01):
//!
//! - A step runs to quiescence: the driver feeds every answer and every
//!   network event that the step allows before the next step. A queue holds
//!   the work in first-in, first-out order: the answer of an effect is a
//!   later input (the session contract), and an exchange delivers one event
//!   at a time (head, one body chunk to the next stop, end or cut), each as
//!   one queue item.
//! - `hold`, `cut` and `server_down` stop an exchange as in the model: the
//!   hold point (`request` and `head` before the head, `body` after `bytes`
//!   body bytes) and the cut position (from the stored `stop`) are the stops
//!   of the delivery; `release` continues a held exchange.
//! - `settle` advances the clock and fires the timers that are due. `wait`
//!   checks that its event occurred after the last `create_session`.
//!
//! Bounds: each step runs at most [`MAX_WORK_PER_STEP`] queue items and
//! each `settle` at most [`MAX_TICKS_PER_SETTLE`] timer inputs; a stream
//! keeps at most [`MAX_STREAM_BYTES`]; at most [`MAX_PROBLEMS`] problems are
//! kept. An engine that does not settle gives a problem, never a hang.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use rivqen_cache::driver::StorageDriver;
use rivqen_cache::key::{Partition, VaryEntry};
use rivqen_cache::txn::{CacheTransaction, Expect, StoredEntry, TxnOp, TxnOutcome};
use rivqen_integrity::legacy::sha1_hex;
use rivqen_model::http::FieldLine;
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_model::safe::Secret;
use rivqen_model::time::{Instant, UnixTime};
use rivqen_session::legacy::engine::LegacyEngine;
use rivqen_session::legacy::record::{self, NewRecord, RequestValues, WriteBasis};
use rivqen_session::legacy::types::{
    DocumentHead, Effect, ExchangeId, Input, InterceptId, LookupResult, Now, OpenContext,
    RequestSpec, SessionId, TransportError, VaryKey,
};
use rivqen_template::legacy::DataBlock;

use super::ClientFixture;
use super::case::{HoldPoint, Step};
use super::json;
use super::projection::{
    Action, CookieCall, InterceptView, Pair, Phase, Projection, RecordView, RequestView,
    ResponseView, ResultView, SessionView, Snapshot,
};
use crate::exchange::{CacheRecord, Exchange, Stop};
use crate::store::FakeStore;

/// The work of one step: at most this many queue items.
pub const MAX_WORK_PER_STEP: usize = 65_536;
/// The timer inputs of one `settle`: at most this many.
pub const MAX_TICKS_PER_SETTLE: usize = 1_024;
/// The bytes of one answered document stream: at most this many are kept.
pub const MAX_STREAM_BYTES: usize = 64 * 1024 * 1024;
/// At most this many problems are kept for one run.
pub const MAX_PROBLEMS: usize = 256;
/// The wall time at the clock 0 (2026-01-01T00:00:00Z). The wall time is
/// this plus the whole seconds of the model clock (CL-04).
pub const WALL_BASE: u64 = 1_767_225_600;
/// The field line that the fake platform adds to every request (CL-09: the
/// model also sends `accept-encoding: gzip`; transport, not projected).
pub const PLATFORM_FIELD: (&str, &str) = ("accept-encoding", "gzip");

/// The legacy request fields of a request projection (README "request
/// `fields`").
const REQUEST_FIELDS: [&str; 5] = [
    "accept-diff",
    "if-none-match",
    "template-tag",
    "cookie",
    "sonic-sdk-version",
];
/// The response fields of a response projection (README "`response`").
const RESPONSE_FIELDS: [&str; 6] = [
    "etag",
    "template-tag",
    "template-change",
    "cache-offline",
    "sonic-etag-key",
    "set-cookie",
];
/// The bytes of the fake key of the `Vary` digests.
const VARY_KEY: [u8; 32] = [0x5a; 32];
/// The domain of the fake partition ids.
const PARTITION_DOMAIN: &[u8] = b"rivqen-testkit/legacy-client/partition";

/// The legacy session engine as the driver calls it. [`LegacyEngine`]
/// implements it; a test can give another engine (for example a scripted
/// one) to test the driver.
pub trait SessionEngine {
    /// The step `create_session` ([`LegacyEngine::open`]).
    fn open(&mut self, now: Now, context: OpenContext) -> (SessionId, Vec<Effect>);
    /// One input of a session ([`LegacyEngine::handle`]).
    fn handle(&mut self, now: Now, session: SessionId, input: Input) -> Vec<Effect>;
    /// A partition change ([`LegacyEngine::partition_changed`]).
    fn partition_changed(&mut self, now: Now, old: PartitionId)
    -> Vec<(Option<SessionId>, Effect)>;
}

impl SessionEngine for LegacyEngine {
    fn open(&mut self, now: Now, context: OpenContext) -> (SessionId, Vec<Effect>) {
        Self::open(self, now, context)
    }

    fn handle(&mut self, now: Now, session: SessionId, input: Input) -> Vec<Effect> {
        Self::handle(self, now, session, input)
    }

    fn partition_changed(
        &mut self,
        now: Now,
        old: PartitionId,
    ) -> Vec<(Option<SessionId>, Effect)> {
        Self::partition_changed(self, now, old)
    }
}

/// The fake partition id of an account (`None`: the account `null`, a
/// partition of its own, CL-05). The product computes a keyed hash; the
/// runner needs only distinct, stable ids.
#[must_use]
pub fn partition_id(account: Option<&str>) -> PartitionId {
    let digest = match account {
        Some(name) => {
            rivqen_integrity::sha256_parts(&[PARTITION_DOMAIN, b"account", name.as_bytes()])
        }
        None => rivqen_integrity::sha256_parts(&[PARTITION_DOMAIN, b"null"]),
    };
    PartitionId::from_bytes(*digest.as_bytes())
}

/// The fake key of the `Vary` digests.
#[must_use]
pub fn vary_key() -> VaryKey {
    VaryKey(Secret::new(VARY_KEY))
}

/// One session that `create_session` made.
struct SessionState {
    id: SessionId,
    /// The 1-based index among the opened sessions, once known.
    index: Option<u32>,
    partition: PartitionId,
    url: String,
    /// Every effect so far was a `Lookup` (CL-07, CL-08: a session that does
    /// not open gives `Lookup` and `Closed` only).
    only_lookup: bool,
    closed: bool,
    destroyed: bool,
    last_result: Option<(i32, i32)>,
    timer: Option<Instant>,
}

/// One `hold` step.
struct Hold {
    point: HoldPoint,
    bytes: usize,
    released: bool,
    /// The exchange that waits at this hold.
    flight: Option<usize>,
}

/// One step of the delivery of an exchange.
#[derive(Clone, Copy)]
enum Op {
    Pause(usize),
    Head,
    Body(usize),
    Cut,
    End,
}

/// An exchange in delivery.
struct Flight {
    slot: usize,
    exchange: ExchangeId,
    /// The phase and the index of its request projection.
    request: (usize, usize),
    status: u16,
    fields: Vec<FieldLine>,
    body: Vec<u8>,
    ops: Vec<Op>,
    at: usize,
    given: usize,
}

/// An answered or waiting intercepted request.
enum InterceptState {
    Waiting,
    Streaming {
        status: u16,
        fields: Vec<Pair>,
        bytes: Vec<u8>,
        len: u64,
    },
    Done,
}

struct Intercept {
    slot: usize,
    state: InterceptState,
}

enum Work {
    Input(usize, Input),
    Deliver(usize),
}

/// The runtime of the lab (CL-02).
struct Runtime {
    account: Option<String>,
    cookie: Option<String>,
    network: bool,
}

/// The faults that the schedule armed for the next exchange.
#[derive(Default)]
struct Pending {
    hold: Option<usize>,
    cut: Option<usize>,
}

pub(crate) struct Runner<'f, E> {
    fixture: &'f ClientFixture,
    engine: &'f mut E,
    store: FakeStore,
    clock: u64,
    runtime: Runtime,
    sessions: Vec<SessionState>,
    by_id: BTreeMap<SessionId, usize>,
    opened: Vec<usize>,
    created: Vec<SessionView>,
    /// The closed phases, then the current one.
    done: Vec<Phase>,
    current: Phase,
    close_phase: bool,
    step_no: u32,
    cursor: usize,
    exchange_no: u32,
    server_down: bool,
    pending: Pending,
    holds: Vec<Hold>,
    active_hold: Option<usize>,
    flights: Vec<Flight>,
    intercepts: BTreeMap<u64, Intercept>,
    next_intercept: u64,
    queue: VecDeque<Work>,
    budget: usize,
    seen: BTreeSet<String>,
    labels: BTreeMap<[u8; 32], Option<String>>,
    urls: BTreeMap<([u8; 32], [u8; 32]), String>,
    problems: Vec<String>,
}

impl<'f, E: SessionEngine> Runner<'f, E> {
    pub(crate) fn new(fixture: &'f ClientFixture, engine: &'f mut E) -> Self {
        let config = &fixture.case.config;
        Self {
            fixture,
            engine,
            store: FakeStore::new(),
            clock: 0,
            runtime: Runtime {
                account: config.account.clone(),
                cookie: config.cookie.clone(),
                network: config.network_valid,
            },
            sessions: Vec::new(),
            by_id: BTreeMap::new(),
            opened: Vec::new(),
            created: Vec::new(),
            done: Vec::new(),
            current: Phase::default(),
            close_phase: false,
            step_no: 0,
            cursor: 0,
            exchange_no: 0,
            server_down: false,
            pending: Pending::default(),
            holds: Vec::new(),
            active_hold: None,
            flights: Vec::new(),
            intercepts: BTreeMap::new(),
            next_intercept: 1,
            queue: VecDeque::new(),
            budget: MAX_WORK_PER_STEP,
            seen: BTreeSet::new(),
            labels: BTreeMap::new(),
            urls: BTreeMap::new(),
            problems: Vec::new(),
        }
    }

    /// Run the case: seed the store, run every step, give the projection.
    pub(crate) fn run(mut self) -> Projection {
        if self.fixture.case.config.support_local_server {
            self.problem("config.support_local_server: CL-37 has no switch in LegacyConfig");
        }
        if let Err(error) = self.store.recover(self.now().wall) {
            self.problem(&format!("the fake store does not start: {error:?}"));
        }
        self.seed();
        let steps = &self.fixture.case.schedule;
        for step in steps {
            self.step(step);
        }
        if self.cursor < self.fixture.exchanges.exchanges.len() {
            self.problem(&format!(
                "stored exchanges that no request used: {}",
                self.fixture
                    .exchanges
                    .exchanges
                    .len()
                    .saturating_sub(self.cursor)
            ));
        }
        self.finish()
    }

    fn finish(mut self) -> Projection {
        self.done.push(std::mem::take(&mut self.current));
        let phases = self
            .done
            .into_iter()
            .map(|mut phase| {
                phase.cookies.sort_by_cached_key(CookieCall::text);
                phase.intercepts.sort_by_cached_key(InterceptView::text);
                phase.results.sort_by_cached_key(ResultView::text);
                phase
            })
            .collect();
        Projection {
            sessions: self.created,
            phases,
            problems: self.problems,
        }
    }

    // --- bookkeeping ---

    fn now(&self) -> Now {
        Now {
            mono: Instant(self.clock),
            wall: UnixTime(WALL_BASE.saturating_add(self.clock.checked_div(1000).unwrap_or(0))),
        }
    }

    fn problem(&mut self, text: &str) {
        if self.problems.len() < MAX_PROBLEMS {
            self.problems.push(format!("step {}: {text}", self.step_no));
        } else if self.problems.len() == MAX_PROBLEMS {
            self.problems
                .push("more problems are not listed".to_owned());
        }
    }

    fn emit(&mut self, event: &str) {
        self.seen.insert(event.to_owned());
    }

    fn phase(&mut self) -> &mut Phase {
        &mut self.current
    }

    /// The request projection at `(phase, index)`.
    fn request_mut(&mut self, (phase, index): (usize, usize)) -> Option<&mut RequestView> {
        let phase = if phase == self.done.len() {
            &mut self.current
        } else {
            self.done.get_mut(phase)?
        };
        phase.requests.get_mut(index)
    }

    fn partition(&mut self, account: Option<&str>) -> PartitionId {
        let id = partition_id(account);
        self.labels
            .insert(*id.as_bytes(), account.map(str::to_owned));
        id
    }

    fn index_of(&mut self, slot: usize) -> u32 {
        let next = u32::try_from(self.opened.len().saturating_add(1)).unwrap_or(u32::MAX);
        let Some(session) = self.sessions.get_mut(slot) else {
            return 0;
        };
        if let Some(index) = session.index {
            return index;
        }
        session.index = Some(next);
        self.opened.push(slot);
        next
    }

    fn url_of(&self, slot: usize) -> String {
        self.sessions
            .get(slot)
            .map(|s| s.url.clone())
            .unwrap_or_default()
    }

    fn target(&mut self, session: Option<u32>, word: &str) -> Option<usize> {
        let found = match session {
            None => self.opened.last().copied(),
            Some(n) => usize::try_from(n)
                .ok()
                .and_then(|n| n.checked_sub(1))
                .and_then(|i| self.opened.get(i).copied()),
        };
        if found.is_none() {
            self.problem(&format!(
                "{word} acts on a session, but no such session opened"
            ));
        }
        found
    }

    // --- the steps ---

    fn step(&mut self, step: &Step) {
        if self.close_phase {
            self.done.push(std::mem::take(&mut self.current));
            self.close_phase = false;
        }
        self.step_no = self.step_no.saturating_add(1);
        let number = self.step_no;
        let phase = self.phase();
        phase.steps = Some(
            phase
                .steps
                .map_or((number, number), |(first, _)| (first, number)),
        );
        self.budget = MAX_WORK_PER_STEP;
        match step {
            Step::CreateSession => self.create_session(),
            Step::BindClient(s) => self.simple(*s, "bind_client", Input::BindClient),
            Step::ClientReady(s) => self.simple(*s, "client_ready", Input::ClientReady),
            Step::PageFinished(s) => self.simple(*s, "page_finished", Input::PageFinished),
            Step::GetDiff(s) => self.simple(*s, "get_diff", Input::GetDiff),
            Step::Refresh(s) => self.simple(*s, "refresh", Input::Refresh),
            Step::RequestResource(s) => self.request_resource(*s),
            Step::Destroy(s) => self.destroy(*s),
            Step::SetAccount(account) => self.set_account(account.clone()),
            Step::SetCookie(cookie) => self.runtime.cookie.clone_from(cookie),
            Step::SetNetwork(valid) => self.runtime.network = *valid,
            Step::Hold { point, bytes } => self.hold(*point, *bytes),
            Step::Release => self.release(),
            Step::Cut(bytes) => self.pending.cut = Some(*bytes),
            Step::ServerDown => self.server_down(),
            Step::ServerUp => self.server_down = false,
            Step::Wait(event) => {
                if !self.seen.contains(event) {
                    self.problem(&format!(
                        "wait for {event}: the event did not occur after the last create_session"
                    ));
                }
            }
            Step::Settle(ms) => {
                self.clock = self.clock.saturating_add(*ms);
                self.fire_timers();
                self.close_phase = true;
            }
            Step::SnapshotCache => {
                let snapshot = self.snapshot();
                self.phase().snapshots.push(snapshot);
                self.emit("cache.snapshot");
            }
            // Already applied in the stored responses (DL-216), or a step
            // that a later version adds.
            _ => {}
        }
    }

    fn create_session(&mut self) {
        self.seen.clear();
        self.emit("session.created");
        let account = self.runtime.account.clone();
        let partition = self.partition(account.as_deref());
        let url_text = self.fixture.case.url.clone();
        let mut view = SessionView {
            step: self.step_no,
            session: None,
            partition: account.clone(),
            url: url_text.clone(),
        };
        let Ok(url) = rivqen_url::parse(&url_text) else {
            self.problem("the case url does not parse");
            self.created.push(view);
            return;
        };
        let context = OpenContext {
            url,
            partition,
            anonymous: account.is_none(),
            network_valid: self.runtime.network,
            platform_fields: vec![FieldLine::new(PLATFORM_FIELD.0, PLATFORM_FIELD.1)],
            vary_key: vary_key(),
        };
        let (id, effects) = self.engine.open(self.now(), context);
        if self.by_id.contains_key(&id) {
            self.problem(&format!(
                "open gave the id of an earlier session ({})",
                id.0
            ));
            self.created.push(view);
            return;
        }
        let slot = self.sessions.len();
        self.sessions.push(SessionState {
            id,
            index: None,
            partition,
            url: url_text,
            only_lookup: true,
            closed: false,
            destroyed: false,
            last_result: None,
            timer: None,
        });
        self.by_id.insert(id, slot);
        self.execute(Some(slot), effects);
        self.drain();
        // CL-07, CL-08: a session that gives only `Lookup` and then `Closed`
        // did not open.
        let not_opened = self
            .sessions
            .get(slot)
            .is_some_and(|s| s.closed && s.only_lookup && s.index.is_none());
        if !not_opened {
            view.session = Some(self.index_of(slot));
        }
        self.created.push(view);
    }

    fn simple(&mut self, session: Option<u32>, word: &str, input: Input) {
        if let Some(slot) = self.target(session, word) {
            self.queue.push_back(Work::Input(slot, input));
            self.drain();
        }
    }

    fn request_resource(&mut self, session: Option<u32>) {
        let Some(slot) = self.target(session, "request_resource") else {
            return;
        };
        let closed = self.sessions.get(slot).is_none_or(|s| s.closed);
        if closed {
            // No session: the web view loads the document itself.
            let index = self.index_of(slot);
            self.phase()
                .intercepts
                .push(InterceptView::Null { session: index });
            self.emit("client.intercept");
            return;
        }
        let id = self.next_intercept;
        self.next_intercept = id.saturating_add(1);
        self.intercepts.insert(
            id,
            Intercept {
                slot,
                state: InterceptState::Waiting,
            },
        );
        self.queue
            .push_back(Work::Input(slot, Input::Intercept(InterceptId(id))));
        self.drain();
    }

    fn destroy(&mut self, session: Option<u32>) {
        let Some(slot) = self.target(session, "destroy") else {
            return;
        };
        if self.sessions.get(slot).is_none_or(|s| s.destroyed) {
            return;
        }
        self.queue.push_back(Work::Input(slot, Input::Destroy));
        self.drain();
        let index = self.index_of(slot);
        let Some(state) = self.sessions.get_mut(slot) else {
            return;
        };
        state.destroyed = true;
        let (src_code, final_code) = state.last_result.unwrap_or((-1, -1));
        self.phase().results.push(ResultView {
            session: index,
            src_code,
            final_code,
        });
        self.emit("session.result");
    }

    fn set_account(&mut self, account: Option<String>) {
        if account == self.runtime.account {
            return;
        }
        let old = self.runtime.account.take();
        let old_id = self.partition(old.as_deref());
        self.runtime.account = account;
        let effects = self.engine.partition_changed(self.now(), old_id);
        for (session, effect) in effects {
            let slot = match session {
                None => None,
                Some(id) => {
                    if let Some(slot) = self.by_id.get(&id).copied() {
                        Some(slot)
                    } else {
                        self.problem(&format!("an effect names an unknown session ({})", id.0));
                        continue;
                    }
                }
            };
            self.effect(slot, effect);
        }
        self.drain();
    }

    fn hold(&mut self, point: HoldPoint, bytes: usize) {
        if self.active_hold.is_some() {
            self.problem("hold: a hold is already armed or active");
            return;
        }
        let id = self.holds.len();
        self.holds.push(Hold {
            point,
            bytes,
            released: false,
            flight: None,
        });
        self.pending.hold = Some(id);
        self.active_hold = Some(id);
    }

    fn release(&mut self) {
        let Some(id) = self.active_hold.take() else {
            self.problem("release without a hold");
            return;
        };
        self.emit("proxy.release");
        let flight = self.holds.get_mut(id).and_then(|hold| {
            hold.released = true;
            hold.flight
        });
        if let Some(flight) = flight {
            self.queue.push_back(Work::Deliver(flight));
            self.drain();
        }
    }

    fn server_down(&mut self) {
        let held = self
            .active_hold
            .and_then(|id| self.holds.get(id))
            .is_some_and(|hold| hold.flight.is_some());
        if held {
            self.problem("server_down while an exchange is held");
        }
        self.server_down = true;
        self.pending = Pending::default();
        self.active_hold = None;
    }

    fn fire_timers(&mut self) {
        for _ in 0..MAX_TICKS_PER_SETTLE {
            let now = self.clock;
            let due = self
                .sessions
                .iter()
                .enumerate()
                .filter_map(|(slot, s)| s.timer.filter(|at| at.0 <= now).map(|at| (at, slot)))
                .min();
            let Some((_, slot)) = due else {
                return;
            };
            if let Some(state) = self.sessions.get_mut(slot) {
                state.timer = None;
            }
            self.budget = MAX_WORK_PER_STEP;
            self.queue.push_back(Work::Input(slot, Input::Tick));
            self.drain();
        }
        self.problem(&format!(
            "more than {MAX_TICKS_PER_SETTLE} timer inputs in one settle"
        ));
    }

    // --- the queue ---

    fn drain(&mut self) {
        while let Some(work) = self.queue.pop_front() {
            if self.budget == 0 {
                self.problem(&format!(
                    "the engine does not settle: more than {MAX_WORK_PER_STEP} inputs in one step"
                ));
                self.queue.clear();
                return;
            }
            self.budget = self.budget.saturating_sub(1);
            match work {
                Work::Input(slot, input) => self.feed(slot, input),
                Work::Deliver(flight) => self.deliver(flight),
            }
        }
    }

    fn feed(&mut self, slot: usize, input: Input) {
        let Some(id) = self.sessions.get(slot).map(|s| s.id) else {
            return;
        };
        let effects = self.engine.handle(self.now(), id, input);
        self.execute(Some(slot), effects);
    }

    fn execute(&mut self, slot: Option<usize>, effects: Vec<Effect>) {
        for effect in effects {
            self.effect(slot, effect);
        }
    }

    // --- the effects ---

    fn effect(&mut self, slot: Option<usize>, effect: Effect) {
        let Some(slot) = slot else {
            // An effect of the engine that names no session: only the revoke
            // of a partition (partition_changed with no open session).
            if let Effect::Cache(txn) = effect {
                self.apply(None, *txn);
            } else {
                self.problem(&format!("{effect:?} names no session"));
            }
            return;
        };
        let Some(closed) = self.sessions.get(slot).map(|s| s.closed) else {
            return;
        };
        if closed {
            self.problem(&format!("{effect:?} after Closed"));
            return;
        }
        if !matches!(effect, Effect::Lookup { .. } | Effect::Closed) {
            if let Some(state) = self.sessions.get_mut(slot) {
                state.only_lookup = false;
            }
            self.index_of(slot);
        }
        self.session_effect(slot, effect);
    }

    /// One effect of the open session of `slot`.
    fn session_effect(&mut self, slot: usize, effect: Effect) {
        match effect {
            Effect::Lookup { partition, key } => self.lookup(slot, partition, key),
            Effect::ReadCookie => {
                let url = self.url_of(slot);
                let value = self.runtime.cookie.clone();
                self.phase().cookies.push(CookieCall::Get {
                    url,
                    value: value.clone(),
                });
                self.emit("runtime.cookie_get");
                self.queue
                    .push_back(Work::Input(slot, Input::Cookie(value)));
            }
            Effect::Send(spec) => self.send(slot, &spec),
            Effect::StoreCookies(values) => {
                let url = self.url_of(slot);
                self.phase().cookies.push(CookieCall::Set { url, values });
                self.emit("runtime.cookie_set");
            }
            Effect::LoadUrl => {
                let session = self.index_of(slot);
                let url = self.url_of(slot);
                self.phase().actions.push(Action::LoadUrl { session, url });
                self.emit("client.load_url");
            }
            Effect::LoadData { head, body } => {
                let session = self.index_of(slot);
                let url = self.url_of(slot);
                self.phase().actions.push(Action::LoadData {
                    session,
                    url,
                    fields: sorted_fields(&head.fields),
                    body_len: len_u64(body.len()),
                    body_sha256: json::sha256_hex(&body),
                });
                self.emit("client.load_data");
            }
            Effect::AnswerNull(id) => {
                if self.answer(slot, id) {
                    let session = self.index_of(slot);
                    self.phase()
                        .intercepts
                        .push(InterceptView::Null { session });
                    self.emit("client.intercept");
                }
            }
            Effect::AnswerDocument { intercept, head } => {
                self.answer_document(slot, intercept, &head);
            }
            Effect::StreamChunk { intercept, bytes } => self.stream_chunk(slot, intercept, &bytes),
            Effect::StreamEnd {
                intercept,
                complete,
            } => self.stream_end(slot, intercept, complete),
            Effect::Message(text) => {
                let session = self.index_of(slot);
                self.phase().actions.push(Action::Message {
                    session,
                    json: text,
                });
                self.emit("session.message");
            }
            Effect::Result { src, final_code } => {
                if let Some(state) = self.sessions.get_mut(slot) {
                    state.last_result = Some((src.value(), final_code.value()));
                }
            }
            Effect::Cache(txn) => self.apply(Some(slot), *txn),
            Effect::ScheduleTimer(at) => {
                if let Some(state) = self.sessions.get_mut(slot) {
                    state.timer = Some(at);
                }
            }
            Effect::Closed => {
                if let Some(state) = self.sessions.get_mut(slot) {
                    state.closed = true;
                    state.timer = None;
                }
            }
            other => self.problem(&format!(
                "{other:?}: an effect that the runner does not know"
            )),
        }
    }

    fn lookup(&mut self, slot: usize, partition: Partition, key: CacheKey) {
        let Some(own) = self.sessions.get(slot).map(|s| s.partition) else {
            return;
        };
        if partition != Partition::Scoped(own) {
            self.problem("Lookup names another partition than the session");
        }
        if let Partition::Scoped(id) = partition {
            let url = self.url_of(slot);
            self.urls.insert((*id.as_bytes(), *key.as_bytes()), url);
        }
        let wall = self.now().wall;
        let disabled = self.store.key_disabled(partition, &key, wall);
        let entry = self.store.read(partition, &key, wall);
        let partition_epoch = self.store.partition_epoch(partition);
        let site_epoch = self.store.site_epoch();
        let (Ok(disabled), Ok(partition_epoch), Ok(site_epoch)) =
            (disabled, partition_epoch, site_epoch)
        else {
            self.problem("the fake store fails a lookup");
            return;
        };
        // A damaged entry is removed by the read (R-CACHE-06): no entry.
        let entry = entry.ok().flatten();
        let result = LookupResult {
            entry,
            disabled,
            partition_epoch,
            site_epoch,
        };
        self.queue
            .push_back(Work::Input(slot, Input::Lookup(Box::new(result))));
    }

    fn apply(&mut self, slot: Option<usize>, txn: CacheTransaction) {
        let keyed = !matches!(
            txn.op,
            TxnOp::RevokePartition { .. } | TxnOp::ClearSite { .. }
        );
        if let Some(own) = slot.and_then(|s| self.sessions.get(s)).map(|s| s.partition)
            && keyed
            && txn.pre.writer.request_epoch().map(|(p, _)| p) != Some(own)
        {
            self.problem("a cache write does not name the request partition (DL-220)");
        }
        if let (Some(slot), TxnOp::Replace { meta, .. }) = (slot, &txn.op)
            && let Partition::Scoped(id) = meta.partition
        {
            let url = self.url_of(slot);
            self.urls
                .entry((*id.as_bytes(), *meta.key.as_bytes()))
                .or_insert(url);
        }
        match self.store.apply(txn) {
            Ok(_) => {}
            Err(error) => self.problem(&format!("the fake store fails a write: {error:?}")),
        }
    }

    fn send(&mut self, slot: usize, spec: &RequestSpec) {
        let mut sent = spec.fields.clone();
        sent.push(FieldLine::new(PLATFORM_FIELD.0, PLATFORM_FIELD.1));
        let faults = std::mem::take(&mut self.pending);
        let Some(stored) = self.fixture.exchanges.exchanges.get(self.cursor) else {
            self.problem("Send: no stored exchange is left (the request has no answer)");
            return;
        };
        self.cursor = self.cursor.saturating_add(1);
        let index = self.index_of(slot);
        self.check_request(stored, index, spec, &sent);
        if stored.stop == Stop::Refused {
            if !self.server_down {
                self.problem("the stored exchange is refused, but the server is up");
            }
            self.queue.push_back(Work::Input(
                slot,
                Input::TransportFailed {
                    exchange: spec.exchange,
                    error: TransportError::Refused,
                },
            ));
            return;
        }
        if self.server_down {
            self.problem("the server is down, but the stored exchange has a response");
        }
        self.exchange_no = self.exchange_no.saturating_add(1);
        if stored.number != Some(self.exchange_no) {
            self.problem(&format!(
                "the stored exchange is number {:?}, the runner sends number {}",
                stored.number, self.exchange_no
            ));
        }
        let phase_at = self.done.len();
        let phase = self.phase();
        let request_at = phase.requests.len();
        phase.requests.push(RequestView {
            exchange: stored.number.unwrap_or(0),
            method: "GET".to_owned(),
            target: spec.target.clone(),
            fields: pick(&sent, &REQUEST_FIELDS),
            response: None,
        });
        self.emit("proxy.request");
        let Some(response) = stored.response.as_ref() else {
            self.problem("the stored exchange has no response");
            return;
        };
        let body = response
            .body
            .as_ref()
            .map(|b| b.bytes().to_vec())
            .unwrap_or_default();
        let cut = match stored.stop {
            Stop::Cut { at } => Some(at.min(body.len())),
            _ => None,
        };
        if cut.is_some() != faults.cut.is_some() {
            self.problem("the stored stop does not follow the cut steps of the schedule");
        }
        let ops = self.ops(faults.hold, cut, body.len());
        let flight = self.flights.len();
        self.flights.push(Flight {
            slot,
            exchange: spec.exchange,
            request: (phase_at, request_at),
            status: response.status,
            fields: response.fields.clone(),
            body,
            ops,
            at: 0,
            given: 0,
        });
        self.queue.push_back(Work::Deliver(flight));
    }

    fn check_request(
        &mut self,
        stored: &Exchange,
        index: u32,
        spec: &RequestSpec,
        sent: &[FieldLine],
    ) {
        if stored.step != self.step_no || stored.session != index {
            self.problem(&format!(
                "the next stored exchange was sent at step {} by session {}, not at step {} by session {index}",
                stored.step, stored.session, self.step_no
            ));
        }
        let same_fields = stored.request.fields.len() == sent.len()
            && stored
                .request
                .fields
                .iter()
                .zip(sent)
                .all(|(a, b)| a.name.eq_ignore_ascii_case(&b.name) && a.value == b.value);
        if stored.request.method != "GET" || stored.request.target != spec.target || !same_fields {
            self.problem(&format!(
                "the request differs from the stored request: sent {}, stored {} (the stored response is used)",
                json::pairs(&lower(sent)),
                json::pairs(&lower(&stored.request.fields))
            ));
        }
    }

    /// The delivery steps of an exchange, as `send` and `pump` of the model.
    fn ops(&self, hold: Option<usize>, cut: Option<usize>, len: usize) -> Vec<Op> {
        let mut ops = Vec::new();
        let hold = hold.and_then(|id| self.holds.get(id).map(|h| (id, h.point, h.bytes)));
        if let Some((id, HoldPoint::Request | HoldPoint::Head, _)) = hold {
            ops.push(Op::Pause(id));
        }
        ops.push(Op::Head);
        // (position, hold id or None for the cut); a stable sort keeps the
        // hold before a cut at the same position.
        let mut stops: Vec<(usize, Option<usize>)> = Vec::new();
        if let Some((id, HoldPoint::Body, bytes)) = hold {
            stops.push((bytes.min(len), Some(id)));
        }
        if let Some(at) = cut {
            stops.push((at, None));
        }
        stops.sort_by_key(|s| s.0);
        for (at, stop) in stops {
            ops.push(Op::Body(at));
            if let Some(id) = stop {
                ops.push(Op::Pause(id));
            } else {
                ops.push(Op::Cut);
                return ops;
            }
        }
        ops.push(Op::Body(len));
        ops.push(Op::End);
        ops
    }

    /// The next event of an exchange: one engine input, or a pause. The
    /// next delivery goes to the back of the queue, after the work that the
    /// input queued.
    fn deliver(&mut self, flight: usize) {
        loop {
            let Some(f) = self.flights.get_mut(flight) else {
                return;
            };
            let Some(op) = f.ops.get(f.at).copied() else {
                return;
            };
            f.at = f.at.saturating_add(1);
            let (slot, exchange, request) = (f.slot, f.exchange, f.request);
            match op {
                Op::Pause(id) => {
                    self.emit("proxy.hold");
                    let Some(hold) = self.holds.get_mut(id) else {
                        continue;
                    };
                    hold.flight = Some(flight);
                    if !hold.released {
                        return;
                    }
                }
                Op::Head => {
                    let Some((status, fields)) = self
                        .flights
                        .get(flight)
                        .map(|f| (f.status, f.fields.clone()))
                    else {
                        return;
                    };
                    if let Some(view) = self.request_mut(request) {
                        view.response = Some(ResponseView {
                            status,
                            fields: pick(&fields, &RESPONSE_FIELDS),
                            complete: false,
                        });
                    }
                    self.emit("proxy.response_head");
                    self.feed(
                        slot,
                        Input::ResponseHead {
                            exchange,
                            status,
                            fields,
                        },
                    );
                    self.queue.push_back(Work::Deliver(flight));
                    return;
                }
                Op::Body(to) => {
                    let Some(f) = self.flights.get_mut(flight) else {
                        return;
                    };
                    if to <= f.given {
                        continue;
                    }
                    let bytes = f
                        .body
                        .get(f.given..to)
                        .map(<[u8]>::to_vec)
                        .unwrap_or_default();
                    f.given = to;
                    self.feed(slot, Input::BodyChunk { exchange, bytes });
                    self.queue.push_back(Work::Deliver(flight));
                    return;
                }
                Op::Cut => {
                    self.emit("proxy.cut");
                    self.emit("proxy.response_end");
                    self.feed(
                        slot,
                        Input::TransportFailed {
                            exchange,
                            error: TransportError::Read,
                        },
                    );
                    return;
                }
                Op::End => {
                    if let Some(response) =
                        self.request_mut(request).and_then(|r| r.response.as_mut())
                    {
                        response.complete = true;
                    }
                    self.emit("proxy.response_end");
                    self.feed(slot, Input::BodyEnd { exchange });
                    return;
                }
            }
        }
    }

    // --- the intercept answers ---

    /// Marks a waiting intercept of `slot` answered; a problem when it is not
    /// one.
    fn answer(&mut self, slot: usize, id: InterceptId) -> bool {
        match self.intercepts.get_mut(&id.0) {
            Some(i) if i.slot == slot && matches!(i.state, InterceptState::Waiting) => {
                i.state = InterceptState::Done;
                true
            }
            _ => {
                self.problem(&format!(
                    "an answer to intercept {} that does not wait",
                    id.0
                ));
                false
            }
        }
    }

    fn answer_document(&mut self, slot: usize, id: InterceptId, head: &DocumentHead) {
        if self.answer(slot, id)
            && let Some(i) = self.intercepts.get_mut(&id.0)
        {
            i.state = InterceptState::Streaming {
                status: head.status,
                fields: sorted_fields(&head.fields),
                bytes: Vec::new(),
                len: 0,
            };
        }
    }

    fn stream_chunk(&mut self, slot: usize, id: InterceptId, chunk: &[u8]) {
        let mut over = false;
        match self.intercepts.get_mut(&id.0) {
            Some(Intercept {
                slot: owner,
                state: InterceptState::Streaming { bytes, len, .. },
            }) if *owner == slot => {
                *len = len.saturating_add(len_u64(chunk.len()));
                if bytes.len().saturating_add(chunk.len()) <= MAX_STREAM_BYTES {
                    bytes.extend_from_slice(chunk);
                } else {
                    over = true;
                }
            }
            _ => {
                self.problem(&format!(
                    "StreamChunk of intercept {} without a stream",
                    id.0
                ));
                return;
            }
        }
        if over {
            self.problem(&format!("the stream is over {MAX_STREAM_BYTES} bytes"));
        }
    }

    fn stream_end(&mut self, slot: usize, id: InterceptId, complete: bool) {
        let Some(intercept) = self.intercepts.get_mut(&id.0) else {
            self.problem(&format!("StreamEnd of an unknown intercept {}", id.0));
            return;
        };
        if intercept.slot != slot {
            self.problem(&format!(
                "StreamEnd of intercept {} of another session",
                id.0
            ));
            return;
        }
        let state = std::mem::replace(&mut intercept.state, InterceptState::Done);
        let InterceptState::Streaming {
            status,
            fields,
            bytes,
            len,
        } = state
        else {
            self.problem(&format!("StreamEnd of intercept {} without a stream", id.0));
            return;
        };
        let session = self.index_of(slot);
        self.phase().intercepts.push(InterceptView::Document {
            session,
            status,
            fields,
            body_len: len,
            body_sha256: json::sha256_hex(&bytes),
            complete,
        });
        self.emit("client.intercept");
    }

    // --- the cache ---

    /// The CL-38 records of `cache_before`, through the commit of the
    /// session record (so that the stored form is the form of the session).
    fn seed(&mut self) {
        let records = &self.fixture.exchanges.cache_before;
        if records.len() != self.fixture.case.cache_before {
            self.problem(&format!(
                "case.json has {} cache_before entries, exchanges.json {}",
                self.fixture.case.cache_before,
                records.len()
            ));
        }
        for (i, record) in records.iter().enumerate() {
            if let Err(why) = self.seed_one(record) {
                self.problem(&format!("cache_before[{i}]: {why}"));
            }
        }
    }

    fn seed_one(&mut self, record: &CacheRecord) -> Result<(), String> {
        let partition = self.partition(record.partition.as_deref());
        let url = rivqen_url::parse(&record.url).map_err(|_| "the url does not parse")?;
        let key = record::entry_key(partition, &url);
        self.urls
            .insert((*partition.as_bytes(), *key.as_bytes()), record.url.clone());
        let scoped = Partition::Scoped(partition);
        let partition_epoch = self
            .store
            .partition_epoch(scoped)
            .map_err(|e| format!("{e:?}"))?;
        let site_epoch = self.store.site_epoch().map_err(|e| format!("{e:?}"))?;
        let basis = WriteBasis {
            partition,
            key,
            partition_epoch,
            site_epoch,
            expect: Expect::Any,
        };
        let page = std::str::from_utf8(record.page.bytes()).map_err(|_| "the page is not UTF-8")?;
        let data: Vec<DataBlock> = record
            .data
            .iter()
            .map(|(key, value)| DataBlock {
                key: key.clone(),
                value: value.clone(),
            })
            .collect();
        let vary = Self::seed_vary(record)?;
        let wall = self.now().wall;
        let new = NewRecord {
            host: host_of(url.origin()),
            vary: &vary,
            page,
            template: &record.template,
            data: &data,
            fields: &record.fields,
            etag: sha1_hex(page.as_bytes()),
            template_tag: sha1_hex(record.template.as_bytes()),
            response_fields: &record.fields,
            request_time: wall,
            response_time: wall,
        };
        let mut txn = record::commit(&basis, &new);
        // CL-40 fixtures store an etag or a template tag that is not the
        // SHA-1 of the page or the template; `NewRecord` takes only computed
        // SHA-1 values, so the runner sets the stored validators.
        if let TxnOp::Replace { meta, .. } = &mut txn.op {
            meta.validators.etag = Some(record.etag.clone());
            meta.validators.template_tag = Some(record.template_tag.clone());
        }
        match self.store.apply(txn) {
            Ok(TxnOutcome::Applied { .. }) => Ok(()),
            Ok(TxnOutcome::Refused(why)) => Err(format!("the store refuses the record: {why:?}")),
            Err(e) => Err(format!("the store fails: {e:?}")),
        }
    }

    /// The `Vary` entries of a seeded record, digested by the session's own
    /// function (`record::vary_entries`) with the stored values as request
    /// values.
    fn seed_vary(record: &CacheRecord) -> Result<Vec<VaryEntry>, String> {
        if record.vary.is_empty() {
            return Ok(Vec::new());
        }
        let names: Vec<&str> = record.vary.iter().map(|(n, _)| n.as_str()).collect();
        let response = [FieldLine::new("vary", names.join(", "))];
        let session_fields: Vec<FieldLine> = record
            .vary
            .iter()
            .filter_map(|(n, v)| v.as_ref().map(|v| FieldLine::new(n.clone(), v.clone())))
            .collect();
        let values = RequestValues {
            session_fields: &session_fields,
            platform_fields: &[],
        };
        record::vary_entries(&response, &values, &vary_key()).ok_or_else(|| {
            "record::vary_entries gives no entries for the stored Vary names".to_owned()
        })
    }

    fn snapshot(&mut self) -> Snapshot {
        let wall = self.now().wall;
        let list = match self.store.list() {
            Ok(list) => list,
            Err(error) => {
                self.problem(&format!("the fake store fails a list: {error:?}"));
                return Snapshot::default();
            }
        };
        let mut entries = Vec::new();
        for summary in list {
            let meta = summary.meta;
            let Partition::Scoped(id) = meta.partition else {
                self.problem("an entry in the public partition");
                continue;
            };
            let label = self.labels.get(id.as_bytes()).cloned();
            let url = self
                .urls
                .get(&(*id.as_bytes(), *meta.key.as_bytes()))
                .cloned();
            let (Some(partition), Some(url)) = (label, url) else {
                self.problem("an entry with a partition or a key that no session or seed used");
                continue;
            };
            let Ok(Some(entry)) = self.store.read(meta.partition, &meta.key, wall) else {
                self.problem("an entry that the fake store does not give back");
                continue;
            };
            entries.push(self.record_view(partition, url, &entry));
        }
        entries.sort_by_cached_key(RecordView::sort_key);
        Snapshot { entries }
    }

    fn record_view(
        &mut self,
        partition: Option<String>,
        url: String,
        entry: &StoredEntry,
    ) -> RecordView {
        let mut vary = Vec::new();
        for item in &entry.meta.vary {
            if item.value.is_none() {
                vary.push((item.name.clone(), None));
            } else {
                // DL-127: the store keeps a keyed digest, the projection the
                // value. No canonical fixture has a counted Vary name
                // (CL-12); the runner cannot show the value, so the record
                // never compares equal and the run has a problem.
                self.problem(&format!(
                    "the record has a Vary digest for `{}`: not supported by the runner (DL-127)",
                    item.name
                ));
                vary.push((item.name.clone(), Some("<vary digest>".to_owned())));
            }
        }
        let body = &entry.body;
        let template = body.template.as_ref();
        let mut data = Vec::new();
        for (key, value) in &body.data {
            if let Ok(text) = std::str::from_utf8(value.as_slice()) {
                data.push((key.clone(), text.to_owned()));
            } else {
                self.problem(&format!("the data value of {key} is not UTF-8"));
            }
        }
        let has_data = template.is_some() || !body.data.is_empty();
        RecordView {
            partition,
            url,
            vary,
            etag: entry.meta.validators.etag.clone(),
            template_tag: entry.meta.validators.template_tag.clone(),
            page_sha256: Some(json::sha256_hex(body.page.as_slice())),
            template_sha256: template.map(|t| json::sha256_hex(t.as_slice())),
            data_sha256: has_data
                .then(|| json::sha256_hex(json::object_of_strings(&data).as_bytes())),
            fields: sorted_fields(&body.fields),
        }
    }
}

// --- helpers ---

fn len_u64(len: usize) -> u64 {
    u64::try_from(len).unwrap_or(u64::MAX)
}

fn lower(fields: &[FieldLine]) -> Vec<Pair> {
    fields
        .iter()
        .map(|f| (f.name.to_ascii_lowercase(), f.value.clone()))
        .collect()
}

/// The lines whose lowercase name is in `names`, in order, names in lower
/// case.
fn pick(fields: &[FieldLine], names: &[&str]) -> Vec<Pair> {
    lower(fields)
        .into_iter()
        .filter(|(n, _)| names.contains(&n.as_str()))
        .collect()
}

/// README value rule 2: names in lower case, sorted by name, stable.
fn sorted_fields(fields: &[FieldLine]) -> Vec<Pair> {
    let mut out = lower(fields);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The host of a serialized origin `scheme://host[:port]`.
fn host_of(origin: &str) -> &str {
    let rest = origin.split_once("://").map_or(origin, |(_, r)| r);
    if rest.starts_with('[') {
        return rest
            .split_once(']')
            .map_or(rest, |(h, _)| h)
            .trim_start_matches('[');
    }
    rest.split_once(':').map_or(rest, |(h, _)| h)
}

#[cfg(test)]
mod tests {
    use super::{host_of, partition_id, pick, sorted_fields};
    use rivqen_model::http::FieldLine;

    #[test]
    fn host_of_an_origin() {
        assert_eq!(host_of("https://shop.example.com"), "shop.example.com");
        assert_eq!(host_of("http://example.com:8080"), "example.com");
        assert_eq!(host_of("http://[::1]:8080"), "::1");
    }

    #[test]
    fn partition_ids_differ_for_null_and_each_account() {
        assert_ne!(partition_id(None), partition_id(Some("")));
        assert_ne!(partition_id(Some("a")), partition_id(Some("b")));
        assert_eq!(partition_id(Some("a")), partition_id(Some("a")));
    }

    #[test]
    fn fields_are_lowercased_and_sorted_stably() {
        let fields = [
            FieldLine::new("X-B", "1"),
            FieldLine::new("Content-Type", "t"),
            FieldLine::new("x-b", "2"),
        ];
        assert_eq!(
            sorted_fields(&fields),
            vec![
                ("content-type".to_owned(), "t".to_owned()),
                ("x-b".to_owned(), "1".to_owned()),
                ("x-b".to_owned(), "2".to_owned()),
            ]
        );
        assert_eq!(
            pick(&fields, &["x-b"]),
            vec![
                ("x-b".to_owned(), "1".to_owned()),
                ("x-b".to_owned(), "2".to_owned())
            ]
        );
    }
}
