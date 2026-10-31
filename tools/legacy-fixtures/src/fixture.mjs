// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Fixture model of fixtures/legacy (README.md there): reading and checking case.json,
// the placeholders, and the expected.json that gen.mjs writes. Every expected value
// comes from the model (src/model.mjs).

import { readFileSync } from 'node:fs';
import { isUtf8 } from 'node:buffer';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { analyze, rebuild, sha1hex, split } from './model.mjs';
import { asciiLower } from './fields.mjs';

export const REPO_ROOT = fileURLToPath(new URL('../../../', import.meta.url));
export const FIXTURES_DIR = join(REPO_ROOT, 'fixtures/legacy');
export const DEFAULT_TARGET = 'https://shop.example.com/catalog';
export const DEFAULT_APP = Object.freeze({ status: 200, headers: [['content-type', 'text/html; charset=utf-8']] });
export const UPSTREAM = Object.freeze({ repo: 'Tencent/VasSonic', sha: '59936beff656d4b5718ff6444d6c5e001a2c5231' });

/** The profile of each fixture name suffix. */
export const SUFFIX_PROFILE = Object.freeze({ JAVA: 'legacy-java', NODE: 'legacy-node', PHP: 'legacy-php' });
export const PROFILE_SUFFIX = Object.freeze({ 'legacy-java': 'JAVA', 'legacy-node': 'NODE', 'legacy-php': 'PHP' });
export const UPSTREAM_PROFILES = Object.freeze(['legacy-java', 'legacy-node', 'legacy-php']);

const NAME = /^FX-LEG-[A-Z0-9]+(-[A-Z0-9]+)*(--(JAVA|NODE|PHP))?$/;
const PAGE_ID = /^[a-z0-9_-]+$/;

/** A defect in a case.json file. */
export class CaseError extends Error {
  constructor(id, message, options) {
    super(`${id}: ${message}`, options);
    this.name = 'CaseError';
    this.code = 'LEGACY_FIXTURE_CASE';
  }
}

