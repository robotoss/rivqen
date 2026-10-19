// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The legacy side of the byte study: a client for the upstream Node.js server
// of tools/upstream-lab (lab-node), and the byte breakdown of a legacy data
// response. The lab runs the unmodified upstream code in a container; this
// module only sends requests and reads the answers. No upstream code is in
// this repository (docs/legal/provenance.md).
//
// Requests (docs/engineering/protocol/legacy-wire.md section 2): accept-diff:
// true and Accept-Encoding: gzip (the Node.js server needs gzip or deflate,
// legacy-wire section 3); on a revisit also If-None-Match and template-tag
// from the first answer.

import { createHash } from 'node:crypto';
import { rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { segments, tally, tallySum } from './json-segments.mjs';
import { sizes } from './sizes.mjs';
import { StudyError } from './errors.mjs';

/** Response fields that the study records. Transport fields (date, connection, transfer-encoding) are left out. */
export const LEGACY_FIELDS = Object.freeze(['etag', 'template-tag', 'template-change', 'cache-offline', 'content-type', 'content-encoding', 'content-length']);

const MAX_BODY = 8 * 1024 * 1024;

async function get(url, headers) {
  const r = await fetch(url, { headers });
  const body = Buffer.from(await r.arrayBuffer());
  if (body.length > MAX_BODY) throw new StudyError('STUDY_LAB', 'lab body larger than the study bound');
  const fields = [];
  for (const name of LEGACY_FIELDS) {
    const v = r.headers.get(name);
    if (v !== null) fields.push([name, v]);
  }
  return { status: r.status, fields, body };
}

const sha256hex = (b) => createHash('sha256').update(b).digest('hex');
const field = (res, name) => res.fields.find(([n]) => n === name)?.[1] ?? null;

/**
 * Run the legacy visits of each scenario through the lab server.
 * scenario = { name: letters a-z only (the lab maps /p/<name> to /pages/<name>.html), pageA, pageB }.
 * The page files are written into pagesDir and deleted again, also on an error.
 */
export async function labRun({ url, pagesDir, scenarios }) {
  const out = [];
  for (const s of scenarios) {
    if (!/^[a-z]{1,40}$/.test(s.name)) throw new StudyError('STUDY_LAB', `bad scenario name ${s.name}`);
    const fa = join(pagesDir, `${s.name}a.html`);
    const fb = join(pagesDir, `${s.name}b.html`);
    try {
      writeFileSync(fa, s.pageA);
      writeFileSync(fb, s.pageB);
      const legacy = { 'accept-diff': 'true', 'accept-encoding': 'gzip' };
      const cold = await get(`${url}/p/${s.name}a`, legacy);
      const etag = field(cold, 'etag');
      const tag = field(cold, 'template-tag');
      if (etag === null || tag === null) throw new StudyError('STUDY_LAB', `${s.name}: no etag or template-tag on the cold visit`);
      const revisit = { ...legacy, 'if-none-match': etag, 'template-tag': tag };
      const unchanged = await get(`${url}/p/${s.name}a`, revisit);
      const data = await get(`${url}/p/${s.name}b`, revisit);
      out.push({
        name: s.name,
        request_cold: Object.entries(legacy),
        request_revisit: Object.entries(revisit),
        cold: { status: cold.status, fields: cold.fields, ...sizes(cold.body), body_sha256: sha256hex(cold.body), body_is_page: cold.body.equals(s.pageA) },
        unchanged: { status: unchanged.status, fields: unchanged.fields, ...sizes(unchanged.body) },
        data_changed: {
          status: data.status,
          fields: data.fields,
          ...sizes(data.body),
          body_sha256: sha256hex(data.body),
          breakdown: field(data, 'template-change') === 'false' ? legacyDataRaw(data.body) : null,
        },
      });
    } finally {
      rmSync(fa, { force: true });
      rmSync(fb, { force: true });
    }
  }
  return out;
}

const MARKER = /<!--sonicdiff-?\w*?(?:-end)?-->/g;

/**
 * Raw rows of a legacy data response body (legacy-wire section 5.4): JSON with
 * "data" (placeholder -> element with its marker comments), "diff",
 * "html-sha1" and "template-tag". The rows add up to the body length when the
 * body is the JSON.stringify form of its value; else `exact` is false.
 */
export function legacyDataRaw(body) {
  const text = body.toString('utf8');
  const value = JSON.parse(text);
  const exact = JSON.stringify(value) === text;
  const t = tally(segments(value));
  if (tallySum(t) !== t.total) throw new StudyError('STUDY_TALLY', 'legacy tally does not add up');
  const rows = {
    json_structure: t.structure,
    member_names: 0,
    placeholder_names: 0,
    marker_comments: 0,
    element_tags: 0,
    content_bytes: 0,
    escapes_and_quotes: 0,
    html_sha1: 0,
    template_tag: 0,
    diff: 0,
  };
  for (const [label, n] of Object.entries(t.keys)) {
    if (label.startsWith('data.')) rows.placeholder_names += n;
    else rows.member_names += n;
  }
  for (const [label, parts] of Object.entries(t.values)) {
    const n = Object.values(parts).reduce((a, b) => a + b, 0);
    if (label === 'html-sha1') rows.html_sha1 += n;
    else if (label === 'template-tag') rows.template_tag += n;
    else if (label === 'diff') rows.diff += n;
    else if (label.startsWith('data.')) {
      const key = label.slice('data.'.length);
      const s = value.data[key];
      const markers = (s.match(MARKER) ?? []).join('');
      const element = s.replace(MARKER, '');
      const open = element.indexOf('>');
      const close = element.lastIndexOf('<');
      const content = open >= 0 && close > open ? element.slice(open + 1, close) : element;
      const markerBytes = Buffer.byteLength(markers, 'utf8');
      const contentBytes = Buffer.byteLength(content, 'utf8');
      const elementBytes = Buffer.byteLength(element, 'utf8');
      rows.marker_comments += markerBytes;
      rows.content_bytes += contentBytes;
      rows.element_tags += elementBytes - contentBytes;
      rows.escapes_and_quotes += n - markerBytes - elementBytes;
    } else throw new StudyError('STUDY_LAB', `unexpected member ${label} in a legacy data response`);
  }
  return { exact, total: t.total, rows, entries: Object.keys(value.data ?? {}).length };
}
