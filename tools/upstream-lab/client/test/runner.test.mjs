// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import { mkdtemp, readdir, readFile, rm, writeFile, mkdir } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { checkTree } from '../src/check.mjs';
import { executeRun } from '../src/conductor.mjs';
import { LabError } from '../src/errors.mjs';
import { parseArgs, refreshRunJson, runAll, selectRuns, storeRun } from '../src/runner.mjs';
import { startUpstream } from './helpers.mjs';

const SCENARIO = {
  id: 'CS-90',
  title: 'Fake scenario',
  modes: ['quick', 'standard'],
  origin: 'lab-node',
  rows: [],
  setup: [{ case: { page: 'a' } }, 'first_load'],
  case: { page: 'b' },
  config: { account: 'rq-test-account' },
  steps: [
    { do: 'hold', point: 'head' },
    { do: 'create_session' },
    { do: 'bind_client' },
    { do: 'client_ready' },
    { do: 'request_resource' },
    { do: 'wait', for: 'proxy.hold' },
    { do: 'release' },
    { do: 'wait', for: 'proxy.response_end' },
    { do: 'settle', ms: 1 },
    { do: 'snapshot_cache' },
    { do: 'destroy' },
  ],
  repeat: 2,
};

/** A lab server: accepts cases, answers every other path with a page. */
async function fakeLab() {
  const cases = [];
  const lab = await startUpstream((req, res, body) => {
    if (req.method === 'POST' && req.url === '/__rivqen/conformance/case') {
      cases.push(JSON.parse(body.toString()));
      res.writeHead(204);
      res.end();
      return;
    }
    res.sendDate = false;
    res.writeHead(200, ['Content-Type', 'text/html', 'ETag', 'e1', 'Content-Length', '5']);
    res.end('<p/>\n');
  });
  return { ...lab, cases };
}

/** A harness that follows the control channel like CONTRACT sections 3 and 4. */
function fakeHarness({ failOn = null } = {}) {
  return async ({ runId, controlPort, proxyPort }) => {
    const base = `http://127.0.0.1:${controlPort}`;
    const pending = [];
    let hseq = 0;
    const event = (ev, members = {}, thread = 'main') =>
      fetch(`${base}/run/event`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ ev, thread, hseq: ++hseq, ...members }) });
    const loop = (async () => {
      const scenario = await (await fetch(`${base}/run/scenario?run=${runId}`)).json();
      assert.equal(scenario.run, runId);
      for (;;) {
        const res = await fetch(`${base}/run/next?run=${runId}`);
        if (res.status === 204) continue;
        const step = await res.json();
        if (step.do === 'end') {
          // A held request may stay open until the conductor stops the proxy: do not wait for it for long.
          await Promise.race([Promise.allSettled(pending), new Promise((resolve) => setTimeout(resolve, 150))]);
          return;
        }
        if (step.do === 'create_session') await event('session.created', { session_id: 'sid' });
        if (step.do === 'snapshot_cache') await event('cache.snapshot', { files: [], db: null });
        // The engine starts the network request when the session is created (CONTRACT section 3).
        if (step.do === 'create_session') {
          pending.push(
            fetch(`http://127.0.0.1:${proxyPort}/catalog`).then(async (r) => {
              const body = Buffer.from(await r.arrayBuffer());
              await event('client.intercept', {
                status: r.status, reason: 'OK', mime: 'text/html', encoding: 'utf-8', headers: [['content-type', 'text/html']],
                body_len: body.length, body_sha256: '0'.repeat(64), complete: true,
              }, 'worker-1');
            }).catch(() => {}),
          );
        }
        await event('step.done', failOn === step.do ? { step, error: 'refused' } : { step });
      }
    })();
    return { stop: async () => { await loop.catch(() => {}); } };
  };
}

const noSleep = async () => {};

