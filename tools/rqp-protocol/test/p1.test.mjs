// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Seeded property tests of the wire form of H-103 and H-104 (WP-17 S6): the
// manifest without block list and template revision, the envelope with only
// next_revision and the operations. For random pages, random changes and
// random server configurations:
//
// 1. Round trip of F: the client accepts the F of the server, and its computed
//    template revision and block hashes are the values of the markup analysis.
//    The manifest has only the members of H-103 and no default member.
// 2. Round trip of D: a delta of the server, applied by the client, gives
//    exactly the bytes and the page revision of the current page (P-DIFF-1).
// 3. Step 7 is the integrity check: a changed content, a dropped operation or
//    a snapshot with another block gives INTEGRITY_ERROR (manifest-patch
//    section 2.4).
// 4. writtenMembers is idempotent and F depends only on the written members.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { POLICY_DEFAULTS, pageRevision } from '../src/common.mjs';
import { handleResponse, havePrefixes, snapshotOf, verifyDelta } from '../src/client.mjs';
import { analyze } from '../src/markup.mjs';
import { handle, writtenMembers } from '../src/server.mjs';
import { ALLOWLIST, TARGET } from './helpers.mjs';

const RUNS = 300;
const APP = { status: 200, headers: [['content-type', 'text/html; charset=utf-8']] };
const HTML = [['content-type', 'text/html; charset=utf-8'], ['rq-version', '1']];
const DELTA = [['content-type', 'application/vnd.rivqen.delta+json']];

