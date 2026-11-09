// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Exact page output of the renderers: badges, cells, notes, links, escaping, width rule.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { esc, renderAndroid, renderIos, widthProblems } from '../src/render.mjs';

const BLOB = 'https://github.com/robotoss/rivqen/blob/main/';
const r = (os, verdict, extra = {}) => ({ os, webview: null, verdict, condition: null, runs: [], recording: null, ...extra });
const c = (id, platform, results, verdict, extra = {}) => ({
  id, platform, title: `T ${id}`, prototype: 'P', questions: ['Q-1'], sprint: 'S1', results, verdict, ...extra,
});
const badge = (type, text) => `<Badge type="${type}" text="${text}" />`;
const section = (page, from, to) => page.split(from)[1].split(to)[0];
const notesOf = (page) => page.split('## Notes\n\n')[1];

const small = () => [
  c('CAP-A-01', 'android', [r('37', 'supported'), r('29', 'conditional')], 'conditional'),
  c('CAP-F-01', 'ffi', [r('host', 'supported')], 'supported'),
  c('CAP-U-01', 'url', [r('host', 'pending')], 'pending'),
  c('CAP-I-01', 'ios', [r('27.0', 'supported')], 'supported'),
];

const PREAMBLE_TAIL =
  `<Badge type="tip" text="FACT" /> This page is generated from [capabilities.json](${BLOB}tools/platform-spikes/capabilities.json) by ` +
  `[matrix/gen.mjs](${BLOB}tools/platform-spikes/matrix/gen.mjs). Do not edit it by hand. ` +
  'Change the catalog and run `node tools/platform-spikes/matrix/gen.mjs`.\n\n';
const verdicts = (platform) =>
  `Verdicts: ${badge('tip', 'supported')} ${badge('warning', 'conditional')} ${badge('danger', 'impossible')} ${badge('info', 'pending')}. ` +
  `The ${platform} verdict of a capability is the weakest verdict of its versions. ` +
  'The number in the first column is the note below the tables.';

test('android page: exact text of title, preamble, table, shared rows and notes', () => {
  const page = renderAndroid(small());
  const expected =
    '# Android capability matrix\n\n' +
    PREAMBLE_TAIL +
    `${verdicts('Android')}\n\n` +
    '## Capabilities\n\nColumns are API levels.\n\n' +
    '| # | ID | 37 | 29 | Verdict |\n|---|---|---|---|---|\n' +
    `| 1 | CAP-A-01 | ${badge('tip', 'supported')} | ${badge('warning', 'conditional')} | ${badge('warning', 'conditional')} |\n\n` +
    '## Shared rows (FFI and URL)\n\n';
  assert.ok(page.startsWith(expected), page.slice(0, expected.length + 40));
  const rest = page.slice(expected.length);
  const [intro, tableAndNotes] = rest.split('\n\n| # |');
  assert.match(
    intro,
    /^These rows are the same on the Android and the iOS page \(the note numbers differ\)\.\s?Each result in the notes is one target: host, device or simulator\.$/,
  );
  assert.equal(
    `| # |${tableAndNotes}`,
    '| # | ID | Verdict |\n|---|---|---|\n' +
      `| 2 | CAP-F-01 | ${badge('tip', 'supported')} |\n` +
      `| 3 | CAP-U-01 | ${badge('info', 'pending')} |\n\n` +
      '## Notes\n\n' +
      '1. **CAP-A-01** T CAP-A-01. Prototype P, sprint S1, questions Q-1.\n' +
      '   - **37**: supported.\n' +
      '   - **29**: conditional.\n' +
      '2. **CAP-F-01** T CAP-F-01. Prototype P, sprint S1, questions Q-1.\n' +
      '   - **host**: supported.\n' +
      '3. **CAP-U-01** T CAP-U-01. Prototype P, sprint S1, questions Q-1.\n' +
      '   - **host**: pending.\n',
  );
});