test('runner_fake_harness', async () => {
  const lab = await fakeLab();
  const out = await mkdtemp(path.join(os.tmpdir(), 'lab-client-'));
  const adbCalls = [];
  try {
    const runs = selectRuns([SCENARIO], { all: true });
    assert.equal(runs.length, 4);
    const results = await runAll({
      runs: runs.slice(0, 2), // quick, repeats 1 and 2
      outDir: out,
      cfg: {},
      deps: { adb: async (args) => { adbCalls.push(args); }, startHarness: fakeHarness(), adbVersion: async () => 'adb 1.0' },
      upstreamSha: '59936bef',
      emulatorImage: 'api28-default-arm64',
      runOptions: { proxyPort: 0, controlPort: 0, originPorts: { 'lab-node': lab.port }, sleep: noSleep, stepMs: 5000, endMs: 5000 },
      now: () => new Date('2026-11-05T10:00:00Z'),
    });
    assert.deepEqual(results.map((r) => [r.scenario, r.mode, r.k, r.ok, r.error]), [
      ['CS-90', 'quick', 1, true, null],
      ['CS-90', 'quick', 2, true, null],
    ]);
    // adb: the app is reset before and stopped after each run
    assert.deepEqual(adbCalls, [
      ['-s', 'emulator-5600', 'shell', 'pm', 'clear', 'org.rivqen.lab.harness'],
      ['-s', 'emulator-5600', 'shell', 'am', 'force-stop', 'org.rivqen.lab.harness'],
      ['-s', 'emulator-5600', 'shell', 'pm', 'clear', 'org.rivqen.lab.harness'],
      ['-s', 'emulator-5600', 'shell', 'am', 'force-stop', 'org.rivqen.lab.harness'],
    ]);
    // the lab got the setup case, then the scenario case, for each run
    assert.deepEqual(lab.cases.map((c) => [c.case_id, c.profile, c.suite]).slice(0, 2), [
      ['CS-90-quick-r1', 'legacy-node', 'legacy'],
      ['CS-90-quick-r1', 'legacy-node', 'legacy'],
    ]);
    assert.equal(lab.cases.length, 4);
    assert.notEqual(lab.cases[0].page_base64, lab.cases[1].page_base64); // page a, then page b

    const dir = path.join(out, 'android', 'CS-90', 'quick');
    assert.deepEqual((await readdir(dir)).sort(), ['run-1-x1.http', 'run-1-x2.http', 'run-1.jsonl', 'run-2-x1.http', 'run-2-x2.http', 'run-2.jsonl']);
    const events = (await readFile(path.join(dir, 'run-1.jsonl'), 'utf8')).trimEnd().split('\n').map((l) => JSON.parse(l));
    assert.equal(events[0].ev, 'run.start');
    assert.deepEqual({ ...events[0], t: 0 }, { ev: 'run.start', mode: 'quick', origin: 'lab-node', repeat: 1, scenario: 'CS-90', seq: 1, src: 'conductor', t: 0, upstream_sha: '59936bef' });
    assert.deepEqual(events.at(-1).ok, true);
    const started = events.filter((e) => e.ev === 'step.start').map((e) => e.step.do);
    // setup first_load (7 steps), then the 11 steps of the scenario
    assert.equal(started.length, 18);
    assert.deepEqual(started.slice(0, 3), ['create_session', 'bind_client', 'wait']);
    assert.deepEqual(events.filter((e) => e.ev === 'step.start').map((e) => e.step.seq), Array.from({ length: 18 }, (_, i) => i + 1));
    assert.equal(events.filter((e) => e.ev === 'proxy.hold').length, 1);
    assert.equal(events.filter((e) => e.ev === 'proxy.response_end').length, 2);

    // run.json and the offline check accept what the runner stored
    const runJson = JSON.parse(await readFile(path.join(out, 'run.json'), 'utf8'));
    assert.equal(runJson.date, '2026-11-05');
    assert.equal(runJson.emulator_image, 'api28-default-arm64');
    assert.deepEqual(runJson.tools, { node: process.version, adb: 'adb 1.0', conductor: '1' });
    assert.deepEqual(runJson.scenarios, [{ id: 'CS-90', modes: { quick: { runs: 2 } } }]);
    const checked = await checkTree(out, { catalog: [{ ...SCENARIO, modes: ['quick'] }] });
    assert.deepEqual(checked.errors, []);
  } finally {
    await lab.close();
    await rm(out, { recursive: true, force: true });
  }
});

