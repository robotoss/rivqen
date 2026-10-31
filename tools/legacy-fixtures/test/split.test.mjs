// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import { readdirSync } from 'node:fs';
import { test } from 'node:test';
import { PROFILES } from '../src/model.mjs';
import { MAX_NAME, rebuild, split } from '../src/split.mjs';
import { page, pagesDir } from './helpers.mjs';

const block = (name, content) => `<!--sonicdiff-${name}-->${content}<!--sonicdiff-${name}-end-->`;

// Pages whose split is consistent in every profile: a rebuild gives the page back.
const CONSISTENT = ['a', 'b', 'c', 'nomark', 'titleattr', 'spaces', 'nested', 'mismatch', 'uppercase', 'unicode'];

test('split_then_rebuild_gives_the_page_bytes_for_pages_with_a_consistent_split', () => {
  for (const profile of PROFILES) {
    for (const id of CONSISTENT) {
      const text = page(id).toString('utf8');
      const parts = split(profile, text);
      assert.equal(parts.ok, true, `${profile} ${id}`);
      assert.deepEqual(Buffer.from(rebuild(parts.template, parts.data), 'utf8'), page(id), `${profile} ${id}`);
    }
  }
});

test('split_then_rebuild_property_over_generated_pages', () => {
  // fixed-seed generator (LCG): pages from static text, an optional title and unique blocks
  let seed = 20261030;
  const next = (n) => {
    seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
    return seed % n;
  };
  const texts = ['<p>x</p>', 'é€😀', '\n\t', '{curly}', 'a--b', '<div class="c">', '</div>'];
  for (let i = 0; i < 300; i++) {
    let doc = '';
    const parts = 1 + next(8);
    for (let j = 0; j < parts; j++) {
      const kind = next(4);
      if (kind === 0) doc += texts[next(texts.length)];
      else if (kind === 1 && !doc.includes('<title')) doc += `<title>T${next(100)}</title>`;
      else if (kind === 2) doc += block(`b${j}`, texts[next(texts.length)] + 'x');
      else doc += texts[next(texts.length)];
    }
    for (const profile of PROFILES) {
      const result = split(profile, doc);
      assert.equal(result.ok, true);
      assert.equal(rebuild(result.template, result.data), doc, `${profile}: ${JSON.stringify(doc)}`);
      assert.equal([...result.data.keys()][0], '{title}');
    }
  }
});

test('split_never_throws_and_the_key_order_is_title_then_document_order', () => {
  const hostile = ['', '<title', '<title>', '</title><title>', '<!--sonicdiff-', '<!--sonicdiff--><!--sonicdiff-end-->', '\u0000\ud800', 'İ'.repeat(10)];
  for (const profile of PROFILES) for (const text of hostile) assert.doesNotThrow(() => split(profile, text));
  const parts = split('legacy-canonical', `${block('z', '1')}${block('a', '2')}`);
  assert.deepEqual([...parts.data.keys()], ['{title}', '{z}', '{a}']);
  assert.equal(parts.blocks, 2);
  assert.equal(parts.data.get('{title}'), '');
});

test('title_does_not_span_a_line_break', () => {
  const text = '<title>Shop\nNew</title><p>x</p>';
  const parts = split('legacy-canonical', text);
  assert.equal(parts.data.get('{title}'), '');
  assert.equal(parts.template, text);
  const next = split('legacy-canonical', '<title>a\nb</title><title>c</title>');
  assert.equal(next.data.get('{title}'), '<title>c</title>');
});

test('block_match_is_lazy_case_insensitive_and_does_not_check_the_end_name', () => {
  const parts = split('legacy-canonical', '<!--SonicDiff-Ab--><p>1</p><!--sonicdiff-zz-end--><!--sonicdiff-cd--><p>2</p><!--sonicdiff-cd-end-->');
  assert.deepEqual([...parts.data.keys()], ['{title}', '{Ab}', '{cd}']);
  assert.equal(parts.template, '{Ab}{cd}');
  assert.equal(split('legacy-canonical', '<!--sonicdiff-a--><!--sonicdiff-a-end-->').blocks, 0);
});

test('names_compare_case_sensitively_for_duplicates', () => {
  const parts = split('legacy-canonical', block('a', '1') + block('A', '2'));
  assert.equal(parts.ok, true);
  assert.deepEqual([...parts.data.keys()], ['{title}', '{a}', '{A}']);
  assert.deepEqual(split('legacy-canonical', block('a', '1') + block('a', '2')), { ok: false, reason: 'DUPLICATE_BLOCK_NAME' });
});

