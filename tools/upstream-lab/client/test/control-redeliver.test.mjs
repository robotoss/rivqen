// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import net from 'node:net';
import test from 'node:test';
import { HarnessChannel } from '../src/channel.mjs';
import { Control } from '../src/control.mjs';
import { EventLog } from '../src/eventlog.mjs';
import { makeRunHandle } from '../src/runhandle.mjs';
import { sleep } from './helpers.mjs';

async function setup() {
  const channel = new HarnessChannel();
  const run = makeRunHandle({ id: 'r1', scenario: () => ({}), log: new EventLog(), channel });
  const control = new Control({ port: 0, run, pollMs: 3000 });
  await control.start();
  return { channel, control, url: (p) => `http://127.0.0.1:${control.port}${p}` };
}

/** A long poll that the harness abandons: the socket is destroyed while the conductor still waits. */
async function abandonedPoll(port) {
  const socket = net.connect(port, '127.0.0.1');
  await new Promise((resolve) => socket.once('connect', resolve));
  socket.write(`GET /run/next?run=r1 HTTP/1.1\r\nHost: 127.0.0.1:${port}\r\n\r\n`);
  await sleep(50); // the server is now inside the long poll
  socket.destroy();
  await sleep(50); // and has seen the close
}

test('control_long_poll_redelivers_lost_answer', async () => {
  const c = await setup();
  try {
    await abandonedPoll(c.control.port);
    const step = { seq: 1, do: 'create_session' };
    const done = c.channel.dispatch(step, 5000);
    done.catch(() => {});
    await sleep(50); // the abandoned poll took the step and its answer went nowhere
    const res = await fetch(c.url('/run/next?run=r1'));
    assert.equal(res.status, 200);
    assert.deepEqual(await res.json(), step);
    // once the harness has the step and acknowledges it, nothing is offered again
    c.channel.accept({ ev: 'step.done', step });
    await done;
    c.channel.finish(1000).catch(() => {});
    assert.deepEqual(await (await fetch(c.url('/run/next?run=r1'))).json(), { do: 'end' });
  } finally {
    await c.control.stop();
  }
});

test('control_long_poll_redelivers_a_lost_end', async () => {
  const c = await setup();
  try {
    await abandonedPoll(c.control.port);
    const finished = c.channel.finish(1000);
    await sleep(50);
    assert.deepEqual(await (await fetch(c.url('/run/next?run=r1'))).json(), { do: 'end' });
    assert.equal(await finished, true);
  } finally {
    await c.control.stop();
  }
});

test('channel_redeliver_ignores_a_step_that_is_not_current', async () => {
  const channel = new HarnessChannel();
  const step = { seq: 1, do: 'destroy' };
  const done = channel.dispatch(step, 5000);
  assert.equal(await channel.poll(10), step);
  channel.accept({ ev: 'step.done', step: { seq: 1 } });
  await done;
  channel.redeliver(step);
  assert.equal(await channel.poll(20), null);
  channel.redeliver({ seq: 9, do: 'x' });
  assert.equal(await channel.poll(20), null);
});
