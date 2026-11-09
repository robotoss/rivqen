// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Commands (no network, pure parsing):
//   node cli.mjs whatwg                 run the reference (Node global URL) -> evidence/.../whatwg-node.json
//   node cli.mjs inputs <file>          write the line format that the JVM and Swift runners read
//   node cli.mjs assemble <raw-file>    raw runner output -> evidence/.../<stack>.json
//   node cli.mjs compare                every result file -> evidence/.../compare.json
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { STACKS, toInputLines, parseRaw, compare } from './lib.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const OUT = join(here, '..', '..', '..', 'evidence', 'spikes', 'wp-04', 'url');
const urls = JSON.parse(readFileSync(join(here, 'urls.json'), 'utf8'));

function writeJson(name, obj) {
  mkdirSync(OUT, { recursive: true });
  writeFileSync(join(OUT, name), JSON.stringify(obj, null, 2) + '\n');
}

function whatwgRecord(u) {
  try {
    const url = u.base === undefined ? new URL(u.input) : new URL(u.input, u.base);
    const qi = url.href.indexOf('?');
    const hi = url.href.indexOf('#');
    const hasQuery = qi >= 0 && (hi < 0 || qi < hi);
    return {
      id: u.id,
      ok: true,
      href: url.href,
      host: url.hostname,
      path: url.pathname,
      query: hasQuery ? url.search.slice(1) : null,
      error: null,
    };
  } catch (e) {
    return { id: u.id, ok: false, href: null, host: null, path: null, query: null, error: e instanceof Error ? e.constructor.name + (e.code ? ':' + e.code : '') : 'unknown' };
  }
}

const [cmd, arg] = process.argv.slice(2);
if (cmd === 'whatwg') {
  writeJson('whatwg-node.json', {
    stack: 'whatwg-node',
    version: `Node.js ${process.version} (ada ${process.versions.ada ?? 'NOT FOUND'}); API: new URL(input, base).href`,
    runtime: `${process.platform} ${process.arch}`,
    results: urls.map(whatwgRecord),
  });
} else if (cmd === 'inputs') {
  writeFileSync(arg, toInputLines(urls));
} else if (cmd === 'assemble') {
  const file = parseRaw(readFileSync(arg, 'utf8'), urls);
  if (!STACKS.includes(file.stack)) throw new Error(`unknown stack ${file.stack}`);
  writeJson(`${file.stack}.json`, file);
} else if (cmd === 'compare') {
  const files = {};
  for (const s of STACKS) {
    const p = join(OUT, `${s}.json`);
    if (existsSync(p)) files[s] = JSON.parse(readFileSync(p, 'utf8'));
  }
  writeJson('compare.json', compare(urls, files));
} else {
  console.error('usage: cli.mjs whatwg | inputs <file> | assemble <raw> | compare');
  process.exit(2);
}
