// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Known answers: the generator inputs and the reference steps against
// spec/rqp/examples/kat (both known-answer sets, H-55).

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';
import { canonicalUrl, deltaBinding, deltaBindingPreimage, pageRevision, policyLimitsText } from '../src/common.mjs';
import { handle } from '../src/server.mjs';
import { clientStep } from '../src/client.mjs';
import { substitute } from '../src/fixtures.mjs';
import { APP, KAT_DIR, TARGET, clientContext, deltaRequest, firstVisit, page, readJson, revisions } from './helpers.mjs';

const kat = readJson(join(KAT_DIR, 'kat.json'));
const manifest = readJson(join(KAT_DIR, kat.manifest));
const set2 = kat.with_policy_limits;
const manifest2 = readJson(join(KAT_DIR, set2.manifest));
const members2 = { policy: manifest2.policy, limits: manifest2.limits };
// H-103: the manifest has no template revision and no block list; kat.json has the computed values.
const computed = kat.computed;

test('page-a is the KAT page and gives the KAT revisions and the computed values', () => {
  assert.deepEqual(page('page-a'), readFileSync(join(KAT_DIR, kat.document)));
  const a = revisions('page-a');
  assert.equal(a.template_revision, computed.template_revision);
  assert.deepEqual(a.blocks.map(({ id, format, sha256 }) => ({ id, format, sha256 })), computed.blocks);
  assert.equal(a.page_revision, manifest.page_revision);
});

test('page-a and page-b give B and R\' of the binding of the KAT price patch (H-107)', () => {
  const patch = readJson(join(KAT_DIR, kat.patches[0]));
  const entry = kat.bindings.find((e) => e.patch === kat.patches[0]);
  assert.deepEqual([entry.base_revision, entry.next_revision], [revisions('page-a').page_revision, revisions('page-b').page_revision]);
  assert.equal(entry.url, TARGET);
  assert.equal(deltaBindingPreimage(entry.url, entry.base_revision, entry.next_revision), entry.preimage);
  assert.equal(deltaBinding(entry.url, entry.base_revision, entry.next_revision), patch.binding);
  assert.equal(entry.binding, patch.binding);
});

test('the KAT binding with a query: U of the reference is the canonical URL of url_input (H-107)', () => {
  const q = kat.binding_with_query;
  assert.equal(canonicalUrl(q.url_input), q.url);
  assert.equal(deltaBindingPreimage(q.url, q.base_revision, q.next_revision), q.preimage);
  assert.equal(deltaBinding(q.url, q.base_revision, q.next_revision), q.binding);
  // The reference server writes this binding for the target url_input (a fragment never reaches a server, but U drops it anyway).
  const r = handle({ ...deltaRequest('page-a'), target: q.url_input }, APP, page('page-b'));
  assert.equal(JSON.parse(r.body.toString('utf8')).binding, q.binding);
});

test('the placeholders give the KAT If-None-Match and Rq-Have values', () => {
  assert.equal(substitute('T', '{{r1:page-a}}'), `"${manifest.page_revision}"`);
  assert.equal(substitute('T', 't={{t1:page-a}}, b={{have:page-a}}'), kat.rq_have);
});

test('the r1 placeholder with client members has the policy line (H-55)', () => {
  assert.equal(substitute('T', '{{r1:page-a}}', members2), `"${manifest2.page_revision}"`);
});

test('the policy line of the second KAT set', () => {
  assert.equal(`PL\t${policyLimitsText(members2)}\n`, set2.policy_line);
  assert.equal(pageRevision(computed.template_revision, computed.blocks, members2), manifest2.page_revision);
  assert.equal(policyLimitsText({}), '');
});

test('the reference server sends the KAT full document for a first visit', () => {
  const r = handle(firstVisit(), APP, page('page-a'));
  assert.deepEqual(r.body, readFileSync(join(KAT_DIR, kat.full_document)));
  assert.equal(r.headers.find(([n]) => n === 'etag')[1], `"${manifest.page_revision}"`);
});

test('the reference server sends the KAT price patch for R(page-a) on page-b', () => {
  const r = handle(deltaRequest('page-a'), APP, page('page-b'));
  assert.equal(r.kind, 'delta');
  assert.deepEqual(JSON.parse(r.body.toString('utf8')), readJson(join(KAT_DIR, kat.patches[0])));
});

test('with the policy and limits of the second set: the KAT full document and the KAT patch', () => {
  const full = handle(firstVisit(), APP, page('page-a'), members2);
  assert.deepEqual(full.body, readFileSync(join(KAT_DIR, set2.full_document)));
  const request = deltaRequest('page-a', { 'if-none-match': `"${manifest2.page_revision}"` });
  const delta = handle(request, APP, page('page-b'), members2);
  assert.deepEqual(JSON.parse(delta.body.toString('utf8')), readJson(join(KAT_DIR, set2.patches[0])));
});

test('the reference client accepts the KAT full documents and applies the KAT patches', () => {
  // H-62 (3): F of a conforming server always has a UTF-8 charset (server step 3).
  const html = [['content-type', 'text/html; charset=utf-8'], ['rq-version', '1']];
  const json = [['content-type', 'application/vnd.rivqen.delta+json']];
  const ctx = clientContext();
  for (const [fullFile, patchFile, m] of [[kat.full_document, kat.patches[0], manifest], [set2.full_document, set2.patches[0], manifest2]]) {
    const stored = clientStep(ctx, { status: 200, headers: html, body: readFileSync(join(KAT_DIR, fullFile)) });
    assert.equal(stored.outcome, 'stored');
    assert.equal(stored.snapshot.page_revision, m.page_revision);
    // The harness snapshot: the stored document, the computed values and the manifest members.
    const snapshot = { ...computed, ...m, document: page('page-a') };
    const patched = clientStep({ ...ctx, navigation: 'delta', snapshot }, { status: 200, headers: json, body: readFileSync(join(KAT_DIR, patchFile)) });
    assert.equal(patched.outcome, 'patched', String(patched.error));
    // After acceptance the page revision of the new snapshot is R' of the binding (H-107).
    const entry = [...kat.bindings, ...set2.bindings].find((e) => e.patch === patchFile);
    assert.equal(patched.snapshot.page_revision, entry.next_revision);
  }
});

test('H-103: the KAT manifests have only protocol, origin, path, page_revision and the non-default members', () => {
  assert.deepEqual(Object.keys(manifest), ['protocol', 'origin', 'path', 'page_revision']);
  assert.deepEqual(Object.keys(manifest2), ['protocol', 'origin', 'path', 'page_revision', 'policy', 'limits']);
  assert.equal(manifest2.policy.mode, undefined); // the default "private" is not written
});

test('H-103: the server writes no default member: a configuration of default values gives the manifest of set 1', () => {
  const defaults = { policy: { mode: 'private', ttl_seconds: 0, offline: false }, limits: {} };
  assert.deepEqual(handle(firstVisit(), APP, page('page-a'), defaults).body, readFileSync(join(KAT_DIR, kat.full_document)));
  // A hint that is not smaller than the server limit bounds nothing (SHOULD): not written.
  const limitOnly = handle(firstVisit(), APP, page('page-a'), { limits: { max_patch_bytes: 262144 } });
  assert.deepEqual(limitOnly.body, readFileSync(join(KAT_DIR, kat.full_document)));
});
