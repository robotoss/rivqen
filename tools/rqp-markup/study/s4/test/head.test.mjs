// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Behavior of the H-79 study: the head rules, the client rules c1 and c2, and the
// parse5 checks of src/head-views.mjs (both scripting views).

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { IMPLIED_H79, clientAccepts, fullDocument, headPosition } from '../src/head.mjs';
import { checkBothViews, checkView } from '../src/head-views.mjs';
import { boundaries, checkServer } from '../src/head-sweep.mjs';
import { protocolDocuments } from '../src/inputs.mjs';
import { findManifestElements } from '../../../../rqp-protocol/src/markup.mjs';

const B = (s) => Buffer.from(s, 'utf8');

test('h79: a start tag of the list before any head start tag gives the "<" of that tag', () => {
  for (const name of IMPLIED_H79) {
    const doc = B(`<!DOCTYPE html><html lang="en">\n<${name}>`);
    assert.deepEqual(headPosition(doc, 'h79'), { at: 32, kind: 'implied', stop: `<${name}>` }, name);
    assert.equal(headPosition(doc, 'shipped').kind, 'none', name);
  }
});

test('h79: noscript, template, body content, text and the end tags of item 1 still give no insertion point', () => {
  for (const s of ['<noscript></noscript>', '<template></template>', '<div>x', 'x<meta>', '</br><meta>', '</head><meta>', '</><meta>', '']) {
    assert.equal(headPosition(B(s), 'h79').kind, 'none', s);
  }
  assert.equal(headPosition(B('<noscript></noscript>'), 'l1').kind, 'implied');
  assert.equal(headPosition(B('<div>x'), 'l2').kind, 'implied');
  assert.equal(headPosition(B('x<div>'), 'l2').kind, 'none');
});

test('h79: an explicit head start tag keeps its insertion point, also after a byte order mark and white space', () => {
  assert.deepEqual(headPosition(B('<html><head lang="x"><meta>'), 'h79'), { at: 21, kind: 'explicit', stop: '<head>' });
  assert.equal(headPosition(B('﻿ \n<meta>'), 'h79').at, 5);
  assert.equal(headPosition(B('<meta><head>'), 'h79').at, 0, 'the walk stops at the first tag of the list');
});

test('server F of an implied head: the manifest is the first child and the only manifest, the tree of O is unchanged (both views)', () => {
  const docs = [
    '<!DOCTYPE html><html lang="en">\n<meta charset="utf-8"><title>T</title><noscript><img src="x"></noscript><template><p>t</p></template><!--c--><html data-x="1">text<div data-rq-block="a">b</div>',
    '<meta charset="utf-8"><head lang="x"><title>t</title></head><body class="b">x',
    '<script>var a = 1 < 2;</script><noscript><div>d</div></noscript><body>x',
    '﻿<link rel="icon" href="i">\n  <style>a{}</style>x',
    '<base href="/"><noscript><meta http-equiv="refresh" content="0"></noscript></head><p>x',
  ];
  for (const s of docs) {
    const r = checkServer(B(s), 'h79');
    assert.equal(r.position.kind, 'implied', s);
    assert.deepEqual(r.failures, [], s);
  }
});

test('c2 rejects a manifest that the server would not write; c1 accepts it and the tree changes', () => {
  // The manifest before an explicit head start tag with attributes: c1 (walk on F) stops
  // at the manifest itself; in the browser the later head start tag is ignored.
  const o = B('<html><head lang="x"><title>t</title>');
  const { full, length } = fullDocument(o, 6);
  assert.deepEqual(clientAccepts(full, 6, 6 + length, 'h79'), { c1: true, c2: false });
  const v = checkBothViews(o, full);
  assert.equal(v.pass, false);
  assert.deepEqual(v.failures, ['enabled-same', 'disabled-same']);
  // White space after the manifest: c2 rejects, as client item 3 does for an explicit head.
  const o2 = B('<html>\n<meta charset="utf-8">');
  const { full: f2, length: l2 } = fullDocument(o2, 6);
  assert.equal(clientAccepts(f2, 6, 6 + l2, 'h79').c2, false);
});

test('the parse5 checks find a manifest that is not the first child and a second manifest', () => {
  const o = B('<html><head>\n<title>t</title></head>');
  // After white space in the head: a text node is the first child of the head.
  const { full } = fullDocument(o, 13);
  const late = checkView(o, full, true);
  assert.equal(late.first, false);
  assert.equal(late.only, true);
  // Two manifests: also the second one is found (both views).
  const twice = fullDocument(fullDocument(o, 12).full, 12).full;
  const v = checkBothViews(o, twice);
  assert.ok(v.failures.includes('enabled-only') && v.failures.includes('disabled-only'), v.failures.join());
});

test('boundaries: the token boundaries up to the end of the token that ends item 1', () => {
  assert.deepEqual(boundaries(B('<!--c--> <meta><p>')), [0, 8, 9, 15]);
  assert.deepEqual(boundaries(B('x')), [0, 1]);
});

test('protocol full documents: the h79 client rules give the shipped answer (no stored F changes)', () => {
  let n = 0;
  for (const [name, bytes] of protocolDocuments()) {
    const found = findManifestElements(bytes);
    if (found === null || found.elements.length !== 1 || found.elements[0].end < 0) continue;
    n++;
    const el = found.elements[0];
    const shipped = clientAccepts(bytes, el.start, el.end, 'shipped');
    const h79 = clientAccepts(bytes, el.start, el.end, 'h79');
    assert.equal(h79.c2, shipped.c1, name);
    assert.equal(h79.c1, shipped.c1, name);
  }
  assert.ok(n >= 40, `only ${n} full documents`);
});