test('no_profile_reproduces_the_several_title_defect', () => {
  const text = page('titles').toString('utf8');
  const expected = split('legacy-canonical', text).template;
  for (const profile of PROFILES) assert.equal(split(profile, text).template, expected, profile);
  assert.ok(expected.startsWith('<!DOCTYPE html><html><head>{title}</head>'));
  assert.ok(expected.includes('<svg><title>Icon</title></svg>'));
});

test('rebuild_replaces_each_placeholder_once_in_template_order', () => {
  const data = new Map([['{a}', 'A'], ['{b}', 'B{a}']]);
  assert.equal(rebuild('{b}-{a}-{a}-{c}-{}', data), 'B{a}-A-{a}-{c}-{}');
  assert.equal(rebuild('', data), '');
});

test('every_lab_page_splits_in_every_profile_without_throwing', () => {
  for (const name of readdirSync(pagesDir())) {
    for (const profile of PROFILES) assert.doesNotThrow(() => split(profile, page(name.replace(/\.html$/, '')).toString('utf8')));
  }
});

// ---- C-16: linear time, and exactly the results of the expressions (DL-077) ----

// The oracle: the expressions of legacy wire C-06 and C-07, used only here. They take
// quadratic time on unclosed openers, so the property test keeps the pages small.
const ORACLE_TITLE = /<title([^\n]*?)<\/title>/i;
const ORACLE_BLOCK = /<!--sonicdiff-?(\w*)-->([\s\S]+?)<!--sonicdiff-?(\w*)-end-->/gi;

function oracleSplit(profile, text) {
  const data = new Map();
  const title = ORACLE_TITLE.exec(text);
  let template = text;
  if (title !== null) template = text.slice(0, title.index) + '{title}' + text.slice(title.index + title[0].length);
  data.set('{title}', title === null ? '' : title[0]);
  const canonical = profile === 'legacy-canonical';
  let blocks = 0;
  let auto = 0;
  let failed = false;
  template = template.replace(ORACLE_BLOCK, (whole, name) => {
    let key;
    if (name === '' && profile === 'legacy-node') key = `{auto${auto++}}`;
    else if (canonical && (name === '' || name.length > MAX_NAME)) return whole;
    else key = `{${name}}`;
    if (canonical && data.has(key)) failed = true;
    data.set(key, whole);
    blocks++;
    return key;
  });
  if (failed) return { ok: false, reason: 'DUPLICATE_BLOCK_NAME' };
  return { ok: true, template, data, blocks };
}

const KIB = 1024;
const LIMIT_MS = 100;

/** Milliseconds of one split call (the best of three, against scheduler noise). */
function timeSplit(text) {
  let best = Infinity;
  for (let i = 0; i < 3; i++) {
    const start = performance.now();
    split('legacy-canonical', text);
    best = Math.min(best, performance.now() - start);
  }
  return best;
}

const fill = (unit, bytes) => unit.repeat(Math.ceil(bytes / unit.length)).slice(0, bytes);

test('split_title_linear_on_many_openers', () => {
  const inputs = {
    'openers only': fill('<title', 256 * KIB),
    'openers, LF, close': `${fill('<title', 256 * KIB - 9)}\n</title>`,
    'mixed case openers': fill('<TiTlE x', 256 * KIB),
  };
  for (const [what, text] of Object.entries(inputs)) {
    assert.equal(text.length >= 256 * KIB, true);
    const ms = timeSplit(text);
    assert.ok(ms < LIMIT_MS, `${what}: ${ms.toFixed(1)} ms`);
    const parts = split('legacy-canonical', text);
    assert.equal(parts.data.get('{title}'), '', what);
    assert.equal(parts.template, text, what);
  }
});

test('split_blocks_linear_on_unclosed_openers', () => {
  const inputs = {
    'named openers': fill('<!--sonicdiff-a-->x', 256 * KIB),
    'bare markers': fill('<!--sonicdiff-', 256 * KIB),
    'openers and near end markers': fill('<!--sonicdiff-a-->x<!--sonicdiff-a-end--', 256 * KIB),
    'empty openers': fill('<!--sonicdiff-->', 256 * KIB),
  };
  for (const [what, text] of Object.entries(inputs)) {
    const ms = timeSplit(text);
    assert.ok(ms < LIMIT_MS, `${what}: ${ms.toFixed(1)} ms`);
    const parts = split('legacy-canonical', text);
    assert.equal(parts.blocks, 0, what);
    assert.equal(parts.template, text, what);
  }
  // many closed blocks: linear too, and every block is found
  const many = Array.from({ length: 8000 }, (_, i) => block(`n${i}`, 'x')).join('');
  assert.ok(many.length >= 256 * KIB);
  assert.ok(timeSplit(many) < LIMIT_MS);
  assert.equal(split('legacy-canonical', many).blocks, 8000);
});

