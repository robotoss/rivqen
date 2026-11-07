// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The S2 steps of H-126 in the conductor: `case` inside the steps, `set_body_member`, macro version 2.
import assert from 'node:assert/strict';
import test from 'node:test';
import { executeRun } from '../src/conductor.mjs';
import { normalizeEvent } from '../src/determinism.mjs';
import { eventProblems } from '../src/check.mjs';
import { FIRST_LOAD, stepProblem } from '../src/steps.mjs';
import { startUpstream } from './helpers.mjs';

/** A lab server that serves the page of the latest posted case (base64 decoded) and records the order of requests. */
async function caseLab() {
  let page = Buffer.from('none');
  const order = [];
  const lab = await startUpstream((req, res, body) => {
    if (req.method === 'POST') {
      page = Buffer.from(JSON.parse(body.toString()).page_base64, 'base64');
      order.push(`post:${page.subarray(0, 40).toString('hex')}`);
      res.writeHead(204);
      res.end();
      return;
    }
    order.push('get');
    res.sendDate = false;
    res.writeHead(200, ['Content-Type', 'text/html', 'Content-Length', String(page.length)]);
    res.end(page);
  });
  return { ...lab, order };
}

/** A harness that fetches the page on create_session and request_resource and reports the length it read. */
function harness() {
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
        if (step.do === 'create_session' || step.do === 'request_resource') {
          const r = await fetch(`http://127.0.0.1:${proxyPort}/catalog`);
          extra.read = (await r.arrayBuffer()).byteLength;
        }
        await fetch(`${base}/run/event`, {
          method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ ev: 'step.done', thread: 'main', hseq: ++hseq, step, ...extra }),
        });
      }
    })();
    return { stop: async () => { await loop.catch(() => {}); } };
  };
}

const scenario = (patch) => ({
  id: 'CS-92', title: 't', modes: ['quick'], origin: 'lab-node', rows: [], setup: [], case: { page: 'a' }, config: {}, steps: [{ do: 'create_session' }], repeat: 1, ...patch,
});
const run = (lab, patch, extra = {}) =>
  executeRun({
    scenario: scenario(patch), mode: 'quick', repeat: 1, upstreamSha: '59936bef', startHarness: harness(), proxyPort: 0, controlPort: 0,
    originPorts: { 'lab-node': lab.port }, sleep: async () => {}, stepMs: 5000, endMs: 5000, ...extra,
  });
const reads = (result) => result.log.events.filter((e) => e.ev === 'step.done' && e.src === 'harness' && 'read' in e).map((e) => e.read);

test('case_step_posts_value_to_origin_before_next_step', async () => {
  const lab = await caseLab();
  try {
    // pages a (296 bytes) and c (315 bytes) of tools/upstream-lab/pages
    const result = await run(lab, { case: { page: 'a' }, steps: [{ do: 'request_resource' }, { do: 'case', value: { page: 'c' } }, { do: 'request_resource' }] });
    assert.equal(result.error, null);
    assert.deepEqual(reads(result), [296, 315]);
    // the lab got the scenario case, the GET, the case of the step, then the second GET
    assert.deepEqual(lab.order.map((o) => o.split(':')[0]), ['post', 'get', 'post', 'get']);
    const posts = lab.seen.filter((s) => s.method === 'POST').map((s) => JSON.parse(s.body.toString()));
    assert.deepEqual(posts.map((p) => [p.case_id, p.profile, p.app.status]), [['CS-92-quick-r1', 'legacy-node', 200], ['CS-92-quick-r1', 'legacy-node', 200]]);
    // the conductor logs the step like its other steps
    const caseEvents = result.log.events.filter((e) => e.step?.do === 'case').map((e) => [e.ev, e.src, e.step.seq, e.error]);
    assert.deepEqual(caseEvents, [['step.start', 'conductor', 2, undefined], ['step.done', 'conductor', 2, undefined]]);
  } finally {
    await lab.close();
  }
});

test('case_step_with_a_status_and_fields_is_posted_as_given', async () => {
  const lab = await caseLab();
  try {
    const value = { page: 'b', status: 500, headers: [['x-a', '1']] };
    const result = await run(lab, { steps: [{ do: 'case', value }] });
    assert.equal(result.error, null);
    const posts = lab.seen.filter((s) => s.method === 'POST').map((s) => JSON.parse(s.body.toString()));
    assert.deepEqual(posts[1].app, { status: 500, headers: [['x-a', '1']] });
  } finally {
    await lab.close();
  }
});