test('runner_stores_a_failed_run_and_reports_the_harness_error', async () => {
  const lab = await fakeLab();
  try {
    const result = await executeRun({
      scenario: SCENARIO, mode: 'quick', repeat: 1, upstreamSha: '59936bef', startHarness: fakeHarness({ failOn: 'bind_client' }),
      proxyPort: 0, controlPort: 0, originPorts: { 'lab-node': lab.port }, sleep: noSleep, stepMs: 5000, endMs: 5000,
    });
    assert.equal(result.ok, false);
    assert.match(result.error, /^STEP_FAILED: harness step 2 failed: refused/);
    const last = result.log.events.at(-1);
    assert.deepEqual([last.ev, last.ok], ['run.end', false]);
    const done = result.log.events.filter((e) => e.ev === 'step.done' && e.error);
    assert.equal(done.length, 1); // the harness logged it; the conductor did not log it a second time
    assert.equal(done[0].src, 'harness');
  } finally {
    await lab.close();
  }
});

test('runner_a_hung_harness_fails_the_run_instead_of_hanging', async () => {
  const lab = await fakeLab();
  try {
    const result = await executeRun({
      scenario: SCENARIO, mode: 'quick', repeat: 1, upstreamSha: '59936bef', startHarness: async () => ({ stop: async () => {} }),
      proxyPort: 0, controlPort: 0, originPorts: { 'lab-node': lab.port }, sleep: noSleep, stepMs: 50, endMs: 20,
    });
    assert.equal(result.ok, false);
    assert.match(result.error, /^STEP_TIMEOUT: /);
    const failed = result.log.events.find((e) => e.ev === 'step.done');
    assert.equal(failed.src, 'conductor');
    assert.match(failed.error, /did not finish step 1/);
  } finally {
    await lab.close();
  }
});

test('runner_a_harness_that_cannot_start_fails_the_run', async () => {
  const lab = await fakeLab();
  try {
    const result = await executeRun({
      scenario: SCENARIO, mode: 'quick', repeat: 1, upstreamSha: '59936bef',
      startHarness: async ({ onFatal }) => { onFatal(new LabError('ADB', 'no device')); return { stop: async () => {} }; },
      proxyPort: 0, controlPort: 0, originPorts: { 'lab-node': lab.port }, sleep: noSleep, stepMs: 5000, endMs: 20,
    });
    assert.equal(result.ok, false);
    assert.match(result.error, /^HARNESS_FAILED/);
  } finally {
    await lab.close();
  }
});

test('runner_fails_when_the_lab_rejects_the_case', async () => {
  const lab = await startUpstream((req, res) => { res.writeHead(400); res.end(); });
  try {
    const result = await executeRun({
      scenario: SCENARIO, mode: 'quick', repeat: 1, upstreamSha: '59936bef', startHarness: fakeHarness(),
      proxyPort: 0, controlPort: 0, originPorts: { 'lab-node': lab.port }, sleep: noSleep, stepMs: 5000, endMs: 5000,
    });
    assert.equal(result.ok, false);
    assert.match(result.error, /^CASE_REJECTED: .* 400/);
  } finally {
    await lab.close();
  }
});

test('runner_conductor_steps_are_logged_and_a_wait_timeout_fails_the_run', async () => {
  const lab = await fakeLab();
  const scenario = { ...SCENARIO, setup: [], steps: [{ do: 'server_down' }, { do: 'server_up' }, { do: 'wait', for: 'never', timeout_ms: 20 }] };
  try {
    const result = await executeRun({
      scenario, mode: 'quick', repeat: 1, upstreamSha: '59936bef', startHarness: fakeHarness(),
      proxyPort: 0, controlPort: 0, originPorts: { 'lab-node': lab.port }, sleep: noSleep, stepMs: 5000, endMs: 5000,
    });
    assert.match(result.error, /^WAIT_TIMEOUT: no event "never" within 20 ms/);
    const done = result.log.events.filter((e) => e.ev === 'step.done').map((e) => [e.step.do, e.error ?? null]);
    assert.deepEqual(done, [['server_down', null], ['server_up', null], ['wait', 'no event "never" within 20 ms']]);
  } finally {
    await lab.close();
  }
});

