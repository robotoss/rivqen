// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Measure transferred bytes for four visits of the demo catalog page:
//   1. cold (no cache)  2. revisit, nothing changed  3. data changed  4. template changed
// Compared strategies:
//   plain      — no Rivqen, no revalidation: full page every time
//   http-etag  — standard HTTP revalidation: 304 when unchanged, full page otherwise
//   rqp        — Rivqen protocol rqp/1: 304, or a delta with only the changed blocks, or full page
//   legacy     — optional: the same page through the UPSTREAM legacy server (LEGACY_URL + LEGACY_PAGES)
//
// Usage: node scripts/measure.js [--out file.json]
// Needs the demo server running (npm start). Body sizes are measured as received,
// plus gzip and brotli sizes computed locally with Node zlib (level defaults).

import { gzipSync, brotliCompressSync } from 'node:zlib';
import { writeFileSync } from 'node:fs';
import { analyzePage } from '../src/markup.js';
import { renderCatalog } from '../src/pages.js';

const BASE = process.env.DEMO_URL ?? 'http://127.0.0.1:8787';
// The server answers only its own Host (DL-036 I4): send the host of ORIGIN (or of DEMO_URL).
const HOST = new URL(process.env.ORIGIN ?? BASE).host;
const out = process.argv.includes('--out') ? process.argv[process.argv.indexOf('--out') + 1] : null;

const sizes = (body) => ({ raw: Buffer.byteLength(body), gzip: body ? gzipSync(body).length : 0, br: body ? brotliCompressSync(body).length : 0 });

async function get(headers = {}) {
  const r = await fetch(`${BASE}/catalog`, { headers: { Host: HOST, ...headers } });
  const body = r.status === 304 ? '' : await r.text();
  return { status: r.status, headers: r.headers, body };
}
// The POST endpoints need the custom header (cross-site protection). fetch sends no Origin header here.
const post = (p) => fetch(`${BASE}${p}`, { method: 'POST', headers: { Host: HOST, 'Rq-Demo-Action': '1' } }).then((r) => r.json());

const RQ = { 'Rq-Version': '1', 'Rq-Capabilities': '(block-patch)', Accept: 'text/html, application/vnd.rivqen.delta+json' };
const results = [];
let snapshot = null; // what an rqp/1 client holds: { page_revision, template_revision, blocks }
let etag = null;

/** The manifest of a full document (the client reads it with the markup analysis; the demo reads it with a pattern). */
function manifestOf(body) {
  const m = /<script type="application\/rivqen-manifest\+json">([^<]*)<\/script>/.exec(body);
  return m ? JSON.parse(m[1]) : null;
}

/** Request fields of an rqp/1 client: If-None-Match with the page revision, Rq-Have with t and b (H-44, H-46). */
function rqpFields() {
  if (!snapshot) return RQ;
  const b = Buffer.concat(snapshot.blocks.map((x) => Buffer.from(x.sha256, 'base64url').subarray(0, 8))).toString('base64');
  return { ...RQ, 'If-None-Match': `"${snapshot.page_revision}"`, 'Rq-Have': `t=${snapshot.template_revision}, b=:${b}:` };
}

async function visit(name) {
  const plain = await get();
  const http = await get(etag ? { 'If-None-Match': etag } : {});
  const rqp = await get(rqpFields());
  etag = plain.headers.get('etag');
  const kind = rqp.status === 304 ? '304' : (rqp.headers.get('content-type') ?? '').includes('delta') ? 'delta' : 'full';
  if (kind === 'full') snapshot = manifestOf(rqp.body);
  if (kind === 'delta') {
    // The client applies the delta: new page revision, new hashes for the changed blocks.
    const d = JSON.parse(rqp.body);
    snapshot = { ...snapshot, page_revision: d.next_revision, blocks: snapshot.blocks.map((x) => d.operations.find((o) => o.block_id === x.id) ? { ...x, sha256: d.operations.find((o) => o.block_id === x.id).sha256 } : x) };
  }
  results.push({
    visit: name,
    plain: { status: plain.status, ...sizes(plain.body) },
    http_etag: { status: http.status, ...sizes(http.body) },
    rqp: { status: rqp.status, kind, ...sizes(rqp.body) },
  });
}

await visit('1. cold');
await visit('2. revisit, unchanged');
await post('/__demo/next-data');
await visit('3. data changed');
await post('/__demo/next-template');
await visit('4. template changed');

// Optional legacy comparison through an upstream legacy server (see tools/upstream-lab).
if (process.env.LEGACY_URL && process.env.LEGACY_PAGES) {
  const { writeFileSync: w } = await import('node:fs');
  const toLegacy = (html) => html
    .replace(/<script type="application\/json" data-rq-block="(\w+)">([\s\S]*?)<\/script>/g, '<!--sonicdiff-$1--><script type="application/json">$2</script><!--sonicdiff-$1-end-->')
    .replace(/<(\w+)([^>]*?) data-rq-block="(\w+)"([^>]*)>([\s\S]*?)<\/\1>/g, '<!--sonicdiff-$3--><$1$2$4>$5</$1><!--sonicdiff-$3-end-->');
  // Reproduce states 1 (before data change) and 2 (after) with the same template.
  const before = toLegacy(renderCatalogAt(0));
  const after = toLegacy(renderCatalogAt(1));
  w(`${process.env.LEGACY_PAGES}/catalegacya.html`, before);
  w(`${process.env.LEGACY_PAGES}/catalegacyb.html`, after);
  const first = await fetch(`${process.env.LEGACY_URL}/p/catalegacya`, { headers: { 'accept-diff': 'true', 'Accept-Encoding': 'gzip' } });
  const firstBody = await first.text();
  const et = first.headers.get('etag');
  const tt = first.headers.get('template-tag');
  const hit = await fetch(`${process.env.LEGACY_URL}/p/catalegacya`, { headers: { 'accept-diff': 'true', 'If-None-Match': et, 'template-tag': tt, 'Accept-Encoding': 'gzip' } });
  const data = await fetch(`${process.env.LEGACY_URL}/p/catalegacyb`, { headers: { 'accept-diff': 'true', 'If-None-Match': et, 'template-tag': tt, 'Accept-Encoding': 'gzip' } });
  const dataBody = await data.text();
  results.push({ visit: 'legacy (upstream server)', legacy: {
    cold: { status: first.status, ...sizes(firstBody) },
    unchanged: { status: hit.status, raw: 0, gzip: 0, br: 0 },
    data_changed: { status: data.status, ...sizes(dataBody) },
  } });
}

function renderCatalogAt(dataStep) {
  // Local render with the same generator, independent of the server state.
  return renderCatalog().replace(/Today: <span data-rq-block="price">[\d.]+<\/span>/, `Today: <span data-rq-block="price">${(120 + dataStep * 5).toFixed(2)}</span>`)
    .replace(/In stock: \d+/, `In stock: ${25 - dataStep}`)
    .replace(/<title data-rq-block="title">Shop — Catalog \([\d.]+\)<\/title>/, `<title data-rq-block="title">Shop — Catalog (${(120 + dataStep * 5).toFixed(2)})</title>`)
    .replace(/"items":\d+/, `"items":${dataStep % 4}`);
}

const page = analyzePage(Buffer.from(renderCatalog(), 'utf8'));
const report = { date: new Date().toISOString(), page_bytes: Buffer.byteLength(renderCatalog()), blocks: page.blocks.map((b) => b.id), results };
console.log(JSON.stringify(report, null, 2));
if (out) writeFileSync(out, JSON.stringify(report, null, 2) + '\n');
