// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The step engine of the client model (CL-01, CL-02, CL-04): it runs the schedule of a client
// case, plays the lab runtime (account, cookie, network), the recording proxy (hold, cut,
// drop_field, set_field, server_down) and the clock, and records the projection of
// fixtures/legacy/README.md "Client expected.json". The session behavior is a profile
// (src/client-canonical.mjs, src/client-android.mjs); the engine only routes events to it.
//
// Bounds: the case is checked by src/client-case.mjs first (at most 256 steps, 16 sessions,
// 32 responses). Each step does work linear in the size of the responses; the clock only
// moves at `settle`, and each timer runs once. A defect of the case during the run is a
// ClientCaseError; the engine throws nothing else for a checked case.

import { createHash } from 'node:crypto';
import { forwardFields, setField } from '../../upstream-lab/client/src/proxy.mjs';
import { ClientCaseError } from './client-case.mjs';
import { REQUEST_FIELDS, RESPONSE_FIELDS, sortedEntries } from './client-trace.mjs';
import { asciiLower } from './fields.mjs';
import { respond } from './model.mjs';

export const sha256hex = (data) => createHash('sha256').update(data).digest('hex');
export const sha1hex = (data) => createHash('sha1').update(data).digest('hex');

/** A failure of the case while it runs (README check 6: a case error with the member name). */
export function caseError(code, message) {
  return new ClientCaseError(code, message);
}

/** A branch that the profile does not define from FACT rows (the profile gives no result). */
export function notModeled(row, what) {
  return new ClientCaseError('CLIENT_NOT_MODELED', `${what} is not modeled (${row}: no FACT row or trace gives the result)`);
}

function pick(fields, names) {
  const wanted = new Set(names);
  return fields.map(([n, v]) => [asciiLower(n), v]).filter(([n]) => wanted.has(n));
}

function newPhase() {
  return { steps: [], requests: [], cookies: [], actions: [], intercepts: [], results: [], snapshots: [] };
}

const noFaults = () => ({ hold: null, cut: null, drop: [], set: [] });

/**
 * One run of a client case under a profile.
 *
 * `profile` is { name, createSession(sim, index, partition, url, step), canOpen(sim, partition, url),
 * accountChanged(sim, from, to), store }.
 */
export class Sim {
  constructor(profile, behavior, env) {
    this.profile = profile;
    this.b = behavior;
    this.env = env;
    this.mode = behavior.mode;
    this.url = behavior.url;
    this.config = behavior.config;
    this.clock = 0;
    this.runtime = { account: behavior.config.account, cookie: behavior.config.cookie, network: behavior.config.network_valid };
    this.sessions = [];
    this.created = [];
    this.phases = [];
    this.phase = newPhase();
    this.exchangeNo = 0;
    this.pending = noFaults();
    this.holdRef = null;
    this.serverDown = false;
    this.current = null;
    this.seen = new Set();
    this.timers = [];
    this.stepNo = 0;
    this.store = profile.createStore(this);
  }

  /** Run the schedule. Returns the `client` member. Throws ClientCaseError. */
  run() {
    for (const entry of this.b.cache_before) this.store.seed(entry);
    let close = false;
    for (const item of this.b.schedule) {
      if (item.case !== undefined) {
        this.current = item.case;
        continue;
      }
      if (close) {
        this.phases.push(this.phase);
        this.phase = newPhase();
        close = false;
      }
      this.stepNo += 1;
      this.phase.steps.push(this.stepNo);
      this.step(item);
      if (item.do === 'settle') close = true;
    }
    this.phases.push(this.phase);
    return {
      sessions: this.created,
      phases: this.phases.map((p) => ({
        steps: p.steps.length === 0 ? [] : [p.steps[0], p.steps.at(-1)],
        requests: p.requests,
        cookies: sortedEntries(p.cookies),
        actions: p.actions,
        intercepts: sortedEntries(p.intercepts),
        results: sortedEntries(p.results),
        snapshots: p.snapshots,
      })),
    };
  }

  emit(ev) {
    this.seen.add(ev);
  }

