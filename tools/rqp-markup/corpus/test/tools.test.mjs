// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Tests of the fetch, layout and summary helpers. No network: the scheduler uses a fake clock.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseList, discoverLink, charsetOf, runPolite, outsideRepo } from "../fetch.mjs";
import { botWall, platformOf } from "../layout.mjs";
import { classStats, greedy, pct, subStats } from "../summary.mjs";
import { redactUrl } from "../run.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));

test("the URL list of the repository parses; ids are unique; no query strings", () => {
  const rows = parseList(readFileSync(path.join(HERE, "..", "url-list.tsv"), "utf8"));
  assert.ok(rows.length > 500);
  assert.equal(new Set(rows.map((r) => r.id)).size, rows.length);
  assert.equal(new Set(rows.map((r) => r.url)).size, rows.length);
  for (const r of rows) assert.equal(new URL(r.url).search, "");
});

test("parseList: columns, discover kind, and refused rows", () => {
  const rows = parseList("# c\n\nhttps://a.example/\tnews\thome+article\ten\nhttps://b.example/x\tdocs\tdocs\tru\n");
  assert.deepEqual(rows, [
    { id: "p0001", url: "https://a.example/", category: "news", kind: "home", discover: "article", lang: "en" },
    { id: "p0002", url: "https://b.example/x", category: "docs", kind: "docs", discover: null, lang: "ru" },
  ]);
  assert.throws(() => parseList("https://a.example/?u=1\tnews\thome\ten\n"), /query/);
  assert.throws(() => parseList("https://u:p@a.example/\tnews\thome\ten\n"), /credentials/);
  assert.throws(() => parseList("ftp://a.example/\tnews\thome\ten\n"), /http/);
  assert.throws(() => parseList("https://a.example/\tnews\n"), /4 columns/);
  assert.throws(() => parseList("https://a.example/\tNews!\thome\ten\n"), /category/);
});

test("discoverLink: same host, no query, no account paths, best score by kind", () => {
  const html = Buffer.from(
    [
      '<a href="https://other.example/news/2026/10/11/a-long-article-about-things">x</a>',
      '<a href="/login">x</a>',
      '<a href="/news/2026/10/11/story-with-a-query?utm=1">x</a>',
      '<a href="/about">x</a>',
      "<a href='/news/short'>x</a>",
      '<a href="/news/2026/10/11/the-real-article-with-many-words">x</a>',
      '<a href="/products/blue-shoe">x</a>',
      '<a href="/style.css">x</a>',
    ].join(""),
    "latin1",
  );
  assert.equal(discoverLink(html, "https://www.site.example/", "article"), "https://www.site.example/news/2026/10/11/the-real-article-with-many-words");
  assert.equal(discoverLink(html, "https://site.example/", "product"), "https://site.example/products/blue-shoe");
  assert.equal(discoverLink(html, "https://site.example/", "unknown"), null);
  assert.equal(discoverLink(Buffer.from('<a href="/login">x</a>'), "https://site.example/", "article"), null);
});

test("charsetOf", () => {
  assert.equal(charsetOf("text/html; charset=UTF-8"), "utf-8");
  assert.equal(charsetOf('text/html;charset="windows-1251"'), "windows-1251");
  assert.equal(charsetOf("text/html"), null);
  assert.equal(charsetOf(null), null);
});

test("runPolite: one request per host at a time, delay per host, global limit, results in order", async () => {
  let clock = 0;
  const now = () => clock;
  const sleep = async (ms) => {
    clock += ms;
  };
  const log = [];
  let active = 0;
  let maxActive = 0;
  const activeHosts = new Set();
  const mk = (host, i) => ({
    host,
    run: async () => {
      assert.ok(!activeHosts.has(host), "two requests to one host at a time");
      activeHosts.add(host);
      active++;
      maxActive = Math.max(maxActive, active);
      log.push({ host, at: clock });
      await Promise.resolve();
      clock += 10;
      activeHosts.delete(host);
      active--;
      return i;
    },
  });
  const tasks = [mk("a", 0), mk("a", 1), mk("b", 2), mk("c", 3), mk("a", 4)];
  const out = await runPolite(tasks, { concurrency: 2, delayMs: 1000, now, sleep });
  assert.deepEqual(out, [0, 1, 2, 3, 4]);
  assert.ok(maxActive <= 2);
  const a = log.filter((x) => x.host === "a").map((x) => x.at);
  for (let k = 1; k < a.length; k++) assert.ok(a[k] - a[k - 1] >= 1000, `delay between requests to host a: ${a}`);
  assert.deepEqual(await runPolite([], { concurrency: 2, delayMs: 1000, now, sleep }), []);
});

