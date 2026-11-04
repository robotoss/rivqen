// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The runner: selects runs, resets the app, runs the conductor and stores the evidence.
import { mkdir, readdir, readFile, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { clearArgs, stopArgs } from './adb.mjs';
import { executeRun } from './conductor.mjs';
import { LabError } from './errors.mjs';
import { buildRunJson } from './runjson.mjs';

export const USAGE = `usage: node run.mjs (--scenario <CS-id> | --all) [--mode quick|standard] [--repeat <k>] [--out <dir>]
  [--package <name>] [--component <name>] [--start-command "<adb args with {package} {component} {run} {control}>"]
  [--serial <adb serial>] [--adb <path>] [--upstream-sha <sha>] [--emulator-image <text>]
  [--step-timeout <ms>] [--scenarios <file>] [--dry-run]`;

const VALUE_FLAGS = {
  '--scenario': 'scenario', '--mode': 'mode', '--repeat': 'repeat', '--out': 'out', '--package': 'package',
  '--component': 'component', '--start-command': 'startCommand', '--serial': 'serial', '--adb': 'adb',
  '--upstream-sha': 'upstreamSha', '--emulator-image': 'emulatorImage', '--step-timeout': 'stepTimeout', '--scenarios': 'scenarios',
};

/** Parse the command line. Throws LabError USAGE. */
export function parseArgs(argv) {
  const opts = { all: false, dryRun: false };
  for (let i = 0; i < argv.length; i++) {
    const flag = argv[i];
    if (flag === '--all') opts.all = true;
    else if (flag === '--dry-run') opts.dryRun = true;
    else if (Object.hasOwn(VALUE_FLAGS, flag)) {
      const value = argv[++i];
      if (value === undefined || value.startsWith('--')) throw new LabError('USAGE', `${flag} needs a value`);
      opts[VALUE_FLAGS[flag]] = value;
    } else throw new LabError('USAGE', `unknown argument ${flag}`);
  }
  if (opts.all === (opts.scenario !== undefined)) throw new LabError('USAGE', 'give exactly one of --scenario and --all');
  if (opts.mode !== undefined && !['quick', 'standard'].includes(opts.mode)) throw new LabError('USAGE', '--mode must be quick or standard');
  for (const key of ['repeat', 'stepTimeout']) {
    if (opts[key] === undefined) continue;
    if (!/^\d+$/.test(opts[key]) || Number(opts[key]) < 1) throw new LabError('USAGE', `--${key === 'repeat' ? 'repeat' : 'step-timeout'} must be a positive integer`);
    opts[key] = Number(opts[key]);
  }
  if (opts.repeat > 100) throw new LabError('USAGE', '--repeat must be at most 100');
  return opts;
}

/**
 * The runs to do: one per scenario, mode and repeat (1-based).
 * @returns {{ scenario: object, mode: string, k: number }[]}
 */
export function selectRuns(scenarios, { scenario, mode, repeat, all }) {
  const chosen = all ? scenarios : scenarios.filter((s) => s.id === scenario);
  if (chosen.length === 0) throw new LabError('USAGE', `no scenario ${scenario}`);
  const out = [];
  for (const s of chosen) {
    if (mode !== undefined && !s.modes.includes(mode)) throw new LabError('USAGE', `${s.id} does not run in mode ${mode}`);
    for (const m of mode ? [mode] : s.modes) {
      for (let k = 1; k <= (repeat ?? s.repeat); k++) out.push({ scenario: s, mode: m, k });
    }
  }
  return out;
}

/** Store the event log and the .http files of one run (CONTRACT sections 5 and 8). */
export async function storeRun(outDir, scenarioId, mode, k, result) {
  const dir = path.join(outDir, 'android', scenarioId, mode);
  await mkdir(dir, { recursive: true });
  for (const name of await readdir(dir)) {
    if (name === `run-${k}.jsonl` || name.startsWith(`run-${k}-x`)) await rm(path.join(dir, name));
  }
  await writeFile(path.join(dir, `run-${k}.jsonl`), result.log.toJsonl());
  for (const [n, bytes] of result.exchanges) await writeFile(path.join(dir, `run-${k}-x${n}.http`), bytes);
}

/**
 * Do the runs one after the other and write run.json.
 * @param {{
 *   runs: { scenario: object, mode: string, k: number }[], outDir: string,
 *   cfg: { package?: string, component?: string, serial?: string, startCommand?: string },
 *   deps: { adb: (args: string[]) => Promise<void>, startHarness: Function, adbVersion?: () => Promise<string> },
 *   upstreamSha: string, emulatorImage?: string, runOptions?: object, now?: () => Date, report?: (line: string) => void
 * }} input
 * @returns {Promise<{ scenario: string, mode: string, k: number, ok: boolean, error: string | null }[]>}
 */
export async function runAll({ runs, outDir, cfg, deps, upstreamSha, emulatorImage, runOptions = {}, now = () => new Date(), report = () => {} }) {
  const results = [];
  for (const { scenario, mode, k } of runs) {
    await deps.adb(clearArgs(cfg));
    const result = await executeRun({ ...runOptions, scenario, mode, repeat: k, upstreamSha, startHarness: deps.startHarness });
    await storeRun(outDir, scenario.id, mode, k, result);
    await deps.adb(stopArgs(cfg));
    results.push({ scenario: scenario.id, mode, k, ok: result.ok, error: result.error });
    report(`${scenario.id} ${mode} run ${k}: ${result.ok ? 'ok' : `FAILED ${result.error}`}`);
  }
  let previous = null;
  try {
    previous = JSON.parse(await readFile(path.join(outDir, 'run.json'), 'utf8'));
  } catch (cause) {
    if (cause.code !== 'ENOENT') throw new LabError('RUN_JSON', 'existing run.json is not valid JSON', { cause });
  }
  const runJson = await buildRunJson({
    root: outDir,
    previous,
    meta: {
      date: now().toISOString().slice(0, 10),
      tools: { node: process.version, adb: (await deps.adbVersion?.()) ?? 'unknown', conductor: '1' },
      upstreamSha,
      emulatorImage,
    },
  });
  await writeFile(path.join(outDir, 'run.json'), `${JSON.stringify(runJson, null, 2)}\n`);
  return results;
}
