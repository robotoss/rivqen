#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Minimal stand-in for the conductor (T-02) for the smoke runs of the Android harness only.
// It serves the control channel of CONTRACT section 4 on 127.0.0.1:18091 and a static legacy
// first-load answer on 127.0.0.1:18090: the status, fields and body of a stored upstream server
// trace (default: evidence/traces/server/node/S02-first-load.http, page a of lab-node). It is not
// the recording proxy: no hold, cut or fault steps.
//
// Usage: node fake-conductor.mjs --run <id> --mode quick|standard [--webview] --out <events.jsonl>
//        [--trace <file.http>]
// It exits after the run: 0 when the harness reported no step error, else 1.

import { createServer } from 'node:http';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, '..', '..', '..', '..');
const MAX_BODY = 8 * 1024 * 1024;
const POLL_MS = 30000;

function arg(name, fallback) {
  const i = process.argv.indexOf(name);
  if (i === -1) return fallback;
  const v = process.argv[i + 1];
  if (v === undefined || v.startsWith('--')) throw new Error(`${name} needs a value`);
  return v;
}

const run = arg('--run');
const mode = arg('--mode', 'quick');
const out = arg('--out');
const webview = process.argv.includes('--webview');
const tracePath = arg('--trace', join(ROOT, 'evidence/traces/server/node/S02-first-load.http'));
if (!run || !out || !['quick', 'standard'].includes(mode)) {
  console.error('usage: fake-conductor.mjs --run <id> --mode quick|standard [--webview] --out <file> [--trace <file>]');
  process.exit(2);
}

// The stored trace: "HTTP <status>", field lines, a blank line, "[body decoded …]", the body bytes.
function loadTrace(path) {
  const text = readFileSync(path, 'utf8');
  const response = text.split('\n--- response ---\n')[1];
  if (!response) throw new Error(`no response section in ${path}`);
  const lines = response.split('\n');
  const status = Number(/^HTTP (\d{3})$/.exec(lines[0])?.[1]);
  if (!status) throw new Error(`no status line in ${path}`);
  const fields = [];
  let i = 1;
  for (; i < lines.length && lines[i] !== ''; i++) {
    const c = lines[i].indexOf(':');
    fields.push([lines[i].slice(0, c), lines[i].slice(c + 1).trim()]);
  }
  const marker = response.indexOf('\n[body decoded');
  const bodyStart = marker === -1 ? -1 : response.indexOf('\n', marker + 1) + 1;
  const body = Buffer.from(bodyStart > 0 ? response.slice(bodyStart) : '', 'utf8');
  // The body is stored decoded: drop the framing and coding fields; send a plain length.
  const drop = new Set(['content-encoding', 'transfer-encoding', 'connection', 'keep-alive', 'content-length']);
  const kept = fields.filter(([n]) => !drop.has(n.toLowerCase()));
  kept.push(['Content-Length', String(body.length)]);
  return { status, fields: kept, body };
}

const trace = loadTrace(tracePath);
const PAGE = 'http://10.0.2.2:18090/catalog';

// Smoke scenario: one first load (CONTRACT section 2 macro "first_load", plus a page message).
const harness = (d, extra = {}) => ({ do: d, ...extra });
const steps = webview
  ? [harness('create_session'), harness('bind_client'), harness('client_ready'),
     { do: 'wait', for: 'proxy.response_end' }, { do: 'settle', ms: 3000 },
     harness('get_diff'), { do: 'settle', ms: 500 }, harness('snapshot_cache'), harness('destroy')]
  : [harness('create_session'), harness('bind_client'), harness('client_ready'), harness('request_resource'),
     { do: 'wait', for: 'proxy.response_end' }, { do: 'settle', ms: 2500 },
     harness('get_diff'), { do: 'settle', ms: 500 }, harness('snapshot_cache'), harness('destroy')];
const scenario = {
  id: 'SMOKE-01', title: 'Smoke: first load', modes: [mode], origin: 'lab-node', rows: [],
  setup: [], case: { page: 'a' }, config: { account: 'rq-test-account' }, steps, repeat: 1,
  ...(webview ? { webview: true } : {}),
};

const start = Date.now();
let seq = 0;
const log = [];
const waiters = [];
let stepErrors = 0;

function sortKeys(v) {
  if (Array.isArray(v)) return v.map(sortKeys);
  if (v && typeof v === 'object') {
    return Object.fromEntries(Object.keys(v).sort().map((k) => [k, sortKeys(v[k])]));
  }
  return v;
}

function record(ev) {
  const e = { ...ev, seq: ++seq, t: Date.now() - start };
  log.push(JSON.stringify(sortKeys(e)));
  if (e.ev === 'step.done' && e.error) stepErrors++;
  for (const w of waiters.splice(0)) w(e);
}

function waitFor(name, timeoutMs) {
  if (log.some((l) => JSON.parse(l).ev === name)) return Promise.resolve(true);
  return new Promise((resolve) => {
    const timer = setTimeout(() => resolve(false), timeoutMs);
    const check = (e) => {
      if (e.ev === name) { clearTimeout(timer); resolve(true); } else waiters.push(check);
    };
    waiters.push(check);
  });
}

