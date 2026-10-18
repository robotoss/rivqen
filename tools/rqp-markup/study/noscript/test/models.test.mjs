// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Behavior of the two noscript models of the study (src/markup-study.mjs) on the
// known shapes, with parse5 8.0.1 as the oracle for the scripting-disabled view.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { compareWithView } from '../../../crosscheck/src/compare.mjs';
import { parse5View } from '../../../crosscheck/src/view.mjs';
import { ALLOW_B0, ALLOW_B1, analyzeStudy, checkAllowlist, checkNoscript } from '../src/markup-study.mjs';

const A = { noscript: 'main' };
const AFX = { noscript: 'main', foreignInContent: true };
const B0 = { noscript: 'allow', allow: ALLOW_B0 };
const B1 = { noscript: 'allow', allow: ALLOW_B1 };

const bytes = (s) => Buffer.from(s, 'utf8');

function study(input, opts) {
  return analyzeStudy(bytes(input), opts);
}

/** Status of a study result against parse5 in one view: agree, disagree or rejected. */
function against(input, opts, scripting) {
  return compareWithView(study(input, opts).result, parse5View(bytes(input), { scripting })).status;
}

const SPAN_BLOCK = '<span data-rq-block="k">y<a href="2">z</a>w</span>';
// markup.md section 5 item 2 (security review T-16, Java): H-37 inside C.
const T16 = `<object><a href="1"><noscript><table><marquee><table></table></marquee></table></noscript>x</a></object>${SPAN_BLOCK}`;
// H-36 inside C: nested forms behind an object, nine times, then a block.
const H36 = `<a href="1"><noscript>${'<form><object><form></form></object></form>'.repeat(9)}</noscript>x</a>${SPAN_BLOCK}`;

test('the shipped M-11 accepts the T-16 shape and parse5 without scripting has no block end tag', () => {
  assert.equal(study(T16).result.valid, true);
  assert.equal(against(T16, {}, true), 'agree');
  assert.equal(against(T16, {}, false), 'disagree');
});

test('model (a) rejects the T-16 shape by M-09 condition 2 on the tokens of C', () => {
  const s = study(T16, A);
  assert.equal(s.result.valid, false);
  assert.equal(s.result.error, 'RQP_MARKUP_STRUCTURE');
  assert.equal(s.why, 'A:M-09.2');
});

test('model (a) rejects the H-36 form pointer shape by M-10 item 2 on the tokens of C', () => {
  assert.equal(study(H36).result.valid, true);
  assert.equal(against(H36, {}, false), 'disagree');
  assert.equal(study(H36, A).why, 'A:M-10.2');
});

test('model (a) sees the form of the main pass from inside C', () => {
  const input = `<form><noscript><form>x</form></noscript></form>${SPAN_BLOCK}`;
  assert.equal(study(input).result.valid, true);
  assert.equal(study(input, A).why, 'A:M-10.2');
});

test('model (a) rejects H-38 and H-27 shapes, which the shipped items 5 and 6 also reject', () => {
  assert.equal(study('<table><tr><td><noscript><td></td></noscript></td></tr></table>', A).why, 'M-11.6');
  assert.equal(study('<noscript><button></noscript>', A).why, 'M-11.5');
});

test('model (a) rejects a table part in C with an outer table as F (M-09 on the C tokens)', () => {
  // A table opened in C makes item 6 hold; the col then has F = the table of C (allowed).
  assert.equal(study('<table><tr><td><noscript><table><col></table></noscript></td></tr></table>', A).result.valid, true);
  assert.equal(study('<table><noscript><table><tr><td>x</td></tr></table></noscript></table>', A).why, 'A:M-09.2');
});

test('model (a) item 7 rejects html, body and head tags in C', () => {
  for (const tag of ['<body></body>', '<html></html>', '<head></head>']) {
    assert.equal(study(`<noscript>${tag}</noscript>`).result.valid, true, tag);
    assert.equal(study(`<noscript>${tag}</noscript>`, A).why, 'A:M-11.7', tag);
  }
});

test('model (a) counts the elements of C in D (M-26), the shipped rule does not (item 7 of M-26)', () => {
  const deep = `<noscript>${'<span>'.repeat(300)}${'</span>'.repeat(300)}</noscript>`;
  assert.equal(study(deep).result.valid, true);
  const s = study(deep, A);
  assert.equal(s.result.error, 'RQP_MARKUP_LIMIT');
  assert.equal(s.why, 'A:M-26');
});

