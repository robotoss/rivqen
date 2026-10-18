// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Behavior of the head rules of the study (src/head-study.mjs), with parse5 8.0.1 as
// the oracle: the head end tag that a rule finds is the end of the head element in
// both views, and an inserted manifest element stays in the head.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { HEAD_NOSCRIPT_H1, HEAD_NOSCRIPT_H2, headNoscriptFailure, headWalk } from '../src/head-study.mjs';
import { headView, manifestView } from '../src/views.mjs';

const bytes = (s) => Buffer.from(s, 'utf8');
const doc = (c) => `<!DOCTYPE html><html><head><meta charset="utf-8"><noscript>${c}</noscript><title>T</title></head><body><p>x</p></body></html>`;
const at = (input, rule) => headWalk(bytes(input), { rule }).at;

/** The parse5 head end in one view, and whether the manifest stays in the head there. */
function parse5Head(input, scripting) {
  const b = bytes(input);
  const end = headView(b, scripting).end;
  const m = end >= 0 ? manifestView(b, end, scripting) : null;
  return { end, manifest: m !== null && m.inHead && m.headEnd };
}

test('the shipped rule finds </head> after a noscript with body content; parse5 without scripting ends the head earlier', () => {
  const input = doc('<img src="https://t.example/p.gif">');
  const p = at(input, 'shipped');
  assert.ok(p > 0);
  assert.equal(parse5Head(input, true).end, p);
  assert.equal(parse5Head(input, false).end, -1, 'the head ends at the img, without an end tag');
  for (const rule of ['h0', 'h1', 'h2']) assert.equal(at(input, rule), -1, rule);
});

test('h1 and h2 keep the head end for in-head-noscript content, and parse5 agrees in both views', () => {
  for (const c of ['<link rel="stylesheet" href="a.css">', '<style>.a{}</style>', '<meta http-equiv="refresh" content="0">', ' \n\t', '<!--x-->', '<noframes>x</noframes>', '<basefont><bgsound>', '</p></meta>', '']) {
    const input = doc(c);
    const p = at(input, 'h1');
    assert.ok(p > 0, c);
    assert.equal(at(input, 'h2'), p, c);
    assert.equal(at(input, 'h0'), -1, c);
    for (const scripting of [true, false]) {
      const v = parse5Head(input, scripting);
      assert.equal(v.end, p, `${c} scripting ${scripting}`);
      assert.equal(v.manifest, true, `${c} scripting ${scripting}`);
    }
  }
});

test('h2 also keeps base, script and title, which close the noscript early but not the head (parse5 agrees)', () => {
  for (const c of ['<script>1</script>', '<title>t</title>', '<base href="/">', '<script>1</script><link rel=x>']) {
    const input = doc(c);
    assert.equal(at(input, 'h1'), -1, c);
    const p = at(input, 'h2');
    assert.ok(p > 0, c);
    assert.equal(parse5Head(input, false).end, p, c);
    assert.equal(parse5Head(input, false).manifest, true, c);
  }
});

test('text, body tags, document tags and the end tags head, body, html, br in C mean: no head end tag', () => {
  for (const c of ['Enable JavaScript', '&nbsp;', '<div>x</div>', '<iframe src=x></iframe>', '<a href=x>y</a>', '</head>', '</body>', '</html>', '</br>', '<head>', '<html>', '<template></template>']) {
    const input = doc(c);
    assert.ok(at(input, 'shipped') > 0, c);
    assert.equal(at(input, 'h1'), -1, c);
    assert.equal(at(input, 'h2'), -1, c);
  }
});

test('a noscript in the body does not change the head rules', () => {
  const input = '<!DOCTYPE html><html><head><title>T</title></head><body><noscript><img src=x></noscript></body></html>';
  const p = at(input, 'shipped');
  for (const rule of ['h0', 'h1', 'h2']) assert.equal(at(input, rule), p, rule);
  assert.equal(headWalk(bytes(input), { rule: 'h1' }).headNoscripts, 0);
});

test('a noscript in a template in the head is template content, not head content', () => {
  const input = '<!DOCTYPE html><html><head><template><noscript><img src=x></noscript></template></head><body></body></html>';
  const p = at(input, 'shipped');
  assert.ok(p > 0);
  assert.equal(at(input, 'h1'), p);
  assert.equal(parse5Head(input, false).end, p);
});

test('a head noscript without an end tag leaves no head end tag in every rule', () => {
  const input = '<!DOCTYPE html><html><head><noscript><link rel=x></head><body></body></html>';
  for (const rule of ['shipped', 'h0', 'h1', 'h2']) assert.equal(at(input, rule), -1, rule);
});

test('headNoscriptFailure names the first token that leaves the head', () => {
  assert.equal(headNoscriptFailure(bytes('<link><img>'), HEAD_NOSCRIPT_H1), '<img>');
  assert.equal(headNoscriptFailure(bytes(' x '), HEAD_NOSCRIPT_H1), 'text');
  assert.equal(headNoscriptFailure(bytes('</body>'), HEAD_NOSCRIPT_H2), '</body>');
  assert.equal(headNoscriptFailure(bytes('<style>a</style><title>x</title>'), HEAD_NOSCRIPT_H2), null);
  assert.equal(headNoscriptFailure(bytes('<style>unclosed'), HEAD_NOSCRIPT_H2), 'eof');
  assert.equal(headNoscriptFailure(bytes('<!--x'), HEAD_NOSCRIPT_H1), 'eof');
});

test('an unknown head rule is an error of the caller', () => {
  assert.throws(() => headWalk(bytes(''), { rule: 'h9' }), RangeError);
});
