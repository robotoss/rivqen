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
 * Client verification of manifest-patch.md section 2.4, steps 3 to 7 (block list and
 * next_revision): returns the new block list, or the code of the first failed check.
 * The schema allows only replace_block (H-47). `snap` is the snapshot manifest: its
 * policy and limits give the policy line of the preimage (H-55).
 */
function applyToBlocks(current, env, snap = manifest) {
  const next = current.map((b) => ({ ...b }));
  let last = -1;
  for (const op of env.operations) {
    const i = next.findIndex((b) => b.id === op.block_id);
    if (i < 0 || i <= last) return 'PATCH_INVALID';
    last = i;
    if (op.format !== next[i].format) return 'PATCH_INVALID';
    if (contentHash(op.content) !== op.sha256) return 'INTEGRITY_ERROR';
    next[i].sha256 = op.sha256;
  }
  if (pageRevision(env.template_revision, next, snap) !== env.next_revision) return 'INTEGRITY_ERROR';
  return next;
}

/**
 * Client check of section 2.4, step 8 (H-48): the markup analysis of the patched document.
 * Returns null when the patched document is valid, has the same template revision and the
 * same blocks (id, format, order), and its page revision is next_revision; else the code.
 * The analysis has no manifest input, so the page revision is computed again with the
 * policy line of the snapshot manifest `snap` (H-55).
 */
function checkPatchedDocument(bytes, blocks, env, snap = manifest) {
  const after = analyze(splice(bytes, blocks, env));
  if (!after.valid) return 'PATCH_CONTENT_REJECTED';
  if (after.template_revision !== env.template_revision) return 'PATCH_CONTENT_REJECTED';
  const shape = (list) => list.map(({ id, format }) => `${id}\t${format}`).join('\n');
  if (shape(after.blocks) !== shape(blocks)) return 'PATCH_CONTENT_REJECTED';
  if (pageRevision(after.template_revision, after.blocks, snap) !== env.next_revision) return 'INTEGRITY_ERROR';
  return null;
}

/**
 * Client check of section 1.8 (H-48): the markup analysis of the original document
 * gives the blocks and the template revision of the manifest, and these with the policy
 * line of the manifest give its page revision (H-55). Returns null or MANIFEST_INVALID.
 */
function checkManifest(original, m) {
  const r = analyze(original);
  if (!r.valid) return 'MANIFEST_INVALID';
  const fields = ({ id, format, sha256 }) => ({ id, format, sha256 });
  const same =
    JSON.stringify(r.blocks.map(fields)) === JSON.stringify(m.blocks.map(fields)) &&
    r.template_revision === m.template_revision &&
    pageRevision(r.template_revision, r.blocks, m) === m.page_revision;
  return same ? null : 'MANIFEST_INVALID';
}

/** The patch for the KAT page that sets one block, with a consistent next_revision. */
function patchOne(id, content) {
  const blocks = manifest.blocks.map((b) => (b.id === id ? { ...b, sha256: contentHash(content) } : b));
  const format = manifest.blocks.find((b) => b.id === id).format;
  return {
    ...structuredClone(patches[0].env),
    next_revision: pageRevision(manifest.template_revision, blocks, manifest),
    operations: [{ op: 'replace_block', block_id: id, format, content, sha256: contentHash(content) }],
  };
}