function isObject(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function onlyKeys(id, object, allowed, required, where) {
  if (!isObject(object)) throw new CaseError(id, `${where} must be an object`);
  for (const key of Object.keys(object)) if (!allowed.includes(key)) throw new CaseError(id, `unknown member ${where}.${key}`);
  for (const key of required) if (!Object.hasOwn(object, key)) throw new CaseError(id, `missing member ${where}.${key}`);
}

function fieldList(id, list, where) {
  if (!Array.isArray(list)) throw new CaseError(id, `${where} must be an array`);
  for (const entry of list) {
    if (!Array.isArray(entry) || entry.length !== 2 || typeof entry[0] !== 'string' || typeof entry[1] !== 'string') {
      throw new CaseError(id, `${where} must hold [name, value] pairs of strings`);
    }
  }
  return list;
}

function stringList(id, list, pattern, where) {
  if (!Array.isArray(list) || list.length === 0 || !list.every((item) => typeof item === 'string' && pattern.test(item))) {
    throw new CaseError(id, `${where} must be a non-empty list of strings matching ${pattern}`);
  }
  return list;
}

/** Whether a fixture name is a profile fixture; returns the profile or null. */
export function profileOfName(name) {
  const at = name.indexOf('--');
  return at < 0 ? null : SUFFIX_PROFILE[name.slice(at + 2)] ?? null;
}

/**
 * Read and check a case.json. Returns the case with `kind` ('canonical' or 'profile')
 * and, for a canonical case, the defaults filled in.
 */
export function loadCase(dir, name) {
  if (!NAME.test(name)) throw new CaseError(name, 'the directory name is not a fixture ID');
  let c;
  try {
    c = JSON.parse(readFileSync(join(dir, name, 'case.json'), 'utf8'));
  } catch (error) {
    throw new CaseError(name, `case.json cannot be read: ${error.message}`);
  }
  if (!isObject(c)) throw new CaseError(name, 'case.json must be an object');
  if (c.id !== name) throw new CaseError(name, `id must be ${name}`);
  if (typeof c.description !== 'string' || c.description === '') throw new CaseError(name, 'description must be a non-empty string');
  stringList(name, c.sources, /^(upstream|docs):\S+$/, 'sources');
  const profile = profileOfName(name);
  if (profile === null) return canonicalCase(name, c);
  return profileCase(name, c, profile);
}

function canonicalCase(id, c) {
  onlyKeys(id, c, ['id', 'description', 'profile', 'rules', 'upstream', 'sources', 'evidence', 'traces', 'page', 'app', 'request', 'level'],
    ['id', 'description', 'profile', 'rules', 'upstream', 'sources', 'evidence', 'page', 'request'], 'case');
  if (c.profile !== 'legacy-canonical') throw new CaseError(id, 'profile must be legacy-canonical');
  stringList(id, c.rules, /^C-\d\d$/, 'rules');
  onlyKeys(id, c.upstream, ['repo', 'sha'], ['repo', 'sha'], 'upstream');
  if (c.upstream.repo !== UPSTREAM.repo || c.upstream.sha !== UPSTREAM.sha) throw new CaseError(id, 'upstream must be the pinned repository and SHA');
  if (c.evidence !== 'trace' && c.evidence !== 'code-reading') throw new CaseError(id, 'evidence must be trace or code-reading');
  if (c.evidence === 'trace') {
    onlyKeys(id, c.traces, UPSTREAM_PROFILES, UPSTREAM_PROFILES, 'traces');
    for (const p of UPSTREAM_PROFILES) if (typeof c.traces[p] !== 'string') throw new CaseError(id, `traces.${p} must be a string`);
  } else if (Object.hasOwn(c, 'traces')) {
    throw new CaseError(id, 'traces is only for evidence trace');
  }
  if (typeof c.page !== 'string' || !PAGE_ID.test(c.page)) throw new CaseError(id, 'page must be a page ID');
  const app = Object.hasOwn(c, 'app') ? c.app : DEFAULT_APP;
  onlyKeys(id, app, ['status', 'headers'], ['status', 'headers'], 'app');
  if (!Number.isInteger(app.status)) throw new CaseError(id, 'app.status must be an integer');
  fieldList(id, app.headers, 'app.headers');
  onlyKeys(id, c.request, ['method', 'target', 'headers'], ['headers'], 'request');
  fieldList(id, c.request.headers, 'request.headers');
  const method = c.request.method ?? 'GET';
  const target = c.request.target ?? DEFAULT_TARGET;
  if (typeof method !== 'string' || typeof target !== 'string') throw new CaseError(id, 'request.method and request.target must be strings');
  const level = c.level ?? 'must';
  if (level !== 'must' && level !== 'should') throw new CaseError(id, 'level must be must or should');
  return { ...c, kind: 'canonical', app, level, request: { method, target, headers: c.request.headers } };
}

function profileCase(id, c, profile) {
  onlyKeys(id, c, ['id', 'description', 'profile', 'replaces', 'divergences', 'sources', 'trace', 'not_reproduced'],
    ['id', 'description', 'profile', 'replaces', 'divergences', 'sources', 'trace'], 'case');
  if (c.profile !== profile) throw new CaseError(id, `profile must be ${profile}`);
  if (c.replaces !== id.slice(0, id.indexOf('--'))) throw new CaseError(id, 'replaces must be the ID without the suffix');
  if (!Array.isArray(c.divergences) || !c.divergences.every((d) => typeof d === 'string' && /^D-\d\d$/.test(d))) {
    throw new CaseError(id, 'divergences must be a list of D-nn');
  }
  if (typeof c.trace !== 'string') throw new CaseError(id, 'trace must be a string');
  if (Object.hasOwn(c, 'not_reproduced') && !(typeof c.not_reproduced === 'string' && /^U-\d\d$/.test(c.not_reproduced))) {
    throw new CaseError(id, 'not_reproduced must be a U-nn');
  }
  return { ...c, kind: 'profile' };
}

/** The bytes of a page. Throws CaseError when the file is missing. */
export function pageFile(id, pagesDir, page) {
  try {
    return readFileSync(join(pagesDir, `${page}.html`));
  } catch (error) {
    const why = error.code === 'ENOENT' ? 'is missing' : `cannot be read (${error.code ?? error.name})`;
    throw new CaseError(id, `page ${page} ${why}`, { cause: error });
  }
}

/** The template-tag of a page under a profile; '' when the profile sends none. */
export function tagOf(profile, bytes) {
  if (!isUtf8(bytes)) return '';
  const parts = split(profile, bytes.toString('utf8'));
  return parts.ok ? sha1hex(parts.template) : '';
}

/** Replace the placeholders of the README in a value under a profile. */
export function resolve(id, text, profile, pagesDir) {
  const out = text.replace(/\{\{(etag|ETAG|tag|TAG):([a-z0-9_-]+)\}\}/g, (_, kind, page) => {
    const bytes = pageFile(id, pagesDir, page);
    const value = asciiLower(kind) === 'etag' ? sha1hex(bytes) : tagOf(profile, bytes);
    return kind === kind.toUpperCase() ? value.toUpperCase() : value;
  });
  if (/\{\{|\}\}/.test(out)) throw new CaseError(id, `unknown placeholder in ${JSON.stringify(text)}`);
  return out;
}

/** The request of a canonical case with the placeholders resolved under a profile. */
export function resolvedRequest(c, profile, pagesDir) {
  return {
    method: c.request.method,
    target: c.request.target,
    headers: c.request.headers.map(([n, v]) => [n, resolve(c.id, v, profile, pagesDir)]),
  };
}

// Always listed in `fields` (`[]` = absent), so the runner checks them on every fixture:
// vary (C-15) and the CDN cache fields (C-13, H-111) as well as the legacy fields.
const FIXED_FIELDS = [
  'etag', 'template-tag', 'template-change', 'cache-offline', 'cache-control', 'content-type', 'sonic-etag-key',
  'vary', 'cdn-cache-control', 'surrogate-control',
];

function fieldsOf(headers) {
  const fields = Object.create(null);
  for (const name of FIXED_FIELDS) fields[name] = [];
  for (const [name, value] of headers) {
    const key = asciiLower(name);
    if (key === 'content-length') continue;
    if (!Object.hasOwn(fields, key)) fields[key] = [];
    fields[key].push(value);
  }
  return fields;
}

/**
 * Run the model for a canonical case under a profile and build the expected object
 * (without profiles_same and trace). `pageBytes` are the bytes of the page.
 */
export function expectedOf(c, profile, pageBytes, pagesDir) {
  const request = resolvedRequest(c, profile, pagesDir);
  const { response, page, kind } = analyze(profile, request, c.app, pageBytes);
  let body;
  if (response.body === null) {
    body = { kind: 'none' };
  } else if (kind === 'data') {
    body = { kind: 'data', data: Object.fromEntries(page.data), template_tag: page.tag, html_sha1: page.etag, diff: '' };
  } else {
    body = { kind: 'page' };
  }
  const splitInfo = page === null
    ? { applied: false, template_sha1: null, keys: [], rebuild_sha1: null }
    : { applied: true, template_sha1: page.tag, keys: [...page.data.keys()], rebuild_sha1: sha1hex(rebuild(page.template, page.data)) };
  return {
    id: c.id,
    profile,
    level: c.level,
    request,
    status: response.status,
    fields: fieldsOf(response.headers),
    body,
    split: splitInfo,
  };
}