test('runner_wait_sees_only_events_after_the_latest_create_session', async () => {
  const lab = await fakeLab();
  // proxy.response_end of the setup load must not satisfy the wait of the scenario
  const scenario = { ...SCENARIO, steps: [{ do: 'hold', point: 'head' }, { do: 'create_session' }, { do: 'wait', for: 'proxy.response_end', timeout_ms: 30 }] };
  try {
    const result = await executeRun({
      scenario, mode: 'quick', repeat: 1, upstreamSha: '59936bef', startHarness: fakeHarness(),
      proxyPort: 0, controlPort: 0, originPorts: { 'lab-node': lab.port }, sleep: noSleep, stepMs: 5000, endMs: 5000,
    });
    assert.match(result.error, /^WAIT_TIMEOUT/);
  } finally {
    await lab.close();
  }
});

test('runner_store_replaces_the_files_of_the_same_run_only', async () => {
  const out = await mkdtemp(path.join(os.tmpdir(), 'lab-client-'));
  try {
    const dir = path.join(out, 'android', 'CS-01', 'quick');
    await mkdir(dir, { recursive: true });
    await writeFile(path.join(dir, 'run-1-x9.http'), 'stale');
    await writeFile(path.join(dir, 'run-2.jsonl'), 'other run');
    const log = { toJsonl: () => '{"ev":"run.start"}\n' };
    await storeRun(out, 'CS-01', 'quick', 1, { log, exchanges: new Map([[1, Buffer.from('x')]]) });
    assert.deepEqual((await readdir(dir)).sort(), ['run-1-x1.http', 'run-1.jsonl', 'run-2.jsonl']);
  } finally {
    await rm(out, { recursive: true, force: true });
  }
});

test('runner_parse_args_accepts_the_documented_options', () => {
  const o = parseArgs(['--scenario', 'CS-05', '--mode', 'quick', '--repeat', '3', '--out', '/tmp/x', '--package', 'a.b', '--serial', 'emulator-5554', '--step-timeout', '9000', '--dry-run']);
  assert.deepEqual(o, { all: false, dryRun: true, runJsonOnly: false, scenario: 'CS-05', mode: 'quick', repeat: 3, out: '/tmp/x', package: 'a.b', serial: 'emulator-5554', stepTimeout: 9000 });
  assert.equal(parseArgs(['--all']).all, true);
  assert.equal(parseArgs(['--run-json', '--out', '/tmp/x']).runJsonOnly, true);
});

test('runner_parse_args_rejects_bad_use', () => {
  const bad = [
    [], ['--all', '--scenario', 'CS-01'], ['--run-json', '--all'], ['--run-json', '--scenario', 'CS-01'], ['--scenario'], ['--scenario', '--all'], ['--scenario', 'CS-01', '--mode', 'fast'],
    ['--scenario', 'CS-01', '--repeat', '0'], ['--scenario', 'CS-01', '--repeat', 'x'], ['--scenario', 'CS-01', '--repeat', '101'],
    ['--scenario', 'CS-01', '--step-timeout', '-1'], ['--scenario', 'CS-01', '--nope'],
  ];
  for (const argv of bad) assert.throws(() => parseArgs(argv), (e) => e instanceof LabError && e.code === 'USAGE', argv.join(' '));
});

test('runner_select_runs_expands_mode_and_repeat', () => {
  const list = [SCENARIO, { ...SCENARIO, id: 'CS-91', modes: ['standard'], repeat: 1 }];
  assert.deepEqual(selectRuns(list, { all: true }).map((r) => `${r.scenario.id}/${r.mode}/${r.k}`), ['CS-90/quick/1', 'CS-90/quick/2', 'CS-90/standard/1', 'CS-90/standard/2', 'CS-91/standard/1']);
  assert.deepEqual(selectRuns(list, { scenario: 'CS-90', mode: 'standard', repeat: 1 }).map((r) => `${r.mode}/${r.k}`), ['standard/1']);
  assert.throws(() => selectRuns(list, { scenario: 'CS-77' }), /no scenario/);
  assert.throws(() => selectRuns(list, { scenario: 'CS-91', mode: 'quick' }), /does not run in mode quick/);
});

