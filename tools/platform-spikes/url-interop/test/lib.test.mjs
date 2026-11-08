// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { toInputLines, encodeRecord, parseRaw, compareEntry, compare } from '../lib.mjs';

const urls = JSON.parse(readFileSync(fileURLToPath(new URL('../urls.json', import.meta.url)), 'utf8'));
const b64 = (s) => Buffer.from(s).toString('base64');
const ok = (o = {}) => ({ id: 'x', ok: true, href: 'https://a.example/p', host: 'a.example', path: '/p', query: null, error: null, ...o });
const header = (n) => ['stack', 'version', 'runtime'].map((k) => `H\t${k}\t${b64(k === 'stack' ? n : 'v')}`).join('\n');

test('urls.json: unique ids, a reason per entry, example hosts only, Q-24 classes present', () => {
  assert.equal(new Set(urls.map((u) => u.id)).size, urls.length);
  for (const u of urls) {
    assert.ok(u.why && u.why.length > 5, u.id);
    assert.ok(!/\.(com|org|net)\b/.test(u.input), `${u.id}: only example hosts`);
  }
  const all = urls.map((u) => u.input).join('\n');
  for (const c of ["'", '|', '^', '`', '{', '}']) assert.ok(urls.some((u) => /\?/.test(u.input) && u.input.split('?')[1].includes(c)), `query has ${c}`);
  assert.match(all, /example\.\//);
  assert.match(all, /%41/);
  assert.match(all, /\\/);
  assert.match(all, /faß/);
  assert.match(all, /user:pw@/);
  assert.match(all, /:443/);
  assert.match(all, /\/\.\//);
});

test('raw format: a record with tabs, newlines and null fields round-trips', () => {
  const us = [{ id: 'x' }];
  const rec = ok({ href: 'a\tb\nc', query: '', host: null, extra: { k: 1 } });
  const f = parseRaw(`${header('okhttp')}\n${encodeRecord(rec)}\n`, us);
  assert.equal(f.stack, 'okhttp');
  assert.deepEqual(f.results[0], rec);
});

test('raw format: missing result, extra result and malformed line are errors', () => {
  const us = [{ id: 'x' }, { id: 'y' }];
  assert.throws(() => parseRaw(`${header('okhttp')}\n${encodeRecord(ok())}\n`, us), /no result for y/);
  assert.throws(() => parseRaw(`${header('okhttp')}\n${encodeRecord(ok())}\n${encodeRecord(ok({ id: 'z' }))}\n`, [{ id: 'x' }]), /2 results for 1/);
  assert.throws(() => parseRaw(`${header('okhttp')}\ngarbage\n`, us), /malformed/);
  assert.throws(() => parseRaw(encodeRecord(ok()), [{ id: 'x' }]), /lacks stack/);
});

test('toInputLines encodes input and base, null base as ~', () => {
  const t = toInputLines([{ id: 'a', input: 'i\tj' }, { id: 'b', input: 'x', base: 'y' }]).split('\n');
  assert.equal(t[0], `a\t${b64('i\tj')}\t~`);
  assert.equal(t[1], `b\t${b64('x')}\t${b64('y')}`);
});

test('compareEntry: refusal combinations', () => {
  const bad = { id: 'x', ok: false, error: 'E' };
  assert.equal(compareEntry(ok(), bad).verdict, 'refuses');
  assert.equal(compareEntry(bad, ok()).verdict, 'accepts-refused');
  assert.equal(compareEntry(bad, bad).verdict, 'both-refuse');
});

test('compareEntry: fields are compared separately; equal href means equal verdict', () => {
  assert.equal(compareEntry(ok(), ok()).verdict, 'equal');
  const d = compareEntry(ok({ query: 'q=it%27s' }), ok({ query: "q=it's", href: "https://a.example/p?q=it's" }));
  assert.equal(d.verdict, 'differ');
  assert.deepEqual([d.host, d.path, d.query, d.href], ['equal', 'equal', 'differ', 'differ']);
  assert.equal(compareEntry(ok({ query: '' }), ok({ query: null })).query, 'differ');
});

test('compareEntry: IPv6 brackets do not count as a host difference', () => {
  assert.equal(compareEntry(ok({ host: '[::1]' }), ok({ host: '::1' })).host, 'equal');
  assert.equal(compareEntry(ok({ host: 'a.example.' }), ok({ host: 'a.example' })).host, 'differ');
});

test('compare: summary counts and requirement of the reference file', () => {
  const us = [{ id: 'x', input: 'i' }, { id: 'y', input: 'j' }];
  const w = { version: 'w', results: [ok(), ok({ id: 'y' })] };
  const s = { version: 's', results: [ok(), ok({ id: 'y', path: '/q', href: 'https://a.example/q' })] };
  const c = compare(us, { 'whatwg-node': w, okhttp: s });
  assert.equal(c.summary.okhttp.equal, 1);
  assert.equal(c.summary.okhttp.total, 2);
  assert.equal(c.summary.okhttp.path_differ, 1);
  assert.equal(c.summary.okhttp.host_differ, 0);
  assert.equal(c.summary.okhttp.differ, 1);
  assert.equal(c.entries[1].stacks.okhttp.path, 'differ');
  assert.throws(() => compare(us, { okhttp: s }), /whatwg-node/);
});
