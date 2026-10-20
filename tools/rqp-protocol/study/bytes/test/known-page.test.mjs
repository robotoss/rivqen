// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Byte counts of a tiny known page, computed by hand (see the comments), and
// the additivity of the raw breakdown.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { deltaRaw, manifestRaw } from '../src/breakdown.mjs';
import { exchange } from '../src/rqp.mjs';

const PAGE_A = Buffer.from('<!DOCTYPE html><html><head><title data-rq-block="t">Hi</title></head><body><p data-rq-block="p">1</p></body></html>');
const PAGE_B = Buffer.from('<!DOCTYPE html><html><head><title data-rq-block="t">Hi</title></head><body><p data-rq-block="p">2</p></body></html>');
const TARGET = 'https://a.example/';

test('the manifest rows of a tiny page have the hand-computed byte counts', () => {
  const ex = exchange(PAGE_A, PAGE_B, TARGET);
  const m = manifestRaw(ex.manifestA);
  assert.deepEqual(m.rows, {
    wrapper: 57, // '<script type="' 14 + type 32 + '">' 2 + '</script>' 9
    json_structure: 30, // outer {} 2, 6 ':' and 5 ',' = 13; blocks [] 2 and 1 ','; each block {} 2, 3 ':', 2 ',' = 7 (x2)
    member_names: 106, // "protocol" 10 "origin" 8 "path" 6 "template_revision" 19 "page_revision" 15 "blocks" 8; ("id" 4 "format" 8 "sha256" 8) x2
    protocol: 7, // "rqp/1"
    origin: 19, // "https://a.example"
    path: 3, // "/"
    template_revision_prefix_quotes: 5,
    template_revision_digest: 43,
    page_revision_prefix_quotes: 5,
    page_revision_digest: 43,
    block_ids: 6, // "t" "p"
    block_formats: 12, // "html" x2
    block_digests: 90, // 45 x2
    policy_limits_values: 0,
  });
  assert.equal(m.total, 426);
  // The manifest element is the only difference between F and the page.
  assert.equal(ex.cold.body.length - PAGE_A.length, 426);
});

test('the delta rows of a tiny page have the hand-computed byte counts', () => {
  const ex = exchange(PAGE_A, PAGE_B, TARGET);
  assert.equal(ex.changed.kind, 'delta');
  const d = deltaRaw(ex.delta);
  assert.deepEqual(d.rows, {
    json_structure: 26, // outer {} 2, 6 ':', 5 ','; operations [] 2; op {} 2, 5 ':', 4 ','
    member_names: 116, // "type" 6 "protocol" 10 "base_revision" 15 "next_revision" 15 "template_revision" 19 "operations" 12; "op" 4 "block_id" 10 "format" 8 "content" 9 "sha256" 8
    type: 14, // "rivqen.patch"
    protocol: 7,
    base_revision_prefix_quotes: 5,
    base_revision_digest: 43,
    next_revision_prefix_quotes: 5,
    next_revision_digest: 43,
    template_revision_prefix_quotes: 5,
    template_revision_digest: 43,
    op_values: 15, // "replace_block"
    block_id_values: 3, // "p"
    format_values: 6,
    content_quotes: 2,
    content_bytes: 1,
    content_escapes: 0,
    op_hashes: 45,
  });
  assert.equal(d.total, 379);
  assert.equal(ex.changed.body.length, 379);
  assert.equal(d.operations, 1);
});

test('JSON escapes of content are counted apart from the content bytes', () => {
  const a = Buffer.from('<!DOCTYPE html><html><head></head><body><script type="application/json" data-rq-block="c">{"a":1}</script></body></html>');
  const b = Buffer.from('<!DOCTYPE html><html><head></head><body><script type="application/json" data-rq-block="c">{"a":2}</script></body></html>');
  const d = deltaRaw(exchange(a, b, TARGET).delta);
  assert.equal(d.rows.content_bytes, 7); // {"a":2}
  assert.equal(d.rows.content_escapes, 2); // the two quotes become \"
  assert.equal(d.rows.format_values, 6); // "json"
});