/** Deterministic PRNG (mulberry32), fixed seed. */
function prng(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const pick = (rand, list) => list[Math.floor(rand() * list.length)];
const HTML_PARTS = ['abc', ' ', 'XYZ', '123', 'é', '\u{1F600}', '&amp;', '<b>x</b>', '<i>y</i>', '"', "'", '\n', '<a href="/p?a=1&amp;b=2">l</a>', ''];

function htmlContent(rand) {
  let s = '';
  const n = Math.floor(rand() * 6);
  for (let i = 0; i < n; i++) s += pick(rand, HTML_PARTS);
  return s;
}

function jsonContent(rand) {
  return JSON.stringify({ n: Math.floor(rand() * 1000), s: pick(rand, ['', 'a b', 'é', '\u{1F600}', 'q"q']) });
}

/** A random page: blocks [{ id, kind }], contents by block id, static text. */
function randomPage(rand) {
  const blocks = [];
  const n = Math.floor(rand() * 7);
  for (let i = 0; i < n; i++) blocks.push({ id: `b${i}`, kind: rand() < 0.25 ? 'json' : 'html' });
  const contents = new Map(blocks.map((b) => [b.id, b.kind === 'json' ? jsonContent(rand) : htmlContent(rand)]));
  return { blocks, contents, text: `Static ${Math.floor(rand() * 100000)}` };
}

function render(page) {
  let body = '';
  for (const b of page.blocks) {
    const c = page.contents.get(b.id);
    body += b.kind === 'json' ? `<script type="application/json" data-rq-block="${b.id}">${c}</script>\n` : `<div data-rq-block="${b.id}">${c}</div>\n`;
  }
  return Buffer.from(`<!DOCTYPE html>\n<html>\n<head>\n<title>T</title>\n</head>\n<body>\n<p>${page.text}</p>\n${body}</body>\n</html>\n`, 'utf8');
}

/** The same page with new contents for a random subset of blocks (at least one when the page has blocks). */
function changed(rand, page) {
  const contents = new Map(page.contents);
  for (const b of page.blocks) if (rand() < 0.5) contents.set(b.id, b.kind === 'json' ? jsonContent(rand) : htmlContent(rand) + 'z');
  return { ...page, contents };
}

/** A random server configuration, with default and non-default members. */
function randomConfig(rand) {
  const config = {};
  if (rand() < 0.6) {
    const policy = {};
    if (rand() < 0.5) policy.mode = pick(rand, ['private', 'public']);
    if (rand() < 0.5) policy.ttl_seconds = pick(rand, [0, 0, 60, 31536000]);
    if (rand() < 0.5) policy.offline = rand() < 0.5;
    config.policy = policy;
  }
  if (rand() < 0.4) config.limits = rand() < 0.3 ? {} : { max_patch_bytes: pick(rand, [1024, 65536, 262144, 524288]) };
  return config;
}

const ctx = (fields = {}) => ({
  url: TARGET,
  redirected: false,
  build: 'release',
  allow_http: false,
  allowlist: ALLOWLIST,
  response_fields_visible: true,
  navigation: 'first-visit',
  snapshot: null,
  ...fields,
});

const first = { method: 'GET', target: TARGET, headers: [['rq-version', '1'], ['rq-capabilities', '(block-patch)'], ['accept', 'text/html']] };
function revisit(snap) {
  const a = snap.analysis;
  return {
    method: 'GET',
    target: TARGET,
    headers: [
      ['rq-version', '1'],
      ['rq-capabilities', '(block-patch)'],
      ['accept', 'text/html, application/vnd.rivqen.delta+json'],
      ['if-none-match', `"${snap.page_revision}"`],
      ['rq-have', `t=${a.template_revision}, b=:${havePrefixes(a.blocks)}:`],
    ],
  };
}

const manifestOfF = (body) => {
  const text = body.toString('utf8');
  const start = text.indexOf('rivqen-manifest+json">') + 'rivqen-manifest+json">'.length;
  return JSON.parse(text.slice(start, text.indexOf('</script>', start)));
};

test('property: the client accepts F of the server and computes the values of the analysis (H-103)', () => {
  const rand = prng(103);
  for (let run = 0; run < RUNS; run++) {
    const page = randomPage(rand);
    const bytes = render(page);
    const config = randomConfig(rand);
    const f = handle(first, APP, bytes, config);
    assert.equal(f.kind, 'full');
    const m = manifestOfF(f.body);
    for (const k of Object.keys(m)) assert.ok(['protocol', 'origin', 'path', 'page_revision', 'policy', 'limits'].includes(k), k);
    for (const [k, v] of Object.entries(m.policy ?? {})) assert.notEqual(POLICY_DEFAULTS[k], v, `default ${k}`);
    if (m.policy !== undefined) assert.ok(Object.keys(m.policy).length > 0);
    if (m.limits !== undefined) assert.ok(m.limits.max_patch_bytes < 262144);
    const r = handleResponse(ctx(), { status: 200, headers: HTML, body: f.body });
    assert.equal(r.outcome, 'stored', JSON.stringify(config));
    const a = analyze(bytes);
    assert.equal(r.snapshot.template_revision, a.template_revision);
    assert.deepEqual(r.snapshot.blocks.map(({ id, format, sha256 }) => ({ id, format, sha256 })), a.blocks.map(({ id, format, sha256 }) => ({ id, format, sha256 })));
    assert.equal(r.snapshot.page_revision, m.page_revision);
    assert.equal(f.headers.find(([n]) => n === 'etag')[1], `"${m.page_revision}"`);
  }
});

test('property: a delta of the server gives the current page byte for byte; a changed content, a dropped operation or another snapshot gives INTEGRITY_ERROR (H-104)', () => {
  const rand = prng(104);
  let deltas = 0;
  for (let run = 0; run < RUNS; run++) {
    const pageA = randomPage(rand);
    const pageB = changed(rand, pageA);
    const config = randomConfig(rand);
    const members = writtenMembers(config);
    const a = render(pageA);
    const b = render(pageB);
    const snap = snapshotOf(a, members, TARGET);
    const d = handle(revisit(snap), APP, b, config);
    if (d.kind !== 'delta') continue;
    deltas++;
    const ok = verifyDelta(snap, d.body, true);
    assert.equal(ok.code, undefined, String(ok.code));
    assert.deepEqual(ok.snapshot.document, b);
    const bAnalysis = analyze(b);
    assert.equal(ok.snapshot.page_revision, pageRevision(bAnalysis.template_revision, bAnalysis.blocks, members));
    const env = JSON.parse(d.body.toString('utf8'));
    assert.deepEqual(Object.keys(env), ['next_revision', 'operations']);
    // A changed content: the hash that the client computes is not the one that next_revision binds.
    const k = Math.floor(rand() * env.operations.length);
    const tampered = { ...env, operations: env.operations.map((op, i) => (i === k ? { ...op, content: `${op.content}q` } : op)) };
    assert.equal(verifyDelta(snap, Buffer.from(JSON.stringify(tampered)), true).code, 'INTEGRITY_ERROR');
    // A dropped operation (as after a prefix collision, H-44).
    if (env.operations.length > 1) {
      const dropped = { ...env, operations: env.operations.filter((_, i) => i !== k) };
      assert.equal(verifyDelta(snap, Buffer.from(JSON.stringify(dropped)), true).code, 'INTEGRITY_ERROR');
    }
    // Another snapshot (another base): a block without an operation differs.
    const untouched = pageA.blocks.find((x) => !env.operations.some((op) => op.block_id === x.id) && x.kind === 'html');
    if (untouched !== undefined) {
      const other = { ...pageA, contents: new Map(pageA.contents).set(untouched.id, `${pageA.contents.get(untouched.id)}w`) };
      const otherSnap = snapshotOf(render(other), members, TARGET);
      assert.equal(verifyDelta(otherSnap, d.body, true).code, 'INTEGRITY_ERROR');
    }
    // Another template (another base page text): the template revision of the snapshot differs.
    const otherTemplate = snapshotOf(render({ ...pageA, text: `${pageA.text}!` }), members, TARGET);
    assert.equal(verifyDelta(otherTemplate, d.body, true).code, 'INTEGRITY_ERROR');
  }
  assert.ok(deltas > RUNS / 4, `only ${deltas} deltas`);
});

test('property: writtenMembers is idempotent, drops only default values, and F depends only on the written members (H-103)', () => {
  const rand = prng(1031);
  for (let run = 0; run < RUNS; run++) {
    const config = randomConfig(rand);
    const w = writtenMembers(config);
    assert.deepEqual(writtenMembers(w), w);
    for (const [k, v] of Object.entries(config.policy ?? {})) {
      assert.equal(Object.hasOwn(w.policy ?? {}, k), POLICY_DEFAULTS[k] !== v, `${k}=${v}`);
    }
    const bytes = render(randomPage(rand));
    assert.deepEqual(handle(first, APP, bytes, config).body, handle(first, APP, bytes, w).body);
  }
});