test('model (a) leaves the main token stack as it was before C (no divergence)', () => {
  const input = `<p><b><noscript><div><a href=x>y</a></div><img src=x></noscript></b></p>${SPAN_BLOCK}`;
  const s = study(input, A);
  assert.equal(s.result.valid, true);
  assert.equal(s.divergences, 0);
  assert.equal(s.noscripts, 1);
});

test('typical noscript fallbacks are valid in both models and agree with parse5 in both views', () => {
  const fallbacks = [
    '<noscript><img src="https://t.example/p.gif" height="1" width="1" style="display:none"></noscript>',
    '<noscript><iframe src="https://t.example/ns.html" height="0" width="0"></iframe></noscript>',
    '<noscript><div>Please enable JavaScript.</div></noscript>',
    '<noscript><p><a href="/x">Continue</a></p></noscript>',
    '<noscript><link rel="stylesheet" href="a.css"><style>.a{}</style></noscript>',
    '<noscript>Enable JavaScript</noscript>',
  ];
  for (const f of fallbacks) {
    const input = `<!DOCTYPE html><html><head></head><body>${f}${SPAN_BLOCK}</body></html>`;
    for (const opts of [A, B0]) {
      assert.equal(study(input, opts).result.valid, true, `${JSON.stringify(opts)} ${f}`);
      assert.equal(against(input, opts, true), 'agree', f);
      assert.equal(against(input, opts, false), 'agree', f);
    }
  }
});

test('model (b) rejects a start tag outside the allowlist and names it', () => {
  assert.equal(study('<noscript><b>x</b></noscript>', B0).why, 'B:b');
  assert.equal(study('<noscript><b>x</b></noscript>', B1).result.valid, true);
  assert.equal(study(T16, B0).why, 'B:table');
  assert.equal(checkAllowlist(bytes('<p>x</p><img>'), ALLOW_B0), null);
  assert.equal(checkAllowlist(bytes('<style><table></style>'), ALLOW_B0), null, 'raw text is not markup');
});

test('the a-fx variant allows a balanced svg in C and applies M-07 to it', () => {
  const icon = `<noscript><svg><path d="M0"></path></svg></noscript>${SPAN_BLOCK}`;
  assert.equal(study(icon, A).why, 'M-11.3:svg');
  assert.equal(study(icon, AFX).result.valid, true);
  assert.equal(against(icon, AFX, false), 'agree');
  assert.equal(study('<noscript><svg><p>x</p></svg></noscript>', AFX).why, 'A:M-07.1');
  assert.equal(study('<noscript><svg><path d="M0"></svg></noscript>', AFX).why, 'M-11.5');
  assert.equal(study('<noscript><svg><path/></svg></noscript>', AFX).result.valid, true);
});

test('checkNoscript keeps the shipped items 1 to 6', () => {
  assert.equal(checkNoscript(bytes('<img src=x>')), null);
  assert.deepEqual(checkNoscript(bytes('<!--x')), { error: 'RQP_MARKUP_STRUCTURE', why: 'M-11.1' });
  assert.deepEqual(checkNoscript(bytes('<p data-rq-block="a">x</p>')), { error: 'RQP_MARKUP_STRUCTURE', why: 'M-11.2' });
  assert.deepEqual(checkNoscript(bytes('<svg></svg>')), { error: 'RQP_MARKUP_STRUCTURE', why: 'M-11.3:svg' });
  assert.deepEqual(checkNoscript(bytes('<script type="application/rivqen-manifest+json">{}</script>')), {
    error: 'RQP_MARKUP_RESERVED',
    why: 'M-11.4',
  });
  assert.deepEqual(checkNoscript(bytes('<div>')), { error: 'RQP_MARKUP_STRUCTURE', why: 'M-11.5' });
  assert.deepEqual(checkNoscript(bytes('<td></td>')), { error: 'RQP_MARKUP_STRUCTURE', why: 'M-11.6' });
});

test('the noscript content to the end of the input is checked by each model', () => {
  assert.equal(study('<p data-rq-block="a">x</p><noscript><img src=x>', A).result.valid, true);
  assert.equal(study('<p data-rq-block="a">x</p><noscript><form><form></form></form>', A).why, 'A:M-10.2');
  assert.equal(study('<p data-rq-block="a">x</p><noscript><b>x</b>', B0).why, 'B:b');
});

test('an unknown model is an error of the caller', () => {
  assert.throws(() => analyzeStudy(bytes('x'), { noscript: 'other' }), RangeError);
  assert.throws(() => analyzeStudy('x'), TypeError);
});
