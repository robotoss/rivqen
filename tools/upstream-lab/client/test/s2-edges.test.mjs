// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import net from 'node:net';
import test from 'node:test';
import { HarnessChannel } from '../src/channel.mjs';
import { eventProblems, runJsonProblems } from '../src/check.mjs';
import { Control } from '../src/control.mjs';
import { normalizeEvent } from '../src/determinism.mjs';
import { pairs } from '../src/proxy.mjs';
import { send, sleep, startProxy, startUpstream } from './helpers.mjs';

const message = (json) => normalizeEvent({ ev: 'session.message', json }).json;

test('normalize_message_keeps_white_space_and_removes_only_the_member', () => {
  assert.equal(message(' { "a" : 1 , "local_refresh_time" : 2 , "b" : [ 1 ] } '), ' { "a" : 1 , "b" : [ 1 ] } ');
  assert.equal(message('\n{"local_refresh_time":2}'), '\n{}');
});

test('normalize_message_ignores_braces_inside_strings', () => {
  assert.equal(message('{"x":"}]","local_refresh_time":1,"y":"{["}'), '{"x":"}]","y":"{["}');
});

test('channel_redeliver_of_the_end_wakes_a_waiting_poll', async () => {
  const channel = new HarnessChannel();
  channel.finish(50).catch(() => {});
  const end = await channel.poll(10);
  assert.deepEqual(end, { do: 'end' });
  const waiting = channel.poll(3000);
  await sleep(20);
  const started = Date.now();
  channel.redeliver(end);
  assert.deepEqual(await waiting, { do: 'end' });
  assert.ok(Date.now() - started < 1500);
});

test('channel_redeliver_wakes_a_waiting_poll', async () => {
  const channel = new HarnessChannel();
  const step = { seq: 1, do: 'create_session' };
  channel.dispatch(step, 5000).catch(() => {});
  assert.equal(await channel.poll(10), step);
  const started = Date.now();
  const waiting = channel.poll(3000);
  await sleep(20);
  channel.redeliver(step);
  assert.equal(await waiting, step);
  assert.ok(Date.now() - started < 1500, 'the poll did not wait for its timeout');
});

test('channel_redeliver_ignores_an_older_step_and_a_step_that_was_not_delivered', async () => {
  const channel = new HarnessChannel();
  const first = { seq: 1, do: 'bind_client' };
  const second = { seq: 2, do: 'destroy' };
  const done = channel.dispatch(first, 5000);
  await channel.poll(10);
  channel.accept({ ev: 'step.done', step: { seq: 1 } });
  await done;
  channel.dispatch(second, 5000).catch(() => {});
  assert.equal(await channel.poll(10), second);
  channel.redeliver(first); // an older step must not reopen the current one
  assert.equal(await channel.poll(20), null);
});

test('channel_redeliver_of_a_step_does_not_depend_on_the_end_of_the_run', async () => {
  const channel = new HarnessChannel();
  const step = { seq: 1, do: 'destroy' };
  channel.dispatch(step, 5000).catch(() => {});
  assert.equal(await channel.poll(10), step);
  channel.finish(50).catch(() => {});
  channel.redeliver(step);
  assert.equal(await channel.poll(20), step);
});

async function lostAnswer({ withRedeliver }) {
  const offered = [];
  let release;
  const run = {
    id: 'r1',
    scenario: () => ({}),
    poll: () => new Promise((resolve) => { release = resolve; }),
    accept: () => {},
    ...(withRedeliver ? { redeliver: (step) => offered.push(step) } : {}),
  };
  const control = new Control({ port: 0, run, pollMs: 3000 });
  await control.start();
  try {
    const socket = net.connect(control.port, '127.0.0.1');
    await new Promise((resolve) => socket.once('connect', resolve));
    socket.on('error', () => {});
    socket.write(`GET /run/next?run=r1 HTTP/1.1\r\nHost: 127.0.0.1:${control.port}\r\n\r\n`);
    await sleep(50);
    socket.resetAndDestroy(); // a reset: the write of the answer fails
    release({ seq: 1, do: 'destroy' }); // the poll ends in the same turn: the server has not seen the close yet
    await sleep(100);
  } finally {
    await control.stop();
  }
  return offered;
}

test('control_offers_the_step_again_when_the_answer_could_not_be_sent', async () => {
  assert.deepEqual(await lostAnswer({ withRedeliver: true }), [{ seq: 1, do: 'destroy' }]);
});

test('control_survives_a_lost_answer_for_a_run_without_redelivery', async () => {
  assert.deepEqual(await lostAnswer({ withRedeliver: false }), []);
});

test('control_does_not_offer_a_step_again_when_the_answer_was_sent', async () => {
  const offered = [];
  const run = { id: 'r1', scenario: () => ({}), poll: async () => ({ seq: 1, do: 'destroy' }), accept: () => {}, redeliver: (s) => offered.push(s) };
  const control = new Control({ port: 0, run, pollMs: 50 });
  await control.start();
  try {
    const res = await fetch(`http://127.0.0.1:${control.port}/run/next?run=r1`);
    assert.deepEqual(await res.json(), { seq: 1, do: 'destroy' });
    await sleep(50);
    assert.deepEqual(offered, []);
  } finally {
    await control.stop();
  }
});

test('proxy_connection_listing_uses_only_connection_fields', async () => {
  const up = await startUpstream((req, res) => {
    res.sendDate = false;
    res.writeHead(200, ['X-A', 'x-b', 'X-B', 'b', 'Connection', 'close,, ', 'Content-Length', '0']);
    res.end();
  });
  const px = await startProxy(up.port);
  try {
    const res = await send({ port: px.proxy.port });
    assert.deepEqual(pairs(res.rawHeaders).filter(([n]) => n.startsWith('X-')), [['X-A', 'x-b'], ['X-B', 'b']]);
  } finally {
    await px.proxy.stop();
    await up.close();
  }
});

test('check_schema_accepts_set_field_events_and_rejects_bad_ones', () => {
  const ok = { ev: 'proxy.set_field', src: 'proxy', seq: 3, t: 1, exchange: 1, name: 'cache-offline', value: 'store' };
  assert.deepEqual(eventProblems(ok), []);
  assert.deepEqual(eventProblems({ ...ok, value: '' }), []);
  assert.notDeepEqual(eventProblems({ ...ok, value: 5 }), []);
  assert.notDeepEqual(eventProblems({ ...ok, name: 5 }), []);
  assert.notDeepEqual(eventProblems({ ...ok, src: 'harness' }), []);
  const { value: _gone, ...without } = ok;
  assert.notDeepEqual(eventProblems(without), []);
});

test('run_json_not_checked_is_validated_only_with_a_valid_summary', () => {
  const base = { date: '2026-11-05', tools: {}, emulator_image: 'x', upstream_sha: '59936bef', scenarios: [] };
  const summary = (s) => ({ ...base, summary: s });
  const key = { scenario: 'CS-01', mode: 'quick' };
  assert.deepEqual(runJsonProblems(summary({ stable: [], unstable: [], not_checked: [key] })), []);
  assert.equal(runJsonProblems(summary({ stable: [], unstable: [], not_checked: [null] })).length, 1);
  assert.equal(runJsonProblems(summary({ stable: [], unstable: [], not_checked: [{ mode: 'quick' }] })).length, 1);
  assert.equal(runJsonProblems(summary({ stable: [], unstable: [], not_checked: [{ scenario: 'CS-01' }] })).length, 1);
  assert.equal(runJsonProblems(summary({ stable: 'x', unstable: [], not_checked: 'x' })).length, 1);
});
