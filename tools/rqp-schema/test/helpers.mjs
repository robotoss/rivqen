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

/** page_revision of rqp/1 (manifest-patch.md section 1.3): blocks = [{ id, format, sha256 }] in document order. */
export function pageRevision(templateRev, blocks) {
  let p = 'rqp-r1\n' + templateRev + '\n';
  for (const b of blocks) p += `${b.id}\t${b.format}\t${b.sha256}\n`;
  return 'r1.' + sha256b64u(Buffer.from(p, 'utf8'));
}