test('split_equals_regex_on_random_pages', () => {
  // fixed-seed generator (LCG); fragments near the edges of the expressions
  let seed = 16;
  const next = (n) => {
    seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
    return seed % n;
  };
  // the markup fragments come twice, so that whole titles and blocks are frequent
  const markup = ['<title', '</title>', '<title>', '\n', '<!--sonicdiff', '<!--sonicdiff-', '-->', '-end-->', '-', 'a', 'b_1', '9'];
  const fragments = [
    ...markup, ...markup,
    '\r', '--', 'Ab_9', 'end', 'title', '{x}', 'x', ' ', 'é', '\ud800', '😀', '<', '>', '!', 'ſ', 'ı', 'K', 'İ',
    '<!--sonicdiff-end-->', '<!--sonicdiff--->', '<!--sonicdiff-x-end-->', '<!--sonicdiffend-->',
  ];
  const mixCase = (text) => [...text].map((c) => (next(2) === 0 ? c.toUpperCase() : c)).join('');
  let matchedBlocks = 0;
  let titles = 0;
  for (let i = 0; i < 3000; i++) {
    let doc = '';
    const count = 1 + next(24);
    for (let j = 0; j < count; j++) {
      const f = fragments[next(fragments.length)];
      doc += next(3) === 0 ? mixCase(f) : f;
    }
    for (const profile of PROFILES) {
      const want = oracleSplit(profile, doc);
      const got = split(profile, doc);
      assert.deepEqual(got, want, `${profile}: ${JSON.stringify(doc)}`);
      if (profile === 'legacy-node' && got.ok) {
        matchedBlocks += got.blocks;
        if (got.data.get('{title}') !== '') titles++;
      }
    }
  }
  // the generator reaches the interesting cases
  assert.ok(matchedBlocks > 200, `blocks ${matchedBlocks}`);
  assert.ok(titles > 200, `titles ${titles}`);
});

test('split_scan_edge_cases_equal_the_expressions', () => {
  const cases = [
    '<title</title>', // empty content
    '<title><title>x</title>', // the first opener matches up to the first close
    '<title>a\n<title>b</title>', // the first opener fails at the LF; the second matches
    '<title>a</title\n></title>',
    '<title\n</title>', // the LF directly before the close
    '<title>a\n</title><title>b</title>',
    '<TITLE>x</TiTlE>',
    '<tİtle>x</title>', // no Unicode case folding
    '<!--sonicdiff-end-->x<!--sonicdiff-end-->', // an opener named end
    '<!--sonicdiff--->x<!--sonicdiff-end-->', // -? takes the dash, empty name
    '<!--sonicdiffab-->x<!--sonicdiffzz-end-->', // no dash before the names
    '<!--sonicdiff-a--><!--sonicdiff-a-end-->', // content needs one code unit
    '<!--sonicdiff-a-->x<!--sonicdiff-a-->y<!--sonicdiff-b-end--><!--sonicdiff-c-end-->', // lazy; no overlap
    '<!--sonicdiff-a-->x<!--sonicdiff-a-END-->',
    '<!--SONICDIFF-Ab-->x<!--SonicDiff-zz-End-->',
    '<!--sonicdiff-ſ-->x<!--sonicdiff-a-end-->', // ſ is not \w
    '<!--sonicdiff-a_1-->x<!--sonicdiff-9-end--><!--sonicdiff_-->y<!--sonicdiff0-end-->', // digits and _ are \w
    '<!--sonicdiff-a-->x<!--sonicdiff-a-end--><!--sonicdiff-a-->y', // the last opener is unclosed
    `<!--sonicdiff-${'p'.repeat(129)}-->x<!--sonicdiff-end-->`,
  ];
  for (const text of cases) {
    for (const profile of PROFILES) assert.deepEqual(split(profile, text), oracleSplit(profile, text), `${profile}: ${JSON.stringify(text)}`);
  }
});

test('split_of_the_openers_page_keeps_the_unclosed_openers_in_the_template', () => {
  const text = page('openers').toString('utf8');
  assert.ok(Buffer.byteLength(text) < 64 * KIB);
  const parts = split('legacy-canonical', text);
  assert.deepEqual([...parts.data.keys()], ['{title}', '{price}', '{stock}']);
  assert.equal(parts.data.get('{title}'), '<title>Shop</title>');
  assert.ok(parts.template.includes('<title'.repeat(2000) + '<!--sonicdiff-a-->x'.repeat(2000)));
  assert.equal(rebuild(parts.template, parts.data), text);
});
