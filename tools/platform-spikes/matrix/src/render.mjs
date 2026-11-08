// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Pure rendering of the two matrix pages from the capability catalog (no I/O).

export const REPO_BLOB = 'https://github.com/robotoss/rivqen/blob/main/';
export const CATALOG_PATH = 'tools/platform-spikes/capabilities.json';
/** 375 px rule: longest visible text of one table cell, and of one table row (all cells together). */
export const MAX_CELL = 14;
export const MAX_ROW = 60;

/** The 8 items of the spike matrix of docs/engineering/platforms/ios.md section 4. */
export const IOS_ITEMS = [
  'HTTPS with valid TLS, redirects',
  'First render time; early render or streaming',
  '`document.location`, `window.origin`, relative URLs, cookies, session and auth',
  'CSP, inline scripts with nonces, Service Worker, IndexedDB, localStorage',
  'History, back and forward, navigation delegate, deep links',
  'CORS, subresources, cross-origin iframes, video, downloads',
  'Bridge isolation, app-bound domains, content worlds',
  'Background, process termination, memory pressure, offline',
];

/**
 * Item numbers (1-8) of each iOS capability. A capability may serve more than one item.
 * An iOS capability that is not in this table stops the generator: map it first.
 */
export const IOS_ITEM_MAP = {
  // Item 1 (TLS, redirects) has no iOS capability: WP-04 did not measure TLS errors or redirects on iOS.
  'CAP-I-01': [2],
  'CAP-I-02': [3],
  'CAP-I-03': [3],
  'CAP-I-04': [2, 7],
  'CAP-I-05': [2],
  'CAP-I-06': [3, 6],
  'CAP-I-07': [2],
  'CAP-I-08': [4],
  'CAP-I-09': [4, 7],
  'CAP-I-10': [5],
  'CAP-I-11': [3, 6],
};

const BADGE = { supported: 'tip', conditional: 'warning', impossible: 'danger', 'not-tested': 'info', pending: 'info' };
const badge = (verdict) => `<Badge type="${BADGE[verdict] ?? 'info'}" text="${verdict}" />`;

/** Escape text for Markdown prose: no raw HTML outside code spans; no double curly braces anywhere. */
export function esc(text) {
  if (text.includes('{{') || text.includes('}}')) throw new Error(`text has double curly braces: ${text.slice(0, 60)}`);
  return text
    .split('`')
    .map((part, i) => (i % 2 === 0 ? part.replaceAll('<', '&lt;').replaceAll('>', '&gt;') : part))
    .join('`');
}

const link = (label, path) => `[${label}](${REPO_BLOB}${path})`;
const sentence = (s) => (/[.!?]$/.test(s) ? s : `${s}.`);

/** Shared rows: FFI and URL capabilities appear, identical, on both pages. */
const isShared = (c) => c.platform === 'ffi' || c.platform === 'url';

function resultLine(r) {
  const parts = [`**${esc(r.os)}**${r.webview ? ` (${esc(r.webview)})` : ''}: ${r.verdict}.`];
  if (r.condition) parts.push(sentence(esc(r.condition)));
  if (r.note) parts.push(sentence(esc(r.note)));
  const links = [];
  (r.runs ?? []).forEach((p, i) => links.push(link(`run ${i + 1}`, p)));
  if (r.metrics) links.push(link('metrics', r.metrics));
  if (r.recording) links.push(link('recording', r.recording));
  if (r.source) links.push(`[source](${r.source})`);
  if (links.length > 0) parts.push(links.join(', '));
  return parts.join(' ');
}

function noteBlock(n, cap) {
  const questions = cap.questions?.length ? `, questions ${cap.questions.map(esc).join(', ')}` : '';
  const head = `${n}. **${cap.id}** ${sentence(esc(cap.title))} Prototype ${esc(cap.prototype)}, sprint ${esc(cap.sprint)}${questions}.`;
  return [head, ...cap.results.map((r) => `   - ${resultLine(r)}`)].join('\n');
}

const cell = (cap, os) => {
  const r = cap.results.find((x) => x.os === os);
  return r ? badge(r.verdict) : 'none';
};

function table(header, rows) {
  return [`| ${header.join(' | ')} |`, `|${header.map(() => '---').join('|')}|`, ...rows.map((r) => `| ${r.join(' | ')} |`)].join('\n');
}

const preamble = (title, platform) =>
  `# ${title}\n\n<Badge type="tip" text="FACT" /> This page is generated from ${link('capabilities.json', CATALOG_PATH)} by ` +
  `${link('matrix/gen.mjs', 'tools/platform-spikes/matrix/gen.mjs')}. Do not edit it by hand. ` +
  `Change the catalog and run \`node tools/platform-spikes/matrix/gen.mjs\`.\n\n` +
  `Verdicts: ${badge('supported')} ${badge('conditional')} ${badge('impossible')} ${badge('pending')}. ` +
  `The ${platform} verdict of a capability is the weakest verdict of its versions. ` +
  `The number in the first column is the note below the tables.`;

