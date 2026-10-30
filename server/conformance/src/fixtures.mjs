// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Case selection and loading of the two suites (CONTRACT.md sections 1 and 3).
//
//   rqp:    the server cases FX-RQ-P* of fixtures/rqp/protocol (README.md there).
//   legacy: fixtures/legacy (README.md there); the profile selection rule of
//           "The runner with --profile" (DL-068).
//
// Fixture files are repository files, not network input, but a defect in one
// of them fails that case (FixtureError); it never stops the run.

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

export const SUITES = Object.freeze(['rqp', 'legacy']);
export const PROFILES = Object.freeze(['legacy-canonical', 'legacy-java', 'legacy-node', 'legacy-php']);
export const DEFAULT_PROFILE = 'legacy-canonical';
export const DEFAULT_DIRS = Object.freeze({
  rqp: fileURLToPath(new URL('../../../fixtures/rqp/protocol/', import.meta.url)),
  legacy: fileURLToPath(new URL('../../../fixtures/legacy/', import.meta.url)),
});
/** The directory name suffix of the profile fixtures of each upstream profile. */
const SUFFIX = Object.freeze({ 'legacy-java': 'JAVA', 'legacy-node': 'NODE', 'legacy-php': 'PHP' });
const PROFILE_FIXTURE = /^(FX-LEG-.+)--(JAVA|NODE|PHP)$/;
const PAGE_ID = /^[A-Za-z0-9_-]+$/;
export const DEFAULT_APP = Object.freeze({ status: 200, headers: [['content-type', 'text/html; charset=utf-8']] });

/** A defect in a fixture file. */
export class FixtureError extends Error {
  constructor(message, options) {
    super(message, options);
    this.name = 'FixtureError';
    this.code = 'CONFORMANCE_FIXTURE';
  }
}

/** Directory names with the prefix, in byte order ("directory order"). */
export function caseNames(dir, prefix) {
  return readdirSync(dir)
    .filter((n) => n.startsWith(prefix) && statSync(join(dir, n)).isDirectory())
    .sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)));
}

function readJson(dir, id, file) {
  let text;
  try {
    text = readFileSync(join(dir, id, file), 'utf8');
  } catch (error) {
    throw new FixtureError(`${id}/${file}: cannot read (${error.code ?? error.name})`, { cause: error });
  }
  try {
    return JSON.parse(text);
  } catch (error) {
    throw new FixtureError(`${id}/${file}: not valid JSON`, { cause: error });
  }
}

const isObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
const isLines = (v) => Array.isArray(v) && v.every((l) => Array.isArray(l) && l.length === 2 && typeof l[0] === 'string' && typeof l[1] === 'string');

function checkRequest(id, request) {
  if (!isObject(request) || typeof request.method !== 'string' || typeof request.target !== 'string' || !isLines(request.headers)) {
    throw new FixtureError(`${id}/expected.json: request must have method, target and headers`);
  }
}

function checkApp(id, app) {
  if (!isObject(app) || !Number.isInteger(app.status) || !isLines(app.headers)) throw new FixtureError(`${id}/case.json: app must have status and headers`);
}

function readPage(dir, id, page) {
  if (typeof page !== 'string' || !PAGE_ID.test(page)) throw new FixtureError(`${id}/case.json: page ${JSON.stringify(page)} is not a page id`);
  try {
    return readFileSync(join(dir, 'pages', `${page}.html`));
  } catch (error) {
    throw new FixtureError(`${id}: cannot read pages/${page}.html (${error.code ?? error.name})`, { cause: error });
  }
}

function levelOf(id, expected) {
  const level = expected.level ?? 'must';
  if (level !== 'must' && level !== 'should') throw new FixtureError(`${id}/expected.json: level ${JSON.stringify(level)}`);
  return level;
}

/**
 * A selected case: { id, skip } (a reason), { id, error } (a fixture defect)
 * or { id, level, request, expected, page, app, serverConfig }.
 */
