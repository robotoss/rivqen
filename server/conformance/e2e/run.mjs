// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// End-to-end run: start the test harness around a handle() module, run the
// conformance runner against its URL, stop the harness (CONTRACT.md §1, §5).
//
// Usage:
//   node server/conformance/e2e/run.mjs --module <path> --suite rqp|legacy
//        [--profile <name>] [--expect fail] [-- <extra runner options>]
//
// --expect fail: exit 0 only when the runner reports a MUST failure (exit 1);
// used for the servers that fail on purpose.

import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const harness = resolve(here, '../harness/harness.mjs');
const runner = resolve(here, '../bin/rivqen-conformance.mjs');

class UsageError extends Error {
  constructor(message) {
    super(message);
    this.code = 'E2E_USAGE';
  }
}

class HarnessError extends Error {
  constructor(message) {
    super(message);
    this.code = 'E2E_HARNESS';
  }
}

function parse(argv) {
  const opts = { module: '', suite: '', profile: '', expect: 'pass', extra: [] };
  for (let i = 0; i < argv.length; i += 1) {
    const a = argv[i];
    if (a === '--') {
      opts.extra = argv.slice(i + 1);
      break;
    }
    const v = argv[i + 1];
    if (v === undefined) throw new UsageError(`missing value for ${a}`);
    if (a === '--module') opts.module = resolve(v);
    else if (a === '--suite') opts.suite = v;
    else if (a === '--profile') opts.profile = v;
    else if (a === '--expect') opts.expect = v;
    else throw new UsageError(`unknown option ${a}`);
    i += 1;
  }
  if (!opts.module || !['rqp', 'legacy'].includes(opts.suite) || !['pass', 'fail'].includes(opts.expect)) {
    throw new UsageError('need --module <path> --suite rqp|legacy [--expect pass|fail]');
  }
  return opts;
}

function startHarness(opts) {
  const child = spawn(process.execPath, [harness, '--module', opts.module, '--suite', opts.suite, '--port', '0', '--conformance'], {
    stdio: ['ignore', 'pipe', 'inherit'],
  });
  return new Promise((resolvePort, reject) => {
    const lines = createInterface({ input: child.stdout });
    const timer = setTimeout(() => reject(new HarnessError('harness did not print "listening <port>" in 10 s')), 10_000);
    child.once('exit', (code) => {
      clearTimeout(timer);
      reject(new HarnessError(`harness exited early with code ${code}`));
    });
    lines.once('line', (line) => {
      clearTimeout(timer);
      const m = /^listening (\d+)$/.exec(line);
      if (m) resolvePort({ child, port: Number(m[1]) });
      else reject(new HarnessError(`unexpected first harness line: ${line}`));
    });
  });
}

function runRunner(opts, port) {
  const args = [runner, '--target', `http://127.0.0.1:${port}`, '--suite', opts.suite];
  if (opts.profile) args.push('--profile', opts.profile);
  args.push(...opts.extra);
  const child = spawn(process.execPath, args, { stdio: 'inherit' });
  return new Promise((done) => child.once('exit', (code) => done(code ?? 2)));
}

async function main() {
  let opts;
  try {
    opts = parse(process.argv.slice(2));
  } catch (e) {
    process.stderr.write(`${e.message}\n`);
    return 2;
  }
  const { child, port } = await startHarness(opts);
  try {
    const code = await runRunner(opts, port);
    if (opts.expect === 'fail') {
      if (code === 1) return 0;
      process.stderr.write(`expected the runner to report failures (exit 1), got exit ${code}\n`);
      return 1;
    }
    return code;
  } finally {
    child.kill();
  }
}

main().then(
  (code) => process.exit(code),
  (e) => {
    process.stderr.write(`e2e: ${e.message}\n`);
    process.exit(2);
  },
);
