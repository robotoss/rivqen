#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Runs scenarios of the client lab (README.md). Needs the lab servers (tools/upstream-lab/start.sh),
// an emulator with the harness installed, and adb.
import { execFile, spawn } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { clearArgs, startArgs, stopArgs } from './src/adb.mjs';
import { LabError } from './src/errors.mjs';
import { parseArgs, refreshRunJson, runAll, selectRuns, USAGE } from './src/runner.mjs';
import { catalogProblems } from './src/steps.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const out = (text) => process.stdout.write(`${text}\n`);
const exec = promisify(execFile);

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.runJsonOnly) {
    const dir = path.resolve(opts.out ?? path.join(here, '..', '..', '..', 'evidence', 'traces', 'client'));
    await refreshRunJson({ outDir: dir, upstreamSha: opts.upstreamSha ?? '59936bef', emulatorImage: opts.emulatorImage });
    out(`run.json rebuilt in ${dir}`);
    return 0;
  }
  const scenariosPath = opts.scenarios ?? path.join(here, 'scenarios.json');
  const scenarios = JSON.parse(await readFile(scenariosPath, 'utf8'));
  const problems = catalogProblems(scenarios);
  if (problems.length > 0) throw new LabError('SCENARIOS', problems.join('; '));
  const runs = selectRuns(scenarios, opts);
  const adb = opts.adb ?? 'adb';
  const cfg = { package: opts.package, component: opts.component, serial: opts.serial, startCommand: opts.startCommand };
  const upstreamSha = opts.upstreamSha ?? '59936bef';
  const outDir = path.resolve(opts.out ?? path.join(here, '..', '..', '..', 'evidence', 'traces', 'client'));

  if (opts.dryRun) {
    out(`${runs.length} runs into ${outDir}`);
    out(`adb ${clearArgs(cfg).join(' ')}`);
    out(`adb ${startArgs(cfg, `${runs[0].scenario.id}-${runs[0].mode}-r1`).join(' ')}`);
    out(`adb ${stopArgs(cfg).join(' ')}`);
    return 0;
  }

  const deps = {
    adb: async (args) => {
      try {
        await exec(adb, args, { timeout: 60_000 });
      } catch (cause) {
        throw new LabError('ADB', `adb ${args[0] === '-s' ? args[2] : args[0]} failed`, { cause });
      }
    },
    adbVersion: async () => {
      try {
        return (await exec(adb, ['version'], { timeout: 10_000 })).stdout.split('\n')[0];
      } catch {
        return 'unknown';
      }
    },
    startHarness: async ({ runId, onFatal }) => {
      const child = spawn(adb, startArgs(cfg, runId), { stdio: 'ignore' });
      child.on('error', (cause) => onFatal(new LabError('ADB', 'cannot start the harness', { cause })));
      child.on('exit', (code) => {
        if (code !== 0 && code !== null) onFatal(new LabError('ADB', `the harness start command exited with ${code}`));
      });
      return { stop: async () => { child.kill(); } };
    },
  };
  const results = await runAll({
    runs, outDir, cfg, deps, upstreamSha, emulatorImage: opts.emulatorImage,
    runOptions: opts.stepTimeout ? { stepMs: opts.stepTimeout } : {},
    report: out,
  });
  return results.every((r) => r.ok) ? 0 : 1;
}

main().then(
  (code) => { process.exitCode = code; },
  (e) => {
    process.stderr.write(`${e instanceof LabError ? `${e.code}: ${e.message}` : e.message}\n`);
    if (e instanceof LabError && e.code === 'USAGE') process.stderr.write(`${USAGE}\n`);
    process.exitCode = 2;
  },
);
