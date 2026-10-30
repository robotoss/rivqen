// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The conformance runner (CONTRACT.md sections 1, 3 and 4): the command line,
// the control requests, one fixture request per case, the comparison and the
// report. It tests a server by URL; it does not start the server.
//
// Exit codes: 0 no MUST case failed; 1 at least one MUST case failed; 2 a
// usage error, the target cannot be reached, GET /__rivqen/conformance/info
// fails, the server does not list the suite or the profile, or no case is
// selected.

import { writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { compareLegacy, compareRqp } from './compare.mjs';
import { DEFAULT_DIRS, DEFAULT_PROFILE, PROFILES, SUITES, legacyCases, rqpCases } from './fixtures.mjs';
import { DEFAULT_TIMEOUT_MS, HttpCode, HttpError, exchange } from './raw-http.mjs';
import { escapeText } from './escape.mjs';
import { capProblems, formatLine, formatSummary, summarize, toJson } from './report.mjs';

export const INFO_PATH = '/__rivqen/conformance/info';
export const CASE_PATH = '/__rivqen/conformance/case';
const MAX_TIMEOUT_MS = 600000;
/** The body limit of POST /case (CONTRACT.md section 2). */
export const MAX_CASE_BYTES = 8 * 1024 * 1024;

/** A usage error or a stop before the first case (exit code 2). */
export class RunnerError extends Error {
  constructor(message, options) {
    super(message, options);
    this.name = 'RunnerError';
    this.code = 'CONFORMANCE_USAGE';
  }
}

export const USAGE = 'usage: rivqen-conformance --target <http://host:port> --suite rqp|legacy [--profile <name>] [--fixtures <dir>] [--only <prefix>] [--json <file>] [--timeout-ms <n>]';

/** The `--target` origin: { origin, host, port, authority }. Only `http://`, no path, query, fragment or user information. */
export function parseTarget(text) {
  let url;
  try {
    url = new URL(text);
  } catch (error) {
    throw new RunnerError(`--target ${JSON.stringify(text)} is not a URL`, { cause: error });
  }
  if (url.protocol !== 'http:') throw new RunnerError('--target must be an http:// origin');
  if (url.username !== '' || url.password !== '') throw new RunnerError('--target must not have user information');
  if (url.pathname !== '/' || url.search !== '' || url.hash !== '' || /[?#]/.test(text)) throw new RunnerError('--target must be an origin without a path, query or fragment');
  const host = url.hostname.startsWith('[') ? url.hostname.slice(1, -1) : url.hostname;
  return { origin: url.origin, host, port: url.port === '' ? 80 : Number(url.port), authority: url.host };
}

/** Parse the command line. Throws RunnerError. */
export function parseArgs(argv) {
  const opts = { target: null, suite: null, profile: null, fixtures: null, only: '', json: null, timeoutMs: DEFAULT_TIMEOUT_MS };
  const seen = new Set();
  for (let i = 0; i < argv.length; i += 2) {
    const option = argv[i];
    const value = argv[i + 1];
    if (value === undefined) throw new RunnerError(`${option} needs a value`);
    if (seen.has(option)) throw new RunnerError(`${option} is given twice`);
    seen.add(option);
    if (option === '--target') opts.target = parseTarget(value);
    else if (option === '--suite') opts.suite = value;
    else if (option === '--profile') opts.profile = value;
    else if (option === '--fixtures') opts.fixtures = resolve(value);
    else if (option === '--only') opts.only = value;
    else if (option === '--json') opts.json = resolve(value);
    else if (option === '--timeout-ms') {
      if (!/^[1-9][0-9]*$/.test(value) || Number(value) > MAX_TIMEOUT_MS) throw new RunnerError(`--timeout-ms must be an integer from 1 to ${MAX_TIMEOUT_MS}`);
      opts.timeoutMs = Number(value);
    } else throw new RunnerError(`unknown option ${option}`);
  }
  if (opts.target === null) throw new RunnerError('--target is required');
  if (!SUITES.includes(opts.suite)) throw new RunnerError('--suite must be rqp or legacy');
  if (opts.suite === 'rqp' && opts.profile !== null) throw new RunnerError('--profile is for the legacy suite only');
  if (opts.suite === 'legacy') {
    opts.profile ??= DEFAULT_PROFILE;
    if (!PROFILES.includes(opts.profile)) throw new RunnerError(`--profile must be one of ${PROFILES.join(', ')}`);
  }
  opts.fixtures ??= DEFAULT_DIRS[opts.suite];
  return opts;
}

/** Split an absolute `request.target` into the authority (the Host value) and the path with the query, bytes as written. */
export function splitTarget(target) {
  const m = /^[A-Za-z][A-Za-z0-9+.-]*:\/\/([^/?#@]+)((?:[/?][^#]*)?)$/.exec(target);
  if (m === null) return null;
  const path = m[2] === '' ? '/' : m[2].startsWith('?') ? `/${m[2]}` : m[2];
  return { authority: m[1], path };
}

/** One exchange with the target. */
function send(opts, method, path, authority, fields, body) {
  return exchange({
    host: opts.target.host,
    port: opts.target.port,
    method,
    target: path,
    fields: [['Host', authority], ...fields, ['Connection', 'close']],
    body,
    timeoutMs: opts.timeoutMs,
  });
}

/** GET /info: the suites and profiles of the server. Throws RunnerError. */
async function readInfo(opts) {
  let response;
  try {
    response = await send(opts, 'GET', INFO_PATH, opts.target.authority, []);
  } catch (error) {
    if (error instanceof HttpError && error.code === HttpCode.CONNECT) throw new RunnerError(`the target cannot be reached: ${error.message}`, { cause: error });
    throw new RunnerError(`GET ${INFO_PATH} failed: ${error.message}`, { cause: error });
  }
  if (response.status !== 200) throw new RunnerError(`GET ${INFO_PATH}: status ${response.status}, expected 200`);
  let info;
  try {
    info = JSON.parse(response.body.toString('utf8'));
  } catch (error) {
    throw new RunnerError(`GET ${INFO_PATH}: the body is not valid JSON`, { cause: error });
  }
  const strings = (v) => Array.isArray(v) && v.every((s) => typeof s === 'string');
  if (info === null || typeof info !== 'object' || info.contract !== 1 || !strings(info.suites) || !strings(info.profiles ?? [])) {
    throw new RunnerError(`GET ${INFO_PATH}: not an answer of contract 1`);
  }
  if (!info.suites.includes(opts.suite)) throw new RunnerError(`the server does not support the suite ${opts.suite}`);
  if (opts.suite === 'legacy' && !(info.profiles ?? []).includes(opts.profile)) throw new RunnerError(`the server does not support the profile ${opts.profile}`);
}

/** POST /case: undefined when the case is stored, else the problem text. */
async function postCase(opts, kase) {
  const payload = {
    suite: opts.suite,
    case_id: kase.id,
    ...(opts.suite === 'legacy' ? { profile: opts.profile } : {}),
    page_base64: kase.page.toString('base64'),
    app: kase.app,
    server_config: kase.serverConfig,
  };
  const body = Buffer.from(JSON.stringify(payload), 'utf8');
  if (body.length > MAX_CASE_BYTES) return `fixture: the case is ${body.length} bytes, more than the ${MAX_CASE_BYTES} bytes of POST ${CASE_PATH}`;
  const fields = [
    ['Content-Type', 'application/json'],
    ['Content-Length', String(body.length)],
  ];
  const response = await send(opts, 'POST', CASE_PATH, opts.target.authority, fields, body);
  if (response.status === 204) return undefined;
  const reason = response.status === 422 ? 'the server does not support this case' : 'the server did not store the case';
  return `POST ${CASE_PATH}: status ${response.status} (${reason})`;
}

/** Run one case: { must, should } problem lists. */
async function runCase(opts, kase) {
  try {
    const stored = await postCase(opts, kase);
    if (stored !== undefined) return { must: [stored], should: [] };
    const split = splitTarget(kase.request.target);
    if (split === null) return { must: [`fixture: request target ${JSON.stringify(kase.request.target)} is not an absolute URL with a host`], should: [] };
    const { method, headers } = kase.request;
    const response = await send(opts, method, split.path, split.authority, headers);
    const result = opts.suite === 'rqp' ? compareRqp(kase.expected, kase.page, response) : compareLegacy(kase.expected, kase.page, response);
    if (response.extraBytes > 0) result.must.unshift(`${response.extraBytes} bytes after the end of the response`);
    return result;
  } catch (error) {
    if (error instanceof HttpError) return { must: [error.message], should: [] };
    throw error;
  }
}

/** A result of the report from the problems of a case (a SHOULD-only failure has level "should"). */
function resultOf(kase, { must, should }) {
  if (kase.level === 'should') {
    const all = [...must, ...should];
    return { id: kase.id, level: 'should', status: all.length > 0 ? 'fail' : 'pass', problems: capProblems(all) };
  }
  if (must.length > 0) return { id: kase.id, level: 'must', status: 'fail', problems: capProblems([...must, ...should.map((s) => `[SHOULD] ${s}`)]) };
  if (should.length > 0) return { id: kase.id, level: 'should', status: 'fail', problems: capProblems(should) };
  return { id: kase.id, level: 'must', status: 'pass', problems: [] };
}

/**
 * Run the selected suite against the target. `out(line)` gets each report
 * line. Returns the JSON report (section 4). Throws RunnerError for exit code 2.
 */
export async function run(opts, out) {
  let cases;
  try {
    cases = opts.suite === 'rqp' ? rqpCases({ dir: opts.fixtures, only: opts.only }) : legacyCases({ dir: opts.fixtures, profile: opts.profile, only: opts.only });
  } catch (error) {
    throw new RunnerError(`cannot read the fixture directory ${opts.fixtures} (${error.code ?? error.name})`, { cause: error });
  }
  await readInfo(opts);
  if (cases.length === 0) throw new RunnerError('no case is selected');
  const results = [];
  for (const kase of cases) {
    let result;
    if (kase.skip !== undefined) result = { id: kase.id, level: 'must', status: 'skip', problems: [kase.skip] };
    else if (kase.error !== undefined) result = { id: kase.id, level: 'must', status: 'fail', problems: [`fixture: ${kase.error}`] };
    else result = resultOf(kase, await runCase(opts, kase));
    results.push(result);
    out(formatLine(result));
  }
  const summary = summarize(results);
  out(formatSummary(summary));
  return toJson(opts, results, summary);
}

/** The command line. Returns the exit code. */
export async function main(argv, { stdout = process.stdout, stderr = process.stderr } = {}) {
  let opts;
  try {
    opts = parseArgs(argv);
  } catch (error) {
    if (!(error instanceof RunnerError)) throw error;
    stderr.write(`rivqen-conformance: ${escapeText(error.message)}\n${USAGE}\n`);
    return 2;
  }
  let report;
  try {
    report = await run(opts, (line) => stdout.write(`${escapeText(line)}\n`));
  } catch (error) {
    if (!(error instanceof RunnerError)) throw error;
    stderr.write(`rivqen-conformance: ${escapeText(error.message)}\n`);
    return 2;
  }
  if (opts.json !== null) {
    try {
      writeFileSync(opts.json, `${JSON.stringify(report, null, 2)}\n`);
    } catch (error) {
      stderr.write(`rivqen-conformance: cannot write ${escapeText(opts.json)} (${escapeText(error.code ?? error.name)})\n`);
      return 2;
    }
  }
  return report.summary.failed > 0 ? 1 : 0;
}