function sharedSection(catalog, startNumber) {
  const shared = catalog.filter(isShared);
  if (shared.length === 0) return { text: '', notes: [] };
  const rows = shared.map((c, i) => [String(startNumber + i), c.id, badge(c.verdict)]);
  return {
    text:
      `## Shared rows (FFI and URL)\n\nThese rows are the same on the Android and the iOS page (the note numbers differ).` +
      `Each result in the notes is one target: host, device or simulator.\n\n${table(['#', 'ID', 'Verdict'], rows)}`,
    notes: shared.map((c, i) => noteBlock(startNumber + i, c)),
  };
}

const versionsOf = (caps) => [...new Set(caps.flatMap((c) => c.results.map((r) => r.os)))];

/** The Android matrix page. */
export function renderAndroid(catalog) {
  const caps = catalog.filter((c) => c.platform === 'android');
  const oses = versionsOf(caps);
  const rows = caps.map((c, i) => [String(i + 1), c.id, ...oses.map((os) => cell(c, os)), badge(c.verdict)]);
  const shared = sharedSection(catalog, caps.length + 1);
  const notes = [...caps.map((c, i) => noteBlock(i + 1, c)), ...shared.notes];
  return (
    [
      preamble('Android capability matrix', 'Android'),
      `## Capabilities\n\nColumns are API levels.\n\n${table(['#', 'ID', ...oses, 'Verdict'], rows)}`,
      shared.text,
      `## Notes\n\n${notes.join('\n')}`,
    ]
      .filter(Boolean)
      .join('\n\n') + '\n'
  );
}

/** The iOS parity matrix page, grouped by the 8 items of the spike matrix. */
export function renderIos(catalog) {
  const caps = catalog.filter((c) => c.platform === 'ios');
  for (const c of caps) {
    if (!IOS_ITEM_MAP[c.id]) throw new Error(`${c.id} has no spike matrix item: add it to IOS_ITEM_MAP`);
  }
  const oses = versionsOf(caps);
  const number = new Map(caps.map((c, i) => [c.id, i + 1]));
  const items = IOS_ITEMS.map((name, i) => {
    const inItem = caps.filter((c) => IOS_ITEM_MAP[c.id].includes(i + 1));
    const body =
      inItem.length === 0
        ? 'No capability. <Badge type="info" text="not measured" /> in WP-04.'
        : table(
            ['#', 'ID', ...oses, 'Verdict'],
            inItem.map((c) => [String(number.get(c.id)), c.id, ...oses.map((os) => cell(c, os)), badge(c.verdict)]),
          );
    return `### Item ${i + 1}: ${name}\n\n${body}`;
  });
  const shared = sharedSection(catalog, caps.length + 1);
  const notes = [...caps.map((c, i) => noteBlock(i + 1, c)), ...shared.notes];
  return (
    [
      preamble('iOS parity matrix', 'iOS'),
      `## Spike matrix items\n\nThe items are the 8 items of the spike matrix in [iOS SDK](/engineering/platforms/ios) section 4. ` +
        `Columns are iOS versions. A capability that serves two items is in both tables.`,
      items.join('\n\n'),
      shared.text,
      `## Notes\n\n${notes.join('\n')}`,
    ]
      .filter(Boolean)
      .join('\n\n') + '\n'
  );
}

const visible = (s) =>
  s
    .replace(/<Badge [^>]*text="([^"]*)"[^>]*\/>/g, '$1')
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replaceAll('`', '')
    .trim();

/** Table rows whose cells or rows are too long for 375 px. Returns messages; empty when the page fits. */
export function widthProblems(markdown, maxCell = MAX_CELL, maxRow = MAX_ROW) {
  const out = [];
  markdown.split('\n').forEach((line, i) => {
    if (!line.startsWith('|') || /^\|[-|]+\|$/.test(line)) return;
    const cells = line.slice(1, -1).split('|').map(visible);
    const long = cells.find((c) => c.length > maxCell);
    if (long) out.push(`line ${i + 1}: cell "${long}" is longer than ${maxCell}`);
    else if (cells.join(' ').length > maxRow) out.push(`line ${i + 1}: row is longer than ${maxRow}`);
  });
  return out;
}

export const PAGES = [
  { file: 'ios-parity-matrix.md', render: renderIos },
  { file: 'android-matrix.md', render: renderAndroid },
];
