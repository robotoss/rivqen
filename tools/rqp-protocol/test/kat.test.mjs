// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Known answers: the generator inputs and the reference steps against
// spec/rqp/examples/kat (both known-answer sets, H-55).

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';
import { pageRevision, policyLimitsText } from '../src/common.mjs';
import { handle } from '../src/server.mjs';
import { clientStep } from '../src/client.mjs';
import { substitute } from '../src/fixtures.mjs';
import { APP, KAT_DIR, deltaRequest, firstVisit, page, readJson, revisions } from './helpers.mjs';

const kat = readJson(join(KAT_DIR, 'kat.json'));
const manifest = readJson(join(KAT_DIR, kat.manifest));
const set2 = kat.with_policy_limits;
const manifest2 = readJson(join(KAT_DIR, set2.manifest));
const members2 = { policy: manifest2.policy, limits: manifest2.limits };

test('page-a is the KAT page and gives the KAT revisions', () => {
  assert.deepEqual(page('page-a'), readFileSync(join(KAT_DIR, kat.document)));
  const a = revisions('page-a');
  assert.equal(a.template_revision, manifest.template_revision);
  assert.equal(a.page_revision, manifest.page_revision);
});

test('page-b gives the next_revision of the KAT price patch', () => {
  const patch = readJson(join(KAT_DIR, kat.patches[0]));
  assert.equal(revisions('page-b').page_revision, patch.next_revision);
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
  assert.equal(pageRevision(manifest2.template_revision, manifest2.blocks, members2), manifest2.page_revision);
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
  const ctx = { url: 'https://shop.example.com/catalog', build: 'release', allow_http: false, navigation: 'first-visit', snapshot: null };
  for (const [fullFile, patchFile, m] of [[kat.full_document, kat.patches[0], manifest], [set2.full_document, set2.patches[0], manifest2]]) {
    const stored = clientStep(ctx, { status: 200, headers: html, body: readFileSync(join(KAT_DIR, fullFile)) });
    assert.equal(stored.outcome, 'stored');
    assert.equal(stored.snapshot.page_revision, m.page_revision);
    const snapshot = { ...m, document: page('page-a') };
    const patched = clientStep({ ...ctx, navigation: 'delta', snapshot }, { status: 200, headers: json, body: readFileSync(join(KAT_DIR, patchFile)) });
    assert.equal(patched.outcome, 'patched', String(patched.error));
    assert.equal(patched.snapshot.page_revision, readJson(join(KAT_DIR, patchFile)).next_revision);
  }
});