test('android page without shared capabilities has no shared section and no empty block', () => {
  const page = renderAndroid(small().filter((x) => x.platform === 'android'));
  assert.doesNotMatch(page, /Shared rows/);
  assert.doesNotMatch(page, /\n\n\n/);
  assert.match(page, /\| 1 \| CAP-A-01 \|[^\n]*\|\n\n## Notes\n\n1\. \*\*CAP-A-01\*\*/);
  assert.ok(page.endsWith('   - **29**: conditional.\n'));
});

test('android page lists only android capabilities in the main table, shared ones after them', () => {
  const page = renderAndroid(small());
  const main = section(page, '## Capabilities', '## Shared rows');
  assert.match(main, /CAP-A-01/);
  assert.doesNotMatch(main, /CAP-I-01|CAP-F-01|CAP-U-01/);
  const shared = section(page, '## Shared rows', '## Notes');
  assert.doesNotMatch(shared, /CAP-A-01|CAP-I-01/);
});

test('shared rows are numbered after the platform capabilities, one number per row', () => {
  const cat = [
    c('CAP-A-01', 'android', [r('37', 'supported')], 'supported'),
    c('CAP-A-02', 'android', [r('37', 'supported')], 'supported'),
    c('CAP-F-01', 'ffi', [r('host', 'supported')], 'supported'),
    c('CAP-U-01', 'url', [r('host', 'supported')], 'supported'),
    c('CAP-I-01', 'ios', [r('27.0', 'supported')], 'supported'),
    c('CAP-I-02', 'ios', [r('27.0', 'supported')], 'supported'),
  ];
  const a = renderAndroid(cat);
  assert.match(a, /\| 3 \| CAP-F-01 \|/);
  assert.match(a, /\| 4 \| CAP-U-01 \|/);
  assert.match(a, /^3\. \*\*CAP-F-01\*\*/m);
  assert.match(a, /^4\. \*\*CAP-U-01\*\*/m);
  const i = renderIos(cat);
  assert.match(i, /\| 3 \| CAP-F-01 \|/);
  assert.match(i, /\| 4 \| CAP-U-01 \|/);
  assert.match(i, /^3\. \*\*CAP-F-01\*\*/m);
  assert.match(i, /^4\. \*\*CAP-U-01\*\*/m);
});

test('the url platform is a shared row on the ios page, and an ios capability is not', () => {
  const page = renderIos(small());
  const shared = section(page, '## Shared rows', '## Notes');
  assert.match(shared, /CAP-U-01/);
  assert.match(shared, /CAP-F-01/);
  assert.doesNotMatch(shared, /CAP-I-01|CAP-A-01/);
});

test('each verdict has its badge type; an unknown verdict falls back to info', () => {
  const cat = [
    c('CAP-A-01', 'android', [r('1', 'supported')], 'supported'),
    c('CAP-A-02', 'android', [r('1', 'conditional')], 'conditional'),
    c('CAP-A-03', 'android', [r('1', 'impossible')], 'impossible'),
    c('CAP-A-04', 'android', [r('1', 'not-tested')], 'not-tested'),
    c('CAP-A-05', 'android', [r('1', 'pending')], 'pending'),
    c('CAP-A-06', 'android', [r('1', 'weird')], 'weird'),
  ];
  const page = renderAndroid(cat);
  const row = (id) => page.split('\n').find((l) => l.startsWith('|') && l.includes(id));
  assert.equal(row('CAP-A-01'), `| 1 | CAP-A-01 | ${badge('tip', 'supported')} | ${badge('tip', 'supported')} |`);
  assert.equal(row('CAP-A-02'), `| 2 | CAP-A-02 | ${badge('warning', 'conditional')} | ${badge('warning', 'conditional')} |`);
  assert.equal(row('CAP-A-03'), `| 3 | CAP-A-03 | ${badge('danger', 'impossible')} | ${badge('danger', 'impossible')} |`);
  assert.equal(row('CAP-A-04'), `| 4 | CAP-A-04 | ${badge('info', 'not-tested')} | ${badge('info', 'not-tested')} |`);
  assert.equal(row('CAP-A-05'), `| 5 | CAP-A-05 | ${badge('info', 'pending')} | ${badge('info', 'pending')} |`);
  assert.equal(row('CAP-A-06'), `| 6 | CAP-A-06 | ${badge('info', 'weird')} | ${badge('info', 'weird')} |`);
});

test('a version without a result for a capability shows none', () => {
  const cat = [
    c('CAP-A-01', 'android', [r('37', 'supported'), r('29', 'supported')], 'supported'),
    c('CAP-A-02', 'android', [r('37', 'impossible')], 'impossible'),
  ];
  const page = renderAndroid(cat);
  assert.match(page, /^\| 2 \| CAP-A-02 \| <Badge type="danger" text="impossible" \/> \| none \| <Badge type="danger" text="impossible" \/> \|$/m);
});

test('result line: webview, condition and note become sentences, in that order', () => {
  const cat = [
    c('CAP-A-01', 'android', [
      r('37', 'supported', { webview: 'Chrome 120', condition: 'Only on wifi', note: 'Measured twice' }),
      r('36', 'supported', { condition: 'Ends with bang!', note: 'Why?' }),
      r('35', 'supported', { note: 'a.b' }),
      r('34', 'supported', { condition: '', note: '' }),
    ], 'supported'),
  ];
  const lines = notesOf(renderAndroid(cat)).split('\n');
  assert.equal(lines[1], '   - **37** (Chrome 120): supported. Only on wifi. Measured twice.');
  assert.equal(lines[2], '   - **36**: supported. Ends with bang! Why?');
  assert.equal(lines[3], '   - **35**: supported. a.b.');
  assert.equal(lines[4], '   - **34**: supported.');
});

test('result line: note alone is shown without a condition', () => {
  const cat = [c('CAP-A-01', 'android', [r('37', 'supported', { note: 'Only a note' })], 'supported')];
  assert.equal(notesOf(renderAndroid(cat)).split('\n')[1], '   - **37**: supported. Only a note.');
});

test('result line: links in order runs, metrics, recording, source, separated by comma', () => {
  const cat = [
    c('CAP-A-01', 'android', [
      r('37', 'supported', {
        runs: ['e/a.jsonl', 'e/b.jsonl'], metrics: 'e/m.json', recording: 'e/v.mp4', source: 'https://example.com/s',
      }),
    ], 'supported'),
  ];
  assert.equal(
    notesOf(renderAndroid(cat)).split('\n')[1],
    `   - **37**: supported. [run 1](${BLOB}e/a.jsonl), [run 2](${BLOB}e/b.jsonl), [metrics](${BLOB}e/m.json), ` +
      `[recording](${BLOB}e/v.mp4), [source](https://example.com/s)`,
  );
});

test('result line: each single link kind is shown alone, and none adds no trailing text', () => {
  const only = (extra) =>
    notesOf(renderAndroid([c('CAP-A-01', 'android', [r('37', 'supported', extra)], 'supported')])).split('\n')[1];
  assert.equal(only({ metrics: 'm.json' }), `   - **37**: supported. [metrics](${BLOB}m.json)`);
  assert.equal(only({ recording: 'v.mp4' }), `   - **37**: supported. [recording](${BLOB}v.mp4)`);
  assert.equal(only({ source: 'https://x.org/' }), '   - **37**: supported. [source](https://x.org/)');
  assert.equal(only({ runs: ['one.jsonl'] }), `   - **37**: supported. [run 1](${BLOB}one.jsonl)`);
  assert.equal(only({ runs: undefined }), '   - **37**: supported.');
  assert.equal(only({ metrics: null, recording: null, source: null }), '   - **37**: supported.');
});

test('result line escapes raw tags in os, webview, condition and note', () => {
  const cat = [c('CAP-A-01', 'android', [r('<os>', 'supported', { webview: '<wv>', condition: '<c>', note: '<n>' })], 'supported')];
  assert.equal(
    notesOf(renderAndroid(cat)).split('\n')[1],
    '   - **&lt;os&gt;** (&lt;wv&gt;): supported. &lt;c&gt;. &lt;n&gt;.',
  );
});

test('note head: title keeps its final punctuation; questions are joined, empty or missing ones are omitted', () => {
  const head = (extra) =>
    notesOf(renderAndroid([c('CAP-A-01', 'android', [r('37', 'supported')], 'supported', extra)])).split('\n')[0];
  assert.equal(head({ title: 'Does it work?', questions: ['Q-1', 'Q-2'] }), '1. **CAP-A-01** Does it work? Prototype P, sprint S1, questions Q-1, Q-2.');
  assert.equal(head({ title: 'Done.', questions: [] }), '1. **CAP-A-01** Done. Prototype P, sprint S1.');
  assert.equal(head({ questions: undefined }), '1. **CAP-A-01** T CAP-A-01. Prototype P, sprint S1.');
  assert.equal(head({ title: '<t>', prototype: '<p>', sprint: '<s>', questions: ['<q>'] }), '1. **CAP-A-01** &lt;t&gt;. Prototype &lt;p&gt;, sprint &lt;s&gt;, questions &lt;q&gt;.');
});

test('every result of a capability is a bullet under its note, notes are numbered from 1', () => {
  const cat = [
    c('CAP-A-01', 'android', [r('37', 'supported'), r('29', 'supported')], 'supported'),
    c('CAP-A-02', 'android', [r('37', 'supported')], 'supported'),
  ];
  const lines = notesOf(renderAndroid(cat)).split('\n');
  assert.deepEqual(lines.map((l) => l.slice(0, 15)), ['1. **CAP-A-01**', '   - **37**: su', '   - **29**: su', '2. **CAP-A-02**', '   - **37**: su', '']);
});

test('esc: a lone opening or closing double brace is refused, the message shows 60 characters', () => {
  assert.throws(() => esc('a {{ b'), /curly/);
  assert.throws(() => esc('a }} b'), /curly/);
  const long = `{{${'x'.repeat(100)}`;
  assert.throws(() => esc(long), (e) => e.message === `text has double curly braces: ${long.slice(0, 60)}`);
});

test('esc escapes angle brackets outside every code span and keeps them inside every code span', () => {
  assert.equal(esc('a <b> `c <d>` e <f> `g <h>` i <j>'), 'a &lt;b&gt; `c <d>` e &lt;f&gt; `g <h>` i &lt;j&gt;');
});

test('ios page: the intro and the 8 item headings have the exact text', () => {
  const page = renderIos(small());
  assert.ok(page.startsWith(`# iOS parity matrix\n\n${PREAMBLE_TAIL}${verdicts('iOS')}\n\n## Spike matrix items\n\n`));
  assert.ok(
    page.includes(
      'The items are the 8 items of the spike matrix in [iOS SDK](/engineering/platforms/ios) section 4. ' +
        'Columns are iOS versions. A capability that serves two items is in both tables.\n\n### Item 1:',
    ),
  );
  const headings = page.split('\n').filter((l) => l.startsWith('### '));
  assert.deepEqual(headings, [
    '### Item 1: HTTPS with valid TLS, redirects',
    '### Item 2: First render time; early render or streaming',
    '### Item 3: `document.location`, `window.origin`, relative URLs, cookies, session and auth',
    '### Item 4: CSP, inline scripts with nonces, Service Worker, IndexedDB, localStorage',
    '### Item 5: History, back and forward, navigation delegate, deep links',
    '### Item 6: CORS, subresources, cross-origin iframes, video, downloads',
    '### Item 7: Bridge isolation, app-bound domains, content worlds',
    '### Item 8: Background, process termination, memory pressure, offline',
  ]);
});

test('ios page: an item with a capability has a table, an item without has the not-measured line', () => {
  const page = renderIos(small());
  assert.equal(
    section(page, '### Item 2:', '### Item 3:').split('\n\n').slice(1).join('\n\n'),
    `| # | ID | 27.0 | Verdict |\n|---|---|---|---|\n| 1 | CAP-I-01 | ${badge('tip', 'supported')} | ${badge('tip', 'supported')} |\n\n`,
  );
  assert.ok(section(page, '### Item 1:', '### Item 2:').endsWith(`No capability. ${badge('info', 'not measured')} in WP-04.\n\n`));
  assert.equal((page.match(/in WP-04\./g) ?? []).length, 7);
});

test('ios page: each iOS capability is in the items of the spike matrix map', () => {
  const ids = Array.from({ length: 11 }, (_, k) => `CAP-I-${String(k + 1).padStart(2, '0')}`);
  const cat = ids.map((id) => c(id, 'ios', [r('27.0', 'supported')], 'supported'));
  const page = renderIos(cat);
  const itemsOf = (id) =>
    [1, 2, 3, 4, 5, 6, 7, 8].filter((n) => section(page, `### Item ${n}:`, n === 8 ? '## Notes' : `### Item ${n + 1}:`).includes(`| ${id} |`));
  assert.deepEqual(itemsOf('CAP-I-01'), [2]);
  assert.deepEqual(itemsOf('CAP-I-02'), [3]);
  assert.deepEqual(itemsOf('CAP-I-03'), [3]);
  assert.deepEqual(itemsOf('CAP-I-04'), [2, 7]);
  assert.deepEqual(itemsOf('CAP-I-05'), [2]);
  assert.deepEqual(itemsOf('CAP-I-06'), [3, 6]);
  assert.deepEqual(itemsOf('CAP-I-07'), [2]);
  assert.deepEqual(itemsOf('CAP-I-08'), [4]);
  assert.deepEqual(itemsOf('CAP-I-09'), [4, 7]);
  assert.deepEqual(itemsOf('CAP-I-10'), [5]);
  assert.deepEqual(itemsOf('CAP-I-11'), [3, 6]);
  assert.match(page, /### Item 1:[^#]*not measured/);
});

test('ios page: the number in a table row is the note number, also when the capability is in two items', () => {
  const cat = [
    c('CAP-I-01', 'ios', [r('27.0', 'supported')], 'supported'),
    c('CAP-I-04', 'ios', [r('27.0', 'supported')], 'supported'),
    c('CAP-I-06', 'ios', [r('27.0', 'supported')], 'supported'),
  ];
  const page = renderIos(cat);
  assert.match(section(page, '### Item 2:', '### Item 3:'), /\| 1 \| CAP-I-01 \|[\s\S]*\| 2 \| CAP-I-04 \|/);
  assert.match(section(page, '### Item 7:', '### Item 8:'), /\| 2 \| CAP-I-04 \|/);
  assert.match(section(page, '### Item 6:', '### Item 7:'), /\| 3 \| CAP-I-06 \|/);
  assert.match(page, /^3\. \*\*CAP-I-06\*\*/m);
});

test('ios page: a table cell without a result shows none and the table lists all versions', () => {
  const cat = [
    c('CAP-I-01', 'ios', [r('27.0', 'supported'), r('18.5', 'impossible')], 'impossible'),
    c('CAP-I-05', 'ios', [r('27.0', 'supported')], 'supported'),
  ];
  const item = section(renderIos(cat), '### Item 2:', '### Item 3:');
  assert.match(item, /\| # \| ID \| 27\.0 \| 18\.5 \| Verdict \|\n\|---\|---\|---\|---\|---\|\n/);
  assert.match(item, /\| 2 \| CAP-I-05 \| <Badge type="tip" text="supported" \/> \| none \| <Badge type="tip" text="supported" \/> \|/);
});

test('ios page without shared capabilities has no shared section; the page ends with one newline', () => {
  const page = renderIos(small().filter((x) => x.platform === 'ios'));
  assert.doesNotMatch(page, /Shared rows/);
  assert.ok(page.endsWith('   - **27.0**: supported.\n'));
  assert.ok(!page.endsWith('\n\n'));
  assert.doesNotMatch(page, /\n\n\n/);
});

test('ios page: the shared section and notes are present when there are shared capabilities', () => {
  const page = renderIos(small());
  assert.match(section(page, '## Shared rows', '## Notes'), /^ \(FFI and URL\)\n\nThese rows are the same on the Android and the iOS page/);
  assert.match(notesOf(page), /^1\. \*\*CAP-I-01\*\*[\s\S]*\n2\. \*\*CAP-F-01\*\*[\s\S]*\n3\. \*\*CAP-U-01\*\*/);
});

test('widthProblems: a cell of 14 visible characters fits, 15 does not, with an exact message and line number', () => {
  assert.deepEqual(widthProblems(`| ${'x'.repeat(14)} |`), []);
  assert.deepEqual(widthProblems(`text\n\n| ${'x'.repeat(15)} |`), [`line 3: cell "${'x'.repeat(15)}" is longer than 14`]);
});

test('widthProblems: a row of 60 visible characters fits, 61 does not, with an exact message', () => {
  const row = (lens) => `| ${lens.map((n) => 'x'.repeat(n)).join(' | ')} |`;
  assert.deepEqual(widthProblems(row([14, 14, 14, 13, 1])), []);
  assert.deepEqual(widthProblems(`a\n${row([14, 14, 14, 14, 1])}`), ['line 2: row is longer than 60']);
});

test('widthProblems: the limits can be given; a long cell is reported instead of the row', () => {
  assert.deepEqual(widthProblems('| abcd |', 4, 60), []);
  assert.deepEqual(widthProblems('| abcde |', 4, 60), ['line 1: cell "abcde" is longer than 4']);
  assert.deepEqual(widthProblems('| ab | cd |', 4, 4), ['line 1: row is longer than 4']);
  assert.deepEqual(widthProblems('| ab | cd |', 4, 5), []);
  assert.deepEqual(widthProblems('| abcdefgh | cd |', 4, 4), ['line 1: cell "abcdefgh" is longer than 4']);
});

test('widthProblems: badges, links and backticks count by their visible text only', () => {
  const fits = [
    `| ${badge('warning', 'conditional')} |`,
    '| [short](https://example.com/a/very/long/address/that/is/not/shown) |',
    '| [ab](https://e.org) | [cd](https://f.org) |',
    '| `abcdefghijklm` |',
    '| <Badge text="supported" type="tip" /> |',
    '| <Badge type="tip" text="supported" extra="long extra attribute" /> |',
  ];
  for (const line of fits) assert.deepEqual(widthProblems(line), [], line);
  assert.deepEqual(widthProblems('| [a link text 15 c](https://e.org) |'), ['line 1: cell "a link text 15 c" is longer than 14']);
});

test('widthProblems: separator rows are skipped even when long; a row that only starts with dashes is checked', () => {
  const dashes = '-'.repeat(15);
  assert.deepEqual(widthProblems(`|${dashes}|${dashes}|`), []);
  assert.deepEqual(widthProblems(`|${dashes}|abc|`), [`line 1: cell "${dashes}" is longer than 14`]);
  assert.deepEqual(widthProblems(`|abc|${dashes}|`), [`line 1: cell "${dashes}" is longer than 14`]);
});

test('widthProblems: only table lines are checked', () => {
  assert.deepEqual(widthProblems(`${'prose '.repeat(20)}|`), []);
  assert.deepEqual(widthProblems(`${'prose '.repeat(20)}`), []);
  assert.equal(widthProblems(`| ${'x'.repeat(30)}`).length, 1);
});