function entry(id, load) {
  try {
    return { id, ...load() };
  } catch (error) {
    if (error instanceof FixtureError) return { id, error: error.message };
    throw error;
  }
}

function loadRqp(dir, id) {
  const kase = readJson(dir, id, 'case.json');
  const expected = readJson(dir, id, 'expected.json');
  checkRequest(id, expected.request);
  checkApp(id, kase.app);
  if (!isObject(expected.body) || !isObject(expected.headers_required) || !Array.isArray(expected.headers_forbidden)) {
    throw new FixtureError(`${id}/expected.json: not a server case`);
  }
  return {
    level: levelOf(id, expected),
    request: expected.request,
    expected,
    page: readPage(dir, id, kase.page),
    app: kase.app,
    serverConfig: expected.server_config ?? {},
  };
}

/** The server cases FX-RQ-P* (README.md there: P = server case). */
export function rqpCases({ dir = DEFAULT_DIRS.rqp, only = '' } = {}) {
  return caseNames(dir, 'FX-RQ-P')
    .filter((id) => id.startsWith(only))
    .map((id) => entry(id, () => loadRqp(dir, id)));
}

function checkLegacyExpected(id, expected) {
  checkRequest(id, expected.request);
  if (!Number.isInteger(expected.status)) throw new FixtureError(`${id}/expected.json: status must be an integer`);
  if (!isObject(expected.fields) || !Object.values(expected.fields).every((v) => Array.isArray(v) && v.every((s) => typeof s === 'string'))) {
    throw new FixtureError(`${id}/expected.json: fields must map names to lists of strings`);
  }
  if (!isObject(expected.body) || !['page', 'data', 'none'].includes(expected.body.kind)) throw new FixtureError(`${id}/expected.json: body.kind must be page, data or none`);
}

function loadLegacy(dir, id, expected) {
  checkLegacyExpected(id, expected);
  const kase = readJson(dir, id, 'case.json');
  // A profile fixture takes page, app and request of the fixture that it replaces.
  const m = PROFILE_FIXTURE.exec(id);
  let base = kase;
  if (m !== null) {
    if (kase.replaces !== m[1]) throw new FixtureError(`${id}/case.json: replaces must be ${m[1]}`);
    base = readJson(dir, m[1], 'case.json');
  }
  const app = base.app ?? DEFAULT_APP;
  checkApp(id, app);
  return { level: levelOf(id, expected), request: expected.request, expected, page: readPage(dir, id, base.page), app, serverConfig: {} };
}

/**
 * The legacy cases of a profile (fixtures/legacy/README.md): legacy-canonical
 * runs every canonical fixture; an upstream profile runs the canonical
 * fixtures that list it in profiles_same and that it does not replace, and
 * its own profile fixtures. A canonical fixture without expected.json is a
 * skip: the runner cannot tell whether it is selected.
 */
export function legacyCases({ dir = DEFAULT_DIRS.legacy, profile = DEFAULT_PROFILE, only = '' } = {}) {
  const names = caseNames(dir, 'FX-LEG-');
  const suffix = SUFFIX[profile];
  const replaced = new Set(names.map((n) => PROFILE_FIXTURE.exec(n)).filter((m) => m !== null && m[2] === suffix).map((m) => m[1]));
  const out = [];
  for (const id of names) {
    if (!id.startsWith(only)) continue;
    const m = PROFILE_FIXTURE.exec(id);
    if (m !== null && m[2] !== suffix) continue;
    if (m === null && id.includes('--')) continue; // not a fixture name of the README
    if (m === null && replaced.has(id)) continue;
    let expected;
    try {
      expected = readJson(dir, id, 'expected.json');
    } catch (error) {
      if (error.cause?.code === 'ENOENT') out.push({ id, skip: 'no expected.json' });
      else out.push({ id, error: error.message });
      continue;
    }
    if (m === null && suffix !== undefined && !(Array.isArray(expected.profiles_same) && expected.profiles_same.includes(profile))) continue;
    out.push(entry(id, () => loadLegacy(dir, id, expected)));
  }
  return out;
}
