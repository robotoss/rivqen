#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// One-time scaffold of the case.json files of the trace-backed catalog rows
// (fixtures/legacy/README.md "Canonical fixtures from the stored traces") and of the
// profile fixtures that generator check 4 requires.
//
//   node tools/legacy-fixtures/scaffold.mjs
//
// It never overwrites a file. A person reviews the files it writes; gen.mjs does not
// call this script. The request lines keep the spelling and the order of the traces.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { FIXTURES_DIR, REPO_ROOT, UPSTREAM, PROFILE_SUFFIX, UPSTREAM_PROFILES, DEFAULT_APP, DEFAULT_TARGET, expectedOf, pageFile } from './src/fixture.mjs';
import { bodyKindOf, compareToTrace, parseTrace } from './src/trace.mjs';
import { formatJson } from '../rqp-protocol/src/fixtures.mjs';

const WIRE = 'docs:engineering/protocol/legacy-wire#canonical-server';
const MARKERS = 'docs:engineering/protocol/legacy-markers#_4-canonical-legacy-grammar-for-rivqen';
const JAVA = 'upstream:sonic-java/src/main/java/com/github/tencent/SonicFilter.java';
const NODE = 'upstream:sonic-nodejs/common/diff.js';
const PHP = 'upstream:sonic-php/sdk/sonic.php';
const AE = ['Accept-Encoding', 'gzip'];
const AD = ['accept-diff', 'true'];
const inm = (v) => ['If-None-Match', v];
const tt = (v) => ['template-tag', v];
const FILES = { java: 'java', node: 'node', php: 'php' };

const data = (page) => [AE, AD, inm('0000000000000000000000000000000000000000'), tt(`{{tag:${page}}}`)];
const change = (extra = {}) => [AE, extra.ad ?? AD, inm('{{etag:a}}'), tt('{{tag:a}}')];

/** key, trace file stem, page, request, rules, description, sources */
const SCENARIOS = [
  ['NON-SONIC-CLIENT', 'S01-non-sonic', 'a', [AE], ['C-02'], 'No accept-diff: the application response goes out unchanged', [WIRE, `${JAVA}:103-144`]],
  ['FIRST-LOAD', 'S02-first-load', 'a', [AE, AD], ['C-04', 'C-08', 'C-09', 'C-10', 'C-12', 'C-13'], 'First load without validators: full page with legacy fields', [WIRE, `${JAVA}:60-149`, `${NODE}:12-122`, `${PHP}:50-145`]],
  ['304', 'S03-304', 'a', [AE, AD, inm('{{etag:a}}'), tt('{{tag:a}}')], ['C-04', 'C-05', 'C-09', 'C-10', 'C-13'], 'The etag matches: 304 with cache-offline store', [WIRE, `${JAVA}:96-101`, `${NODE}:26-33`, `${PHP}:75-80`]],
  ['304-UPPERCASE', 'S04-304-uppercase-etag', 'a', [AE, AD, inm('{{ETAG:a}}'), tt('{{tag:a}}')], ['C-05'], 'The etag in upper-case hex still matches (canonical: case-insensitive)', [WIRE, `${JAVA}:96-101`]],
  ['304-QUOTED', 'S05-304-quoted-etag', 'a', [AE, AD, inm('"{{etag:a}}"'), tt('{{tag:a}}')], ['C-05'], 'The etag in quotes still matches (canonical: one pair of quotes is removed)', [WIRE]],
  ['DATA-CHANGE', 'S06-data-change', 'b', change(), ['C-06', 'C-07', 'C-08', 'C-11', 'C-13'], 'Same template, data changed: data-only response', [WIRE, `${JAVA}:105-144`, `${NODE}:40-60`, `${PHP}:99-126`]],
  ['TEMPLATE-CHANGE', 'S07-template-change', 'c', change(), ['C-08', 'C-12'], 'The template changed: full page, template-change true', [WIRE, `${JAVA}:105-144`]],
  ['DETECT-ACCEPT-DIFF-FALSE', 'S08-accept-diff-false', 'b', change({ ad: ['accept-diff', 'false'] }), ['C-02'], 'accept-diff false: not a legacy request', [WIRE, `${JAVA}:69-77`, 'upstream:sonic-nodejs/middleware/compress.js:26-28', `${PHP}:59`]],
  ['DETECT-ACCEPT-DIFF-YES', 'S09-accept-diff-yes', 'b', change({ ad: ['accept-diff', 'yes'] }), ['C-02'], 'accept-diff yes: not a legacy request', [WIRE, `${JAVA}:69-77`, `${PHP}:59`]],
  ['DETECT-NAME-CASE', 'S10-header-capitalized', 'b', [AE, ['Accept-Diff', 'true'], inm('{{etag:a}}'), ['Template-Tag', '{{tag:a}}']], ['C-02'], 'Field names in capitals: the names compare case-insensitively', [WIRE, `${PHP}:59`]],
  ['COMPRESSED-IDENTITY', 'S11-identity-encoding', 'b', [['Accept-Encoding', 'identity'], AD, inm('{{etag:a}}'), tt('{{tag:a}}')], ['C-14'], 'accept-encoding identity does not change the legacy result', [WIRE, 'upstream:sonic-nodejs/middleware/compress.js:26-28']],
];

