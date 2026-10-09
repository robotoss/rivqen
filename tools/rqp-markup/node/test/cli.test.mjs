// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Command line (tools/rqp-markup/CONTRACT.md section 2).

import { after, before, describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, spawn, spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { CliError, EXIT_INTERNAL, EXIT_OK, EXIT_USAGE, main, readBounded, sortedSubdirs } from '../src/cli.mjs';
import { LIMITS } from '../src/markup.mjs';

const CLI = fileURLToPath(new URL('../src/cli.mjs', import.meta.url));
const SECRET = 'secret-content-7f3a';
const VALID = `<p data-rq-block="a">${SECRET}</p>`;
const INVALID = `<p data-rq-block="a">${SECRET}</p><p data-rq-block="a">x</p>`;

function capture(argv, analyzer) {
  const out = [];
  const err = [];
  const code = main(argv, { stdout: (s) => out.push(s), stderr: (s) => err.push(s) }, analyzer);
  return { code, stdout: out.join(''), stderr: err.join('') };
}

let dir;
before(() => {
  dir = mkdtempSync(path.join(tmpdir(), 'rqp-markup-node-'));
  writeFileSync(path.join(dir, 'valid.html'), VALID);
  writeFileSync(path.join(dir, 'invalid.html'), INVALID);
  const batch = path.join(dir, 'batch');
  for (const name of ['b', 'a', 'é', 'Z', 'no-input']) mkdirSync(path.join(batch, name), { recursive: true });
  writeFileSync(path.join(batch, 'b', 'input.html'), INVALID);
  writeFileSync(path.join(batch, 'a', 'input.html'), VALID);
  writeFileSync(path.join(batch, 'é', 'input.html'), '');
  writeFileSync(path.join(batch, 'Z', 'input.html'), VALID);
  writeFileSync(path.join(batch, 'file-not-dir'), VALID);
  mkdirSync(path.join(dir, 'broken', 'x', 'input.html'), { recursive: true });
  mkdirSync(path.join(dir, 'dangling'));
  symlinkSync(path.join(dir, 'nowhere'), path.join(dir, 'dangling', 'link'));
  writeFileSync(path.join(dir, 'too-large.html'), Buffer.alloc(LIMITS.maxDocumentBytes + 1, 0x61));
});
after(() => rmSync(dir, { recursive: true, force: true }));

describe('cli', () => {
  it('prints one result line for a valid document and exits 0', () => {
    const r = capture([path.join(dir, 'valid.html')]);
    assert.equal(r.code, EXIT_OK);
    assert.ok(r.stdout.endsWith('}\n'));
    const result = JSON.parse(r.stdout);
    assert.equal(result.valid, true);
    assert.deepEqual(result.blocks.map((b) => [b.id, b.start, b.end]), [['a', 21, 21 + SECRET.length]]);
    assert.equal(r.stderr, '');
  });

  it('prints the invalid result for an invalid document and exits 0', () => {
    const r = capture([path.join(dir, 'invalid.html')]);
    assert.equal(r.code, EXIT_OK);
    assert.deepEqual(JSON.parse(r.stdout), {
      valid: false, error: 'RQP_MARKUP_DUPLICATE', blocks: [], template_revision: null, page_revision: null,
    });
    assert.equal(r.stderr, '');
  });

  it('prints one line per fixture in byte order of the names in batch mode', () => {
    const r = capture(['--batch', path.join(dir, 'batch')]);
    assert.equal(r.code, EXIT_OK);
    const lines = r.stdout.trimEnd().split('\n').map((l) => JSON.parse(l));
    assert.deepEqual(lines.map((l) => l.fixture), ['Z', 'a', 'b', 'é']);
    assert.deepEqual(lines.map((l) => l.valid), [true, true, false, true]);
    assert.equal(r.stderr, '');
  });

  it('lists only subdirectories, sorted by UTF-8 bytes', () => {
    assert.deepEqual(sortedSubdirs(path.join(dir, 'batch')), ['Z', 'a', 'b', 'no-input', 'é']);
  });

  for (const argv of [[], ['--batch'], ['a', 'b'], ['--help'], ['--batch', 'a', 'b']]) {
    it(`exits 2 with the usage for the arguments ${JSON.stringify(argv)}`, () => {
      const r = capture(argv);
      assert.equal(r.code, EXIT_USAGE);
      assert.match(r.stderr, /usage:/);
      assert.equal(r.stdout, '');
    });
  }

  it('exits 2 for a missing input file', () => {
    const r = capture([path.join(dir, 'missing.html')]);
    assert.equal(r.code, EXIT_USAGE);
    assert.match(r.stderr, /cannot open .*missing\.html \(ENOENT\)/);
  });

  it('does not turn a defect into a usage error', () => {
    const failingOutput = { stdout: () => { throw new TypeError('defect'); }, stderr: () => {} };
    assert.throws(() => main([path.join(dir, 'valid.html')], failingOutput), TypeError);
  });

  it('keeps the I/O error as the cause', () => {
    assert.throws(() => readBounded(path.join(dir, 'missing.html')), (error) => {
      assert.ok(error instanceof CliError);
      assert.equal(error.code, 'RQP_CLI_USAGE');
      assert.equal(error.cause.code, 'ENOENT');
      return true;
    });
  });

  it('exits 2 when a directory entry cannot be read', () => {
    const r = capture(['--batch', path.join(dir, 'dangling')]);
    assert.equal(r.code, EXIT_USAGE);
    assert.match(r.stderr, /cannot read .*link \(ENOENT\)/);
  });

  it('reports RQP_MARKUP_LIMIT for a file of 5 MiB + 1 byte', () => {
    assert.equal(readBounded(path.join(dir, 'too-large.html')).length, LIMITS.maxDocumentBytes + 1);
    const r = capture([path.join(dir, 'too-large.html')]);
    assert.equal(r.code, EXIT_OK);
    assert.equal(JSON.parse(r.stdout).error, 'RQP_MARKUP_LIMIT');
  });

  it('exits 2 for a missing fixtures directory', () => {
    const r = capture(['--batch', path.join(dir, 'missing')]);
    assert.equal(r.code, EXIT_USAGE);
    assert.match(r.stderr, /cannot read directory/);
  });

  it('exits 2 when an input.html cannot be read', () => {
    const r = capture(['--batch', path.join(dir, 'broken')]);
    assert.equal(r.code, EXIT_USAGE);
    assert.match(r.stderr, /cannot read .*input\.html \(EISDIR\)/);
  });

  it('fails closed, reports the defect without content and exits 70 when the analyzer throws', () => {
    const failing = () => {
      throw new RangeError(SECRET);
    };
    const single = capture([path.join(dir, 'valid.html')], failing);
    assert.equal(single.code, EXIT_INTERNAL);
    assert.equal(JSON.parse(single.stdout).error, 'RQP_MARKUP_STRUCTURE');
    assert.match(single.stderr, /internal error \(RangeError\) for the input/);
    const batch = capture(['--batch', path.join(dir, 'batch')], failing);
    assert.equal(batch.code, EXIT_INTERNAL);
    assert.equal(batch.stdout.trimEnd().split('\n').length, 4);
    assert.match(batch.stderr, /internal error \(RangeError\) for fixture Z/);
    for (const r of [single, batch]) assert.ok(!r.stderr.includes(SECRET));
  });

  it('reads at most limit + 1 bytes', () => {
    const file = path.join(dir, 'big.html');
    writeFileSync(file, 'x'.repeat(200_000));
    assert.equal(readBounded(file, 100).length, 101);
    assert.equal(readBounded(file, 150_000).length, 150_001);
    assert.equal(readBounded(file, 200_000).length, 200_000);
    assert.equal(readBounded(file).length, 200_000);
  });

  it('runs as a command and never prints document content to stderr', () => {
    const ok = spawnSync(process.execPath, [CLI, path.join(dir, 'invalid.html')], { encoding: 'utf8' });
    assert.equal(ok.status, 0);
    assert.equal(JSON.parse(ok.stdout).error, 'RQP_MARKUP_DUPLICATE');
    assert.equal(ok.stderr, '');
    const usage = spawnSync(process.execPath, [CLI], { encoding: 'utf8' });
    assert.equal(usage.status, EXIT_USAGE);
    assert.match(usage.stderr, /^rqp-markup: usage:/);
    const batch = execFileSync(process.execPath, [CLI, '--batch', path.join(dir, 'batch')], { encoding: 'utf8' });
    assert.equal(batch.trimEnd().split('\n').length, 4);
  });

  it('exits 2 with a reason when stdout is closed', async () => {
    const child = spawn(process.execPath, [CLI, '--batch', path.join(dir, 'batch')], {
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    child.stdout.destroy();
    let stderr = '';
    child.stderr.on('data', (chunk) => {
      stderr += chunk;
    });
    const code = await new Promise((resolve) => child.on('close', resolve));
    assert.equal(code, EXIT_USAGE);
    assert.match(stderr, /cannot write output \(EPIPE\)/);
  });
});
