// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Soundness of the attribute rule (manifest-patch section 4.2; H-77, H-83, H-83a,
// DL-048): for random raw attribute values, the browser view is computed with
// independent tools, and the rule must reject every value that the browser reads as
// a javascript:, vbscript: or data: URL, except a raster data: image where H-83
// allows it. 0 misses.
//
// Browser view (the oracle):
// - character references: parse5 8.0.1 (the WHATWG tokenizer) decodes the value of a
//   quoted attribute; it is the development dependency of tools/rqp-markup/crosscheck
//   (tests only, not a dependency of this package);
// - the scheme: the WHATWG URL parser of Node.js (`new URL(value, base)`);
// - srcset: the candidate URLs of "parse a srcset attribute" on the decoded value;
// - data: the type part of the serialized URL up to the first "," (Fetch, "data: URL
//   processor"), white space stripped, percent-decoded (stricter than Fetch), and its
//   essence up to ";". Raster means "image/" and not "image/svg+xml".

import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { attributeRejected } from '../src/markup.mjs';

const PARSE5 = new URL('../../rqp-markup/crosscheck/node_modules/parse5/dist/index.js', import.meta.url);
if (!existsSync(fileURLToPath(PARSE5))) {
  throw new Error('parse5 is missing: run "npm ci --ignore-scripts" in tools/rqp-markup/crosscheck (README, "Requirements")');
}
const { parseFragment } = await import(PARSE5.href);

/** The value of the attribute x in `<a x="raw">` after the tokenizer decoded its character references. */
function decoded(raw) {
  const a = parseFragment(`<a x="${raw}">`).childNodes[0];
  return a.attrs[0].value;
}

/** The scheme of a URL value as the WHATWG URL parser reads it, or null when the value is no URL. */
function schemeOf(value) {
  try {
    return new URL(value, 'https://base.example/').protocol;
  } catch {
    return null;
  }
}

const ASCII_WS = /[\t\n\f\r ]/;

/** The candidate URLs of a decoded srcset value (WHATWG HTML, "parse a srcset attribute"; every candidate). */
function srcsetCandidates(s) {
  const urls = [];
  let p = 0;
  while (p < s.length) {
    while (p < s.length && (ASCII_WS.test(s[p]) || s[p] === ',')) p++;
    if (p >= s.length) break;
    let url = '';
    while (p < s.length && !ASCII_WS.test(s[p])) url += s[p++];
    if (url.endsWith(',')) {
      urls.push(url.replace(/,+$/, ''));
      continue;
    }
    urls.push(url);
    let parens = false;
    for (; p < s.length; p++) {
      if (parens) parens = s[p] !== ')';
      else if (s[p] === ',') break;
      else parens = s[p] === '(';
    }
  }
  return urls;
}

/** True when a data: URL value is a raster image for the browser (see the header). */
function rasterImage(value) {
  const href = new URL(value, 'https://base.example/').href;
  const comma = href.indexOf(',');
  if (comma < 0) return true; // the data: URL processor fails: nothing is loaded
  let type = href.slice('data:'.length, comma);
  try {
    type = decodeURIComponent(type);
  } catch {
    // An invalid percent sequence stays as it is.
  }
  const essence = type.replace(/^[\t\n\f\r ]+/, '').split(';')[0].replace(/[\t\n\r ]+$/, '').toLowerCase();
  return essence.startsWith('image/') && essence !== 'image/svg+xml';
}

/** H-83: the elements and attributes where a raster data: image is allowed, in HTML context. */
const IMAGE_PLACES = new Set(['img src', 'img srcset', 'source src', 'source srcset', 'video poster']);

/** True when the browser reads the decoded value as a URL that the rule must reject. */
function dangerous(value, element, name, html) {
  const urls = name === 'srcset' ? srcsetCandidates(value) : [value];
  const asSrc = name === 'srcset' ? 'src' : name;
  return urls.some((url) => {
    const scheme = schemeOf(url);
    if (scheme === 'javascript:' || scheme === 'vbscript:') return true;
    if (scheme !== 'data:') return false;
    return !(html && IMAGE_PLACES.has(`${element} ${asSrc}`) && rasterImage(url));
  });
}

function prng(seed) {
  let s = seed >>> 0;
  return (n) => {
    s = (Math.imul(s, 1664525) + 1013904223) >>> 0;
    return Math.floor((s / 2 ** 32) * n);
  };
}

const pick = (rnd, list) => list[rnd(list.length)];

/** A character as a numeric reference: decimal or hex, leading zeros, ";" optional. */
function reference(rnd, c) {
  const zeros = '0'.repeat(rnd(3));
  const code = c.charCodeAt(0);
  const body = rnd(2) === 0 ? `#${zeros}${code}` : `#${pick(rnd, ['x', 'X'])}${zeros}${code.toString(16)[rnd(2) === 0 ? 'toLowerCase' : 'toUpperCase']()}`;
  return `&${body}${rnd(3) === 0 ? '' : ';'}`;
}

const NAMED = { '\t': '&Tab;', '\n': '&NewLine;', ':': '&colon;', '.': '&period;', '+': '&plus;', ',': '&comma;', '(': '&lpar;', ')': '&rpar;' };
const NOISE = ['\t', '\n', '\r', ' ', '\x01', '\x0c', '\x00', '&#9;', '&#x0A', '&Tab;', '&NewLine;', '&#32;', '&amp;', '&#169;', '&quot;'];