const PAGES = [
  ['S12', 'NO-MARKERS', 'nomark', ['C-06', 'C-07'], 'a page without markers: only the title is a block'],
  ['S13', 'EMPTY-NAME', 'emptyname', ['C-07'], 'blocks with an empty name stay in the template'],
  ['S14', 'DUPLICATE-BLOCKS', 'dup', ['C-07'], 'two blocks with the same name: the split fails and the response is passed through'],
  ['S15', 'TITLE-ATTRIBUTE', 'titleattr', ['C-06', 'C-07'], 'a title element with an attribute is the title block'],
  ['S16', 'TITLE-SEVERAL', 'titles', ['C-06', 'C-07'], 'several title elements: only the first one is replaced'],
  ['S17', 'MARKER-SPACES', 'spaces', ['C-07'], 'markers with spaces are not markers'],
  ['S18', 'MARKER-NESTED', 'nested', ['C-07'], 'nested markers: the match ends at the first end marker'],
  ['S19', 'MARKER-MISMATCH', 'mismatch', ['C-07'], 'the end marker name is not checked'],
  ['S20', 'MARKER-UPPERCASE', 'uppercase', ['C-07'], 'markers in upper case match'],
  ['S21', 'UNICODE', 'unicode', ['C-07', 'C-11'], 'non-ASCII text and HTML special characters in a block'],
];

for (const [sid, key, page, rules, text] of PAGES) {
  SCENARIOS.push([`${key}-FIRST`, `${sid}-${page}-first`, page, [AE, AD], ['C-08', 'C-12', ...rules.filter((r) => r !== 'C-11')], `First load of ${text}`, [WIRE, MARKERS]]);
  SCENARIOS.push([`${key}-DATA`, `${sid}-${page}-data`, page, data(page), ['C-08', 'C-11', ...rules.filter((r) => r !== 'C-11')], `Data request for ${text}`, [WIRE, MARKERS]]);
}

const PROFILE_SOURCES = {
  'legacy-java': [`${JAVA}:60-149`],
  'legacy-node': [`${NODE}:12-122`, 'upstream:sonic-nodejs/middleware/compress.js:18-83'],
  'legacy-php': [`${PHP}:50-145`],
};

function write(dir, value) {
  if (existsSync(join(dir, 'case.json'))) return false;
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, 'case.json'), `${formatJson(value)}\n`);
  return true;
}

