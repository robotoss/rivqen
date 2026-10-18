#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Conformance harness of the rqp/1 protocol fixtures (fixtures/rqp/protocol).
//
//   node tools/rqp-protocol/check.mjs                       both reference steps
//   node tools/rqp-protocol/check.mjs --server <module.mjs> a server under test
//   node tools/rqp-protocol/check.mjs --client <module.mjs> a client under test
//   options: --only <prefix> (fixture name prefix), --fixtures <dir>
//
// Server module: export handle(request, appResponse, pageBytes, config) ->
// { status, headers, body } (or a Promise of it). request = { method, target,
// headers: [[name, value]] }, appResponse = { status, headers }, pageBytes =
// Uint8Array, config = { policy?, limits? } (the manifest members of the server
// configuration). headers: [[name, value]] lines, a plain object or a Headers
// object; body: Uint8Array, string or null.
//
// Client module: export clientStep(context, response) -> { outcome, error,
// next, metric, snapshot } and optionally clientRequest(context) -> [[name,
// value]]. context = { url, final_url, redirected, build, allow_http,
// allowlist, credentials, response_fields_visible, public_origin, navigation,
// snapshot: null | { document, template_revision, page_revision, blocks,
// policy?, limits?, partition } }. url is the request URL and the cache key of
// the snapshot; final_url is the URL of the response after redirects (a client
// parses both with the WHATWG URL parser, DL-040); redirected tells whether the
// response came through a redirect, also one to the same URL (H-67); allowlist
// is the origin allowlist as serialized origins (exact match, DL-038; every
// entry is valid, H-69);
// credentials is true, false or null (unknown); response_fields_visible tells
// whether the native SDK saw all header fields of the response (H-64);
// partition is the partition of the snapshot, "request" or "public" (H-65).
//
// Comparison rules: docs/engineering/protocol/fixtures.md section 4.1
// ("Comparison of F") and section 5; README.md of the fixtures; and RQP-08
// (no Rq-* field and no Accept in Vary of F and of a 304).
// Exit codes: 0 every fixture passes, 1 a fixture fails, 2 a usage error.

import { existsSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { isDeepStrictEqual } from 'node:util';
import { MANIFEST_END, MANIFEST_START, sha256b64u } from './src/common.mjs';
import { FIXTURES_DIR, fixtureNames, readJsonFile } from './src/fixtures.mjs';
import { acceptsMediaType, combined, has, normalizeHeaders } from './src/http.mjs';
import { parseStrict } from './src/json-strict.mjs';
import { pageBytes } from './src/pages.mjs';
import { loadSchema, validate } from './src/schema.mjs';

const PATCH_SCHEMA = loadSchema('patch');
const START = Buffer.from(MANIFEST_START, 'latin1');
const END = Buffer.from(MANIFEST_END, 'latin1');

function toBuffer(body) {
  if (body === null || body === undefined) return null;
  if (typeof body === 'string') return Buffer.from(body, 'utf8');
  return Buffer.from(body.buffer, body.byteOffset, body.byteLength);
}

/** Compare the body of a full document F (fixtures.md section 4.1, "Comparison of F"). */
function compareFull(expected, page, body, fail) {
  const at = expected.insert_at;
  if (!body.subarray(0, at).equals(page.subarray(0, at))) return fail('body: the bytes before the manifest element differ from the page');
  if (!body.subarray(at, at + START.length).equals(START)) return fail('body: no manifest start tag immediately before the head end tag of the document');
  const textStart = at + START.length;
  const lt = body.indexOf(0x3c, textStart);
  if (lt < 0 || !body.subarray(lt, lt + END.length).equals(END)) return fail('body: the manifest text has "<" or no </script>');
  if (!body.subarray(lt + END.length).equals(page.subarray(at))) return fail('body: the bytes after the manifest element differ from the page');
  const json = parseStrict(body.subarray(textStart, lt));
  if (!json.ok) return fail(`body: the manifest is not a valid JSON text (${json.rule})`);
  if (!isDeepStrictEqual(json.value, expected.manifest)) return fail('body: the manifest value differs');
  return undefined;
}

/** Compare the body of a delta D (README.md: envelope fields and the ordered changed blocks). */
function compareDelta(expected, body, fail) {
  const json = parseStrict(body);
  if (!json.ok) return fail(`body: the delta is not a valid JSON text (${json.rule})`);
  const errors = validate(PATCH_SCHEMA, json.value);
  if (errors.length > 0) return fail(`body: the delta fails the schema (${errors[0].keyword} at "${errors[0].instancePath}")`);
  const env = json.value;
  for (const f of ['base_revision', 'next_revision', 'template_revision']) {
    if (env[f] !== expected[f]) fail(`body: ${f} ${env[f]}, expected ${expected[f]}`);
  }
  const got = env.operations.map((op) => op.block_id);
  const want = expected.blocks.map((b) => b.id);
  if (got.join(',') !== want.join(',')) return fail(`body: changed blocks [${got}], expected [${want}]`);
  env.operations.forEach((op, i) => {
    if (op.format !== expected.blocks[i].format) fail(`body: block ${op.block_id} format ${op.format}`);
    if (op.sha256 !== expected.blocks[i].sha256) fail(`body: block ${op.block_id} sha256 is not the hash of the current content`);
    if (sha256b64u(Buffer.from(op.content, 'utf8')) !== op.sha256) fail(`body: block ${op.block_id} content does not match its sha256`);
  });
  return undefined;
}

/**
 * Compare a server response with expected.json. Returns a list of failures
 * (empty when the response conforms).
 */
export function compareServer(expected, page, response) {
  const failures = [];
  const fail = (m) => {
    failures.push(m);
  };
  const headers = normalizeHeaders(response.headers);
  const body = toBuffer(response.body);
  if (response.status !== expected.status) fail(`status ${response.status}, expected ${expected.status}`);
  for (const [name, value] of Object.entries(expected.headers_required)) {
    const got = combined(headers, name);
    if (got !== value) fail(`header ${name}: ${got === null ? 'absent' : JSON.stringify(got)}, expected ${JSON.stringify(value)}`);
  }
  for (const name of expected.headers_forbidden) if (has(headers, name)) fail(`header ${name}: forbidden`);
  // DL-036 (I2): an optional field, when present, is exactly one line with exactly the expected value.
  for (const [name, value] of Object.entries(expected.headers_optional ?? {})) {
    const lines = headers.filter(([n]) => n === name);
    if (lines.length > 1) fail(`header ${name}: ${lines.length} field lines, expected at most one`);
    else if (lines.length === 1 && combined(lines, name) !== value) fail(`header ${name}: ${JSON.stringify(combined(lines, name))}, expected ${JSON.stringify(value)} or absent`);
  }
  const kind = expected.body.kind;
  if (kind === 'full' || expected.status === 304 || (kind === 'none' && expected.status === 200)) {
    const vary = combined(headers, 'vary');
    if (vary !== null) {
      for (const member of vary.split(',').map((v) => v.trim().toLowerCase())) {
        if (member.startsWith('rq-') || member === 'accept') fail(`header vary: lists ${member} (RQP-08)`);
      }
    }
  }
  const length = combined(headers, 'content-length');
  if (length !== null && body !== null && kind !== 'none' && Number(length) !== body.length) {
    fail(`header content-length ${length}, body has ${body.length} bytes`);
  }
  if (kind === 'none') {
    if (body !== null && body.length > 0) fail(`body: ${body.length} bytes, expected none`);
  } else if (body === null) {
    fail(`body: none, expected ${kind}`);
  } else if (kind === 'app') {
    if (!body.equals(page)) fail('body: not the application response body');
  } else if (kind === 'full') {
    compareFull(expected.body, page, body, fail);
  } else if (kind === 'delta') {
    compareDelta(expected.body, body, fail);
  }
  return failures;
}

/** Remove block content that expected.json does not list (content over 4 KiB). */
function sameSnapshot(expected, got) {
  if (expected === null || got === null || got === undefined) return expected === (got ?? null);
  const trimmed = {
    ...got,
    blocks: (got.blocks ?? []).map((b, i) => {
      const want = expected.blocks[i];
      if (want === undefined || Object.hasOwn(want, 'content')) return b;
      const { content, ...rest } = b;
      return rest;
    }),
  };
  return isDeepStrictEqual(trimmed, expected);
}

const RQP_REQUEST_FIELDS = ['rq-version', 'rq-capabilities', 'if-none-match', 'rq-have'];

/** Compare a client result (and its request fields) with expected.json. Returns a list of failures. */
export function compareClient(expected, result, request) {
  const failures = [];
  const want = expected.result;
  for (const f of ['outcome', 'error', 'next', 'metric']) {
    if (result[f] !== want[f]) failures.push(`${f} ${JSON.stringify(result[f])}, expected ${JSON.stringify(want[f])}`);
  }
  if (!sameSnapshot(want.snapshot, result.snapshot ?? null)) failures.push('snapshot differs');
  if (request !== undefined) {
    const got = normalizeHeaders(request);
    const exp = expected.request.headers;
    for (const name of RQP_REQUEST_FIELDS) {
      if (combined(got, name) !== combined(exp, name)) failures.push(`request ${name}: ${combined(got, name) ?? 'absent'}, expected ${combined(exp, name) ?? 'absent'}`);
    }
    for (const [name] of got) {
      if (name.startsWith('rq-') && !RQP_REQUEST_FIELDS.includes(name)) failures.push(`request ${name}: not an rqp/1 field`);
    }
    const accepts = (h) => acceptsMediaType(combined(h, 'accept'), 'application/vnd.rivqen.delta+json');
    if (accepts(got) !== accepts(exp)) failures.push('request accept: the delta media type differs');
  }
  return failures;
}

/** Run the server fixtures. Returns [{ name, failures }]. */
export async function runServer(mod, { dir = FIXTURES_DIR, only = '' } = {}) {
  const out = [];
  for (const name of fixtureNames(dir)) {
    if (!name.startsWith(only)) continue;
    const c = readJsonFile(join(dir, name, 'case.json'));
    if (c.kind === 'client') continue;
    const expected = readJsonFile(join(dir, name, 'expected.json'));
    const page = pageBytes(c.page);
    let failures;
    try {
      const response = await mod.handle(expected.request, structuredClone(c.app), Uint8Array.from(page), expected.server_config ?? {});
      failures = compareServer(expected, page, response);
    } catch (error) {
      failures = [`the server threw ${error instanceof Error ? error.name : typeof error}`];
    }
    out.push({ name, failures, level: expected.level ?? 'must' });
  }
  return out;
}

/** Run the client fixtures. Returns [{ name, failures }]. */
export async function runClient(mod, { dir = FIXTURES_DIR, only = '' } = {}) {
  const out = [];
  for (const name of fixtureNames(dir)) {
    if (!name.startsWith(only)) continue;
    const expected = readJsonFile(join(dir, name, 'expected.json'));
    if (expected.kind !== 'client') continue;
    const ctx = expected.context;
    const snapshot = ctx.snapshot === null ? null : { ...ctx.snapshot, document: Uint8Array.from(pageBytes(ctx.snapshot.page)) };
    const context = {
      url: ctx.url,
      final_url: ctx.final_url,
      redirected: ctx.redirected,
      build: ctx.build,
      allow_http: ctx.allow_http,
      allowlist: ctx.allowlist,
      credentials: ctx.credentials,
      response_fields_visible: ctx.response_fields_visible,
      public_origin: ctx.public_origin,
      navigation: ctx.navigation,
      snapshot,
    };
    const bodyPath = join(dir, name, 'response.body');
    let failures;
    try {
      if (expected.response.body !== null && !existsSync(bodyPath)) {
        out.push({ name, failures: ['response.body is missing'] });
        continue;
      }
      const response = {
        status: expected.response.status,
        headers: expected.response.headers,
        body: expected.response.body === null ? null : Uint8Array.from(readFileSync(bodyPath)),
      };
      const request = typeof mod.clientRequest === 'function' ? await mod.clientRequest(context) : undefined;
      failures = compareClient(expected, await mod.clientStep(context, response), request);
    } catch (error) {
      failures = [`the client threw ${error instanceof Error ? error.name : typeof error}`];
    }
    out.push({ name, failures });
  }
  return out;
}

async function load(path, exportName) {
  const mod = await import(pathToFileURL(resolve(path)).href);
  if (typeof mod[exportName] !== 'function') throw new Error(`${path} does not export ${exportName}()`);
  return mod;
}

function parseArgs(argv) {
  const opts = { server: null, client: null, only: '', dir: FIXTURES_DIR };
  for (let i = 0; i < argv.length; i += 2) {
    const value = argv[i + 1];
    if (value === undefined) return null;
    if (argv[i] === '--server') opts.server = value;
    else if (argv[i] === '--client') opts.client = value;
    else if (argv[i] === '--only') opts.only = value;
    else if (argv[i] === '--fixtures') opts.dir = resolve(value) + '/';
    else return null;
  }
  if (opts.server === null && opts.client === null) {
    opts.server = fileURLToPath(new URL('./src/server.mjs', import.meta.url));
    opts.client = fileURLToPath(new URL('./src/client.mjs', import.meta.url));
  }
  return opts;
}

async function main(argv) {
  const opts = parseArgs(argv);
  if (opts === null) {
    process.stderr.write('usage: node check.mjs [--server <module>] [--client <module>] [--only <prefix>] [--fixtures <dir>]\n');
    return 2;
  }
  const results = [];
  try {
    if (opts.server !== null) results.push(...(await runServer(await load(opts.server, 'handle'), opts)));
    if (opts.client !== null) results.push(...(await runClient(await load(opts.client, 'clientStep'), opts)));
  } catch (error) {
    process.stderr.write(`rqp-protocol check: ${error.message}\n`);
    return 2;
  }
  // A fixture with level "should" checks a SHOULD rule. Its failure is counted
  // and labelled apart from the failures of MUST rules.
  let failed = 0;
  let shouldFailed = 0;
  for (const { name, failures, level } of results) {
    if (failures.length === 0) continue;
    const should = level === 'should';
    if (should) shouldFailed++;
    else failed++;
    for (const f of failures) process.stdout.write(`FAIL${should ? ' [SHOULD]' : ''} ${name}: ${f}\n`);
  }
  const passed = results.length - failed - shouldFailed;
  process.stdout.write(`rqp-protocol check: ${passed} passed, ${failed} failed, ${shouldFailed} SHOULD failed\n`);
  return failed + shouldFailed === 0 ? 0 : 1;
}

if (process.argv[1] !== undefined && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  process.exitCode = await main(process.argv.slice(2));
}
