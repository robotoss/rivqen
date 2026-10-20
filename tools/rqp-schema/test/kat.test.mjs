// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Known-answer test: the example manifest and patches agree with the document in
// spec/rqp/examples/kat/, with the revision algorithm of manifest-patch.md section 1.3,
// with the client verification of section 2.4, and with the Node.js markup candidate.
// Since H-103 the manifest has no template revision and no block list: the client
// computes them (kat.json member `computed`) and checks them through page_revision.
// Since H-104 and H-107 the envelope is the binding and the operations (block_id,
// content): the client computes the next page revision R' and checks the binding of the
// URL, its base revision and R' (section 2.4, step 7).

import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { EXAMPLES, ROOT, bindingUrl, contentHash, deltaBinding, pageRevision, readJson, sha256b64u, templateRevision } from './helpers.mjs';

const KAT = join(EXAMPLES, 'kat');
const CLI = join(ROOT, 'tools', 'rqp-markup', 'node', 'src', 'cli.mjs');
const kat = readJson(join(KAT, 'kat.json'));
const page = readFileSync(join(KAT, kat.document));
const manifest = readJson(join(KAT, kat.manifest));
const patches = kat.patches.map((p) => ({ name: p, env: readJson(join(KAT, p)) }));
/** The values that the client computes with its markup analysis (H-103). */
const computed = kat.computed;
/** U of the binding (H-107): the URL of the KAT manifest (no query). */
const URL_U = `${manifest.origin}${manifest.path}`;

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
 * Client verification of manifest-patch.md section 2.4, steps 3 to 7 (operation rules,
 * the next page revision R' and the binding; H-104, H-107): returns the new block list, or
 * the code of the first failed check. The client computes the hash of each content; the
 * template revision is the one of its snapshot (`current` and `t`). `snap` is the snapshot
 * manifest: its policy and limits give the policy line of the preimage (H-55). B of the
 * binding is the page revision of the snapshot (its blocks and policy line); `url` is U.
 */
function applyToBlocks(current, env, snap = manifest, t = computed.template_revision, url = URL_U) {
  const next = current.map((b) => ({ ...b }));
  let last = -1;
  for (const op of env.operations) {
    const i = next.findIndex((b) => b.id === op.block_id);
    if (i < 0 || i <= last) return 'PATCH_INVALID';
    last = i;
    next[i].sha256 = contentHash(op.content);
  }
  if (deltaBinding(url, pageRevision(t, current, snap), pageRevision(t, next, snap)) !== env.binding) return 'INTEGRITY_ERROR';
  return next;
}

/**
 * Client check of section 2.4, step 8 (H-48): the markup analysis of the patched document.
 * Returns null when the patched document is valid, has the same template revision and the
 * same blocks (id, format, order), and its page revision is R' (step 6: the blocks of the
 * snapshot with the hashes of the contents); else the code. The analysis has no manifest
 * input, so the page revision is computed again with the policy line of the snapshot
 * manifest `snap` (H-55).
 */
function checkPatchedDocument(bytes, blocks, env, snap = manifest) {
  const hashes = new Map(env.operations.map((op) => [op.block_id, contentHash(op.content)]));
  const next = computed.blocks.map((b) => ({ ...b, sha256: hashes.get(b.id) ?? b.sha256 }));
  const after = analyze(splice(bytes, blocks, env));
  if (!after.valid) return 'PATCH_CONTENT_REJECTED';
  if (after.template_revision !== computed.template_revision) return 'PATCH_CONTENT_REJECTED';
  const shape = (list) => list.map(({ id, format }) => `${id}\t${format}`).join('\n');
  if (shape(after.blocks) !== shape(blocks)) return 'PATCH_CONTENT_REJECTED';
  if (pageRevision(after.template_revision, after.blocks, snap) !== pageRevision(computed.template_revision, next, snap)) return 'INTEGRITY_ERROR';
  return null;
}

/**
 * Client check of section 1.8, item 5 (H-48, H-103): the markup analysis of the original
 * document gives the template revision and the blocks; these with the policy line of the
 * manifest (H-55) give its page revision. Returns null or MANIFEST_INVALID.
 */
function checkManifest(original, m) {
  const r = analyze(original);
  if (!r.valid) return 'MANIFEST_INVALID';
  return pageRevision(r.template_revision, r.blocks, m) === m.page_revision ? null : 'MANIFEST_INVALID';
}

/** The patch for the KAT page that sets one block, with a consistent binding (H-107). */
function patchOne(id, content) {
  const blocks = computed.blocks.map((b) => (b.id === id ? { ...b, sha256: contentHash(content) } : b));
  return {
    binding: deltaBinding(URL_U, manifest.page_revision, pageRevision(computed.template_revision, blocks, manifest)),
    operations: [{ block_id: id, content }],
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

test('the KAT document is valid for RQP and the computed values are its blocks in document order (H-103)', () => {
  assert.equal(base.valid, true);
  assert.deepEqual(computed.blocks, base.blocks.map(({ id, format, sha256 }) => ({ id, format, sha256 })));
  assert.deepEqual(Object.keys(manifest), ['protocol', 'origin', 'path', 'page_revision']);
});

test('each manifest block hash is SHA-256 of the exact content bytes', () => {
  for (const b of base.blocks) assert.equal(b.sha256, sha256b64u(page.subarray(b.start, b.end)), b.id);
});

test('template_revision is t1. + B64U(SHA-256("rqp-t1\\n" || template))', () => {
  assert.equal(templateRevision(template(page, base.blocks)), computed.template_revision);
  assert.equal(base.template_revision, computed.template_revision);
});

test('page_revision is r1. + B64U(SHA-256(P)) over the template revision and the block lines', () => {
  assert.equal(pageRevision(computed.template_revision, computed.blocks), manifest.page_revision);
  assert.equal(base.page_revision, manifest.page_revision);
});

test('the exact page_revision preimage of the KAT document', () => {
  const p =
    'rqp-r1\n' + computed.template_revision + '\n' +
    'title\thtml\t0Aqua3-_d6RWbY3zaAXXrpc4lNqfUz7JZRazHgKLces\n' +
    'price\thtml\tXg3oaiol3ywUdxTQZNp7JJJu7_R8kUL-HK4accfrDKs\n' +
    'cart\tjson\tzyEL3KkZlLmAgTTFnonr3pdX-DnDM81i9xFoREUEaX4\n';
  assert.equal('r1.' + sha256b64u(Buffer.from(p, 'utf8')), manifest.page_revision);
});

for (const { name, env } of patches) {
  test(`${name}: the envelope has only the binding and the operations of block_id and content (H-104, H-107)`, () => {
    assert.deepEqual(Object.keys(env), ['binding', 'operations']);
    for (const op of env.operations) assert.deepEqual(Object.keys(op), ['block_id', 'content']);
  });

  test(`${name}: the client computes R' from its blocks and the operations, and the binding matches`, () => {
    const next = applyToBlocks(computed.blocks, env);
    assert.ok(Array.isArray(next), String(next));
  });

  test(`${name}: the patched document passes the markup analysis and has the page revision R' (H-48)`, () => {
    assert.equal(checkPatchedDocument(page, base.blocks, env), null);
  });
}

test('kat.json bindings: each entry has the URL, the base and next revisions, the preimage and the binding of its patch (H-107)', () => {
  const entries = [...kat.bindings, ...kat.with_policy_limits.bindings];
  assert.deepEqual(entries.map((e) => e.patch), [...kat.patches, ...kat.with_policy_limits.patches]);
  for (const e of entries) {
    assert.equal(e.url, URL_U);
    assert.equal(e.preimage, `rqp-d1\n${e.url}\n${e.base_revision}\n${e.next_revision}\n`);
    assert.equal('d1.' + sha256b64u(Buffer.from(e.preimage, 'utf8')), e.binding);
    assert.equal(readJson(join(KAT, e.patch)).binding, e.binding);
  }
  assert.equal(kat.bindings[0].base_revision, manifest.page_revision);
});

test('the exact binding preimage of the KAT price patch, written out by hand (H-107)', () => {
  const p =
    'rqp-d1\n' +
    'https://shop.example.com/catalog\n' +
    'r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8\n' +
    'r1.psbsc6whgs8ooESisNWp6E3PrQfkg4D69WmXAo12pbI\n';
  // The bytes: 6 + 1 + 32 + 1 + 46 + 1 + 46 + 1, all ASCII, each line ends with 0A.
  assert.equal(Buffer.byteLength(p, 'utf8'), 134);
  assert.equal('d1.' + sha256b64u(Buffer.from(p, 'utf8')), 'd1.SGYLGtFc4PsLoFIzWsurTmxLupiief5xgV7ZCtXDcpc');
  assert.equal(patches[0].env.binding, 'd1.SGYLGtFc4PsLoFIzWsurTmxLupiief5xgV7ZCtXDcpc');
});

test('U has the query and no fragment; host case and the default port are normalized by the URL serializer (H-107)', () => {
  const q = kat.binding_with_query;
  assert.equal(bindingUrl(q.url_input), q.url);
  assert.equal(q.url, 'https://shop.example.com/catalog?sort=price&page=2');
  assert.equal(deltaBinding(q.url, q.base_revision, q.next_revision), q.binding);
  assert.equal(q.binding, 'd1.3suck1gLHbFVWy0goIS-Q6G8z3wdhw5_BqlUJ7yv93U');
  // The same base and next revision without the query: another binding. A client that leaves out the query fails.
  assert.notEqual(q.binding, patches[0].env.binding);
  assert.equal(applyToBlocks(computed.blocks, patches[0].env, manifest, computed.template_revision, q.url), 'INTEGRITY_ERROR');
});

test('a server that omits a changed block (digest prefix collision) fails the binding check', () => {
  const env = structuredClone(patches.find((p) => p.env.operations.length > 1).env);
  env.operations = env.operations.filter((op) => op.block_id !== 'cart');
  assert.equal(applyToBlocks(computed.blocks, env), 'INTEGRITY_ERROR');
});

test('a content that the binding does not bind fails with INTEGRITY_ERROR at step 7 (H-104, H-107)', () => {
  const env = structuredClone(patches[0].env);
  env.operations[0].content = '131.00';
  assert.equal(applyToBlocks(computed.blocks, env), 'INTEGRITY_ERROR');
});

test('a client whose unchanged block differs from the server copy (another base) fails the binding check (H-104, H-107)', () => {
  const stale = computed.blocks.map((b) => (b.id === 'title' ? { ...b, sha256: contentHash('Shop!') } : b));
  assert.equal(applyToBlocks(stale, patches[0].env), 'INTEGRITY_ERROR');
});

test('a client with another template revision fails the binding check (H-104, H-107)', () => {
  const other = templateRevision(Buffer.from('another template'));
  assert.equal(applyToBlocks(computed.blocks, patches[0].env, manifest, other), 'INTEGRITY_ERROR');
});

test('H-107: the all-blocks patch applies only on its base and its URL', () => {
  // shop-all replaces every block, so before H-107 its R' fitted every snapshot of the template.
  const all = patches.find((p) => p.env.operations.length === computed.blocks.length).env;
  assert.ok(Array.isArray(applyToBlocks(computed.blocks, all)));
  // Another base: a snapshot with other contents (another user, an earlier or later version).
  const other = computed.blocks.map((b) => (b.id === 'price' ? { ...b, sha256: contentHash('99.00') } : b));
  assert.equal(applyToBlocks(other, all), 'INTEGRITY_ERROR');
  // Its own target (a stale replay): the snapshot already has the result.
  const target = applyToBlocks(computed.blocks, all);
  assert.equal(applyToBlocks(target, all), 'INTEGRITY_ERROR');
  // Another URL with the same template and the same base.
  assert.equal(applyToBlocks(computed.blocks, all, manifest, computed.template_revision, 'https://shop.example.com/catalog/2'), 'INTEGRITY_ERROR');
});

test('operations out of document order, for an unknown block or two for one block are rejected', () => {
  const env = structuredClone(patches.find((p) => p.env.operations.length > 1).env);
  env.operations.reverse();
  assert.equal(applyToBlocks(computed.blocks, env), 'PATCH_INVALID');
  assert.equal(applyToBlocks(computed.blocks, { ...env, operations: [{ block_id: 'stock', content: '1' }] }), 'PATCH_INVALID');
  const twice = patches[0].env.operations[0];
  assert.equal(applyToBlocks(computed.blocks, { ...env, operations: [twice, twice] }), 'PATCH_INVALID');
});

test('an operation with empty content of an html block passes all client checks', () => {
  const env = patchOne('price', '');
  assert.ok(Array.isArray(applyToBlocks(computed.blocks, env)));
  assert.equal(checkPatchedDocument(page, base.blocks, env), null);
});

// Patches below have correct hashes and a correct binding. Only the markup analysis
// of the patched document (H-48) finds the defect.
const rejected = {
  'empty content of a json block (M-25)': ['cart', ''],
  // H-78: script, style, iframe … are valid markup in a block; the patch content rule (step 9) rejects them.
  'a textarea element in html content (M-22, kept by H-78)': ['price', '<textarea>x</textarea>'],
  'an end tag that closes the block early (M-23)': ['price', '1</p><p>2'],
  'the end tag of a title block in its content': ['title', 'Shop</title><script>x</script><title>'],
};
for (const [what, [id, content]] of Object.entries(rejected)) {
  test(`a patch with ${what} passes the hash checks and is rejected by the patched-document analysis`, () => {
    const env = patchOne(id, content);
    assert.ok(Array.isArray(applyToBlocks(computed.blocks, env)));
    assert.equal(checkPatchedDocument(page, base.blocks, env), 'PATCH_CONTENT_REJECTED');
  });
}

test('the client accepts the KAT manifest for the KAT document (H-48)', () => {
  assert.equal(checkManifest(page, manifest), null);
});

test('a manifest with the page revision of another document is rejected (H-48, H-103)', () => {
  // A server defect: the manifest has the page revision of page B (price 130.00) with the body of page A.
  const blocks = computed.blocks.map((b) => (b.id === 'price' ? { ...b, sha256: contentHash('130.00') } : b));
  const wrong = { ...manifest, page_revision: pageRevision(computed.template_revision, blocks) };
  assert.equal(checkManifest(page, wrong), 'MANIFEST_INVALID');
});

test('the Rq-Have example is t and the 8-byte digest prefixes of the computed blocks (H-44, H-103)', () => {
  const b = Buffer.concat(computed.blocks.map((x) => Buffer.from(x.sha256, 'base64url').subarray(0, 8)));
  assert.equal(b.length, 8 * computed.blocks.length);
  assert.equal(kat.rq_have, `t=${computed.template_revision}, b=:${b.toString('base64')}:`);
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
  // Written out by hand, independent of the helper in helpers.mjs. Since H-103 the
  // second set has only members that are not the default (no mode, no offline false).
  const p =
    'rqp-r1\n' + 't1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM\n' +
    'PL\tpolicy;ttl_seconds=60;offline=true;limits;max_patch_bytes=65536\n' +
    'title\thtml\t0Aqua3-_d6RWbY3zaAXXrpc4lNqfUz7JZRazHgKLces\n' +
    'price\thtml\tXg3oaiol3ywUdxTQZNp7JJJu7_R8kUL-HK4accfrDKs\n' +
    'cart\tjson\tzyEL3KkZlLmAgTTFnonr3pdX-DnDM81i9xFoREUEaX4\n';
  assert.ok(p.includes(pl.policy_line));
  assert.equal('r1.' + sha256b64u(Buffer.from(p, 'utf8')), manifestPl.page_revision);
  assert.equal(manifestPl.page_revision, 'r1.pONNMoySlMlYw50BpgdvbUlGESnMzZBCOmQ4SlebDxs');
});

test('policy and limits change the page revision; template and blocks stay those of the analysis (H-55)', () => {
  assert.deepEqual(Object.keys(manifestPl), ['protocol', 'origin', 'path', 'page_revision', 'policy', 'limits']);
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
  test(`${name}: R' and B of the binding include the policy line of the snapshot (H-55, H-107)`, () => {
    const entry = pl.bindings.find((e) => e.patch === name);
    assert.deepEqual([entry.base_revision, entry.next_revision], [manifestPl.page_revision, pl.page_revision_b]);
    assert.equal(env.binding, deltaBinding(URL_U, manifestPl.page_revision, pl.page_revision_b));
    assert.ok(Array.isArray(applyToBlocks(computed.blocks, env, manifestPl)));
    assert.equal(checkPatchedDocument(page, base.blocks, env, manifestPl), null);
  });

  test(`${name}: a client whose snapshot has other policy or limits fails the binding check (H-55)`, () => {
    // The server changed its policy after the client stored the page: one fallback load, never a stale policy.
    const other = { ...manifestPl, policy: { ...manifestPl.policy, ttl_seconds: 61 } };
    assert.equal(applyToBlocks(computed.blocks, env, other), 'INTEGRITY_ERROR');
    assert.equal(applyToBlocks(computed.blocks, env, manifest), 'INTEGRITY_ERROR');
  });
}

test('the patch of the KAT page without policy fails against a snapshot with policy (H-55)', () => {
  assert.equal(applyToBlocks(computed.blocks, patches[0].env, manifestPl), 'INTEGRITY_ERROR');
});

test('the delta of the price change is smaller than the full document (negotiation section 4, step 7)', () => {
  assert.ok(Buffer.byteLength(JSON.stringify(patches[0].env)) < full.length);
});
