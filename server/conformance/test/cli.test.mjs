// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Command line, exit codes and the report (CONTRACT.md sections 1 and 4).

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, test } from 'node:test';
import { escapeText } from '../src/escape.mjs';
import { capProblems, formatLine, toJson } from '../src/report.mjs';
import { parseArgs, parseTarget, splitTarget } from '../src/runner.mjs';
import { LEGACY_DIR, legacyResponder, runMain, startFake } from './helpers.mjs';

const BIN = fileURLToPath(new URL('../bin/rivqen-conformance.mjs', import.meta.url));

let fake = null;
const temps = [];
afterEach(async () => {
  if (fake !== null) await fake.close();
  fake = null;
  for (const t of temps.splice(0)) rmSync(t, { recursive: true, force: true });
});

async function closedPort() {
  const s = await startFake({ respond: () => ({ hang: true }) });
  await s.close();
  return s.port;
}

describe('exit code 2', () => {
  test('unreachable_target_exits_2', async () => {
    const port = await closedPort();
    const r = await runMain(['--target', `http://127.0.0.1:${port}`, '--suite', 'legacy', '--fixtures', LEGACY_DIR]);
    assert.equal(r.code, 2);
    assert.equal(r.out, '');
    assert.match(r.err, /^rivqen-conformance: the target cannot be reached: cannot connect to 127\.0\.0\.1:\d+ \(ECONNREFUSED\)\n$/);
    // The same through the executable.
    const p = spawnSync(process.execPath, [BIN, '--target', `http://127.0.0.1:${port}`, '--suite', 'rqp'], { encoding: 'utf8' });
    assert.equal(p.status, 2);
    assert.match(p.stderr, /the target cannot be reached/);
  });

  test('unsupported_profile_exits_2', async () => {
    fake = await startFake({ info: { contract: 1, suites: ['legacy'], profiles: ['legacy-canonical'] }, respond: legacyResponder() });
    const base = ['--target', fake.target, '--fixtures', LEGACY_DIR];
    const java = await runMain([...base, '--suite', 'legacy', '--profile', 'legacy-java']);
    assert.equal(java.code, 2);
    assert.equal(java.err, 'rivqen-conformance: the server does not support the profile legacy-java\n');
    assert.equal(fake.state.cases.length, 0);
    const rqp = await runMain(['--target', fake.target, '--suite', 'rqp', '--only', 'FX-RQ-P01']);
    assert.equal(rqp.code, 2);
    assert.equal(rqp.err, 'rivqen-conformance: the server does not support the suite rqp\n');
    const unknown = await runMain([...base, '--suite', 'legacy', '--profile', 'legacy-go']);
    assert.equal(unknown.code, 2);
    assert.match(unknown.err, /--profile must be one of legacy-canonical, legacy-java, legacy-node, legacy-php/);
    const canonical = await runMain([...base, '--suite', 'legacy']);
    assert.equal(canonical.code, 0);
  });

  test('a failing or invalid info answer exits 2', async () => {
    const answers = [
      [null, `GET /__rivqen/conformance/info: status 404, expected 200`],
      [{ contract: 2, suites: ['legacy'], profiles: [] }, 'not an answer of contract 1'],
      [{ contract: 1, suites: 'legacy' }, 'not an answer of contract 1'],
      [{ contract: 1, suites: ['legacy'], profiles: [1] }, 'not an answer of contract 1'],
      [{ contract: 1, suites: ['legacy'] }, 'does not support the profile legacy-canonical'],
    ];
    for (const [info, message] of answers) {
      fake = await startFake({ info, respond: legacyResponder() });
      const r = await runMain(['--target', fake.target, '--suite', 'legacy', '--fixtures', LEGACY_DIR]);
      assert.equal(r.code, 2, JSON.stringify(info));
      assert.ok(r.err.includes(message), r.err);
      await fake.close();
      fake = null;
    }
  });

  test('an info answer that is not JSON or does not come exits 2', async () => {
    const { createServer } = await import('node:net');
    const serve = async (bytes) => {
      const server = createServer((s) => s.once('data', () => (bytes === null ? undefined : s.end(Buffer.from(bytes)))));
      await new Promise((r) => server.listen(0, '127.0.0.1', r));
      return server;
    };
    const cases = [
      ['HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\n{x}', 'GET /__rivqen/conformance/info: the body is not valid JSON'],
      ['HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nnull', 'not an answer of contract 1'],
      ['garbage\r\n\r\n', 'GET /__rivqen/conformance/info failed: malformed response'],
      [null, 'GET /__rivqen/conformance/info failed: no complete response within 100 ms'],
    ];
    for (const [bytes, message] of cases) {
      const server = await serve(bytes);
      const r = await runMain(['--target', `http://127.0.0.1:${server.address().port}`, '--suite', 'legacy', '--fixtures', LEGACY_DIR, '--timeout-ms', '100']);
      server.close();
      server.unref();
      assert.equal(r.code, 2);
      assert.ok(r.err.includes(message), r.err);
    }
  });

  test('usage errors exit 2 and print the usage', async () => {
    const t = ['--target', 'http://127.0.0.1:1'];
    const bad = [
      [[], '--target is required'],
      [['--suite', 'rqp'], '--target is required'],
      [[...t], '--suite must be rqp or legacy'],
      [[...t, '--suite', 'other'], '--suite must be rqp or legacy'],
      [[...t, '--suite', 'rqp', '--profile', 'legacy-canonical'], '--profile is for the legacy suite only'],
      [[...t, '--suite'], '--suite needs a value'],
      [[...t, '--suite', 'rqp', '--suite', 'rqp'], '--suite is given twice'],
      [[...t, '--suite', 'rqp', '--verbose', 'x'], 'unknown option --verbose'],
      [[...t, '--suite', 'rqp', '--timeout-ms', '0'], '--timeout-ms must be an integer from 1 to 600000'],
      [[...t, '--suite', 'rqp', '--timeout-ms', '600001'], '--timeout-ms must be an integer'],
      [[...t, '--suite', 'rqp', '--timeout-ms', '1.5'], '--timeout-ms must be an integer'],
      [['--target', 'https://127.0.0.1:1', '--suite', 'rqp'], '--target must be an http:// origin'],
      [['--target', 'http://127.0.0.1:1/x', '--suite', 'rqp'], 'without a path'],
      [['--target', 'http://127.0.0.1:1?', '--suite', 'rqp'], 'without a path'],
      [['--target', 'http://127.0.0.1:1/#', '--suite', 'rqp'], 'without a path'],
      [['--target', 'http://u:p@127.0.0.1:1', '--suite', 'rqp'], 'must not have user information'],
      [['--target', 'not a url', '--suite', 'rqp'], 'is not a URL'],
    ];
    for (const [args, message] of bad) {
      const r = await runMain(args);
      assert.equal(r.code, 2, args.join(' '));
      assert.ok(r.err.includes(message), `${args.join(' ')}: ${r.err}`);
      assert.ok(r.err.endsWith('usage: rivqen-conformance --target <http://host:port> --suite rqp|legacy [--profile <name>] [--fixtures <dir>] [--only <prefix>] [--json <file>] [--timeout-ms <n>]\n'));
    }
  });

  test('a missing fixture directory or an empty selection exits 2', async () => {
    fake = await startFake({ respond: legacyResponder() });
    const missing = await runMain(['--target', fake.target, '--suite', 'legacy', '--fixtures', join(LEGACY_DIR, 'nope')]);
    assert.equal(missing.code, 2);
    assert.match(missing.err, /cannot read the fixture directory .*nope \(ENOENT\)/);
    const none = await runMain(['--target', fake.target, '--suite', 'legacy', '--fixtures', LEGACY_DIR, '--only', 'FX-LEG-NONE']);
    assert.equal(none.code, 2);
    assert.equal(none.err, 'rivqen-conformance: no case is selected\n');
  });
});

