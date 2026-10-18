#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// One corpus page for the H-42 study (WP-17 S3 T-02). Run by src/corpus.mjs in a
// child process with a timeout. Prints one JSON object with numbers, rule labels and
// tag names only: never page text.
//
// Usage: node src/corpus-page.mjs <input.html> [--charset <label>]
//
// Steps: the noscript inventory; the four markup variants and the four head rules
// against parse5 in both views (src/evaluate.mjs); the simulated blocks of the S1
// corpus method (../../../corpus/classify.mjs: choosePicks, insertBlocks; variants
// "small" and "main") for a page that the shipped rules accept, with each markup
// variant against parse5 in both views.

import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { choosePicks, classify, insertBlocks } from '../../../corpus/classify.mjs';
import { evaluateDocument } from './evaluate.mjs';
import { noscriptInventory, safeName } from './inventory.mjs';

/** A rule label with a safe tag name ("B:<name>", "M-11.3:<name>", "A:M-09.2"). */
export function safeWhy(why) {
  if (why === null) return null;
  const m = /^(B:|M-11\.3:)(.*)$/.exec(why);
  return m ? `${m[1]}${safeName(m[2])}` : why;
}

function compact(markup) {
  return Object.fromEntries(Object.entries(markup).map(([k, r]) => [
    k,
    { valid: r.valid, blocks: r.blocks, why: safeWhy(r.why), enabled: r.enabled ?? null, disabled: r.disabled ?? null },
  ]));
}

function compactHead(head) {
  return Object.fromEntries(Object.entries(head).map(([k, r]) => [
    k,
    { at: r.at, enabled: r.enabled ?? null, disabled: r.disabled ?? null, parse5DisabledEnd: r.disabledDetails?.parse5End ?? null },
  ]));
}

export function evaluatePage(bytes, charset) {
  const e = evaluateDocument(bytes);
  const out = {
    bytes: bytes.length,
    inventory: noscriptInventory(bytes),
    candidateEqual: e.candidateEqual,
    divergences: e.divergences,
    markup: compact(e.markup),
    head: compactHead(e.head),
    variants: {},
  };
  if (!e.markup.shipped.valid) return out;
  const cl = classify(bytes, { charset, blocks: false });
  if (!Array.isArray(cl.picks)) return out;
  const picks = choosePicks(cl.picks);
  for (const [name, prefix] of [['small', 's'], ['main', 'm']]) {
    if (picks[name].length === 0) continue;
    const doc = insertBlocks(bytes, picks[name], prefix);
    if (doc === null) {
      out.variants[name] = { error: 'insert refused' };
      continue;
    }
    out.variants[name] = { markup: compact(evaluateDocument(doc, { head: false }).markup) };
  }
  return out;
}

function main(argv) {
  let file = null;
  let charset = null;
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === '--charset') charset = argv[++i] ?? null;
    else file = argv[i];
  }
  if (file === null) throw new Error('usage: corpus-page.mjs <file> [--charset label]');
  process.stdout.write(`${JSON.stringify(evaluatePage(readFileSync(file), charset))}\n`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main(process.argv.slice(2));
  } catch (err) {
    process.stderr.write(`corpus-page: ${err instanceof Error ? err.message : String(err)}\n`);
    process.exit(2);
  }
}
