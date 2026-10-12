// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Known-answer test: the example manifest and patches agree with the document in
// spec/rqp/examples/kat/, with the revision algorithm of manifest-patch.md section 1.3,
// with the client verification of section 2.4, and with the Node.js markup candidate.

import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { EXAMPLES, ROOT, contentHash, pageRevision, readJson, sha256b64u, templateRevision } from './helpers.mjs';

const KAT = join(EXAMPLES, 'kat');
const CLI = join(ROOT, 'tools', 'rqp-markup', 'node', 'src', 'cli.mjs');
const kat = readJson(join(KAT, 'kat.json'));
const page = readFileSync(join(KAT, kat.document));
const manifest = readJson(join(KAT, kat.manifest));
const patches = kat.patches.map((p) => ({ name: p, env: readJson(join(KAT, p)) }));

function analyze(bytes) {
  const dir = mkdtempSync(join(tmpdir(), 'rqp-kat-'));
  try {
    const file = join(dir, 'input.html');
    writeFileSync(file, bytes);
    return JSON.parse(execFileSync(process.execPath, [CLI, file], { encoding: 'utf8' }));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

/** The markup candidate result of the KAT page; offsets are UTF-8 byte offsets. */
const base = analyze(page);

/** Template bytes: the document without the content of each block (start and end tags stay). */
function template(bytes, blocks) {
  const parts = [];
  let at = 0;
  for (const b of blocks) {
    parts.push(bytes.subarray(at, b.start));
    at = b.end;
  }
  parts.push(bytes.subarray(at));
  return Buffer.concat(parts);
}

/**
 * Client verification of manifest-patch.md section 2.4, without the markup re-check:
 * returns the new block list, or the name of the first failed check.
 */
function applyToBlocks(current, env) {
  const next = current.map((b) => ({ ...b }));
  let last = -1;
  for (const op of env.operations) {
    const i = next.findIndex((b) => b.id === op.block_id);
    if (i < 0 || i <= last) return 'PATCH_INVALID';
    last = i;
    if (op.op === 'remove_block') {
      if (next[i].format !== 'html') return 'PATCH_INVALID';
      next[i].sha256 = contentHash('');
    } else {
      if (op.format !== next[i].format) return 'PATCH_INVALID';
      if (contentHash(op.content) !== op.sha256) return 'INTEGRITY_ERROR';
      next[i].sha256 = op.sha256;
    }
  }
  if (pageRevision(env.template_revision, next) !== env.next_revision) return 'INTEGRITY_ERROR';
  return next;
}

/** The document after the patch: the template with the new content bytes of each patched block. */
function splice(bytes, blocks, env) {
  const content = new Map(env.operations.map((op) => [op.block_id, op.op === 'remove_block' ? '' : op.content]));
  const parts = [];
  let at = 0;
  for (const b of blocks) {
    parts.push(bytes.subarray(at, b.start));
    parts.push(content.has(b.id) ? Buffer.from(content.get(b.id), 'utf8') : bytes.subarray(b.start, b.end));
    at = b.end;
  }
  parts.push(bytes.subarray(at));
  return Buffer.concat(parts);
}

test('the KAT document is valid for RQP and the manifest lists its blocks in document order', () => {
  assert.equal(base.valid, true);
  assert.deepEqual(manifest.blocks, base.blocks.map(({ id, format, sha256 }) => ({ id, format, sha256 })));
});

test('each manifest block hash is SHA-256 of the exact content bytes', () => {
  for (const b of base.blocks) assert.equal(b.sha256, sha256b64u(page.subarray(b.start, b.end)), b.id);
});

test('template_revision is t1. + B64U(SHA-256("rqp-t1\\n" || template))', () => {
  assert.equal(templateRevision(template(page, base.blocks)), manifest.template_revision);
  assert.equal(base.template_revision, manifest.template_revision);
});

test('page_revision is r1. + B64U(SHA-256(P)) over the template revision and the block lines', () => {
  assert.equal(pageRevision(manifest.template_revision, manifest.blocks), manifest.page_revision);
  assert.equal(base.page_revision, manifest.page_revision);
});

test('the exact page_revision preimage of the KAT document', () => {
  const p =
    'rqp-r1\n' + manifest.template_revision + '\n' +
    'title\thtml\t0Aqua3-_d6RWbY3zaAXXrpc4lNqfUz7JZRazHgKLces\n' +
    'price\thtml\tXg3oaiol3ywUdxTQZNp7JJJu7_R8kUL-HK4accfrDKs\n' +
    'cart\tjson\tzyEL3KkZlLmAgTTFnonr3pdX-DnDM81i9xFoREUEaX4\n';
  assert.equal('r1.' + sha256b64u(Buffer.from(p, 'utf8')), manifest.page_revision);
});

for (const { name, env } of patches) {
  test(`${name}: base and template revisions are those of the manifest`, () => {
    assert.equal(env.base_revision, manifest.page_revision);
    assert.equal(env.template_revision, manifest.template_revision);
  });

  test(`${name}: the client recomputes next_revision from its blocks and the operations`, () => {
    const next = applyToBlocks(manifest.blocks, env);
    assert.ok(Array.isArray(next), String(next));
  });

  test(`${name}: next_revision is the page revision of the patched document (markup candidate)`, () => {
    const after = analyze(splice(page, base.blocks, env));
    assert.equal(after.valid, true);
    assert.equal(after.template_revision, manifest.template_revision);
    assert.equal(after.page_revision, env.next_revision);
  });
}

test('a server that omits a changed block (digest prefix collision) fails the next_revision check', () => {
  const env = structuredClone(patches.find((p) => p.env.operations.length > 1).env);
  env.operations = env.operations.filter((op) => op.block_id !== 'cart');
  assert.equal(applyToBlocks(manifest.blocks, env), 'INTEGRITY_ERROR');
});

test('a content that does not match its operation hash fails with INTEGRITY_ERROR', () => {
  const env = structuredClone(patches[0].env);
  env.operations[0].content = '131.00';
  assert.equal(applyToBlocks(manifest.blocks, env), 'INTEGRITY_ERROR');
});

test('a client whose unchanged block differs from the server copy fails the next_revision check', () => {
  const stale = manifest.blocks.map((b) => (b.id === 'title' ? { ...b, sha256: contentHash('Shop!') } : b));
  assert.equal(applyToBlocks(stale, patches[0].env), 'INTEGRITY_ERROR');
});

test('remove_block on a json block is rejected', () => {
  const env = { ...structuredClone(patches[0].env), operations: [{ op: 'remove_block', block_id: 'cart' }] };
  assert.equal(applyToBlocks(manifest.blocks, env), 'PATCH_INVALID');
});

test('operations out of document order are rejected', () => {
  const env = structuredClone(patches.find((p) => p.env.operations.length > 1).env);
  env.operations.reverse();
  assert.equal(applyToBlocks(manifest.blocks, env), 'PATCH_INVALID');
});