describe('options', () => {
  test('defaults: legacy-canonical, the suite directory, 10 s', () => {
    const legacy = parseArgs(['--target', 'http://127.0.0.1:8080', '--suite', 'legacy']);
    assert.equal(legacy.profile, 'legacy-canonical');
    assert.equal(legacy.timeoutMs, 10000);
    assert.equal(legacy.only, '');
    assert.equal(legacy.json, null);
    assert.ok(legacy.fixtures.endsWith('/fixtures/legacy/'));
    const rqp = parseArgs(['--target', 'http://127.0.0.1:8080/', '--suite', 'rqp', '--timeout-ms', '600000']);
    assert.equal(rqp.profile, null);
    assert.equal(rqp.timeoutMs, 600000);
    assert.ok(rqp.fixtures.endsWith('/fixtures/rqp/protocol/'));
  });

  test('parseTarget gives the connect host, the port and the Host value', () => {
    assert.deepEqual(parseTarget('http://127.0.0.1:8080'), { origin: 'http://127.0.0.1:8080', host: '127.0.0.1', port: 8080, authority: '127.0.0.1:8080' });
    assert.deepEqual(parseTarget('http://[::1]:9/'), { origin: 'http://[::1]:9', host: '::1', port: 9, authority: '[::1]:9' });
    assert.deepEqual(parseTarget('http://localhost'), { origin: 'http://localhost', host: 'localhost', port: 80, authority: 'localhost' });
  });

  test('splitTarget keeps the bytes of the authority, the path and the query', () => {
    assert.deepEqual(splitTarget('https://shop.example.com/catalog'), { authority: 'shop.example.com', path: '/catalog' });
    assert.deepEqual(splitTarget('https://shop.example.com/a/../b?q=%41&x'), { authority: 'shop.example.com', path: '/a/../b?q=%41&x' });
    assert.deepEqual(splitTarget('https://Shop.Example.com:8443'), { authority: 'Shop.Example.com:8443', path: '/' });
    assert.deepEqual(splitTarget('https://shop.example.com?q=1'), { authority: 'shop.example.com', path: '/?q=1' });
    assert.equal(splitTarget('https://shop.example.com/a#frag'), null);
    assert.equal(splitTarget('https://user@shop.example.com/'), null);
    assert.equal(splitTarget('/catalog'), null);
    assert.equal(splitTarget('https:///catalog'), null);
  });
});

