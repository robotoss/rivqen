// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import http from 'node:http';
import test from 'node:test';
import { HarnessChannel } from '../src/channel.mjs';
import { Control } from '../src/control.mjs';
import { EventLog } from '../src/eventlog.mjs';
import { makeRunHandle } from '../src/runhandle.mjs';

const RUN = '?run=CS-01-quick-r1';
let hseq = 1000;
const harnessEvent = (ev, extra = {}) => ({ ev, thread: 'main', hseq: ++hseq, ...extra });
const post = (url, body, type = 'application/json') => fetch(url, { method: 'POST', headers: { 'content-type': type }, body: typeof body === 'string' ? body : JSON.stringify(body) });

async function setup() {
  const log = new EventLog();
  const channel = new HarnessChannel();
  const run = makeRunHandle({ id: 'CS-01-quick-r1', scenario: () => ({ run: 'CS-01-quick-r1' }), log, channel });
  const control = new Control({ port: 0, run, pollMs: 50 });
  await control.start();
  return { log, control, url: (p) => `http://127.0.0.1:${control.port}${p}`, stop: () => control.stop() };
}

const get = (port, headers, path = `/run/scenario${RUN}`) =>
  new Promise((resolve, reject) => {
    const req = http.request({ host: '127.0.0.1', port, path, headers }, (res) => {
      const chunks = [];
      res.on('data', (c) => chunks.push(c));
      res.on('end', () => resolve([res.statusCode, Buffer.concat(chunks).toString()]));
    });
    req.on('error', reject);
    req.end();
  });

test('control_answers_name_their_error', async () => {
  const c = await setup();
  try {
    const error = async (res) => (await res.json()).error;
    assert.equal(await error(await fetch(c.url('/nothing'))), 'NOT_FOUND');
    assert.equal(await error(await fetch(c.url('/run/event'))), 'METHOD');
    assert.equal(await error(await fetch(c.url('/run/next?run=other'))), 'UNKNOWN_RUN');
    assert.equal(await error(await fetch(c.url('/run/next'))), 'NO_RUN');
    assert.equal(await error(await post(c.url('/run/event'), '{}', 'text/plain')), 'CONTENT_TYPE');
    assert.equal(await error(await post(c.url('/run/event'), 'x')), 'BAD_EVENT');
    const rejected = await post(c.url('/run/event'), harnessEvent('proxy.request'));
    assert.deepEqual(await rejected.json(), { error: 'BAD_EVENT', message: 'the harness may not send proxy.request' });
    assert.equal(await error(await post(c.url('/run/event'), JSON.stringify({ ev: 'x', pad: 'y'.repeat(1024 * 1024) }))), 'BODY');
  } finally {
    await c.stop();
  }
});

test('control_host_names_are_exact_and_case_insensitive', async () => {
  const c = await setup();
  try {
    const port = c.control.port;
    assert.equal((await get(port, { host: `localhost:${port}` }))[0], 200);
    assert.equal((await get(port, { host: `LocalHost:${port}` }))[0], 200);
    assert.equal((await get(port, { host: `127.0.0.1:${port}` }))[0], 200);
    assert.deepEqual(await get(port, { host: `127.0.0.1:${port + 1}` }), [403, '{"error":"BAD_HOST"}']);
    assert.equal((await get(port, { host: '127.0.0.1' }))[0], 403);
  } finally {
    await c.stop();
  }
});

test('control_event_content_type_is_json_with_optional_parameters_only', async () => {
  const c = await setup();
  try {
    const good = () => harnessEvent('session.created', { session_id: null });
    for (const type of ['application/json', 'APPLICATION/JSON', 'application/json;charset=utf-8', 'application/json ; charset=utf-8']) {
      assert.equal((await post(c.url('/run/event'), good(), type)).status, 204, type);
    }
    for (const type of ['application/jsonp', 'xapplication/json', 'application/json x', 'text/json']) {
      assert.equal((await post(c.url('/run/event'), good(), type)).status, 415, type);
    }
  } finally {
    await c.stop();
  }
});

test('control_event_body_limit_is_one_mebibyte_inclusive', async () => {
  const c = await setup();
  try {
    const event = (pad) => JSON.stringify({ ev: 'session.message', thread: 'main', hseq: ++hseq, json: 'x'.repeat(pad) });
    const empty = Buffer.byteLength(event(0));
    assert.equal((await post(c.url('/run/event'), event(1024 * 1024 - empty))).status, 204);
    assert.equal((await post(c.url('/run/event'), event(1024 * 1024 - empty + 1))).status, 413);
  } finally {
    await c.stop();
  }
});

test('control_internal_errors_answer_500_without_details', async () => {
  const run = {
    id: 'r',
    scenario: () => ({}),
    poll: async () => {
      throw new Error('secret detail');
    },
    accept: () => {
      throw new Error('secret detail');
    },
  };
  const control = new Control({ port: 0, run, pollMs: 50 });
  await control.start();
  try {
    const base = `http://127.0.0.1:${control.port}`;
    for (const res of [await fetch(`${base}/run/next?run=r`), await post(`${base}/run/event`, { ev: 'x' })]) {
      assert.equal(res.status, 500);
      assert.deepEqual(await res.json(), { error: 'INTERNAL' });
    }
  } finally {
    await control.stop();
    await control.stop(); // stopping twice is harmless
  }
});

test('control_start_fails_with_listen_when_the_port_is_taken', async () => {
  const c = await setup();
  try {
    const run = { id: 'r', scenario: () => ({}), poll: async () => null, accept: () => {} };
    await assert.rejects(new Control({ port: c.control.port, run }).start(), (e) => e.code === 'LISTEN' && e.message === `cannot listen on 127.0.0.1:${c.control.port}`);
  } finally {
    await c.stop();
  }
});

test('run_handle_refuses_events_with_a_reason', () => {
  const log = new EventLog();
  const channel = new HarnessChannel();
  const run = makeRunHandle({ id: 'r', scenario: () => ({}), log, channel });
  const refuse = (event, message) => assert.throws(() => run.accept(event), (e) => e.code === 'BAD_EVENT' && e.message === message, JSON.stringify(event));
  refuse(null, 'event needs a name "ev"');
  refuse([], 'event needs a name "ev"');
  refuse({ ev: 5 }, 'event needs a name "ev"');
  refuse({ ev: '' }, 'event needs a name "ev"');
  refuse({ ev: 'proxy.request' }, 'the harness may not send proxy.request');
  refuse({ ev: 'run.start' }, 'the harness may not send run.start');
  refuse({ ev: 'step.start' }, 'the harness may not send step.start');
  refuse({ ev: 'run.end', ok: true }, 'the harness sends run.end only with ok false');
  refuse({ ev: 'run.end' }, 'the harness sends run.end only with ok false');
  refuse({ ev: 'session.created' }, 'event needs a "thread"');
  refuse({ ev: 'session.created', thread: '' }, 'event needs a "thread"');
  refuse({ ev: 'session.created', thread: 3 }, 'event needs a "thread"');
  refuse({ ev: 'session.created', thread: 'main' }, 'event needs a positive integer "hseq"');
  refuse({ ev: 'session.created', thread: 'main', hseq: 0 }, 'event needs a positive integer "hseq"');
  refuse({ ev: 'session.created', thread: 'main', hseq: 1.5 }, 'event needs a positive integer "hseq"');
  assert.equal(log.events.length, 0);
  run.accept({ ev: 'proxyish', thread: 'main', hseq: 1 }); // only the prefix "proxy." is reserved
  assert.equal(log.events.length, 1);
});