test('runner_run_json_takes_the_meta_files_of_the_function_runners', async () => {
  const out = await mkdtemp(path.join(os.tmpdir(), 'lab-client-'));
  try {
    const iosMeta = { platform: 'ios', macos_version: '26.1.0', macos_build: 'Version 26.1 (Build X)', sdk_version: '27.0', compiler: 'clang', upstream_sha: '59936bef', cases: 2, limit: 'Foundation of today', non_public_calls: ['SonicServer -encodingFromHeaders'] };
    await mkdir(path.join(out, 'ios'), { recursive: true });
    await writeFile(path.join(out, 'ios', 'functions.jsonl.meta.json'), JSON.stringify(iosMeta));
    const first = await refreshRunJson({ outDir: out, upstreamSha: '59936bef', now: () => new Date('2026-11-05T00:00:00Z') });
    assert.deepEqual(first.functions, { ios: iosMeta });
    assert.equal(first.date, '2026-11-05');
    assert.deepEqual(JSON.parse(await readFile(path.join(out, 'run.json'), 'utf8')).functions.ios.non_public_calls, ['SonicServer -encodingFromHeaders']);
    await rm(path.join(out, 'ios', 'functions.jsonl.meta.json'));
    assert.equal('functions' in (await refreshRunJson({ outDir: out, upstreamSha: '59936bef' })), false);
    await writeFile(path.join(out, 'run.json'), '{');
    await assert.rejects(refreshRunJson({ outDir: out, upstreamSha: '59936bef' }), (e) => e.code === 'RUN_JSON');
    await writeFile(path.join(out, 'run.json'), '{}');
    await mkdir(path.join(out, 'android'), { recursive: true });
    await writeFile(path.join(out, 'android', 'functions.jsonl.meta.json'), '{');
    await assert.rejects(refreshRunJson({ outDir: out, upstreamSha: '59936bef' }), (e) => e.code === 'BAD_JSON');
  } finally {
    await rm(out, { recursive: true, force: true });
  }
});

test('runner_request_resource_may_take_wait_ms_longer_than_the_step_timeout', async () => {
  const lab = await fakeLab();
  const slow = (delayMs) => async ({ runId, controlPort }) => {
    const base = `http://127.0.0.1:${controlPort}`;
    let hseq = 0;
    const loop = (async () => {
      for (;;) {
        const res = await fetch(`${base}/run/next?run=${runId}`);
        if (res.status === 204) continue;
        const step = await res.json();
        if (step.do === 'end') return;
        await new Promise((resolve) => setTimeout(resolve, step.do === 'request_resource' ? delayMs : 0));
        await fetch(`${base}/run/event`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ ev: 'step.done', thread: 'main', hseq: ++hseq, step }) });
      }
    })();
    return { stop: async () => { await loop.catch(() => {}); } };
  };
  const scenario = { ...SCENARIO, setup: [], steps: [{ do: 'request_resource', wait_ms: 400 }] };
  const common = { scenario, mode: 'quick', repeat: 1, upstreamSha: '59936bef', proxyPort: 0, controlPort: 0, originPorts: { 'lab-node': lab.port }, sleep: noSleep, endMs: 2000 };
  try {
    assert.equal((await executeRun({ ...common, startHarness: slow(150), stepMs: 60 })).ok, true);
    const withoutWait = { ...scenario, steps: [{ do: 'request_resource', wait_ms: 0 }] };
    const res = await executeRun({ ...common, scenario: withoutWait, startHarness: slow(150), stepMs: 60 });
    assert.match(res.error, /^STEP_TIMEOUT/);
  } finally {
    await lab.close();
  }
});
