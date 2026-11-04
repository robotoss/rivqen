// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The per-run control token (CONTRACT.md §2; H-117).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { CONTROL_TOKEN_FIELD, main, readToken } from '../src/runner.mjs';

const TOKEN = 'abcdefghijklmnopqrstuvwxyz012345-_';

function tokenFile(text) {
  const dir = mkdtempSync(join(tmpdir(), 'rivqen-token-'));
  const file = join(dir, 'token');
  writeFileSync(file, text);
  return { file, done: () => rmSync(dir, { recursive: true, force: true }) };
}

test('readToken trims the file and checks the form', () => {
  const ok = tokenFile(`${TOKEN}\n`);
  try {
    assert.equal(readToken(ok.file), TOKEN);
  } finally {
    ok.done();
  }
  for (const bad of ['short', 'with space in the token value', 'x'.repeat(257), 'abc+/=abcdefghijklmnop']) {
    const f = tokenFile(bad);
    try {
      assert.throws(() => readToken(f.file), /16 to 256 base64url characters/);
    } finally {
      f.done();
    }
  }
  assert.throws(() => readToken('/nonexistent/rivqen-token'), /cannot read the control token file \(ENOENT\)/);
  assert.equal(readToken(null), null);
});

test('the runner sends the token on GET /info and POST /case, and never in a case request', async () => {
  const seen = [];
  const server = createServer((req, res) => {
    seen.push({ path: req.url, token: req.headers[CONTROL_TOKEN_FIELD.toLowerCase()] ?? null });
    if (req.url === '/__rivqen/conformance/info') {
      res.writeHead(200, { 'content-type': 'application/json' });
      res.end(JSON.stringify({ contract: 1, suites: ['rqp'], profiles: [] }));
    } else if (req.url === '/__rivqen/conformance/case') {
      req.resume();
      req.on('end', () => {
        res.writeHead(204);
        res.end();
      });
    } else {
      res.writeHead(404);
      res.end();
    }
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const { port } = server.address();
  const f = tokenFile(TOKEN);
  const out = { write: () => true };
  try {
    await main(['--target', `http://127.0.0.1:${port}`, '--suite', 'rqp', '--only', 'FX-RQ-P01-FIRST-VISIT', '--control-token-file', f.file], { stdout: out, stderr: out });
  } finally {
    f.done();
    server.close();
  }
  const control = seen.filter((s) => s.path.startsWith('/__rivqen/'));
  assert.ok(control.length >= 2, JSON.stringify(seen));
  assert.ok(control.every((s) => s.token === TOKEN), JSON.stringify(seen));
  const cases = seen.filter((s) => !s.path.startsWith('/__rivqen/'));
  assert.ok(cases.length >= 1 && cases.every((s) => s.token === null), JSON.stringify(seen));
});

test('a bad token file is a usage error (exit 2) and the token is not printed', async () => {
  const f = tokenFile('bad token value with spaces');
  let err = '';
  const sink = { write: (s) => { err += s; return true; } };
  try {
    const code = await main(['--target', 'http://127.0.0.1:9', '--suite', 'rqp', '--control-token-file', f.file], { stdout: sink, stderr: sink });
    assert.equal(code, 2);
  } finally {
    f.done();
  }
  assert.match(err, /control token must be/);
  assert.doesNotMatch(err, /spaces/);
});
