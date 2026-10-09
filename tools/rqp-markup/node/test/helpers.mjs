// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { ErrorCode, analyze } from '../src/markup.mjs';

const CODES = new Set(Object.values(ErrorCode));

/** Wrap body markup in a small valid document. */
export function doc(body, head = '') {
  return `<!DOCTYPE html><html><head><title>T</title>${head}</head><body>${body}</body></html>`;
}

/** Analyze a string or bytes. */
export function run(input) {
  return analyze(typeof input === 'string' ? Buffer.from(input, 'utf8') : input);
}

/** Error code of a document, or null when it is valid. */
export function errorOf(input) {
  return run(input).error;
}

/** Blocks of a valid document as [id, format, content] (content as a UTF-8 string). */
export function blocksOf(input) {
  const bytes = typeof input === 'string' ? Buffer.from(input, 'utf8') : input;
  const result = analyze(bytes);
  if (!result.valid) throw new Error(`expected a valid document, got ${result.error}`);
  return result.blocks.map((b) => [b.id, b.format, bytes.toString('utf8', b.start, b.end)]);
}

/** Deterministic pseudo-random generator (mulberry32) for property tests. */
export function rng(seed) {
  let a = seed >>> 0;
  const next = () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  return {
    next,
    int: (n) => Math.floor(next() * n),
    pick: (list) => list[Math.floor(next() * list.length)],
  };
}

const sha = (data) => createHash('sha256').update(data).digest('base64url');

/** The invariants of every result (CONTRACT.md sections 3 and 4). */
export function checkResult(input, result) {
  if (!result.valid) {
    assert.ok(CODES.has(result.error), String(result.error));
    assert.deepEqual(result.blocks, []);
    assert.equal(result.template_revision, null);
    assert.equal(result.page_revision, null);
    return;
  }
  assert.equal(result.error, null);
  let last = 0;
  const template = [];
  const ids = new Set();
  for (const b of result.blocks) {
    assert.ok(Number.isInteger(b.start) && b.start >= last && b.start <= b.end && b.end <= input.length);
    assert.equal(input[b.start - 1], 0x3e, 'start is the byte after ">"');
    assert.equal(input[b.end], 0x3c, 'end is the "<" of the end tag');
    assert.equal(b.sha256, sha(input.subarray(b.start, b.end)), 'content bytes == input [start, end)');
    assert.match(b.id, /^[a-z0-9][a-z0-9_-]{0,63}$/);
    assert.ok(!ids.has(b.id));
    ids.add(b.id);
    template.push(input.subarray(last, b.start));
    last = b.end;
  }
  template.push(input.subarray(last));
  assert.equal(result.template_revision, `t1.${sha(Buffer.concat([Buffer.from('rqp-t1\n'), ...template]))}`);
}
