// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// End-to-end run: start the test harness around a handle() module (or any server
// program), run the conformance runner against its URL, stop the server
// (CONTRACT.md §1, §5; DL-090).
//
// Usage:
//   node server/conformance/e2e/run.mjs (--module <path> | --command <program> [--arg <value>]...)
//        --suite rqp|legacy [--profile <name>] [--expect fail] [--start-timeout-ms <n>]
//        [-- <extra runner options>]
//
// --start-timeout-ms: how long to wait for the `listening <port>` line (default 30000;
// macOS checks a freshly linked binary on its first start, which can be slow).
//
// --command: start <program> with the --arg values as its arguments (no shell). The
// program must print `listening <port>` as its first stdout line and serve on
// 127.0.0.1:<port> (for example the reference server with --conformance --port 0).
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
  const opts = { module: '', command: '', args: [], suite: '', profile: '', expect: 'pass', startTimeoutMs: 30_000, extra: [] };
  for (let i = 0; i < argv.length; i += 1) {
    const a = argv[i];
    if (a === '--') {
      opts.extra = argv.slice(i + 1);
      break;
    }
    const v = argv[i + 1];
    if (v === undefined) throw new UsageError(`missing value for ${a}`);
    if (a === '--module') opts.module = resolve(v);
    else if (a === '--command') opts.command = resolve(v);
    else if (a === '--arg') opts.args.push(v);
    else if (a === '--suite') opts.suite = v;
    else if (a === '--profile') opts.profile = v;
    else if (a === '--expect') opts.expect = v;
    else if (a === '--start-timeout-ms') {
      const n = Number(v);
      if (!Number.isInteger(n) || n < 1 || n > 600_000) throw new UsageError('--start-timeout-ms must be 1..600000');
      opts.startTimeoutMs = n;
    }
    else throw new UsageError(`unknown option ${a}`);
    i += 1;
  }
  if (Boolean(opts.module) === Boolean(opts.command) || !['rqp', 'legacy'].includes(opts.suite) || !['pass', 'fail'].includes(opts.expect)) {
    throw new UsageError('need (--module <path> | --command <program> [--arg <value>]...) --suite rqp|legacy [--expect pass|fail]');
  }
  if (opts.module && opts.args.length > 0) throw new UsageError('--arg needs --command');
  return opts;
}

function startHarness(opts) {
  const [program, args] = opts.command
    ? [opts.command, opts.args]
    : [process.execPath, [harness, '--module', opts.module, '--suite', opts.suite, '--port', '0', '--conformance']];
  const child = spawn(program, args, { stdio: ['ignore', 'pipe', 'inherit'] });
  return new Promise((resolvePort, reject) => {
    const lines = createInterface({ input: child.stdout });
    const fail = (message) => {
      child.kill();
      reject(new HarnessError(message));
    };
    const timer = setTimeout(() => fail(`harness did not print "listening <port>" in ${opts.startTimeoutMs} ms`), opts.startTimeoutMs);
    child.once('exit', (code) => {
      clearTimeout(timer);
      reject(new HarnessError(`harness exited early with code ${code}`));
    });
    child.once('error', (e) => {
      clearTimeout(timer);
      reject(new HarnessError(`cannot start ${program}: ${e.code ?? e.message}`));
    });
    lines.once('line', (line) => {
      clearTimeout(timer);
      const m = /^listening (\d+)$/.exec(line);
      if (m) resolvePort({ child, port: Number(m[1]) });
      else fail(`unexpected first harness line: ${line}`);
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
