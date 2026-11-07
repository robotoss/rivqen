// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// run.json (CONTRACT section 8): rebuilt from the stored runs, so that it always matches the tree.
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { PLATFORMS, listDir, readOptionalJson } from './check.mjs';
import { firstDifference, normalizeRun, outcomeRun } from './determinism.mjs';

const MODES = ['quick', 'standard'];

/**
 * @param {{ root: string, previous?: object | null,
 *   meta: { date: string, tools: Record<string, string>, upstreamSha: string, emulatorImage?: string } }} input
 */
export async function buildRunJson({ root, previous = null, meta }) {
  const scenarios = [];
  const stable = [];
  const unstable = [];
  const notChecked = [];
  const oldReasons = new Map((previous?.summary?.unstable ?? []).map((u) => [`${u.scenario}/${u.mode}`, u.reason]));
  const dirs = (await listDir(path.join(root, 'android'))).filter((d) => d.isDirectory() && d.name.startsWith('CS-'));
  for (const sd of dirs.sort((a, b) => a.name.localeCompare(b.name))) {
    const modes = {};
    for (const mode of MODES) {
      const names = (await listDir(path.join(root, 'android', sd.name, mode))).map((d) => d.name).filter((n) => /^run-\d+\.jsonl$/.test(n)).sort();
      if (names.length === 0) continue;
      const runs = [];
      const outcomes = [];
      for (const n of names) {
        const text = await readFile(path.join(root, 'android', sd.name, mode, n), 'utf8');
        const events = text.split('\n').filter((l) => l !== '').map((l) => JSON.parse(l));
        runs.push(normalizeRun(events));
        outcomes.push(outcomeRun(events));
      }
      modes[mode] = { runs: names.length };
      const diff = firstDifference(runs);
      const key = `${sd.name}/${mode}`;
      if (names.length < 2) notChecked.push({ scenario: sd.name, mode }); // CONTRACT section 6
      else if (!diff) stable.push({ scenario: sd.name, mode });
      else {
        const reason = oldReasons.get(key) ?? `runs differ: run ${diff.run + 1} at line ${diff.index + 1}`;
        // outcome: the projection of determinism.mjs (what the client did), see outcomeRun
        unstable.push({ scenario: sd.name, mode, reason, outcome: firstDifference(outcomes) ? 'unstable' : 'stable' });
      }
    }
    scenarios.push({ id: sd.name, modes });
  }
  const functions = {};
  for (const platform of PLATFORMS) {
    const fromRunner = await readOptionalJson(path.join(root, platform, 'functions.jsonl.meta.json'));
    if (fromRunner !== undefined) functions[platform] = fromRunner;
  }
  return {
    date: meta.date,
    tools: meta.tools,
    emulator_image: meta.emulatorImage ?? previous?.emulator_image ?? 'unknown',
    upstream_sha: meta.upstreamSha,
    scenarios,
    ...(Object.keys(functions).length > 0 ? { functions } : {}),
    summary: { stable, unstable, not_checked: notChecked },
  };
}