// Step machine: harness steps go out through /run/next; conductor steps run here.
let cursor = 0;
let pendingHarness = null; // step sent, waiting for step.done
let pollers = [];

async function advance() {
  while (cursor < steps.length && !pendingHarness) {
    const s = steps[cursor];
    if (s.do === 'wait' || s.do === 'settle') {
      record({ ev: 'step.start', src: 'conductor', step: s });
      let error = null;
      if (s.do === 'settle') await new Promise((r) => setTimeout(r, s.ms));
      else if (!(await waitFor(s.for, s.timeout_ms ?? 30000))) error = `timeout waiting for ${s.for}`;
      record({ ev: 'step.done', src: 'conductor', step: s, error });
      cursor++;
      continue;
    }
    pendingHarness = { seq: cursor + 1, ...s };
    flushPollers();
    return;
  }
  if (cursor >= steps.length && !pendingHarness) flushPollers();
}

function flushPollers() {
  const p = pollers.splice(0);
  for (const { res, timer } of p) {
    clearTimeout(timer);
    sendNext(res);
  }
}

let sent = new Set();
let ended = false;
function sendNext(res) {
  if (pendingHarness && !sent.has(pendingHarness.seq)) {
    sent.add(pendingHarness.seq);
    return json(res, 200, pendingHarness);
  }
  if (cursor >= steps.length && !pendingHarness) {
    ended = true;
    setTimeout(finish, 5000);
    return json(res, 200, { do: 'end' });
  }
  return null;
}

function json(res, status, body) {
  const b = Buffer.from(JSON.stringify(body));
  res.writeHead(status, { 'Content-Type': 'application/json', 'Content-Length': b.length });
  res.end(b);
}

function readBody(req) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    let n = 0;
    req.on('data', (c) => {
      n += c.length;
      if (n > MAX_BODY) { reject(new Error('body over 8 MiB')); req.destroy(); } else chunks.push(c);
    });
    req.on('end', () => resolve(Buffer.concat(chunks)));
    req.on('error', reject);
  });
}

const control = createServer(async (req, res) => {
  const url = new URL(req.url, 'http://127.0.0.1');
  try {
    if (req.method === 'GET' && url.pathname === '/run/scenario') {
      return json(res, 200, { run, scenario, mode });
    }
    if (req.method === 'GET' && url.pathname === '/run/next') {
      if (url.searchParams.get('run') !== run) return json(res, 404, { error: 'unknown run' });
      if (sendNext(res) !== null) return;
      const timer = setTimeout(() => {
        pollers = pollers.filter((p) => p.res !== res);
        res.writeHead(204); res.end();
      }, POLL_MS);
      pollers.push({ res, timer });
      return;
    }
    if (req.method === 'POST' && url.pathname === '/run/event') {
      const ev = JSON.parse((await readBody(req)).toString('utf8'));
      record(ev);
      res.writeHead(204); res.end();
      if (ev.ev === 'step.done' && pendingHarness && ev.step && ev.step.seq === pendingHarness.seq) {
        pendingHarness = null;
        cursor++;
        advance();
      }
      return;
    }
    json(res, 404, { error: 'not found' });
  } catch (e) {
    json(res, 400, { error: String(e.message) });
  }
});

let exchange = 0;
const origin = createServer(async (req, res) => {
  const x = ++exchange;
  const fields = [];
  for (let i = 0; i < req.rawHeaders.length; i += 2) fields.push([req.rawHeaders[i], req.rawHeaders[i + 1]]);
  record({ ev: 'proxy.request', src: 'proxy', exchange: x, method: req.method, target: req.url, fields });
  await readBody(req).catch(() => Buffer.alloc(0));
  res.writeHead(trace.status, trace.fields.flat());
  record({ ev: 'proxy.response_head', src: 'proxy', exchange: x, status: trace.status, fields: trace.fields });
  res.end(trace.body, () => {
    record({ ev: 'proxy.response_end', src: 'proxy', exchange: x, body_len: trace.body.length,
      body_sha256: createHash('sha256').update(trace.body).digest('hex'), complete: true });
  });
});

function finish() {
  record({ ev: 'run.end', src: 'conductor', ok: stepErrors === 0, error: stepErrors ? `${stepErrors} step errors` : null });
  writeFileSync(out, log.join('\n') + '\n');
  console.log(`fake-conductor: ${log.length} events -> ${out}`);
  control.close();
  origin.close();
  process.exit(stepErrors === 0 ? 0 : 1);
}

control.listen(18091, '127.0.0.1', () => {
  origin.listen(18090, '127.0.0.1', () => {
    record({ ev: 'run.start', src: 'conductor', scenario: scenario.id, mode, repeat: 1, origin: 'lab-node',
      upstream_sha: '59936beff656d4b5718ff6444d6c5e001a2c5231' });
    console.log(`fake-conductor: run ${run} (${mode}${webview ? ', webview' : ''}) ready on 18090/18091; page ${PAGE}`);
    advance();
  });
});
setTimeout(() => { console.error('fake-conductor: run over 5 min'); finish(); }, 5 * 60 * 1000).unref();