  // --- records (the projection) ---
  cookieGet(url) {
    this.phase.cookies.push({ call: 'get', url, value: this.runtime.cookie ?? null });
    this.emit('runtime.cookie_get');
  }
  cookieSet(url, values) {
    this.phase.cookies.push({ call: 'set', url, values });
    this.emit('runtime.cookie_set');
  }
  loadUrl(session) {
    this.phase.actions.push({ do: 'load_url', session: session.index, url: session.url });
    this.emit('client.load_url');
  }
  loadData(session, doc) {
    this.phase.actions.push({ do: 'load_data', session: session.index, url: session.url, fields: doc.fields, body_len: doc.bytes.length, body_sha256: sha256hex(doc.bytes) });
    this.emit('client.load_data');
  }
  message(session, json) {
    this.phase.actions.push({ do: 'message', session: session.index, json });
    this.emit('session.message');
  }
  interceptNull(session) {
    this.phase.intercepts.push({ session: session.index, null: true });
    this.emit('client.intercept');
  }
  interceptBody(session, status, fields, bytes, complete) {
    this.phase.intercepts.push({ session: session.index, status, fields, body_len: bytes.length, body_sha256: sha256hex(bytes), complete });
    this.emit('client.intercept');
  }
  result(session, src, final) {
    this.phase.results.push({ session: session.index, src_code: src, final_code: final });
    this.emit('session.result');
  }

  /**
   * Run `fn` when the clock reaches `due` (a later `settle`); `owner` cancels it at destroy.
   * Returns the timer: `cancelTimer(t)` cancels this one only; `t.done` is true after it ran.
   */
  timer(due, owner, fn) {
    const t = { due, owner, fn, seq: this.timers.length, done: false };
    this.timers.push(t);
    return t;
  }
  cancelTimers(owner) {
    this.timers = this.timers.filter((t) => t.owner !== owner);
  }
  cancelTimer(timer) {
    this.timers = this.timers.filter((t) => t !== timer);
  }

  // --- steps ---
  target(step) {
    if (this.sessions.length === 0) throw caseError('CASE_NO_SESSION', `step ${this.stepNo} (${step.do}) acts on a session, but no session is open`);
    if (step.session === undefined) return this.sessions.at(-1);
    const s = this.sessions[step.session - 1];
    if (s === undefined) throw caseError('CASE_NO_SESSION', `step ${this.stepNo}: session ${step.session} was not opened`);
    return s;
  }

  step(step) {
    switch (step.do) {
      case 'create_session':
        return this.createSession();
      case 'bind_client':
        return this.target(step).bind();
      case 'client_ready':
        return this.target(step).ready();
      case 'request_resource':
        return this.target(step).intercept();
      case 'page_finished':
        return this.target(step).pageFinished();
      case 'get_diff':
        return this.target(step).getDiff();
      case 'refresh':
        return this.target(step).refresh();
      case 'destroy':
        return this.target(step).destroy();
      case 'set_account': {
        const from = this.runtime.account;
        this.runtime.account = step.value;
        if (from !== step.value) this.profile.accountChanged(this, from, step.value);
        return undefined;
      }
      case 'set_cookie':
        this.runtime.cookie = step.value;
        return undefined;
      case 'set_network':
        this.runtime.network = step.valid;
        return undefined;
      case 'hold':
        if (this.holdRef !== null) throw caseError('CASE_HOLD', `step ${this.stepNo}: a hold is already armed or active`);
        this.holdRef = { point: step.point, bytes: step.bytes ?? 0, released: false, ex: null };
        this.pending.hold = this.holdRef;
        return undefined;
      case 'release':
        return this.release();
      case 'cut':
        this.pending.cut = { bytes: step.bytes };
        return undefined;
      case 'drop_field':
        this.pending.drop.push(step.name);
        return undefined;
      case 'set_field':
        this.pending.set.push({ name: step.name, value: step.value });
        return undefined;
      case 'server_down':
        if (this.holdRef?.ex) throw caseError('CASE_HOLD', `step ${this.stepNo}: server_down while an exchange is held is not modeled`);
        this.serverDown = true;
        this.pending = noFaults();
        this.holdRef = null;
        return undefined;
      case 'server_up':
        this.serverDown = false;
        return undefined;
      case 'wait':
        if (!this.seen.has(step.for)) throw caseError('CASE_WAIT', `step ${this.stepNo}: wait for ${step.for}: the event does not occur after the last create_session (CL-01)`);
        return undefined;
      case 'settle':
        return this.settle(step.ms);
      case 'snapshot_cache':
        this.phase.snapshots.push({ entries: this.store.snapshot() });
        this.emit('cache.snapshot');
        return undefined;
      default:
        throw caseError('CASE_STEP', `step ${this.stepNo}: unknown step ${step.do}`);
    }
  }

  createSession() {
    this.seen.clear();
    this.emit('session.created');
    const partition = this.runtime.account;
    const step = this.stepNo;
    if (!this.profile.canOpen(this, partition, this.url)) {
      this.created.push({ step, opened: false, partition, url: this.url });
      return;
    }
    const index = this.sessions.length + 1;
    const session = this.profile.createSession(this, index, partition, this.url);
    this.sessions.push(session);
    this.created.push({ step, opened: true, session: index, partition, url: this.url });
    session.start();
  }