describe('report', () => {
  test('report_json_matches_text', async () => {
    const dir = `${mkdtempSync(join(tmpdir(), 'rivqen-report-'))}/`;
    temps.push(dir);
    const change = (resp, kase) => (kase.case_id === 'FX-LEG-T-DATA' ? { ...resp, status: 500 } : resp);
    fake = await startFake({ respond: legacyResponder({ change }) });
    const r = await runMain(['--target', fake.target, '--suite', 'legacy', '--fixtures', LEGACY_DIR, '--json', `${dir}report.json`]);
    assert.equal(r.code, 1);
    const report = JSON.parse(readFileSync(`${dir}report.json`, 'utf8'));
    assert.deepEqual(Object.keys(report), ['target', 'suite', 'profile', 'results', 'summary']);
    assert.equal(report.target, fake.target);
    assert.equal(report.suite, 'legacy');
    assert.equal(report.profile, 'legacy-canonical');
    assert.deepEqual(report.summary, { passed: 2, failed: 1, should_failed: 0, skipped: 1 });
    const s = report.summary;
    assert.deepEqual(r.lines, [...report.results.map(formatLine), `${s.passed} passed, ${s.failed} failed, ${s.should_failed} SHOULD failed, ${s.skipped} skipped`]);
    assert.deepEqual(report.results[1], { id: 'FX-LEG-T-DATA', level: 'must', status: 'fail', problems: ['status 500, expected 200'] });
    assert.deepEqual(report.results[3], { id: 'FX-LEG-T-PENDING', level: 'must', status: 'skip', problems: ['no expected.json'] });
    for (const x of report.results) assert.deepEqual(Object.keys(x), ['id', 'level', 'status', 'problems']);
  });

  test('the JSON report of the rqp suite has profile null', async () => {
    const dir = `${mkdtempSync(join(tmpdir(), 'rivqen-report-'))}/`;
    temps.push(dir);
    fake = await startFake({ respond: () => ({ status: 500, headers: [], body: null }) });
    const r = await runMain(['--target', fake.target, '--suite', 'rqp', '--only', 'FX-RQ-P01-FIRST-VISIT', '--json', `${dir}r.json`]);
    assert.equal(r.code, 1);
    const report = JSON.parse(readFileSync(`${dir}r.json`, 'utf8'));
    assert.equal(report.profile, null);
    assert.equal(report.suite, 'rqp');
  });

  test('a report file that cannot be written exits 2', async () => {
    fake = await startFake({ respond: legacyResponder() });
    const r = await runMain(['--target', fake.target, '--suite', 'legacy', '--fixtures', LEGACY_DIR, '--json', join(LEGACY_DIR, 'no-dir', 'r.json')]);
    assert.equal(r.code, 2);
    assert.match(r.err, /cannot write .*r\.json \(ENOENT\)/);
  });

  test('formatLine', () => {
    assert.equal(formatLine({ id: 'A', level: 'must', status: 'pass', problems: [] }), 'PASS A');
    assert.equal(formatLine({ id: 'A', level: 'must', status: 'fail', problems: ['x', 'y'] }), 'FAIL A: x; y');
    assert.equal(formatLine({ id: 'A', level: 'should', status: 'fail', problems: ['x'] }), 'FAIL [SHOULD] A: x');
    assert.equal(formatLine({ id: 'A', level: 'must', status: 'skip', problems: ['why'] }), 'SKIP A: why');
    assert.equal(formatLine({ id: 'A', level: 'must', status: 'fail', problems: ['a\u001b[2Jb\u009bc\u00ffd'] }), 'FAIL A: a\\u001b[2Jb\\u009bc\u00ffd');
  });
});

