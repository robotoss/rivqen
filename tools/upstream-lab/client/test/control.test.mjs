// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import http from 'node:http';
import test from 'node:test';
import { HarnessChannel } from '../src/channel.mjs';
import { Control } from '../src/control.mjs';
import { LabError } from '../src/errors.mjs';
import { EventLog } from '../src/eventlog.mjs';
import { makeRunHandle } from '../src/runhandle.mjs';

async function setup(pollMs = 200) {
  const log = new EventLog();
  const channel = new HarnessChannel();
  const run = makeRunHandle({ id: 'CS-01-quick-r1', scenario: () => ({ run: 'CS-01-quick-r1', scenario: { id: 'CS-01' }, mode: 'quick' }), log, channel });
  const control = new Control({ port: 0, run, pollMs });
  await control.start();
  const url = (p) => `http://127.0.0.1:${control.port}${p}`;
  return { log, channel, control, url, stop: () => control.stop() };
}
const RUN = '?run=CS-01-quick-r1';
const post = (url, body, type = 'application/json') => fetch(url, { method: 'POST', headers: { 'content-type': type }, body: typeof body === 'string' ? body : JSON.stringify(body) });
const harnessEvent = (ev, extra = {}) => ({ ev, thread: 'main', ...extra });

test('control_scenario_returns_the_run', async () => {
  const c = await setup();
  try {
    const res = await fetch(c.url(`/run/scenario${RUN}`));
    assert.equal(res.status, 200);
    assert.equal(res.headers.get('content-type'), 'application/json');
    assert.deepEqual(await res.json(), { run: 'CS-01-quick-r1', scenario: { id: 'CS-01' }, mode: 'quick' });
  } finally {
    await c.stop();
  }
});

test('control_next_long_poll_delivers_the_dispatched_step', async () => {
  const c = await setup(2000);
  try {
    const poll = fetch(c.url(`/run/next${RUN}`));
    const step = { seq: 1, do: 'create_session' };
    const done = c.channel.dispatch(step, 5000);
    const res = await poll;
    assert.equal(res.status, 200);
    assert.deepEqual(await res.json(), step);
    assert.equal((await post(c.url('/run/event'), harnessEvent('step.done', { step }))).status, 204);
    assert.equal((await done).ev, 'step.done');
  } finally {
    await c.stop();
  }
});

test('control_next_answers_204_when_the_poll_times_out', async () => {
  const c = await setup(30);
  try {
    assert.equal((await fetch(c.url(`/run/next${RUN}`))).status, 204);
  } finally {
    await c.stop();
  }
});

test('control_next_gives_the_next_step_only_after_the_ack', async () => {
  const c = await setup(60);
  try {
    const a = { seq: 1, do: 'create_session' };
    const b = { seq: 2, do: 'bind_client' };
    const doneA = c.channel.dispatch(a, 5000);
    assert.deepEqual(await (await fetch(c.url(`/run/next${RUN}`))).json(), a);
    assert.equal((await fetch(c.url(`/run/next${RUN}`))).status, 204); // a is not acknowledged yet
    await post(c.url('/run/event'), harnessEvent('step.done', { step: a }));
    await doneA;
    const doneB = c.channel.dispatch(b, 5000);
    assert.deepEqual(await (await fetch(c.url(`/run/next${RUN}`))).json(), b);
    await post(c.url('/run/event'), harnessEvent('step.done', { step: b }));
    await doneB;
  } finally {
    await c.stop();
  }
});

test('control_next_answers_end_after_finish', async () => {
  const c = await setup(500);
  try {
    const finished = c.channel.finish(2000);
    const res = await fetch(c.url(`/run/next${RUN}`));
    assert.deepEqual(await res.json(), { do: 'end' });
    assert.equal(await finished, true);
  } finally {
    await c.stop();
  }
});

test('control_event_gets_a_sequence_number_and_the_harness_source', async () => {
  const c = await setup();
  try {
    await post(c.url('/run/event'), harnessEvent('session.created', { session_id: null, seq: 99, src: 'proxy', t: 99999 }));
    await post(c.url('/run/event'), harnessEvent('session.result', { src_code: 1000, final_code: 1000 }));
    const [first, second] = c.log.events;
    assert.equal(first.seq, 1);
    assert.equal(first.src, 'harness');
    assert.ok(first.t < 99999);
    assert.equal(second.seq, 2);
    assert.equal(second.final_code, 1000);
  } finally {
    await c.stop();
  }
});

test('control_event_rejects_bad_events', async () => {
  const c = await setup();
  try {
    const bad = [
      ['not json', 'x'],
      ['no name', {}],
      ['array', []],
      ['no thread', { ev: 'session.created' }],
      ['proxy name', harnessEvent('proxy.request')],
      ['run name', harnessEvent('run.end')],
      ['step.start', harnessEvent('step.start')],
    ];
    for (const [name, body] of bad) {
      const res = await post(c.url('/run/event'), body);
      assert.equal(res.status, 400, name);
    }
    assert.equal(c.log.events.length, 0);
  } finally {
    await c.stop();
  }
});

