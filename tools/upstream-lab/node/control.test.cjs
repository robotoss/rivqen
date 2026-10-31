// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Unit tests of the lab control endpoint parsing. Run: node --test tools/upstream-lab/node/
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const c = require('./control.cjs');

const body = (over = {}) => JSON.stringify({
  suite: 'legacy', case_id: 'FX-LEG-X', profile: 'legacy-node', page_base64: Buffer.from('<p>x</p>').toString('base64'),
  app: { status: 200, headers: [['content-type', 'text/html; charset=utf-8']] }, server_config: {}, ...over,
});

test('parseCase stores the page, status and field lines', () => {
  const r = c.parseCase(body(), 'legacy-node');
  assert.equal(r.error, undefined);
  assert.equal(r.case.caseId, 'FX-LEG-X');
  assert.equal(r.case.page.toString(), '<p>x</p>');
  assert.equal(r.case.status, 200);
  assert.deepEqual(r.case.headers, [['content-type', 'text/html; charset=utf-8']]);
});

test('parseCase: invalid JSON is BAD_CASE', () => {
  assert.equal(c.parseCase('{', 'legacy-node').error, 'BAD_CASE');
  assert.equal(c.parseCase('[]', 'legacy-node').error, 'BAD_CASE');
});

test('parseCase: bad members are BAD_CASE', () => {
  for (const over of [
    { suite: 'x' }, { case_id: 1 }, { profile: undefined }, { page_base64: 'abc' }, { page_base64: 'ab!d' }, { page_base64: 1 },
    { app: null }, { app: { status: 99, headers: [] } }, { app: { status: 600, headers: [] } }, { app: { status: 200.5, headers: [] } },
    { app: { status: 200, headers: [['a b', 'v']] } }, { app: { status: 200, headers: [['a', 'v\r\nx: y']] } },
    { app: { status: 200, headers: [['a']] } }, { server_config: [] },
  ]) {
    assert.equal(c.parseCase(body(over), 'legacy-node').error, 'BAD_CASE', JSON.stringify(over));
  }
});

test('parseCase: another suite or profile is UNSUPPORTED', () => {
  assert.equal(c.parseCase(body({ profile: 'legacy-php' }), 'legacy-node').error, 'UNSUPPORTED');
  assert.equal(c.parseCase(body({ profile: 'legacy-canonical' }), 'legacy-node').error, 'UNSUPPORTED');
  assert.equal(c.parseCase(body({ suite: 'rqp' }), 'legacy-node').error, 'UNSUPPORTED');
});

test('parseCase: BAD_CASE wins over UNSUPPORTED', () => {
  assert.equal(c.parseCase(body({ profile: 'legacy-php', page_base64: 'x' }), 'legacy-node').error, 'BAD_CASE');
});

test('isControlHost accepts loopback names with the published port only', () => {
  assert.equal(c.isControlHost('127.0.0.1:18081', '18081'), true);
  assert.equal(c.isControlHost('LOCALHOST:18081', '18081'), true);
  assert.equal(c.isControlHost('[::1]:18081', '18081'), true);
  assert.equal(c.isControlHost('127.0.0.1:8080', '18081'), false);
  assert.equal(c.isControlHost('shop.example.com', '18081'), false);
  assert.equal(c.isControlHost(undefined, '18081'), false);
});

test('isControlHost fails closed when the published port is not set', () => {
  assert.equal(c.isControlHost('127.0.0.1:', ''), false);
  assert.equal(c.isControlHost('127.0.0.1:undefined', undefined), false);
});

test('isJsonContentType checks the media type only', () => {
  assert.equal(c.isJsonContentType('application/json'), true);
  assert.equal(c.isJsonContentType('Application/JSON; charset=utf-8'), true);
  assert.equal(c.isJsonContentType(' application/json '), true);
  assert.equal(c.isJsonContentType('text/plain'), false);
  assert.equal(c.isJsonContentType('application/jsonx'), false);
  assert.equal(c.isJsonContentType(undefined), false);
});

test('groupHeaders keeps order and turns a repeated name into one multi-value entry', () => {
  assert.deepEqual(c.groupHeaders([['a', '1'], ['B', '2'], ['A', '3']]), [['a', ['1', '3']], ['B', '2']]);
  assert.deepEqual(c.groupHeaders([]), []);
});