describe('runner requests and bounds (DL-078)', () => {
  test('runner_sends_host_and_json_content_type', async () => {
    fake = await startFake({ respond: legacyResponder() });
    const r = await runMain(['--target', fake.target, '--suite', 'legacy', '--fixtures', LEGACY_DIR, '--only', 'FX-LEG-T-FIRST']);
    assert.equal(r.code, 0);
    const authority = fake.target.slice('http://'.length);
    const requests = fake.state.raw.map((_, i) => fake.rawRequest(i));
    const info = requests.find((q) => q.startsWith('GET /__rivqen/conformance/info '));
    const post = requests.find((q) => q.startsWith('POST /__rivqen/conformance/case '));
    assert.ok(info.includes(`\r\nHost: ${authority}\r\n`), info);
    assert.ok(post.includes(`\r\nHost: ${authority}\r\n`), post);
    assert.ok(post.includes('\r\nContent-Type: application/json\r\n'), post);
  });

  test('problems_capped_at_50_with_more_line', async () => {
    const change = (response, kase) => {
      if (kase.case_id !== 'FX-LEG-T-DATA') return response;
      const body = JSON.parse(response.body.toString('utf8'));
      for (let i = 0; i < 120; i++) body.data[`extra${i}`] = 'x';
      return { ...response, body: Buffer.from(JSON.stringify(body)) };
    };
    fake = await startFake({ respond: legacyResponder({ change }) });
    const dir = `${mkdtempSync(join(tmpdir(), 'rivqen-cap-'))}/`;
    temps.push(dir);
    const r = await runMain(['--target', fake.target, '--suite', 'legacy', '--fixtures', LEGACY_DIR, '--only', 'FX-LEG-T-DATA', '--json', `${dir}r.json`]);
    assert.equal(r.code, 1);
    const problems = JSON.parse(readFileSync(`${dir}r.json`, 'utf8')).results[0].problems;
    assert.equal(problems.length, 51);
    assert.equal(problems[50], '… 70 more');
    assert.ok(r.lines[0].endsWith('; … 70 more'), r.lines[0]);
    assert.equal(capProblems(['a', 'b']).length, 2);
    const list = (n) => Array.from({ length: n }, (_, i) => String(i));
    assert.equal(capProblems(list(50)).length, 50);
    assert.deepEqual(capProblems(list(51)).slice(49), ['49', '… 1 more']);
  });

  test('stderr_messages_escaped', async () => {
    const r = await runMain(['--target', 'http://127.0.0.1:1', '--suite', 'rqp', '--x‮\u001b[2J', 'v']);
    assert.equal(r.code, 2);
    assert.ok(r.err.includes('unknown option --x\\u202e\\u001b[2J'), r.err);
    assert.ok(!/[‮\u001b]/.test(r.err));
    const missing = await runMain(['--target', 'http://127.0.0.1:1', '--suite', 'rqp', '--fixtures', '/no/⁦dir\u0085']);
    assert.equal(missing.code, 2);
    assert.ok(missing.err.includes('/no/\\u2066dir\\u0085'), missing.err);
    const bin = spawnSync(process.execPath, [BIN, '--target', 'http://127.0.0.1:1', '--suite', 'rqp', '--y‮', 'v'], { encoding: 'utf8' });
    assert.equal(bin.status, 2);
    assert.ok(!/[‮]/.test(bin.stderr));
  });

  test('bidi_and_line_separators_escaped', () => {
    const all = ['\u0000', '\u001f', '\u007f', '\u0080', '\u009f', ' ', ' ', '‪', '‫', '‬', '‭', '‮', '⁦', '⁧', '⁨', '⁩'];
    for (const c of all) {
      const line = formatLine({ id: `I${c}`, level: 'must', status: 'fail', problems: [`a${c}b`] });
      assert.ok(!line.includes(c), JSON.stringify(c));
      const hex = c.charCodeAt(0).toString(16).padStart(4, '0');
      assert.equal(line, `FAIL I\\u${hex}: a\\u${hex}b`);
    }
    for (const c of [' ', '‧', ' ', '⁥', '⁪', 'é']) assert.equal(escapeText(c), c, 'neighbours stay');
    assert.equal(escapeText(escapeText('‮')), '\\u202e', 'idempotent');
    const results = [{ id: 'A', level: 'must', status: 'fail', problems: ['‮'] }];
    assert.equal(toJson({ target: { origin: 'o' }, suite: 'rqp' }, results, {}).results[0].problems[0], '‮', 'the JSON report keeps raw strings');
  });
});