test("botWall and platformOf", () => {
  assert.equal(botWall("<title>Just a moment...</title>", 5000), true);
  assert.equal(botWall(Buffer.from("<title>Один момент...</title>", "utf8").toString("latin1"), 3000), true);
  assert.equal(botWall('<meta http-equiv="refresh" content="0;URL=/x">', 900), true);
  assert.equal(botWall('<meta http-equiv="refresh" content="0;URL=/x">', 9000), false);
  assert.equal(botWall("<title>Just a moment...</title>", 200000), false);
  assert.equal(botWall('<meta name="robots" content="noindex"><p>A real small page', 3000), false);
  assert.equal(platformOf('<link href="/wp-content/themes/x.css">'), "wordpress");
  assert.equal(platformOf('<script src="https://cdn.shopify.com/s/x.js">'), "shopify");
  assert.equal(platformOf('<script src="/bitrix/js/main/core.js">'), "bitrix");
  assert.equal(platformOf('<div data-tilda-page="1">'), "tilda");
  assert.equal(platformOf("<p>plain</p>"), "other");
});

test("summary helpers: alone and total, greedy removal", () => {
  const pages = [
    { classes: { A: 1 } },
    { classes: { A: 1, B: 1 } },
    { classes: { B: 1 } },
    { classes: { B: 1 } },
    { classes: {} },
  ];
  const s = classStats(pages);
  assert.equal(s.get("A").total, 2);
  assert.equal(s.get("A").alone, 1);
  assert.equal(s.get("B").total, 3);
  assert.equal(s.get("B").alone, 2);
  assert.deepEqual(greedy(pages, 5), [
    { rule: "B", valid: 3 },
    { rule: "A", valid: 5 },
  ]);
  assert.equal(pct(1, 3), "33.3 %");
  assert.equal(pct(1, 0), "–");
});

test("subStats: sub-case of a class by its detail keys", () => {
  const pages = [
    { classes: { "M-09.2": 1 }, details: { "M-09.2": { "<form>": 1 } } },
    { classes: { "M-09.2": 1, "M-02": 1 }, details: { "M-09.2": { "<form>": 1 } } },
    { classes: { "M-09.2": 1 }, details: { "M-09.2": { "<form>": 1, "<div>": 1 } } },
    { classes: {}, details: {} },
  ];
  const form = subStats(pages, "M-09.2", (k) => k === "<form>");
  assert.equal(form.total, 3);
  assert.equal(form.alone, 1);
  const other = subStats(pages, "M-09.2", (k) => k !== "<form>");
  assert.equal(other.total, 1);
  assert.equal(other.alone, 0);
});

test("redactUrl: a slug with an embedded admin edit path keeps four words", () => {
  assert.equal(
    redactUrl("https://news.example/a/b/one-two-three-four-five-httpsadminnewsexampleeditid42-six"),
    "https://news.example/a/b/one-two-three-four-REDACTED",
  );
  assert.equal(redactUrl("https://news.example/a/one-two-three-four-five"), "https://news.example/a/one-two-three-four-five");
  assert.equal(redactUrl("not a url"), "not a url");
});

test("outsideRepo: page data never goes inside the repository", () => {
  const repo = path.resolve("/r/repo");
  for (const out of ["/r/repo", "/r/repo/", "/r/repo/data", "/r/repo/..data/x", "/r/repo/a/../b"]) {
    assert.throws(() => outsideRepo(out, repo), /inside the repository/, out);
  }
  assert.equal(outsideRepo("/r/data", repo), path.resolve("/r/data"));
  assert.equal(outsideRepo("/r/repo-data", repo), path.resolve("/r/repo-data"));
  assert.equal(outsideRepo("/tmp/corpus", repo), path.resolve("/tmp/corpus"));
});
