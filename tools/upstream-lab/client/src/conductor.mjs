// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The conductor: executes one run (one scenario, one mode, one repeat) with the recording proxy,
// the control channel and the scripted steps (CONTRACT sections 2 to 5).
import { EventLog } from './eventlog.mjs';
import { HarnessChannel } from './channel.mjs';
import { Control } from './control.mjs';
import { Proxy } from './proxy.mjs';
import { formatExchange } from './httpfile.mjs';
import { LabError } from './errors.mjs';
import { makeRunHandle } from './runhandle.mjs';
import { postCase, PAGES_DIR } from './lab.mjs';
import { MACRO_VERSION, ORIGINS, expandSetup, isHarnessStep } from './steps.mjs';

export const DEFAULTS = {
  proxyPort: 18090,
  controlPort: 18091,
  controlUrl: 'http://10.0.2.2:18091',
  stepMs: 60_000,
  waitMs: 30_000,
  endMs: 5_000,
  pollMs: 30_000,
};

const sleepMs = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const describe = (e) => (e instanceof LabError ? `${e.code}: ${e.message}` : `INTERNAL: ${e?.message}`);

/**
 * Run one scenario in one mode once.
 * @param {{
 *   scenario: object, mode: 'quick'|'standard', repeat: number, upstreamSha: string,
 *   startHarness: (ctx: { runId: string, controlUrl: string, controlPort: number, proxyPort: number, onFatal: (e: Error) => void }) => Promise<{ stop: () => Promise<void> }>,
 *   originPorts?: Record<string, number>, pagesDir?: string, fetchImpl?: typeof fetch,
 *   clock?: () => number, sleep?: (ms: number) => Promise<void>,
 *   proxyPort?: number, controlPort?: number, controlUrl?: string,
 *   stepMs?: number, waitMs?: number, endMs?: number, pollMs?: number
 * }} options
 * @returns {Promise<{ ok: boolean, error: string | null, log: EventLog, exchanges: Map<number, Buffer> }>}
 */
export async function executeRun(options) {
  const o = { ...DEFAULTS, originPorts: ORIGINS, pagesDir: PAGES_DIR, fetchImpl: fetch, sleep: sleepMs, ...options };
  const { scenario, mode } = o;
  const runId = `${scenario.id}-${mode}-r${o.repeat}`;
  const log = new EventLog({ clock: o.clock });
  const exchanges = new Map();
  const channel = new HarnessChannel();
  const proxy = new Proxy({
    upstreamPort: o.originPorts[scenario.origin],
    listenPort: o.proxyPort,
    emit: (ev, members) => log.emit('proxy', ev, members),
    onExchange: (rec) => exchanges.set(rec.exchange, formatExchange(rec)),
  });
  const run = makeRunHandle({ id: runId, scenario: () => ({ run: runId, scenario, mode }), log, channel });
  const control = new Control({ port: o.controlPort, run, pollMs: o.pollMs });

  let epoch = 0; // seq of the latest create_session; `wait` looks at events after it
  const post = (c) => postCase({ origin: scenario.origin, caseId: runId, scenarioCase: c, pagesDir: o.pagesDir, fetchImpl: o.fetchImpl, ports: o.originPorts });
  async function conductorStep(step) {
    switch (step.do) {
      case 'hold':
      case 'cut':
      case 'drop_field':
      case 'set_field':
      case 'set_body_member':
        return proxy.arm(step);
      case 'case':
        return post(step.value);
      case 'release':
        return proxy.release();
      case 'server_down':
        return proxy.stop();
      case 'server_up':
        return proxy.start();
      case 'wait':
        await log.waitFor(step.for, epoch, step.timeout_ms ?? o.waitMs);
        return undefined;
      default:
        return o.sleep(step.ms);
    }
  }
  async function runStep(step, seq) {
    const s = { ...step, seq };
    const started = log.emit('conductor', 'step.start', { step: s });
    if (s.do === 'create_session') epoch = started.seq;
    try {
      if (isHarnessStep(s)) {
        await channel.dispatch(s, o.stepMs + (s.do === 'request_resource' ? (s.wait_ms ?? 1000) : 0)); // the harness logs its own step.done
      } else {
        await conductorStep(s);
        log.emit('conductor', 'step.done', { step: s });
      }
    } catch (e) {
      const harnessReported = isHarnessStep(s) && e.code === 'STEP_FAILED';
      if (!harnessReported) log.emit('conductor', 'step.done', { step: s, error: e.message });
      throw e;
    }
  }

  let ok = true;
  let error = null;
  let harness = null;
  try {
    await proxy.start();
    await control.start();
    log.emit('conductor', 'run.start', { scenario: scenario.id, mode, repeat: o.repeat, origin: scenario.origin, upstream_sha: o.upstreamSha, macro: MACRO_VERSION });
    harness = await o.startHarness({ runId, controlUrl: o.controlUrl, controlPort: control.port, proxyPort: proxy.port, onFatal: (e) => channel.fail(e) });
    let seq = 0;
    for (const item of expandSetup(scenario.setup)) {
      if ('case' in item) await post(item.case);
      else await runStep(item.step, ++seq);
    }
    await post(scenario.case);
    for (const step of scenario.steps) await runStep(step, ++seq);
  } catch (e) {
    ok = false;
    error = describe(e);
  }
  if (harness) {
    const fetched = await channel.finish(o.endMs);
    if (ok && !fetched) {
      ok = false;
      error = 'HARNESS_LOST: the harness did not fetch the end of the run';
    }
  }
  log.emit('conductor', 'run.end', { ok, error });
  await harness?.stop();
  await control.stop();
  await proxy.stop();
  return { ok, error, log, exchanges };
}