function divergences(profile, key, canonical, trace) {
  const out = new Set();
  const dup = key.startsWith('DUPLICATE-BLOCKS');
  if (key === 'NON-SONIC-CLIENT' && profile === 'legacy-java') out.add('D-02');
  if ((key === 'DETECT-ACCEPT-DIFF-FALSE' || key === 'DETECT-ACCEPT-DIFF-YES') && profile !== 'legacy-php') out.add('D-01');
  if (key === 'DETECT-NAME-CASE' && profile === 'legacy-php') out.add('D-01');
  if (key === 'COMPRESSED-IDENTITY' && profile === 'legacy-node') out.add('D-10');
  if ((key === '304-UPPERCASE' && profile !== 'legacy-java') || key === '304-QUOTED') out.add('D-03');
  if (canonical.status === 304 || trace.status === 304) {
    if (profile === 'legacy-java') out.add('D-12');
  }
  if (key.startsWith('EMPTY-NAME')) out.add('D-05');
  if ((key.startsWith('NO-MARKERS') || key.startsWith('MARKER-SPACES')) && profile === 'legacy-node') out.add('D-06');
  if (key.startsWith('TITLE-SEVERAL') && profile !== 'legacy-node') out.add('D-13');
  if (profile !== 'legacy-node' && bodyKindOf(trace.body) === 'data') out.add('D-08');
  const suppress = out.has('D-01') || out.has('D-10') || dup || (key === 'NON-SONIC-CLIENT');
  if (!suppress) out.add('D-09');
  return [...out].sort();
}

const NAMES = {
  'D-01': 'legacy request detection',
  'D-02': 'legacy fields on a non-legacy response',
  'D-03': '304 comparison',
  'D-05': 'empty block name',
  'D-06': 'page without markers',
  'D-08': 'JSON escaping of the data body',
  'D-09': 'cache-control',
  'D-10': 'compression',
  'D-12': 'cache-offline on 304',
  'D-13': 'several title elements',
};

let written = 0;
for (const [key, stem, page, headers, rules, description, sources] of SCENARIOS) {
  const id = `FX-LEG-${key}`;
  const traces = Object.fromEntries(UPSTREAM_PROFILES.map((p) => [p, `evidence/traces/server/${FILES[PROFILE_SUFFIX[p].toLowerCase()]}/${stem}.http`]));
  const base = {
    id,
    description,
    profile: 'legacy-canonical',
    rules,
    upstream: { ...UPSTREAM },
    sources,
    evidence: 'trace',
    traces,
    page,
    app: { status: DEFAULT_APP.status, headers: DEFAULT_APP.headers },
    request: { method: 'GET', target: DEFAULT_TARGET, headers },
  };
  if (write(join(FIXTURES_DIR, id), base)) written++;
}

// Profile fixtures: one for each profile whose trace differs from the canonical response.
const bytesOf = (c) => pageFile(c.id, join(FIXTURES_DIR, 'pages'), c.page);
for (const [key, stem, page, headers, rules, description, sources] of SCENARIOS) {
  const id = `FX-LEG-${key}`;
  const c = JSON.parse(readFileSync(join(FIXTURES_DIR, id, 'case.json'), 'utf8'));
  const canonicalCase = { ...c, kind: 'canonical', level: 'must', request: { ...c.request } };
  const bytes = bytesOf(c);
  const canonical = expectedOf(canonicalCase, 'legacy-canonical', bytes, join(FIXTURES_DIR, 'pages'));
  for (const profile of UPSTREAM_PROFILES) {
    const trace = parseTrace(readFileSync(join(REPO_ROOT, c.traces[profile])));
    if (compareToTrace(canonical, trace, bytes).length === 0) continue;
    const d = divergences(profile, key, canonical, trace);
    const names = d.map((x) => NAMES[x]).filter(Boolean);
    const dup = key.startsWith('DUPLICATE-BLOCKS');
    const profileCase = {
      id: `${id}--${PROFILE_SUFFIX[profile]}`,
      description: `${profile}: ${dup ? 'the last value of a repeated block name wins and the template keeps both placeholders (legacy-markers section 3, DL-075)' : names.join('; ')}`,
      profile,
      replaces: id,
      divergences: d,
      sources: PROFILE_SOURCES[profile],
      trace: c.traces[profile],
    };
    if (d.includes('D-13')) {
      profileCase.not_reproduced = 'U-11';
      profileCase.description += ' (the page-corrupting title replacement U-11 is not reproduced, DL-069)';
    }
    if (write(join(FIXTURES_DIR, profileCase.id), profileCase)) written++;
  }
}
process.stdout.write(`scaffold: ${written} case.json files written\n`);
