// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Shared helpers for the rqp/1 schema tests.

import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import Ajv2020 from 'ajv/dist/2020.js';

export const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
export const SCHEMAS = join(ROOT, 'spec', 'rqp', 'schemas');
export const EXAMPLES = join(ROOT, 'spec', 'rqp', 'examples');
export const KINDS = ['manifest', 'patch'];

export function readJson(path) {
  return JSON.parse(readFileSync(path, 'utf8'));
}

export function schemaPath(kind) {
  return join(SCHEMAS, `${kind}.schema.json`);
}

/** Ajv for Draft 2020-12, strict: unknown keywords and type mistakes in a schema are errors. */
export function newAjv() {
  return new Ajv2020({ strict: true, allErrors: true });
}

/** Compiled validators for both schemas, keyed by kind. */
export function validators() {
  const ajv = newAjv();
  return Object.fromEntries(KINDS.map((k) => [k, ajv.compile(readJson(schemaPath(k)))]));
}

/** JSON files of one example directory, sorted by name: [{ name, path }]. */
export function exampleFiles(validity, kind) {
  const dir = join(EXAMPLES, validity, kind);
  return readdirSync(dir)
    .filter((f) => f.endsWith('.json'))
    .sort()
    .map((f) => ({ name: f, path: join(dir, f) }));
}

/** base64url without padding of SHA-256 over bytes. */
export function sha256b64u(bytes) {
  return createHash('sha256').update(bytes).digest('base64url');
}

/** Block hash of an operation content: SHA-256 over the UTF-8 bytes of the JSON string value. */
export function contentHash(content) {
  return sha256b64u(Buffer.from(content, 'utf8'));
}

/** template_revision of rqp/1 (manifest-patch.md section 1.3). */
export function templateRevision(templateBytes) {
  return 't1.' + sha256b64u(Buffer.concat([Buffer.from('rqp-t1\n', 'utf8'), templateBytes]));
}

/** Decimal digits of a non-negative integer, no sign, no leading zero (manifest-patch.md section 1.3, step 4). */
function decimal(name, n) {
  if (!Number.isSafeInteger(n) || n < 0) throw new TypeError(`${name} is not a non-negative integer`);
  return String(n);
}

/**
 * Text S of the policy line (manifest-patch.md section 1.3, step 4; H-55): the members of
 * `policy` and `limits` that the manifest has, in a fixed order. A missing member writes
 * nothing; a present object writes its name also when it is empty. Returns '' when the
 * manifest has neither `policy` nor `limits` (then the preimage has no policy line).
 * `m` is the parsed manifest (or any object with the optional members `policy` and `limits`).
 */
export function policyLimitsText(m) {
  const has = (o, k) => Object.hasOwn(o, k);
  const items = [];
  if (has(m, 'policy')) {
    const p = m.policy;
    items.push('policy');
    if (has(p, 'mode')) {
      if (p.mode !== 'private' && p.mode !== 'public') throw new TypeError('mode is not private or public');
      items.push(`mode=${p.mode}`);
    }
    if (has(p, 'ttl_seconds')) items.push(`ttl_seconds=${decimal('ttl_seconds', p.ttl_seconds)}`);
    if (has(p, 'offline')) {
      if (typeof p.offline !== 'boolean') throw new TypeError('offline is not a boolean');
      items.push(`offline=${p.offline}`);
    }
  }
  if (has(m, 'limits')) {
    items.push('limits');
    if (has(m.limits, 'max_patch_bytes')) items.push(`max_patch_bytes=${decimal('max_patch_bytes', m.limits.max_patch_bytes)}`);
  }
  return items.join(';');
}

/** The page_revision preimage P of rqp/1 (manifest-patch.md section 1.3, step 4) as a string. */
export function pageRevisionPreimage(templateRev, blocks, m = {}) {
  const s = policyLimitsText(m);
  let p = 'rqp-r1\n' + templateRev + '\n';
  if (s !== '') p += `PL\t${s}\n`;
  for (const b of blocks) p += `${b.id}\t${b.format}\t${b.sha256}\n`;
  return p;
}

/**
 * page_revision of rqp/1 (manifest-patch.md section 1.3): blocks = [{ id, format, sha256 }] in
 * document order; `m` holds the optional `policy` and `limits` of the manifest (H-55).
 */
export function pageRevision(templateRev, blocks, m = {}) {
  return 'r1.' + sha256b64u(Buffer.from(pageRevisionPreimage(templateRev, blocks, m), 'utf8'));
}