  settle(ms) {
    this.clock += ms;
    for (;;) {
      const due = this.timers.filter((t) => t.due <= this.clock).sort((a, b) => a.due - b.due || a.seq - b.seq);
      if (due.length === 0) return;
      const t = due[0];
      this.timers = this.timers.filter((x) => x !== t);
      t.done = true;
      t.fn();
    }
  }

  // --- the proxy ---
  responseOf(name) {
    const entry = this.b.responses.get(name);
    if (entry.fixture !== undefined) {
      const fx = this.env.fixture?.(entry.fixture) ?? null;
      if (fx === null) throw caseError('CASE_FIXTURE', `responses: server fixture ${entry.fixture} is not a canonical server fixture`);
      return { app: fx.app, bytes: this.pageBytes(fx.page) };
    }
    return { app: entry.app, bytes: this.pageBytes(entry.page) };
  }

  pageBytes(id) {
    const bytes = this.env.page?.(id) ?? null;
    if (!(bytes instanceof Uint8Array)) throw caseError('CASE_PAGE', `page ${id} is missing`);
    return Buffer.from(bytes);
  }

  /**
   * Send a request of `session` (generation `gen`). Returns the exchange, or null when the
   * connection is refused (server_down: no exchange, CONTRACT section 5).
   */
  send(session, gen, fields) {
    if (this.serverDown) return null;
    if (this.current === null) throw caseError('CASE_NO_RESPONSE', `step ${this.stepNo}: a request is sent before the first case entry`);
    const { app, bytes } = this.responseOf(this.current);
    const response = respond(this.b.server_profile, { method: 'GET', headers: fields }, app, bytes);
    const faults = this.pending;
    this.pending = noFaults();
    let headers = forwardFields(response.headers, faults.drop);
    for (const { name, value } of faults.set) headers = setField(headers, name, value);
    const body = response.body === null ? Buffer.alloc(0) : Buffer.from(response.body);
    const target = new URL(session.url);
    const entry = { exchange: ++this.exchangeNo, method: 'GET', target: `${target.pathname}${target.search}`, fields: pick(fields, REQUEST_FIELDS), response: null };
    this.phase.requests.push(entry);
    this.emit('proxy.request');
    const ops = [];
    if (faults.hold?.point === 'request' || faults.hold?.point === 'head') ops.push({ op: 'pause', hold: faults.hold });
    ops.push({ op: 'head' });
    const stops = [];
    if (faults.hold?.point === 'body') stops.push({ at: Math.min(faults.hold.bytes, body.length), hold: faults.hold });
    if (faults.cut) stops.push({ at: Math.min(faults.cut.bytes, body.length), cut: true });
    stops.sort((a, b) => a.at - b.at);
    for (const s of stops) {
      ops.push({ op: 'body', to: s.at });
      if (s.hold) ops.push({ op: 'pause', hold: s.hold });
      else {
        ops.push({ op: 'cut' });
        break;
      }
    }
    if (ops.at(-1).op !== 'cut') ops.push({ op: 'body', to: body.length }, { op: 'end' });
    const ex = { n: entry.exchange, session, gen, status: response.status, headers, body, given: 0, ended: false, complete: false, entry, ops, at: 0, drop: faults.drop, set: faults.set };
    this.pump(ex);
    return ex;
  }

  pump(ex) {
    while (ex.at < ex.ops.length) {
      const op = ex.ops[ex.at++];
      if (op.op === 'pause') {
        op.hold.ex = ex;
        this.emit('proxy.hold');
        if (!op.hold.released) return;
      } else if (op.op === 'head') {
        for (const _ of ex.drop) this.emit('proxy.drop_field');
        for (const _ of ex.set) this.emit('proxy.set_field');
        ex.entry.response = { status: ex.status, fields: pick(ex.headers, RESPONSE_FIELDS), complete: false };
        this.emit('proxy.response_head');
        ex.session.onHead(ex);
      } else if (op.op === 'body') {
        if (op.to > ex.given) {
          ex.given = op.to;
          ex.session.onBody(ex);
        }
      } else if (op.op === 'cut') {
        ex.ended = true;
        this.emit('proxy.cut');
        this.emit('proxy.response_end');
        ex.session.onEnd(ex);
      } else {
        ex.ended = true;
        ex.complete = true;
        ex.entry.response.complete = true;
        this.emit('proxy.response_end');
        ex.session.onEnd(ex);
      }
    }
  }

  release() {
    const hold = this.holdRef;
    if (hold === null) throw caseError('CASE_HOLD', `step ${this.stepNo}: release without a hold`);
    hold.released = true;
    this.emit('proxy.release');
    this.holdRef = null;
    if (hold.ex !== null) this.pump(hold.ex);
  }
}