/** The document after the patch: the template with the new content bytes of each patched block. */
function splice(bytes, blocks, env) {
  const content = new Map(env.operations.map((op) => [op.block_id, op.content]));
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

  test(`${name}: the patched document passes the markup analysis and has next_revision (H-48)`, () => {
    assert.equal(checkPatchedDocument(page, base.blocks, env), null);
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

test('operations out of document order are rejected', () => {
  const env = structuredClone(patches.find((p) => p.env.operations.length > 1).env);
  env.operations.reverse();
  assert.equal(applyToBlocks(manifest.blocks, env), 'PATCH_INVALID');
});

test('the replace_block of an html block with empty content passes all client checks', () => {
  const env = patchOne('price', '');
  assert.ok(Array.isArray(applyToBlocks(manifest.blocks, env)));
  assert.equal(checkPatchedDocument(page, base.blocks, env), null);
});

// Patches below have correct hashes and a correct next_revision. Only the markup analysis
// of the patched document (H-48) finds the defect.
const rejected = {
  'empty content of a json block (M-25)': ['cart', ''],
  'a script element in html content (M-22)': ['price', '<script>alert(1)</script>'],
  'an end tag that closes the block early (M-23)': ['price', '1</p><p>2'],
  'the end tag of a title block in its content': ['title', 'Shop</title><script>x</script><title>'],
};
for (const [what, [id, content]] of Object.entries(rejected)) {
  test(`a patch with ${what} passes the hash checks and is rejected by the patched-document analysis`, () => {
    const env = patchOne(id, content);
    assert.ok(Array.isArray(applyToBlocks(manifest.blocks, env)));
    assert.equal(checkPatchedDocument(page, base.blocks, env), 'PATCH_CONTENT_REJECTED');
  });
}

test('the client accepts the KAT manifest for the KAT document (H-48)', () => {
  assert.equal(checkManifest(page, manifest), null);
});

test('a manifest that is consistent in itself but not with the document is rejected (H-48)', () => {
  // A server defect: the manifest lists the price hash of page B and the matching page revision.
  const blocks = manifest.blocks.map((b) => (b.id === 'price' ? { ...b, sha256: contentHash('130.00') } : b));
  const wrong = { ...manifest, blocks, page_revision: pageRevision(manifest.template_revision, blocks) };
  assert.equal(checkManifest(page, wrong), 'MANIFEST_INVALID');
});

test('the Rq-Have example is t and the 8-byte digest prefixes of the manifest blocks (H-44)', () => {
  const b = Buffer.concat(manifest.blocks.map((x) => Buffer.from(x.sha256, 'base64url').subarray(0, 8)));
  assert.equal(b.length, 8 * manifest.blocks.length);
  assert.equal(kat.rq_have, `t=${manifest.template_revision}, b=:${b.toString('base64')}:`);
});

const TYPE = '<script type="application/rivqen-manifest+json">';
const full = readFileSync(join(KAT, kat.full_document));

// Known-answer set 2 (H-55): the same page with policy and limits in the manifest.
const pl = kat.with_policy_limits;
const manifestPl = readJson(join(KAT, pl.manifest));
const fullPl = readFileSync(join(KAT, pl.full_document));
const patchesPl = pl.patches.map((p) => ({ name: p, env: readJson(join(KAT, p)) }));

for (const [label, doc, m] of [
  ['the full document', full, manifest],
  ['the full document with policy and limits', fullPl, manifestPl],
]) {
  test(`${label} is the KAT document with the manifest element immediately after the head start tag (H-72)`, () => {
    // The KAT page has only white space, a DOCTYPE and an html start tag before <head>,
    // and no "<head>" text in a comment, so a byte search finds the head start tag.
    const at = page.indexOf('<head>') + '<head>'.length;
    assert.equal(page.subarray(0, at).toString('latin1'), '<!DOCTYPE html>\n<html>\n<head>');
    const start = doc.indexOf(TYPE);
    const end = doc.indexOf('</script>', start) + '</script>'.length;
    assert.equal(start, at);
    assert.equal(doc.indexOf(TYPE, start + 1), -1, 'exactly one manifest element');
    assert.deepEqual(Buffer.concat([doc.subarray(0, start), doc.subarray(end)]), page);
  });

  test(`the manifest text in ${label} has no "<", is the KAT manifest and passes the client check`, () => {
    const start = doc.indexOf(TYPE) + TYPE.length;
    const text = doc.subarray(start, doc.indexOf('</script', start));
    assert.equal(text.indexOf('<'), -1);
    assert.deepEqual(JSON.parse(text.toString('utf8')), m);
    assert.equal(checkManifest(page, JSON.parse(text.toString('utf8'))), null);
  });

  test(`${label} is invalid for RQP (M-13), so a server never inserts a second manifest`, () => {
    assert.equal(analyze(doc).error, 'RQP_MARKUP_RESERVED');
  });
}

test('the KAT manifest has no policy and no limits, so its page revision is the markup analysis value (H-55)', () => {
  assert.equal(Object.hasOwn(manifest, 'policy') || Object.hasOwn(manifest, 'limits'), false);
  assert.equal(base.page_revision, manifest.page_revision);
});

test('the exact page_revision preimage with policy and limits: one PL line before the block lines (H-55)', () => {
  // Written out by hand, independent of the helper in helpers.mjs.
  const p =
    'rqp-r1\n' + 't1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM\n' +
    'PL\tpolicy;mode=private;ttl_seconds=60;offline=false;limits;max_patch_bytes=262144\n' +
    'title\thtml\t0Aqua3-_d6RWbY3zaAXXrpc4lNqfUz7JZRazHgKLces\n' +
    'price\thtml\tXg3oaiol3ywUdxTQZNp7JJJu7_R8kUL-HK4accfrDKs\n' +
    'cart\tjson\tzyEL3KkZlLmAgTTFnonr3pdX-DnDM81i9xFoREUEaX4\n';
  assert.ok(p.includes(pl.policy_line));
  assert.equal('r1.' + sha256b64u(Buffer.from(p, 'utf8')), manifestPl.page_revision);
  assert.equal(manifestPl.page_revision, 'r1.ke0dIjtkqBtefkyyLQU5ICLWPia5PfammTko2xmMLOQ');
});

test('policy and limits change the page revision; template and blocks stay those of the analysis (H-55)', () => {
  assert.equal(manifestPl.template_revision, base.template_revision);
  assert.deepEqual(manifestPl.blocks, manifest.blocks);
  assert.notEqual(manifestPl.page_revision, base.page_revision);
  assert.equal(pageRevision(base.template_revision, base.blocks, manifestPl), manifestPl.page_revision);
});

test('a manifest that adds policy or limits but keeps the old page revision is rejected (H-55)', () => {
  // A server changes its configuration and does not recompute the revision: the strong ETag would lie.
  const policyOnly = { ...manifest, policy: manifestPl.policy };
  const limitsOnly = { ...manifest, limits: manifestPl.limits };
  const stale = { ...manifestPl, page_revision: manifest.page_revision };
  for (const m of [policyOnly, limitsOnly, stale]) assert.equal(checkManifest(page, m), 'MANIFEST_INVALID');
});

test('a manifest with policy and limits removed keeps a stale page revision and is rejected (H-55)', () => {
  const { policy, limits, ...rest } = manifestPl;
  assert.ok(policy && limits);
  assert.equal(checkManifest(page, rest), 'MANIFEST_INVALID');
});

for (const { name, env } of patchesPl) {
  test(`${name}: base and next revisions include the policy line of the snapshot (H-55)`, () => {
    assert.equal(env.base_revision, manifestPl.page_revision);
    assert.equal(env.template_revision, manifestPl.template_revision);
    assert.equal(env.next_revision, pl.page_revision_b);
    assert.ok(Array.isArray(applyToBlocks(manifestPl.blocks, env, manifestPl)));
    assert.equal(checkPatchedDocument(page, base.blocks, env, manifestPl), null);
  });

  test(`${name}: a client whose snapshot has other policy or limits fails the next_revision check (H-55)`, () => {
    // The server changed its policy after the client stored the page: one fallback load, never a stale policy.
    const other = { ...manifestPl, policy: { ...manifestPl.policy, ttl_seconds: 61 } };
    assert.equal(applyToBlocks(manifestPl.blocks, env, other), 'INTEGRITY_ERROR');
    assert.equal(applyToBlocks(manifestPl.blocks, env, manifest), 'INTEGRITY_ERROR');
    assert.equal(checkPatchedDocument(page, base.blocks, env, manifest), 'INTEGRITY_ERROR');
  });
}

test('the patch of the KAT page without policy fails against a snapshot with policy (H-55)', () => {
  assert.equal(applyToBlocks(manifestPl.blocks, patches[0].env, manifestPl), 'INTEGRITY_ERROR');
});

test('the delta of the price change is smaller than the full document (negotiation section 4, step 7)', () => {
  assert.ok(Buffer.byteLength(JSON.stringify(patches[0].env)) < full.length);
});
