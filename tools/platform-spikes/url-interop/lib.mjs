// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// URL interop spike (WP-04 S2 T-07, Q-24, DL-135). Pure functions, no I/O.
//
// Raw runner format (one line per record, tab-separated; every value is base64 of its UTF-8 bytes,
// and the single character "~" stands for null). The format survives tabs, newlines and any
// character in the inputs, and the JVM and Swift runners need no JSON library.
//   H <key> <value>                                     header: stack, version, runtime
//   R <id> <ok:0|1> <href> <host> <path> <query> <error> <extra-json>

export const STACKS = [
  'whatwg-node',
  'okhttp',
  'java-net-uri',
  'android-net-uri',
  'foundation-url-macos',
  'foundation-urlcomponents-macos',
  'foundation-url-ios-sim',
  'foundation-urlcomponents-ios-sim',
];

const b64 = (s) => Buffer.from(s, 'utf8').toString('base64');
const unb64 = (s) => (s === '~' ? null : Buffer.from(s, 'base64').toString('utf8'));

/** The input list as the line format that the runners read: `id`, input, base (all base64). */
export function toInputLines(urls) {
  return urls.map((u) => [u.id, b64(u.input), u.base === undefined ? '~' : b64(u.base)].join('\t')).join('\n') + '\n';
}

export function encodeRecord(r) {
  const f = (v) => (v === null || v === undefined ? '~' : b64(String(v)));
  return ['R', r.id, r.ok ? '1' : '0', f(r.href), f(r.host), f(r.path), f(r.query), f(r.error), f(r.extra ? JSON.stringify(r.extra) : null)].join('\t');
}

/** Parse the raw output of a runner into a result file object. Throws on a malformed line. */
export function parseRaw(text, urls) {
  const header = {};
  const byId = new Map();
  for (const line of text.split('\n')) {
    if (line === '') continue;
    const p = line.split('\t');
    if (p[0] === 'H' && p.length === 3) {
      header[p[1]] = unb64(p[2]);
    } else if (p[0] === 'R' && p.length === 9) {
      const rec = {
        id: p[1],
        ok: p[2] === '1',
        href: unb64(p[3]),
        host: unb64(p[4]),
        path: unb64(p[5]),
        query: unb64(p[6]),
        error: unb64(p[7]),
      };
      const extra = unb64(p[8]);
      if (extra !== null) rec.extra = JSON.parse(extra);
      byId.set(rec.id, rec);
    } else {
      throw new Error(`malformed runner line: ${JSON.stringify(line.slice(0, 80))}`);
    }
  }
  for (const k of ['stack', 'version', 'runtime']) {
    if (header[k] === undefined) throw new Error(`runner header lacks ${k}`);
  }
  const results = urls.map((u) => {
    const r = byId.get(u.id);
    if (!r) throw new Error(`runner gave no result for ${u.id}`);
    return r;
  });
  if (byId.size !== urls.length) throw new Error(`runner gave ${byId.size} results for ${urls.length} inputs`);
  return { stack: header.stack, version: header.version, runtime: header.runtime, results };
}

const stripBrackets = (h) => (typeof h === 'string' ? h.replace(/^\[(.*)\]$/, '$1') : h);

function fieldStatus(name, w, s) {
  const a = name === 'host' ? stripBrackets(w[name]) : w[name];
  const b = name === 'host' ? stripBrackets(s[name]) : s[name];
  return a === b ? 'equal' : 'differ';
}

/** Compare one stack entry against the WHATWG entry of the same input. */
export function compareEntry(w, s) {
  if (!w.ok && !s.ok) return { verdict: 'both-refuse', error: s.error };
  if (!w.ok) return { verdict: 'accepts-refused', href: s.href, error: null };
  if (!s.ok) return { verdict: 'refuses', error: s.error };
  const host = fieldStatus('host', w, s);
  const path = fieldStatus('path', w, s);
  const query = fieldStatus('query', w, s);
  const href = fieldStatus('href', w, s);
  const verdict = href === 'equal' ? 'equal' : 'differ';
  return { verdict, host, path, query, href, href_stack: s.href };
}

/** compare.json: per input and stack, and counts per stack. `files` maps a stack name to its result file. */
export function compare(urls, files) {
  const w = files['whatwg-node'];
  if (!w) throw new Error('whatwg-node result file is required');
  const stacks = Object.keys(files).filter((n) => n !== 'whatwg-node');
  const wById = new Map(w.results.map((r) => [r.id, r]));
  const entries = urls.map((u) => {
    const wr = wById.get(u.id);
    const out = { id: u.id, input: u.input, ...(u.base === undefined ? {} : { base: u.base }), whatwg: wr, stacks: {} };
    for (const n of stacks) {
      const sr = files[n].results.find((r) => r.id === u.id);
      out.stacks[n] = sr ? compareEntry(wr, sr) : { verdict: 'missing' };
    }
    return out;
  });
  const summary = {};
  for (const n of stacks) {
    const c = { total: urls.length, equal: 0, differ: 0, refuses: 0, 'accepts-refused': 0, 'both-refuse': 0, missing: 0, host_differ: 0, path_differ: 0, query_differ: 0 };
    for (const e of entries) {
      const v = e.stacks[n];
      c[v.verdict] += 1;
      if (v.host === 'differ') c.host_differ += 1;
      if (v.path === 'differ') c.path_differ += 1;
      if (v.query === 'differ') c.query_differ += 1;
    }
    summary[n] = c;
  }
  return {
    reference: 'whatwg-node',
    reference_version: w.version,
    versions: Object.fromEntries(stacks.map((n) => [n, files[n].version])),
    note: 'href is the serialization the stack would send; host strips [] before the comparison; null and empty are different values',
    summary,
    entries,
  };
}
