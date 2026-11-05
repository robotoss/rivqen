// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import test from 'node:test';
import { executeRun } from '../src/conductor.mjs';
import { startUpstream } from './helpers.mjs';

const noSleep = async () => {};

async function fakeLab() {
  return startUpstream((req, res, body) => {
    if (req.method === 'POST') {
      res.writeHead(204);
      res.end();
      return;
    }
    res.sendDate = false;
    res.writeHead(200, ['Content-Type', 'text/html', 'ETag', 'e1', 'Content-Length', '20']);
    res.end('0123456789abcdefghij');
  });
}

/** A harness that acknowledges every step; request_resource fetches the page and reports `fetch_ok`. */
function harness({ stopBeforeEnd = false } = {}) {
  return async ({ runId, controlPort, proxyPort }) => {
    const base = `http://127.0.0.1:${controlPort}`;
    let hseq = 0;
    const loop = (async () => {
      for (;;) {
        const res = await fetch(`${base}/run/next?run=${runId}`);
        if (res.status === 204) continue;
        const step = await res.json();
        if (step.do === 'end') return;
        const extra = {};
        if (step.do === 'request_resource') {
          try {
            const r = await fetch(`http://127.0.0.1:${proxyPort}/catalog`);
            extra.fetch_ok = (await r.arrayBuffer()).byteLength === 20;
          } catch {
            extra.fetch_ok = false;
          }
        }
        await fetch(`${base}/run/event`, {
          method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ ev: 'step.done', thread: 'main', hseq: ++hseq, step, ...extra }),
        });
        if (stopBeforeEnd) return;
      }
    })();
    return { stop: async () => { await loop.catch(() => {}); } };
  };
}

const scenario = (steps) => ({
  id: 'CS-91', title: 't', modes: ['quick'], origin: 'lab-node', rows: [], setup: [], case: { page: 'a' }, config: {}, steps, repeat: 1,
});
const run = (lab, steps, startHarness = harness(), extra = {}) =>
  executeRun({
    scenario: scenario(steps), mode: 'quick', repeat: 1, upstreamSha: '59936bef', startHarness, proxyPort: 0, controlPort: 0,
    originPorts: { 'lab-node': lab.port }, sleep: noSleep, stepMs: 5000, endMs: 5000, ...extra,
  });
const rr = { do: 'request_resource' };

test('conductor_runs_the_fault_steps_cut_drop_field_server_down_and_server_up', async () => {
  const lab = await fakeLab();
  try {
    const result = await run(lab, [
      { do: 'cut', bytes: 3 }, rr, // the stream is cut: the harness cannot read all 20 bytes
      { do: 'drop_field', name: 'etag' }, rr,
      { do: 'server_down' }, rr, // the proxy refuses the connection
      { do: 'server_up' }, rr,
    ]);
    assert.equal(result.error, null);
    const fetched = result.log.events.filter((e) => e.ev === 'step.done' && e.step.do === 'request_resource').map((e) => e.fetch_ok);
    assert.deepEqual(fetched, [false, true, false, true]);
    assert.deepEqual(result.log.events.filter((e) => e.ev === 'proxy.cut').map((e) => e.bytes), [3]);
    assert.deepEqual(result.log.events.filter((e) => e.ev === 'proxy.drop_field').map((e) => e.name), ['etag']);
    const conductorDone = result.log.events.filter((e) => e.ev === 'step.done' && e.src === 'conductor').map((e) => e.step.do);
    assert.deepEqual(conductorDone, ['cut', 'drop_field', 'server_down', 'server_up']);
  } finally {
    await lab.close();
  }
});

test('conductor_settle_waits_through_the_injected_sleep', async () => {
  const lab = await fakeLab();
  const slept = [];
  try {
    const result = await run(lab, [{ do: 'settle', ms: 123 }], harness(), { sleep: async (ms) => { slept.push(ms); } });
    assert.equal(result.ok, true);
    assert.deepEqual(slept, [123]);
  } finally {
    await lab.close();
  }
});

test('conductor_fails_the_run_when_the_harness_never_fetches_the_end', async () => {
  const lab = await fakeLab();
  try {
    const result = await run(lab, [{ do: 'create_session' }], harness({ stopBeforeEnd: true }), { endMs: 30 });
    assert.equal(result.ok, false);
    assert.equal(result.error, 'HARNESS_LOST: the harness did not fetch the end of the run');
    assert.deepEqual([result.log.events.at(-1).ev, result.log.events.at(-1).ok], ['run.end', false]);
  } finally {
    await lab.close();
  }
});

test('conductor_reports_an_unexpected_error_with_its_message', async () => {
  const lab = await fakeLab();
  try {
    const result = await run(lab, [{ do: 'create_session' }], async () => { throw new Error('no adb'); });
    assert.equal(result.error, 'INTERNAL: no adb');
    assert.equal(result.ok, false);
  } finally {
    await lab.close();
  }
});

test('conductor_logs_the_clock_of_the_run', async () => {
  const lab = await fakeLab();
  let now = 1000;
  try {
    const result = await run(lab, [{ do: 'settle', ms: 1 }], harness(), { clock: () => (now += 5) });
    assert.deepEqual(result.log.events.map((e) => e.t).slice(0, 3), [5, 10, 15]);
  } finally {
    await lab.close();
  }
});
