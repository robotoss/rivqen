// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The generator: the committed files are current, --check finds stale,
// missing and extra files, and defective cases stop the generator.

import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { cpSync, mkdtempSync, rmSync, writeFileSync, readFileSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { after, test } from 'node:test';
import { checkPages, run } from '../gen.mjs';
import { CaseError, FIXTURES_DIR, expectFixture, fixtureNames, formatJson, readJsonFile, substitute } from '../src/fixtures.mjs';
import { GENERATED, pageBytes } from '../src/pages.mjs';
import { deltaBinding, pageRevision, sha256b64u } from '../src/common.mjs';
import { analyze } from '../src/markup.mjs';

const GEN = fileURLToPath(new URL('../gen.mjs', import.meta.url));
const tmp = mkdtempSync(join(tmpdir(), 'rqp-protocol-gen-'));
after(() => rmSync(tmp, { recursive: true, force: true }));

function copyFixture(name) {
  const dir = join(tmp, `${name}-${Math.random().toString(36).slice(2)}`);
  mkdirSync(dir);
  cpSync(join(FIXTURES_DIR, name), join(dir, name), { recursive: true });
  return dir;
}

test('gen.mjs --check: every committed page and expected file is current', () => {
  const out = execFileSync(process.execPath, [GEN, '--check'], { encoding: 'utf8' });
  assert.match(out, /241 fixtures are current/);
});

test('every catalog case P-01 to P-41 and C-01 to C-31 has at least one fixture', () => {
  const names = fixtureNames();
  for (let i = 1; i <= 41; i++) assert.ok(names.some((n) => n.startsWith(`FX-RQ-P${String(i).padStart(2, '0')}-`)), `P-${i}`);
  for (let i = 1; i <= 31; i++) assert.ok(names.some((n) => n.startsWith(`FX-RQ-C${String(i).padStart(2, '0')}-`)), `C-${i}`);
});

test('H-107: in each rejected C-31 case only B or U of the binding differs; R\' alone (the check of H-104) fits the snapshot', () => {
  for (const name of ['FX-RQ-C31-ROLLBACK', 'FX-RQ-C31-STALE-OWN-TARGET', 'FX-RQ-C31-ALL-BLOCKS-OTHER-BASE', 'FX-RQ-C31-ALL-BLOCKS-OTHER-URL']) {
    const e = expectFixture(FIXTURES_DIR, name).files;
    const expected = e['expected.json'];
    assert.deepEqual([expected.result.outcome, expected.result.error], ['rejected', 'INTEGRITY_ERROR'], name);
    const env = JSON.parse(e['response.body'].toString('utf8'));
    const snap = analyze(pageBytes(expected.context.snapshot.page));
    const hashes = new Map(env.operations.map((op) => [op.block_id, sha256b64u(Buffer.from(op.content, 'utf8'))]));
    const next = pageRevision(snap.template_revision, snap.blocks.map((b) => ({ ...b, sha256: hashes.get(b.id) ?? b.sha256 })));
    assert.equal(next, analyze(pageBytes('page-b')).page_revision, name);
    // The binding of the snapshot (U of the context, B of the snapshot) with this R' is not the binding of the envelope.
    assert.notEqual(deltaBinding(expected.context.url, snap.page_revision, next), env.binding, name);
  }
});

test('context.allowlist (DL-039/DL-040): default the origin of url, required for a cross-origin final_url; another shape is a defective case', () => {
  const name = 'FX-RQ-C25-REDIRECT-OFF-ALLOWLIST';
  const dir = copyFixture(name);
  const file = join(dir, name, 'case.json');
  const c = JSON.parse(readFileSync(file, 'utf8'));
  const withContext = (context) => {
    writeFileSync(file, JSON.stringify({ ...c, context }));
    return expectFixture(dir, name).files['expected.json'];
  };
  const { allowlist: _allowlist, ...rest } = c.context;
  assert.throws(() => withContext(rest), (e) => e instanceof CaseError && /allowlist is required/.test(e.message));
  assert.equal(withContext({ ...rest, allowlist: ['https://shop.example.com', 'https://pay.example.net'] }).result.outcome, 'stored');
  const { final_url: _final, ...noRedirect } = rest;
  assert.deepEqual(withContext(noRedirect).context.allowlist, ['https://shop.example.com']);
  assert.equal(withContext({ ...rest, allowlist: ['https://shop.example.com'] }).result.outcome, 'normal');
  for (const bad of ['https://shop.example.com', [1], null, {}]) {
    assert.throws(() => withContext({ ...rest, allowlist: bad }), (e) => e instanceof CaseError && /allowlist must be a list of origin strings/.test(e.message), JSON.stringify(bad));
  }
  assert.throws(() => withContext({ ...rest, final_url: 'not a url' }), (e) => e instanceof CaseError && /not a URL/.test(e.message));
});

test('context.redirected (H-67): default "final_url differs from url"; true with the same URL; false with another final URL is a defect', () => {
  const name = 'FX-RQ-C24-PUBLIC-REDIRECT-SAME-URL';
  const dir = copyFixture(name);
  const file = join(dir, name, 'case.json');
  const c = JSON.parse(readFileSync(file, 'utf8'));
  const withContext = (context) => {
    writeFileSync(file, JSON.stringify({ ...c, context }));
    return expectFixture(dir, name).files['expected.json'];
  };
  const same = withContext(c.context);
  assert.deepEqual([same.context.redirected, same.result.snapshot.partition], [true, 'request']);
  const { redirected: _r, ...rest } = c.context;
  const none = withContext(rest);
  assert.deepEqual([none.context.redirected, none.result.snapshot.partition], [false, 'public']);
  const other = withContext({ ...rest, final_url: 'https://shop.example.com/catalog?x=1' });
  assert.equal(other.context.redirected, true);
  assert.throws(() => withContext({ ...rest, final_url: 'https://shop.example.com/catalog?x=1', redirected: false }), (e) => e instanceof CaseError && /redirected is false/.test(e.message));
  assert.throws(() => withContext({ ...rest, redirected: 'yes' }), (e) => e instanceof CaseError && /redirected must be a boolean/.test(e.message));
});

test('context.allowlist (H-69): an entry that is not its own serialized origin is a defective case', () => {
  const name = 'FX-RQ-C25-REDIRECT-OFF-ALLOWLIST';
  const dir = copyFixture(name);
  const file = join(dir, name, 'case.json');
  const c = JSON.parse(readFileSync(file, 'utf8'));
  const withList = (allowlist) => {
    writeFileSync(file, JSON.stringify({ ...c, context: { ...c.context, allowlist } }));
    return expectFixture(dir, name).files['expected.json'];
  };
  assert.deepEqual(withList(['https://shop.example.com', 'https://xn--fa-hia.example']).context.allowlist, ['https://shop.example.com', 'https://xn--fa-hia.example']);
  for (const bad of ['https://SHOP.example.com', 'https://shop.example.com:443', 'https://shop.example.com/', 'https://shop.example.com/catalog', 'https://faß.example', '*']) {
    assert.throws(() => withList(['https://pay.example.net', bad]), (e) => e instanceof CaseError && e.message.includes(`entry ${JSON.stringify(bad)} is not a serialized origin (H-69)`), bad);
  }
});

test('a context snapshot for a URL with user information is a defective case (H-68)', () => {
  const name = 'FX-RQ-C25-USERINFO-REQUEST';
  const dir = copyFixture(name);
  const file = join(dir, name, 'case.json');
  const c = JSON.parse(readFileSync(file, 'utf8'));
  writeFileSync(file, JSON.stringify({ ...c, context: { ...c.context, navigation: 'delta', snapshot: 'page-a' } }));
  assert.throws(() => expectFixture(dir, name), (e) => e instanceof CaseError && /needs a url without user information \(H-68\)/.test(e.message));
});

test('code review round 4: every redirect case lists its allowlist; the expected context has redirected', () => {
  for (const name of fixtureNames()) {
    const c = readJsonFile(join(FIXTURES_DIR, name, 'case.json'));
    if (c.kind !== 'client') continue;
    const e = readJsonFile(join(FIXTURES_DIR, name, 'expected.json'));
    assert.equal(typeof e.context.redirected, 'boolean', name);
    if (e.context.redirected) assert.ok(Array.isArray(c.context.allowlist), `${name}: a redirect case must list context.allowlist`);
  }
});

test('context.response_fields_visible (H-64): default true; a value that is not a boolean is a defective case', () => {
  const name = 'FX-RQ-C24-PUBLIC-NO-CREDENTIALS';
  const dir = copyFixture(name);
  const file = join(dir, name, 'case.json');
  const c = JSON.parse(readFileSync(file, 'utf8'));
  const visible = (value) => {
    writeFileSync(file, JSON.stringify({ ...c, context: { ...c.context, response_fields_visible: value } }));
    return expectFixture(dir, name).files['expected.json'];
  };
  const absent = expectFixture(dir, name).files['expected.json'];
  assert.deepEqual([absent.context.response_fields_visible, absent.result.snapshot.partition], [true, 'public']);
  assert.equal(visible(false).result.snapshot.partition, 'request');
  assert.throws(() => visible('no'), (e) => e instanceof CaseError && /response_fields_visible must be a boolean/.test(e.message));
});

test('context.snapshot.partition (H-65): request (default) or public; public needs mode public', () => {
  const name = 'FX-RQ-C27-PUBLIC-SNAPSHOT-DELTA-STAYS-PUBLIC';
  const dir = copyFixture(name);
  const file = join(dir, name, 'case.json');
  const c = JSON.parse(readFileSync(file, 'utf8'));
  const withSnapshot = (snapshot) => {
    writeFileSync(file, JSON.stringify({ ...c, context: { ...c.context, snapshot } }));
    return expectFixture(dir, name).files['expected.json'];
  };
  const { partition: _p, ...requestSnapshot } = c.context.snapshot;
  const req = withSnapshot(requestSnapshot);
  assert.deepEqual([req.context.snapshot.partition, req.result.snapshot.partition], ['request', 'request']);
  assert.deepEqual([withSnapshot(c.context.snapshot).context.snapshot.partition, withSnapshot(c.context.snapshot).result.snapshot.partition], ['public', 'public']);
  assert.throws(() => withSnapshot({ ...c.context.snapshot, partition: 'shared' }), (e) => e instanceof CaseError && /partition shared/.test(e.message));
  assert.throws(() => withSnapshot({ page: 'page-a', partition: 'public' }), (e) => e instanceof CaseError && /needs the member policy with mode public/.test(e.message));
  assert.throws(() => withSnapshot({ ...c.context.snapshot, members: { policy: { mode: 'private' } } }), (e) => e instanceof CaseError && /mode public/.test(e.message));
});

test('headers_optional is written for a 304 only', () => {
  const nm = expectFixture(FIXTURES_DIR, 'FX-RQ-P05-NOT-MODIFIED').files['expected.json'];
  assert.deepEqual(nm.headers_optional, { 'rq-version': '1' });
  assert.equal(Object.hasOwn(expectFixture(FIXTURES_DIR, 'FX-RQ-P01-FIRST-VISIT').files['expected.json'], 'headers_optional'), false);
});

test('context.credentials: true, false or null (unknown, the default); another value is a defective case (H-62)', () => {
  const name = 'FX-RQ-C24-PUBLIC-CREDENTIALS-UNKNOWN';
  const dir = copyFixture(name);
  const file = join(dir, name, 'case.json');
  const c = JSON.parse(readFileSync(file, 'utf8'));
  const credentials = (value) => {
    writeFileSync(file, JSON.stringify({ ...c, context: { ...c.context, credentials: value } }));
    return expectFixture(dir, name).files['expected.json'];
  };
  const absent = expectFixture(dir, name).files['expected.json'];
  assert.equal(absent.context.credentials, null);
  assert.deepEqual(credentials(null), absent);
  assert.equal(credentials(false).context.credentials, false);
  assert.throws(() => credentials('yes'), (e) => e instanceof CaseError && /credentials must be a boolean or null/.test(e.message));
});

test('--check reports a stale expected file and a missing one, and the generator rewrites them', () => {
  const dir = copyFixture('FX-RQ-P09-DELTA');
  const file = join(dir, 'FX-RQ-P09-DELTA', 'expected.json');
  const good = readFileSync(file);
  writeFileSync(file, good.toString('utf8').replace('"price"', '"title"'));
  assert.deepEqual(run({ check: true, dir }).problems, ['FX-RQ-P09-DELTA/expected.json: stale']);
  rmSync(file);
  assert.deepEqual(run({ check: true, dir }).problems, ['FX-RQ-P09-DELTA/expected.json: missing']);
  assert.deepEqual(run({ check: false, dir }).problems, []);
  assert.deepEqual(readFileSync(file), good);
});

test('--check reports an unknown file and a response.body that the case does not have', () => {
  const dir = copyFixture('FX-RQ-P09-DELTA');
  writeFileSync(join(dir, 'FX-RQ-P09-DELTA', 'response.body'), 'x');
  writeFileSync(join(dir, 'FX-RQ-P09-DELTA', 'notes.txt'), 'x');
  const problems = run({ check: true, dir }).problems;
  assert.ok(problems.includes('FX-RQ-P09-DELTA/response.body: not generated for this case'));
  assert.ok(problems.includes('FX-RQ-P09-DELTA/notes.txt: unknown file'));
});

test('a stale response.body of a client case is found', () => {
  const dir = copyFixture('FX-RQ-C01-OTHER-BASE');
  writeFileSync(join(dir, 'FX-RQ-C01-OTHER-BASE', 'response.body'), '{}');
  assert.deepEqual(run({ check: true, dir }).problems, ['FX-RQ-C01-OTHER-BASE/response.body: stale']);
});

test('defective cases stop with a case error: id, unknown member, placeholder, page', () => {
  const dir = copyFixture('FX-RQ-P09-DELTA');
  const file = join(dir, 'FX-RQ-P09-DELTA', 'case.json');
  const c = JSON.parse(readFileSync(file, 'utf8'));
  const expectError = (mutated, pattern) => {
    writeFileSync(file, JSON.stringify(mutated));
    assert.throws(() => expectFixture(dir, 'FX-RQ-P09-DELTA'), (e) => e instanceof CaseError && pattern.test(e.message));
  };
  expectError({ ...c, id: 'FX-RQ-OTHER' }, /not the directory name/);
  expectError({ ...c, extra: 1 }, /unknown member case.extra/);
  expectError({ ...c, request: { ...c.request, headers: [['if-none-match', '{{r2:page-a}}']] } }, /unknown placeholder kind r2/);
  expectError({ ...c, request: { ...c.request, headers: [['if-none-match', '{{r1:page-a}']] } }, /not closed/);
  expectError({ ...c, request: { ...c.request, headers: [['If-None-Match', 'x']] } }, /not lower case/);
  expectError({ ...c, page: 'page-zzz' }, /unknown page page-zzz/);
  writeFileSync(file, JSON.stringify({ ...c, request: { ...c.request, headers: [['if-none-match', '{{r1:page-c}}']] } }));
  assert.ok(run({ check: true, dir }).problems.some((p) => /page page-c is not valid for RQP/.test(p)));
});

test('body recipe manifest_after (H-72): after the first copy of the text; a defective value stops with a case error', () => {
  const name = 'FX-RQ-C21-HEAD-WHITESPACE';
  const dir = copyFixture(name);
  const file = join(dir, name, 'case.json');
  const c = JSON.parse(readFileSync(file, 'utf8'));
  const body = expectFixture(dir, name).files['response.body'];
  const at = pageBytes('page-a').indexOf('<head>\n') + '<head>\n'.length;
  assert.equal(body.indexOf('<script type="application/rivqen-manifest+json">'), at);
  const withBody = (extra) => writeFileSync(file, JSON.stringify({ ...c, response: { ...c.response, body: { ...c.response.body, ...extra } } }));
  for (const [extra, pattern] of [
    [{ manifest_after: '<nothing>' }, /no such position/],
    [{ manifest_after: '' }, /manifest_after must be a text/],
    [{ manifest_after: 3 }, /manifest_after must be a text/],
    [{ manifest_position: 'body' }, /manifest_position and manifest_after together/],
  ]) {
    withBody(extra);
    assert.throws(() => expectFixture(dir, name), (e) => e instanceof CaseError && pattern.test(e.message), JSON.stringify(extra));
  }
});

test('level "should" is copied to expected.json; another level is a defective case', () => {
  const name = 'FX-RQ-P35-ATTRIBUTE-RULE';
  const dir = copyFixture(name);
  const file = join(dir, name, 'case.json');
  const c = JSON.parse(readFileSync(file, 'utf8'));
  assert.equal(expectFixture(dir, name).files['expected.json'].level, 'should');
  writeFileSync(file, JSON.stringify({ ...c, level: 'must' }));
  assert.throws(() => expectFixture(dir, name), (e) => e instanceof CaseError && /level must be "should"/.test(e.message));
  const { level: _level, ...withoutLevel } = c;
  writeFileSync(file, JSON.stringify(withoutLevel));
  assert.equal(Object.hasOwn(expectFixture(dir, name).files['expected.json'], 'level'), false);
});

test('placeholders: r1 with quotes, t1 as a token, have as a Byte Sequence; ":" for zero blocks', () => {
  assert.match(substitute('T', '{{r1:page-a}}'), /^"r1\.[A-Za-z0-9_-]{43}"$/);
  assert.match(substitute('T', '{{t1:page-a}}'), /^t1\.[A-Za-z0-9_-]{43}$/);
  assert.equal(substitute('T', '{{have:page-zero}}'), '::');
  assert.equal(substitute('T', 'no placeholder'), 'no placeholder');
});

test('page recipes: the sizes of fixtures.md section 4.1 and the expected markup results', () => {
  assert.deepEqual(checkPages(), []);
  const size = (id) => pageBytes(id).length;
  assert.ok(size('page-big-a') > 19 * 1024 && size('page-big-a') < 22 * 1024);
  const huge = pageBytes('page-huge-a').toString('latin1');
  assert.equal(huge.split('data-rq-block="huge">')[1].split('</div>')[0].length, 300 * 1024);
  const a = pageBytes('page-huge-a');
  const b = pageBytes('page-huge-b');
  const diff = [...a.keys()].filter((i) => a[i] !== b[i]);
  assert.equal(diff.length, 1);
  assert.equal(pageBytes('page-esc-a').toString('latin1').split('"').length - 1, 2048 + 2); // and the 2 quotes of the block attribute
  assert.deepEqual(Object.keys(GENERATED).sort(), ['page-big-a', 'page-big-b', 'page-esc-a', 'page-esc-b', 'page-huge-a', 'page-huge-b']);
});

test('checkPages finds a page with the wrong markup result', () => {
  const problems = checkPages((id) => (id === 'page-c' ? pageBytes('page-a') : pageBytes(id)));
  assert.deepEqual(problems, ['page page-c: expected RQP_MARKUP_DUPLICATE, got valid']);
});

test('formatJson keeps header lines on one line and is valid JSON', () => {
  const value = { a: [['x', 'y']], b: [], c: {}, d: [{ e: 1 }] };
  const text = formatJson(value);
  assert.deepEqual(JSON.parse(text), value);
  assert.match(text, /\["x","y"\]/);
});

/** expectFixture of a copy of fixture `name` whose case.json is change(case). */
function withCase(name, change) {
  const dir = copyFixture(name);
  const file = join(dir, name, 'case.json');
  writeFileSync(file, JSON.stringify(change(JSON.parse(readFileSync(file, 'utf8')))));
  return () => expectFixture(dir, name);
}

const isCaseError = (e) => e instanceof CaseError && e.name === 'CaseError' && e.code === 'RQP_FIXTURE_CASE';

test('defective server cases: description, rules, header lines, client page, and an object where one is required', () => {
  const defects = [
    (c) => ({ ...c, description: undefined }),
    (c) => ({ ...c, rules: 'RQP-01' }),
    (c) => ({ ...c, request: { ...c.request, headers: {} } }),
    (c) => ({ ...c, request: { ...c.request, headers: ['ab'] } }),
    (c) => ({ ...c, request: { ...c.request, headers: [['a', 'b', 'c']] } }),
    (c) => ({ ...c, request: { ...c.request, headers: [[1, 'x']] } }),
    (c) => ({ ...c, request: { ...c.request, headers: [['a', 1]] } }),
    (c) => ({ ...c, client_page: 'page-zzz' }),
    (c) => ({ ...c, app: null }),
  ];
  for (const [i, change] of defects.entries()) assert.throws(withCase('FX-RQ-P09-DELTA', change), isCaseError, String(i));
});

test('server cases: request.target is used, and Date is never a required header', () => {
  const target = 'https://shop.example.com/other';
  const date = ['date', 'Sun, 18 Oct 2026 10:00:00 GMT'];
  const e = withCase('FX-RQ-P01-FIRST-VISIT', (c) => ({ ...c, request: { ...c.request, target }, app: { ...c.app, headers: [...c.app.headers, date] } }))().files['expected.json'];
  assert.equal(e.request.target, target);
  assert.equal(e.body.manifest.path, '/other');
  assert.equal(Object.hasOwn(e.headers_required, 'date'), false);
});

test('defective client cases: members, context values, header lines and body recipes', () => {
  const defects = [
    ['FX-RQ-C01-OTHER-BASE', (c) => ({ ...c, extra: 1 })],
    ['FX-RQ-C01-OTHER-BASE', (c) => ({ ...c, context: null })],
    ['FX-RQ-C01-OTHER-BASE', (c) => ({ ...c, context: { ...c.context, navigation: 'reload' } })],
    ['FX-RQ-C01-OTHER-BASE', (c) => ({ ...c, context: { ...c.context, build: 'beta' } })],
    ['FX-RQ-C01-OTHER-BASE', (c) => ({ ...c, context: { ...c.context, allow_http: 'yes' } })],
    ['FX-RQ-C01-OTHER-BASE', (c) => ({ ...c, context: { ...c.context, public_origin: 1 } })],
    ['FX-RQ-C01-OTHER-BASE', (c) => ({ ...c, context: { ...c.context, public_enabled: 'true' } })],
    ['FX-RQ-C01-OTHER-BASE', (c) => ({ ...c, response: { ...c.response, headers: {} } })],
    // H-104: an operation of a recipe has only block_id and content.
    ['FX-RQ-C01-OTHER-BASE', (c) => ({ ...c, response: { ...c.response, body: { delta: { from: 'page-a', ops: [{ block_id: 'price', content: 'x', format: 'html' }] } } } })],
    // The envelope before H-104 needs the format of each block: an unknown block is a defect there.
    ['FX-RQ-C01-OTHER-BASE', (c) => ({ ...c, response: { ...c.response, body: { delta: { from: 'page-a', ops: [{ block_id: 'nope', content: 'x' }], old_envelope: true } } } })],
    ['FX-RQ-C01-OTHER-BASE', (c) => ({ ...c, response: { ...c.response, body: { delta: { from: 'page-zero', to: 'page-a' } } } })],
    ['FX-RQ-C21-HEAD-WHITESPACE', (c) => ({ ...c, response: { ...c.response, body: { full: 'page-a', manifest_position: 'head' } } })],
    ['FX-RQ-C21-HEAD-WHITESPACE', (c) => ({ ...c, response: { ...c.response, body: { full: 'page-a', manifest_count: 3 } } })],
  ];
  for (const [i, [name, change]] of defects.entries()) assert.throws(withCase(name, change), isCaseError, `${i} ${name}`);
  // An operation for a block that the page does not have is a valid recipe (FX-RQ-C30-DELTA-UNKNOWN-BLOCK): the client rejects it.
  const unknown = withCase('FX-RQ-C01-OTHER-BASE', (c) => ({ ...c, response: { ...c.response, body: { delta: { from: 'page-a', ops: [{ block_id: 'nope', content: 'x' }] } } } }))();
  assert.equal(unknown.files['expected.json'].result.error, 'PATCH_INVALID');
});

test('H-81 (L1): a public context snapshot with the app setting off is a valid case; the client acts as without a snapshot', () => {
  for (const flag of [undefined, false]) {
    const e = withCase('FX-RQ-C27-PUBLIC-SNAPSHOT-304', (c) => ({ ...c, context: { ...c.context, public_enabled: flag } }))().files['expected.json'];
    // The harness still gives the snapshot, in the public partition, to the client under test.
    assert.equal(e.context.snapshot.partition, 'public');
    assert.equal(e.context.public_enabled, false);
    assert.equal(e.request.headers.some(([n]) => n === 'if-none-match' || n === 'rq-have'), false);
    assert.deepEqual([e.result.outcome, e.result.next], ['normal', 'webview-load']);
  }
});

test('client cases: no body member is a null body; manifest_after may match at offset 0', () => {
  const e = withCase('FX-RQ-C01-OTHER-BASE', (c) => ({ ...c, response: { status: 304, headers: [] } }))().files;
  assert.equal(e['expected.json'].response.body, null);
  assert.equal(Object.hasOwn(e, 'response.body'), false);
  const doctype = '<!DOCTYPE html>';
  const body = withCase('FX-RQ-C21-HEAD-WHITESPACE', (c) => ({ ...c, response: { ...c.response, body: { full: 'page-a', manifest_after: doctype } } }))().files['response.body'];
  assert.equal(body.indexOf('<script type="application/rivqen-manifest+json">'), doctype.length);
});

test('delta recipe: B and R\' of the binding use the members of the context snapshot only for a delta from the snapshot page', () => {
  const members = { limits: { max_patch_bytes: 100000 } };
  const T = 'https://shop.example.com/catalog';
  const env = (from, extra = {}) => {
    const files = withCase('FX-RQ-C01-OTHER-BASE', (c) => ({
      ...c,
      context: { ...c.context, snapshot: { page: 'page-a', members } },
      response: { ...c.response, body: { delta: { from, to: from === 'page-a' ? 'page-b' : 'page-a', ...extra } } },
    }))().files;
    return JSON.parse(files['response.body'].toString('utf8'));
  };
  const a = analyze(pageBytes('page-a'));
  const b = analyze(pageBytes('page-b'));
  const aRev = pageRevision(a.template_revision, a.blocks, members);
  const bRev = pageRevision(b.template_revision, b.blocks, members);
  assert.equal(env('page-a').binding, deltaBinding(T, aRev, bRev));
  assert.equal(env('page-b').binding, deltaBinding(T, b.page_revision, a.page_revision));
  // H-104, H-107: the envelope has exactly the binding and operations of block_id and content.
  assert.deepEqual(env('page-a'), { binding: deltaBinding(T, aRev, bRev), operations: [{ block_id: 'price', content: '130.00' }] });
  // binding_url replaces U; all_blocks writes every block of `to`.
  assert.equal(env('page-a', { binding_url: 'https://shop.example.com/x#f' }).binding, deltaBinding('https://shop.example.com/x', aRev, bRev));
  assert.deepEqual(env('page-a', { all_blocks: true }).operations.map((op) => op.block_id), ['title', 'price', 'cart']);
  // next_revision_envelope: the form of H-104 (next_revision = R' in place of the binding).
  assert.deepEqual(env('page-a', { next_revision_envelope: true }), { next_revision: bRev, operations: [{ block_id: 'price', content: '130.00' }] });
  // old_envelope: the form before H-104, with the base revision of the snapshot members.
  const old = env('page-a', { old_envelope: true });
  assert.deepEqual(Object.keys(old), ['type', 'protocol', 'base_revision', 'next_revision', 'template_revision', 'operations']);
  assert.equal(old.base_revision, aRev);
  assert.equal(old.next_revision, bRev);
  assert.deepEqual(old.operations[0], { op: 'replace_block', block_id: 'price', format: 'html', content: '130.00', sha256: b.blocks[1].sha256 });
  assert.deepEqual(env('page-a', { operation_sha256: true }).operations, [{ block_id: 'price', content: '130.00', sha256: b.blocks[1].sha256 }]);
  // Defective options.
  assert.throws(() => env('page-a', { binding_url: 'not a url' }), (e) => isCaseError(e) && /binding_url/.test(e.message));
  assert.throws(() => env('page-a', { old_envelope: true, next_revision_envelope: true }), isCaseError);
  assert.throws(() => env('page-a', { ops: [{ block_id: 'price', content: '1' }], all_blocks: true }), isCaseError);
  assert.throws(() => env('page-a', { next_revision_envelope: true, duplicate_member: true }), (e) => isCaseError(e) && /duplicate_member/.test(e.message));
});

test('delta and full recipes: the duplicate member is written first, with another value', () => {
  const files = (body) => withCase('FX-RQ-C01-OTHER-BASE', (c) => ({ ...c, response: { ...c.response, body } }))().files['response.body'].toString('utf8');
  const a = analyze(pageBytes('page-a'));
  const T = 'https://shop.example.com/catalog';
  const first = deltaBinding(T, a.page_revision, a.page_revision);
  const second = deltaBinding(T, a.page_revision, analyze(pageBytes('page-b')).page_revision);
  assert.ok(files({ delta: { from: 'page-a', to: 'page-b', duplicate_member: true } }).startsWith(`{"binding":"${first}","binding":"${second}",`));
  assert.ok(files({ full: 'page-a', duplicate_member: true }).includes('{"protocol":"rqp/1","protocol":"rqp/1",'));
  // old_manifest: the manifest before H-103 (template_revision and the block list).
  const full = files({ full: 'page-a', old_manifest: true });
  assert.ok(full.includes(`"template_revision":"${a.template_revision}","page_revision":"${a.page_revision}","blocks":[{"id":"title"`));
  for (const flag of ['all_blocks', 'duplicate_member', 'old_envelope', 'next_revision_envelope', 'operation_sha256']) {
    assert.throws(() => files({ delta: { from: 'page-a', to: 'page-b', [flag]: 1 } }), isCaseError, flag);
  }
  assert.throws(() => files({ full: 'page-a', old_manifest: 'yes' }), isCaseError);
});

test('server delta expected: binding_inputs are U, B and R\' of the binding, for the diagnosis', () => {
  const e = expectFixture(FIXTURES_DIR, 'FX-RQ-P09-DELTA-QUERY').files['expected.json'];
  const i = e.body.binding_inputs;
  assert.equal(i.url, 'https://shop.example.com/catalog?sort=price&page=2');
  assert.equal(i.base_revision, analyze(pageBytes('page-big-a')).page_revision);
  assert.equal(i.next_revision, analyze(pageBytes('page-big-b')).page_revision);
  assert.equal(e.body.binding, deltaBinding(i.url, i.base_revision, i.next_revision));
});

test('delta recipe "size": exactly one operation with empty content and no sha256, and a reachable size', () => {
  const sized = (delta) => withCase('FX-RQ-C16-AT-LIMIT', (c) => ({ ...c, response: { ...c.response, body: { delta: { ...c.response.body.delta, ...delta } } } }));
  const body = sized({ size: 1000 })().files['response.body'];
  assert.equal(body.length, 1000);
  // The smallest size: the envelope with empty content.
  const smallest = 1000 - JSON.parse(body.toString('utf8')).operations[0].content.length;
  assert.equal(JSON.parse(sized({ size: smallest })().files['response.body'].toString('utf8')).operations[0].content, '');
  const defects = [
    { ops: [{ block_id: 'price', content: '' }, { block_id: 'title', content: '' }] },
    { ops: [{ block_id: 'price', content: 'x' }] },
    { ops: [{ block_id: 'price', content: '', sha256: 'x' }] }, // no sha256 in a recipe operation since H-104
    { ops: 'price' },
    { size: 1000.5 },
    { size: 10 }, // smaller than the envelope without content
  ];
  for (const delta of defects) assert.throws(sized(delta), isCaseError, JSON.stringify(delta));
});

test('fixtureNames: only FX-RQ-* directories, in byte order', () => {
  const dir = mkdtempSync(join(tmp, 'names-'));
  for (const n of ['FX-RQ-a', 'FX-RQ-B', 'FX-RQ-10', 'other']) mkdirSync(join(dir, n));
  writeFileSync(join(dir, 'FX-RQ-file'), '');
  writeFileSync(join(dir, 'README.md'), '');
  assert.deepEqual(fixtureNames(dir), ['FX-RQ-10', 'FX-RQ-B', 'FX-RQ-a']);
});

test('formatJson: an empty object and an array of scalars with null stay on one line', () => {
  assert.equal(formatJson({}), '{}');
  assert.equal(formatJson({ a: {} }), '{\n  "a": {}\n}');
  assert.equal(formatJson([null, 1, 'x']), '[null,1,"x"]');
});

test('body recipe defects name the member and the reason in the case error', () => {
  const full = (body) => withCase('FX-RQ-C05-TYPE-DOTLESS-I', (c) => ({ ...c, response: { ...c.response, body } }));
  const delta = (body) => withCase('FX-RQ-C18-FALLBACK-DELTA', (c) => ({ ...c, response: { ...c.response, body } }));
  const base = { from: 'page-a', to: 'page-b' };
  const cases = [
    [full({ full: 'page-a', old_manifest: false }), /old_manifest must be true when present/],
    [full({ full: 'page-a', extra: 1 }), /unknown member body\.extra/],
    [delta({ delta: { ...base, extra: 1 } }), /unknown member body\.delta\.extra/],
    [delta({ delta: { ...base, old_envelope: false } }), /body\.delta\.old_envelope must be true when present/],
    [delta({ delta: { ...base, duplicate_member: 1 } }), /body\.delta\.duplicate_member must be true when present/],
    [delta({ delta: { from: 'page-a', ops: 'price' } }), /body\.delta\.ops must be a list/],
    [delta({ delta: { from: 'page-a', ops: [{ block_id: 'price', content: '', extra: 1 }] } }), /unknown member body\.delta\.ops\[\]\.extra/],
    [delta({ delta: { from: 'page-a', to: 'page-no-head-body-first' } }), /page-no-head-body-first does not have the blocks of page-a/],
    [delta({ delta: { from: 'page-a', ops: [{ block_id: 'price', content: 'x' }], size: 1000 } }), /size needs exactly one operation with empty content/],
  ];
  for (const [attempt, pattern] of cases) assert.throws(attempt, (e) => isCaseError(e) && pattern.test(e.message), String(pattern));
});

test('the usage error exits 2', () => {
  assert.throws(() => execFileSync(process.execPath, [GEN, '--write'], { stdio: 'pipe' }), (e) => e.status === 2);
});