/** A word with references, case changes and noise inside. */
function mangle(rnd, word) {
  let out = '';
  for (const c of word) {
    if (rnd(14) === 0) out += pick(rnd, NOISE);
    const r = rnd(10);
    if (r === 0) out += reference(rnd, c);
    else if (r === 1 && NAMED[c] !== undefined) out += NAMED[c];
    else if (r === 2 && c === 'f' && rnd(2) === 0) out += '&fjlig;';
    else out += rnd(4) === 0 ? c.toUpperCase() : c;
  }
  return out;
}

const SCHEMES = ['javascript', 'javascript', 'javascript', 'vbscript', 'vbscript', 'data', 'data', 'data', 'data', 'https', 'database', 'java', 'x', 'dat'];
const COLONS = [':', ':', ':', '&colon;', '&#58;', '&#x3A', '&#0058;', '&COLON;', '&amp;:', '', '&period;:', '&#x2E;'];
const LEADS = ['', '', '', ' ', '\x01', '\t', '\n ', '&#32;', '&#x09;', '&Tab;', '&NewLine;', '\x00', '/', '&#106;'];
const RESTS = [
  'x', 'alert(1)', 'image/png;base64,iVBO', 'image/png,x', 'IMAGE/JPEG,x', 'image/svg+xml,x', 'image/SVG+xml;base64,x',
  'image/svg%2Bxml,x', 'image/svg%2bxml,x', 'image/sv&#103;+xml,x', 'image/svg&plus;xml,x', 'image/svg&#43;xml,x',
  ' image/svg+xml,x', '&#32;image/svg+xml,x', 'image/ svg+xml,x', 'image/svg+xml ;x,y', 'text/html,x', ',x', 'image/png&#59;x,y',
  'image/png;x=%41,y', '/p?a=1&b=2', 'image/gif', 'image/png&#44;x',
];

/** A raw URL value: lead, a scheme word, a colon and a rest, each with noise. */
function urlValue(rnd) {
  return `${pick(rnd, LEADS)}${mangle(rnd, pick(rnd, SCHEMES))}${pick(rnd, COLONS)}${rnd(4) === 0 ? mangle(rnd, pick(rnd, RESTS)) : pick(rnd, RESTS)}`;
}

const SEPARATORS = [', ', ',', ' ,', '\n', ' ', ',,', '&#44;', '&comma;', '&#32;', '&#x2c '];
const DESCRIPTORS = ['', ' 1x', ' 2x', ' 100w', ' (a, b)', ' (', ' )'];

/** A raw srcset value of one to three candidates. */
function srcsetValue(rnd) {
  const n = 1 + rnd(3);
  let out = '';
  for (let i = 0; i < n; i++) {
    if (i > 0) out += pick(rnd, SEPARATORS);
    out += (rnd(3) === 0 ? '/a.png' : urlValue(rnd).replace(/^ +/, '')) + pick(rnd, DESCRIPTORS);
  }
  return out;
}

/** Values per run; RQP_SOUNDNESS_VALUES sets another count for a longer one-off run. */
const VALUES = Number(process.env.RQP_SOUNDNESS_VALUES ?? 100000);

const PLACES = [
  ['img', 'src', true], ['img', 'srcset', true], ['source', 'src', true], ['source', 'srcset', true], ['video', 'poster', true],
  ['video', 'src', true], ['a', 'href', true], ['img', 'alt', true], ['a', 'srcset', true], ['image', 'href', false],
  ['img', 'src', false], ['use', 'href', false],
];

test('property: the attribute rule rejects every javascript:, vbscript: and non-raster data: URL that the browser reads (0 misses)', (t) => {
  const rnd = prng(0x4877);
  let danger = 0;
  let accepted = 0;
  const misses = [];
  for (let i = 0; i < VALUES; i++) {
    const [element, name, html] = pick(rnd, PLACES);
    const raw = (name === 'srcset' ? srcsetValue(rnd) : urlValue(rnd)).replaceAll('"', '');
    const value = decoded(raw);
    const bad = dangerous(value, element, name, html);
    const rejected = attributeRejected(name, Buffer.from(raw, 'utf8'), element, html);
    if (bad) danger++;
    if (!rejected) accepted++;
    if (bad && !rejected) misses.push({ element, name, html, raw, value });
  }
  t.diagnostic(`${VALUES} values, ${danger} dangerous, ${accepted} accepted, ${misses.length} misses`);
  assert.deepEqual(misses.slice(0, 5), []);
  // The generator reaches both sides: many dangerous values, and many values the rule accepts.
  assert.ok(danger > VALUES / 8, `only ${danger} dangerous values`);
  assert.ok(accepted > VALUES / 6, `only ${accepted} accepted values`);
});

test('the oracle reads the H-77 trap, the srcset split and the data: types as the browser does', () => {
  assert.equal(decoded('java&#x73\ncript:x'), 'javas\ncript:x');
  assert.equal(schemeOf(decoded('java&#x73\ncript:x')), 'javascript:');
  assert.deepEqual(srcsetCandidates('/a.png 1x,data:x 2x, /b.png (1, 2) ,/c.png,,'), ['/a.png', 'data:x', '/b.png', '/c.png']);
  assert.equal(rasterImage('data:image/png;base64,x'), true);
  assert.equal(rasterImage('data:image/svg%2Bxml,x'), false);
  assert.equal(rasterImage('data: image/svg+xml,x'), false);
  assert.equal(dangerous('data:image/png,x', 'img', 'src', true), false);
  assert.equal(dangerous('data:image/png,x', 'img', 'src', false), true);
  assert.equal(dangerous('/a.png 1x, javascript:x 2x', 'img', 'srcset', true), true);
});