test('case_step_fails_the_run_when_the_lab_refuses_it', async () => {
  const lab = await caseLab();
  try {
    const result = await run(lab, { steps: [{ do: 'case', value: { page: 'missing' } }, { do: 'create_session' }] });
    assert.equal(result.ok, false);
    assert.match(result.error, /^NO_PAGE: /);
    const done = result.log.events.find((e) => e.ev === 'step.done' && e.step.do === 'case');
    assert.match(done.error, /cannot read page missing/);
    assert.ok(!result.log.events.some((e) => e.ev === 'step.start' && e.step.do === 'create_session'));
  } finally {
    await lab.close();
  }
});

test('first_load_macro_v2_settles_and_marks_run_start', async () => {
  const lab = await caseLab();
  const slept = [];
  try {
    const result = await run(lab, { setup: [{ case: { page: 'a' } }, 'first_load'], case: { page: 'b' }, steps: [{ do: 'create_session' }] }, { sleep: async (ms) => { slept.push(ms); } });
    assert.equal(result.error, null);
    const start = result.log.events[0];
    assert.deepEqual([start.ev, start.macro], ['run.start', 2]);
    assert.deepEqual(eventProblems(start), []);
    const steps = result.log.events.filter((e) => e.ev === 'step.start').map((e) => e.step);
    assert.deepEqual(steps.map(({ seq, ...s }) => s), [...FIRST_LOAD, { do: 'create_session' }]);
    assert.deepEqual(slept, [300, 2500, 1500]);
    // the 300 ms settle comes after the wait for the end of the body and before client_ready
    const order = steps.map((s) => s.do);
    assert.deepEqual(order.slice(2, 5), ['wait', 'settle', 'client_ready']);
    assert.deepEqual(order.slice(7, 9), ['destroy', 'settle']);
  } finally {
    await lab.close();
  }
});

test('set_body_member_step_is_armed_by_the_conductor', async () => {
  const lab = await caseLab();
  try {
    const result = await run(lab, { case: { page: 'empty' }, steps: [{ do: 'set_body_member', name: 'html-sha1', value: 'x' }, { do: 'request_resource' }] });
    assert.equal(result.error, null);
    const ev = result.log.events.filter((e) => e.ev === 'proxy.set_body_member');
    assert.deepEqual(ev.map((e) => [e.exchange, e.applied, e.reason]), [[1, false, 'empty body']]);
  } finally {
    await lab.close();
  }
});

test('run_start_macro_is_checked_and_normalized', () => {
  const base = { ev: 'run.start', seq: 1, src: 'conductor', t: 0, scenario: 'CS-01', mode: 'quick', repeat: 1, origin: 'lab-node', upstream_sha: '59936bef' };
  assert.deepEqual(eventProblems(base), []); // stored runs before H-126 have no macro
  assert.deepEqual(eventProblems({ ...base, macro: 2 }), []);
  assert.deepEqual(eventProblems({ ...base, macro: 1 }), ['run.start: bad macro']);
  assert.deepEqual(eventProblems({ ...base, macro: '2' }), ['run.start: bad macro']);
  assert.deepEqual(normalizeEvent({ ...base, macro: 2, repeat: 3 }), normalizeEvent(base));
});

test('step_problem_checks_set_body_member_and_case', () => {
  assert.equal(stepProblem({ do: 'set_body_member', name: 'html-sha1', value: '0' }), null);
  assert.equal(stepProblem({ do: 'set_body_member', name: 'x', value: '' }), null);
  assert.equal(stepProblem({ do: 'case', value: { page: 'c' } }), null);
  assert.equal(stepProblem({ do: 'case', value: { page: 'c', status: 404, headers: [['a', 'b']] } }), null);
  const bad = {
    'set_body_member: name must be a non-empty string': [{ do: 'set_body_member', value: 'v' }, { do: 'set_body_member', name: '', value: 'v' }],
    'set_body_member: value must be a string': [{ do: 'set_body_member', name: 'x' }, { do: 'set_body_member', name: 'x', value: 1 }],
    'set_body_member: unknown member bytes': [{ do: 'set_body_member', name: 'x', value: 'v', bytes: 1 }],
    'case: case must be an object': [{ do: 'case' }, { do: 'case', value: [] }, { do: 'case', value: null }],
    'case: case.page must be a page name': [{ do: 'case', value: { page: '../a' } }],
    'case: case.status must be 100..599': [{ do: 'case', value: { page: 'a', status: 99 } }],
    'case: case: unknown member x': [{ do: 'case', value: { page: 'a', x: 1 } }],
    'case: unknown member page': [{ do: 'case', page: 'a' }],
  };
  for (const [problem, steps] of Object.entries(bad)) for (const s of steps) assert.equal(stepProblem(s), problem, JSON.stringify(s));
});
