#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Lay out the fetched pages as a fixtures-like directory (WP-17 S1 T-15).
//
// Usage: node layout.mjs --data <data-dir>
//
// Reads <data-dir>/fetch.jsonl and <data-dir>/raw/<id>.bin. Writes
// <data-dir>/pages/<id>/input.html (the exact response bytes) for each HTML page with
// status 200, and <data-dir>/pages.jsonl with one line per fetched URL: the fetch data, the
// corpus decision (in corpus, or why not), and the platform fingerprint.
//
// A page is not in the corpus when the status is not 200, the response is not HTML, the
// body is empty, or the page is a bot-challenge or access-denied interstitial (not the
// real page; the heuristic is in botWall()).
// The bytes are untrusted data: this script only copies them and does bounded substring
// searches (no parsing, no execution).

import { mkdirSync, readFileSync, writeFileSync, existsSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

/** Platform fingerprints: first match wins. Substring or regular expression tests only. */
const PLATFORMS = [
  ["wordpress", /\/wp-content\/|\/wp-includes\/|<meta name="generator" content="WordPress/i],
  ["drupal", /Drupal\.settings|data-drupal-|drupal-settings-json|<meta name="Generator" content="Drupal/i],
  ["ghost", /<meta name="generator" content="Ghost/i],
  ["shopify", /cdn\.shopify\.com|Shopify\.theme|shopify-section/],
  ["tilda", /tildacdn\.|data-tilda-|tilda-blocks/],
  ["bitrix", /\/bitrix\/(js|templates|cache|components|css)\/|BX\.setJSList|BX\.message\(/],
  ["joomla", /\/media\/jui\/|<meta name="generator" content="Joomla/i],
  ["wix", /static\.wixstatic\.com|wix-bolt|_wixCssImports/],
  ["squarespace", /static1\.squarespace\.com|Static\.SQUARESPACE_CONTEXT/],
  ["webflow", /data-wf-page|data-wf-site/],
  ["mediawiki", /<meta name="generator" content="MediaWiki|mw-parser-output/],
  ["discourse", /<meta name="generator" content="Discourse/i],
  ["xenforo", /data-xf-init|XF\.extendObject|<html[^>]{0,200}id="XF"/],
  ["phpbb", /phpBB|viewtopic\.php/],
  ["sphinx", /<meta name="generator" content="Docutils|sphinx_highlight|_static\/sphinx/],
  ["mkdocs", /<meta name="generator" content="mkdocs/i],
  ["docusaurus", /<meta name="generator" content="Docusaurus/i],
  ["vitepress", /<meta name="generator" content="VitePress/i],
  ["hugo", /<meta name="generator" content="Hugo/i],
  ["jekyll", /<meta name="generator" content="Jekyll/i],
  ["nextjs", /\/_next\/static\/|__NEXT_DATA__/],
  ["nuxt", /\/_nuxt\/|__NUXT__/],
  ["gatsby", /___gatsby/],
  ["angular", /ng-version=|<app-root/],
];

/** @param {string} head Latin-1 view of the bytes */
export function platformOf(head) {
  for (const [name, re] of PLATFORMS) if (re.test(head)) return name;
  return "other";
}

/**
 * True when a small page looks like a bot challenge or an access-denied interstitial.
 * @param {string} head Latin-1 view of the first bytes
 * @param {number} size
 */
export function botWall(head, size) {
  if (size > 64 * 1024) return false;
  if (
    /challenge-platform|cf-chl-|Just a moment\.\.\.|_Incapsula_Resource|px-captcha|captcha-delivery|Checking your browser|Access Denied|Request unsuccessful|Pardon Our Interruption|showcaptcha|smartcaptcha|__qrator|qrator|ddos-guard|servicepipe|are you a robot|enable JavaScript and cookies to continue|awswaf|perimeterx|Client Challenge|I Challenge Thee|anubis|Auth-controller|Privacy Gate|bm-verify/i.test(head)
  ) {
    return true;
  }
  // Cyrillic markers ("one moment", "access is limited", "are you a robot") as UTF-8 bytes in the Latin-1 view.
  const latin1 = (s) => Buffer.from(s, "utf8").toString("latin1");
  if ([latin1("Один момент"), latin1("Доступ ограничен"), latin1("Вы робот"), latin1("Вы не робот")].some((m) => head.includes(m))) return true;
  // Very small stubs that only redirect or check the client, not the real page.
  return size < 4096 && /http-equiv=["']?refresh|location\.replace|retpath|#include virtual/i.test(head);
}

function main(argv) {
  let data = null;
  for (let i = 0; i < argv.length; i++) if (argv[i] === "--data") data = argv[++i] ?? null;
  if (data === null) throw new Error("usage: layout.mjs --data <data-dir>");
  const dir = path.resolve(data);
  const pagesDir = path.join(dir, "pages");
  if (existsSync(pagesDir) && readdirSync(pagesDir).length > 0) throw new Error(`${pagesDir} is not empty`);
  mkdirSync(pagesDir, { recursive: true });
  const rows = readFileSync(path.join(dir, "fetch.jsonl"), "utf8")
    .split("\n")
    .filter((l) => l !== "")
    .map((l) => JSON.parse(l));
  rows.sort((a, b) => (a.id < b.id ? -1 : 1));
  const out = [];
  for (const r of rows) {
    let corpus = "yes";
    let platform = null;
    const raw = path.join(dir, "raw", `${r.id}.bin`);
    if (r.status !== 200) corpus = r.status === null ? `no: ${r.error}` : `no: HTTP ${r.status}`;
    else if (!/html/i.test(r.content_type ?? "")) corpus = "no: not HTML";
    else if (!existsSync(raw) || r.bytes === 0) corpus = "no: empty";
    if (corpus === "yes") {
      const bytes = readFileSync(raw);
      const head = Buffer.from(bytes.subarray(0, 2 * 1024 * 1024)).toString("latin1");
      platform = platformOf(head);
      if (botWall(head, bytes.length)) corpus = "no: bot wall";
      else {
        mkdirSync(path.join(pagesDir, r.id));
        writeFileSync(path.join(pagesDir, r.id, "input.html"), bytes);
      }
    }
    out.push({ ...r, corpus, platform });
  }
  writeFileSync(path.join(dir, "pages.jsonl"), out.map((x) => JSON.stringify(x)).join("\n") + "\n");
  const n = out.filter((x) => x.corpus === "yes").length;
  process.stderr.write(`layout: ${n} pages in the corpus, ${out.length - n} not\n`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main(process.argv.slice(2));
  } catch (err) {
    process.stderr.write(`layout: ${err instanceof Error ? err.message : String(err)}\n`);
    process.exit(2);
  }
}
