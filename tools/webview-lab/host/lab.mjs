// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Shared parts of the host tools: paths, the typed error, the case file parser (CONTRACT section 2.1)
// and the case list (CONTRACT section 2.3).

import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

export const HOST_DIR = dirname(fileURLToPath(import.meta.url));
export const LAB_DIR = dirname(HOST_DIR);
export const REPO_ROOT = dirname(dirname(LAB_DIR));
export const CASES_DIR = join(LAB_DIR, "cases");
export const FIXTURES_DIR = join(REPO_ROOT, "fixtures", "rqp", "markup");
export const FIXTURE_PREFIX = "FX-RQ-MARKUP-";

export const UTF8_CT = "text/html; charset=utf-8";
/** The text of the marker element of the Q-20 pages (CONTRACT section 2.3). */
export const MARKER = "Grüße – 日本";

/** An error with a stable code and a cause. */
export class LabError extends Error {
  /** @param {string} code @param {string} message @param {unknown} [cause] */
  constructor(code, message, cause) {
    super(message, cause === undefined ? undefined : { cause });
    this.name = "LabError";
    this.code = code;
  }
}

const ID_RE = /^[A-Z0-9-]{1,64}$/;
/** @param {unknown} id */
export function isValidId(id) {
  return typeof id === "string" && ID_RE.test(id);
}

/**
 * @typedef {object} LabCase
 * @property {string} id
 * @property {"Q-19"|"Q-20"} question
 * @property {string} source  repository path of the body bytes
 * @property {[string, string][]} headers
 * @property {("on"|"off")[]} views
 * @property {string|null} probe
 */

/**
 * Parse and check a case file. The input comes from disk, so it enters as unknown.
 * @param {unknown} raw
 * @param {string} where  file name, for the error message
 * @returns {LabCase}
 */
export function parseCase(raw, where) {
  const bad = (/** @type {string} */ why) => new LabError("CASE_INVALID", `${where}: ${why}`);
  if (raw === null || typeof raw !== "object" || Array.isArray(raw)) throw bad("not an object");
  const o = /** @type {Record<string, unknown>} */ (raw);
  if (!isValidId(o.id)) throw bad("bad id");
  if (o.question !== "Q-19" && o.question !== "Q-20") throw bad("bad question");
  if (typeof o.source !== "string" || o.source === "") throw bad("bad source");
  if (!Array.isArray(o.headers)) throw bad("bad headers");
  /** @type {[string, string][]} */
  const headers = [];
  for (const h of o.headers) {
    if (!Array.isArray(h) || h.length !== 2 || typeof h[0] !== "string" || typeof h[1] !== "string") {
      throw bad("bad header line");
    }
    // A name or a value with CR, LF or NUL would split the response.
    if (!/^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/.test(h[0]) || /[\r\n\0]/.test(h[1])) throw bad("bad header line");
    headers.push([h[0], h[1]]);
  }
  if (!Array.isArray(o.views) || o.views.length === 0) throw bad("bad views");
  /** @type {("on"|"off")[]} */
  const views = [];
  for (const v of o.views) {
    if (v !== "on" && v !== "off") throw bad("bad view");
    views.push(v);
  }
  if (o.probe !== null && typeof o.probe !== "string") throw bad("bad probe");
  return { id: o.id, question: o.question, source: o.source, headers, views, probe: o.probe };
}

/**
 * Q-20 cases in the order of CONTRACT section 2.3.
 * `expect`: "utf8" (the page is read as UTF-8), "latin1" (a rule makes another encoding win),
 * "info" (no rule decides; the lab records what the WebView does).
 * @type {{ id: string, source: string, headers: [string, string][], expect: "utf8"|"latin1"|"info" }[]}
 */
export const Q20_CASES = [
  { id: "CT-UTF8", source: "q20.html", headers: [["Content-Type", UTF8_CT]], expect: "utf8" },
  { id: "CT-NONE", source: "q20.html", headers: [], expect: "info" },
  { id: "CT-NO-CHARSET", source: "q20.html", headers: [["Content-Type", "text/html"]], expect: "info" },
  { id: "CT-LATIN1", source: "q20.html", headers: [["Content-Type", "text/html; charset=iso-8859-1"]], expect: "latin1" },
  {
    id: "CT-TWO-LINES",
    source: "q20.html",
    headers: [
      ["Content-Type", UTF8_CT],
      ["Content-Type", "text/html; charset=iso-8859-1"],
    ],
    expect: "info",
  },
  { id: "BOM-LATIN1", source: "q20-bom.html", headers: [["Content-Type", "text/html; charset=iso-8859-1"]], expect: "utf8" },
  { id: "BOM-NONE", source: "q20-bom.html", headers: [], expect: "utf8" },
  { id: "META-LATIN1", source: "q20-meta-latin1.html", headers: [["Content-Type", UTF8_CT]], expect: "utf8" },
];
