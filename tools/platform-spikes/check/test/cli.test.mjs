// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { cp, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { promisify } from 'node:util';
import { catalogProblems } from '../src/catalog.mjs';
import { runCheck } from '../src/check.mjs';
import { FIXTURE, loadCatalog } from './helpers.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const CLI = path.join(here, '..', 'check.mjs');
const REPO = path.resolve(here, '..', '..', '..', '..');
const run = promisify(execFile);

/** Runs the CLI and returns its exit code and output. */
async function cli(args) {
  try {
    const { stdout } = await run(process.execPath, [CLI, ...args]);
    return { code: 0, stdout, stderr: '' };
  } catch (e) {
    return { code: e.code, stdout: e.stdout, stderr: e.stderr };
  }
}

test('current catalog: the repository catalog and its linked evidence pass', async () => {
  const catalog = JSON.parse(await readFile(path.join(REPO, 'tools/platform-spikes/capabilities.json'), 'utf8'));
  assert.deepEqual(catalogProblems(catalog), []);
  const { errors, counts } = await runCheck({ root: REPO, catalog });
  assert.deepEqual(errors, []);
  assert.ok(counts.results >= 0);
});

test('cli: the valid fixture passes with exit code 0 and one summary line', async () => {
  const result = await cli(['--root', FIXTURE, '--catalog', path.join(FIXTURE, 'capabilities.json')]);
  assert.equal(result.code, 0);
  assert.match(result.stdout, /^ok: 1 capabilities, 1 results, 3 run logs, 1 recordings\n$/);
});

test('cli: an unknown option gives exit code 2', async () => {
  const result = await cli(['--bogus']);
  assert.equal(result.code, 2);
  assert.match(result.stderr, /usage:/);
});

test('cli: a missing option value gives exit code 2', async () => {
  const result = await cli(['--root']);
  assert.equal(result.code, 2);
});

test('cli: an unreadable catalog gives exit code 2 and names the error code only', async () => {
  const result = await cli(['--catalog', path.join(FIXTURE, 'missing.json')]);
  assert.equal(result.code, 2);
  assert.equal(result.stderr, 'check failed: ENOENT\n');
});

test('cli: a catalog that is not JSON gives exit code 2 and the parse message', async (t) => {
  const dir = await tempDir(t);
  await writeFile(path.join(dir, 'bad.json'), '{oops');
  const result = await cli(['--root', dir, '--catalog', path.join(dir, 'bad.json')]);
  assert.equal(result.code, 2);
  assert.match(result.stderr, /^check failed: .*JSON/);
  assert.doesNotMatch(result.stderr, /undefined/);
});

for (const [name, args] of [
  ['an option value that starts with --', ['--root', '--catalog']],
  ['a repeated --root', ['--root', FIXTURE, '--root', FIXTURE]],
  ['a repeated --catalog', ['--catalog', 'a.json', '--catalog', 'b.json']],
  ['a flag with a value, unknown', ['--bogus', 'x']],
  ['a positional argument', ['x', 'y']],
  ['a missing value after a valid pair', ['--root', FIXTURE, '--catalog']],
]) {
  test(`cli: ${name} gives exit code 2 and the usage line`, async () => {
    const result = await cli(args);
    assert.equal(result.code, 2);
    assert.equal(result.stderr, 'usage: node check.mjs [--root <dir>] [--catalog <path>]\n');
    assert.equal(result.stdout, '');
  });
}

/** A temporary tree with `tools/platform-spikes/capabilities.json` holding an empty catalog. */
async function tempDir(t) {
  const dir = await mkdtemp(path.join(os.tmpdir(), 'spikes-cli-'));
  t.after(() => rm(dir, { recursive: true, force: true }));
  return dir;
}

test('cli: without options the catalog is tools/platform-spikes/capabilities.json under the root', async (t) => {
  const dir = await tempDir(t);
  await mkdir(path.join(dir, 'tools', 'platform-spikes'), { recursive: true });
  await writeFile(path.join(dir, 'tools', 'platform-spikes', 'capabilities.json'), '[]');
  const result = await cli(['--root', dir]);
  assert.equal(result.code, 0);
  assert.equal(result.stdout, 'ok: 0 capabilities, 0 results, 0 run logs, 0 recordings\n');
});

test('cli: without --root the root is the repository that holds the tool', async (t) => {
  const dir = await tempDir(t);
  await writeFile(path.join(dir, 'empty.json'), '[]');
  const result = await cli(['--catalog', path.join(dir, 'empty.json')]);
  assert.equal(result.code, 0);
  const real = await cli([]);
  assert.equal(real.code, 0);
  assert.match(real.stdout, /^ok: \d+ capabilities, \d+ results, \d+ run logs, \d+ recordings\n$/);
});

test('cli: findings give exit code 1 and one ERROR line each', async () => {
  const catalog = await loadCatalog(FIXTURE);
  catalog[0].verdict = 'supported-ish';
  const dir = await mkdtemp(path.join(os.tmpdir(), 'spikes-cli-'));
  try {
    await cp(FIXTURE, dir, { recursive: true });
    await writeFile(path.join(dir, 'capabilities.json'), JSON.stringify(catalog));
    const result = await cli(['--root', dir, '--catalog', path.join(dir, 'capabilities.json')]);
    assert.equal(result.code, 1);
    assert.equal(result.stdout.trim().split('\n').length, 1);
    assert.match(result.stdout, /^ERROR CAP-F-01: verdict "supported-ish" is not allowed/);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});
