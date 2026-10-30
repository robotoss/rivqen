// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import { readdirSync } from 'node:fs';
import { test } from 'node:test';
import { PROFILES } from '../src/model.mjs';
import { rebuild, split } from '../src/split.mjs';
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