test('control_event_needs_the_json_content_type', async () => {
  const c = await setup();
  try {
    assert.equal((await post(c.url('/run/event'), '{}', 'text/plain')).status, 415);
    assert.equal((await post(c.url('/run/event'), harnessEvent('session.created', { session_id: null }), 'application/json; charset=utf-8')).status, 204);
  } finally {
    await c.stop();
  }
});

test('control_event_body_is_bounded', async () => {
  const c = await setup();
  try {
    const res = await post(c.url('/run/event'), JSON.stringify(harnessEvent('session.message', { json: 'x'.repeat(1024 * 1024 + 1) })));
    assert.equal(res.status, 413);
  } finally {
    await c.stop();
  }
});

test('control_rejects_unknown_run_path_method_and_host', async () => {
  const c = await setup();
  try {
    assert.equal((await fetch(c.url('/run/next?run=other'))).status, 404);
    assert.equal((await fetch(c.url('/run/next'))).status, 400);
    assert.equal((await fetch(c.url('/nothing'))).status, 404);
    assert.equal((await fetch(c.url('/run/toString'))).status, 404);
    assert.equal((await post(c.url(`/run/scenario${RUN}`), {})).status, 405);
    assert.equal((await fetch(c.url('/run/event'))).status, 405);
    const status = await new Promise((resolve, reject) => {
      const req = http.request({ host: '127.0.0.1', port: c.control.port, path: `/run/scenario${RUN}`, headers: { host: `evil.example:${c.control.port}` } }, (res) => {
        res.resume();
        resolve(res.statusCode);
      });
      req.on('error', reject);
      req.end();
    });
    assert.equal(status, 403);
  } finally {
    await c.stop();
  }
});

test('control_accepts_the_emulator_host_name', async () => {
  const c = await setup();
  try {
    const status = await new Promise((resolve, reject) => {
      const req = http.request({ host: '127.0.0.1', port: c.control.port, path: `/run/scenario${RUN}`, headers: { host: `10.0.2.2:${c.control.port}` } }, (res) => {
        res.resume();
        resolve(res.statusCode);
      });
      req.on('error', reject);
      req.end();
    });
    assert.equal(status, 200);
  } finally {
    await c.stop();
  }
});

test('control_channel_step_times_out_when_the_harness_is_silent', async () => {
  const channel = new HarnessChannel();
  await assert.rejects(channel.dispatch({ seq: 1, do: 'create_session' }, 20), (e) => e instanceof LabError && e.code === 'STEP_TIMEOUT');
});

test('control_channel_step_fails_when_the_harness_reports_an_error', async () => {
  const channel = new HarnessChannel();
  const step = { seq: 1, do: 'create_session' };
  const pending = channel.dispatch(step, 1000);
  assert.equal(channel.accept({ ev: 'step.done', step, error: 'boom' }), true);
  await assert.rejects(pending, (e) => e.code === 'STEP_FAILED' && e.message.includes('boom'));
});

test('control_channel_ignores_an_ack_for_another_step', async () => {
  const channel = new HarnessChannel();
  const pending = channel.dispatch({ seq: 2, do: 'bind_client' }, 1000);
  assert.equal(channel.accept({ ev: 'step.done', step: { seq: 1 } }), false);
  assert.equal(channel.accept({ ev: 'session.created', step: { seq: 2 } }), false);
  assert.equal(channel.accept({ ev: 'step.done', step: { seq: 2 } }), true);
  await pending;
});

test('control_channel_fail_rejects_the_step_and_the_next_ones', async () => {
  const channel = new HarnessChannel();
  const pending = channel.dispatch({ seq: 1, do: 'create_session' }, 1000);
  channel.fail(new Error('adb'));
  await assert.rejects(pending, (e) => e.code === 'HARNESS_FAILED');
  await assert.rejects(channel.dispatch({ seq: 2, do: 'destroy' }, 1000), (e) => e.code === 'HARNESS_FAILED');
});

test('control_channel_finish_gives_up_when_the_harness_never_polls', async () => {
  assert.equal(await new HarnessChannel().finish(20), false);
});

test('control_channel_allows_one_step_at_a_time', async () => {
  const channel = new HarnessChannel();
  const first = channel.dispatch({ seq: 1, do: 'create_session' }, 1000);
  await assert.rejects(channel.dispatch({ seq: 2, do: 'destroy' }, 1000), (e) => e.code === 'BUSY');
  channel.accept({ ev: 'step.done', step: { seq: 1 } });
  await first;
});
