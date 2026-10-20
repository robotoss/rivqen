// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The head sweep (WP-17 S5 T-09, Q-21): a repeatable check of the insertion point of the
// manifest element (manifest-patch section 1.7, "Document head"; H-72, H-79a; client rule C2).
//
// For each original document O:
//   1. The insertion point `at` is the one that the shipped reference gives
//      (tools/rqp-protocol `documentHeadStart`). The full document F is O with the manifest
//      element at `at` (tools/rqp-protocol `fullDocument`).
//   2. Client rule C2: the manifest element is the only one in F, it starts at `at`, and the
//      position computed on O (F without the element) is `at`.
//   3. When every candidate says that O is valid (the server sends a manifest only for such a
//      page): in each scripting view of parse5 the manifest element is the first child of the
//      head element of F, it is the only manifest, and the tree of F without it equals the
//      tree of O (head-views.mjs); and the block boundaries do not move: the blocks that the
//      candidates give on O, shifted by the length of the element, are the blocks of the parse5
//      tree of F.
//   4. Every candidate rejects F (markup M-13: the original document has no manifest element).
//   5. Hostile positions: a manifest element at every token boundary up to the end of the
//      token that ends item 1. The client rule must accept none but `at`.
//
// The module never returns document text: names, kinds, offsets and tag names only.

import { existsSync, readFileSync, readdirSync } from "node:fs";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { PAGE_IDS, pageBytes } from "../../../rqp-protocol/src/pages.mjs";
import { findManifestElements, documentHeadStart } from "../../../rqp-protocol/src/markup.mjs";
import { fullDocument } from "../../../rqp-protocol/src/server.mjs";
import { TokenType, Tokenizer } from "../../node/src/tokenizer.mjs";
import { compareCandidates, resultShapeError, show } from "./compare.mjs";
import { generateHeadDocument } from "./head-gen.mjs";
import { checkViews } from "./head-views.mjs";
import { CrosscheckError, buildCandidate, loadCandidates, runBatch, writeBatchDir } from "./run.mjs";
import { parse5View } from "./view.mjs";

/** Paths of the repository inputs. `root` is the repository root. */
function inputPaths(root) {
  return {
    fixtures: path.join(root, "fixtures", "rqp", "markup"),
    protocol: path.join(root, "fixtures", "rqp", "protocol"),
    examples: path.join(root, "spec", "rqp", "examples"),
    cases: path.join(root, "tools", "rqp-markup", "crosscheck", "cases"),
  };
}

/**
 * The fixed inputs: the golden markup fixtures, the protocol pages and bodies, the spec examples
 * and the case files. Each item is { name, original } or, for a document that already holds one
 * manifest element, { name, original, full, at } (original = F without the element).
 *
 * @param {string} root repository root
 * @returns {{ name: string, original: Buffer, full?: Buffer, at?: number }[]}
 */
export function staticItems(root) {
  const p = inputPaths(root);
  const raw = [];
  for (const n of readdirSync(p.fixtures).sort()) {
    const f = path.join(p.fixtures, n, "input.html");
    if (existsSync(f)) raw.push([`fixture:${n}`, readFileSync(f)]);
  }
  for (const id of PAGE_IDS) raw.push([`page:${id}`, pageBytes(id)]);
  for (const n of readdirSync(p.protocol).sort()) {
    const f = path.join(p.protocol, n, "response.body");
    if (existsSync(f)) raw.push([`body:${n}`, readFileSync(f)]);
  }
  const walk = (dir) => {
    for (const e of readdirSync(dir, { withFileTypes: true }).sort((a, b) => (a.name < b.name ? -1 : 1))) {
      const q = path.join(dir, e.name);
      if (e.isDirectory()) walk(q);
      else if (e.name.endsWith(".html")) raw.push([`spec:${path.relative(p.examples, q)}`, readFileSync(q)]);
    }
  };
  walk(p.examples);
  for (const file of readdirSync(p.cases).filter((f) => f.endsWith(".json")).sort()) {
    const list = JSON.parse(readFileSync(path.join(p.cases, file), "utf8"));
    for (const c of list) raw.push([`case:${file}:${c.id}`, Buffer.from(c.input, "utf8")]);
  }
  return raw.map(([name, bytes]) => splitFull(name, bytes));
}

/** A document with exactly one manifest element is a full document: split it into O and F. */
function splitFull(name, bytes) {
  const found = findManifestElements(bytes);
  if (found !== null && found.elements.length === 1 && found.elements[0].end > 0) {
    const el = found.elements[0];
    return { name, original: Buffer.concat([bytes.subarray(0, el.start), bytes.subarray(el.end)]), full: bytes, at: el.start };
  }
  return { name, original: bytes };
}

/**
 * The generated documents of the head generator profiles, one item for each index.
 * @param {number} seed
 * @param {number} from first index
 * @param {number} to  end index (exclusive)
 */
export function* generatedItems(seed, from, to) {
  for (let i = from; i < to; i++) yield { name: `gen seed=${seed} index=${i}`, original: Buffer.from(generateHeadDocument(seed, i), "utf8") };
}

/** The tokens before the head, up to the token that ends item 1 (DL-034): token boundaries of O. */
export function boundaries(original) {
  const tok = new Tokenizer(Buffer.from(original.buffer, original.byteOffset, original.byteLength));
  const out = new Set([0]);
  for (let n = 0; n < 64; n++) {
    const t = tok.next();
    out.add(t.start);
    out.add(t.end);
    const before =
      t.type === TokenType.COMMENT ||
      t.type === TokenType.DOCTYPE ||
      (t.type === TokenType.START_TAG && t.name === "html") ||
      (t.type === TokenType.END_TAG && !["head", "body", "html", "br"].includes(t.name));
    if (!before || t.type === TokenType.EOF) break;
  }
  return [...out].sort((a, b) => a - b);
}

const EMPTY_MANIFEST = Object.freeze({});

/**
 * The insertion point and the full document F of an item.
 *
 * @param {{ original: Buffer, full?: Buffer, at?: number }} item
 * @param {(bytes: Buffer) => number} position the server position function
 * @returns {{ at: number, full: Buffer | null }} `at` is -1 when O has no insertion point
 */
export function prepare(item, position = documentHeadStart) {
  if (item.full !== undefined) return { at: item.at, full: item.full };
  const at = position(item.original);
  if (at < 0) return { at, full: null };
  return { at, full: fullDocument(item.original, at, EMPTY_MANIFEST) };
}

/** The client rule C2 (manifest-patch 1.7, client item 3): the position computed on O equals the offset in F. */
export function clientC2(full, start, end, position = documentHeadStart) {
  const original = Buffer.concat([full.subarray(0, start), full.subarray(end)]);
  return position(original) === start;
}

/** The client rule C1 (a walk on F). It is not shipped; the tests use it as a known-bad control. */
export function clientC1(full, start, _end, position = documentHeadStart) {
  return position(full) === start;
}

/**
 * A document that holds one manifest element already is a full document. The client accepts it
 * only when the element is at the position that the server computes on O. The protocol fixtures
 * also hold documents with the element at another place: these are negative controls, not input
 * of the sweep.
 */
export function fullDocumentAccepted(item, position = documentHeadStart) {
  return item.full === undefined || position(item.original) === item.at;
}

/**
 * Check one prepared item.
 *
 * @param {{ original: Buffer, full?: Buffer, at?: number }} item
 * @param {{ at: number, full: Buffer | null }} prep result of prepare()
 * @param {{ id: string, format: string, start: number, end: number }[] | null} blocks
 *   the blocks that the candidates give on O (all valid and equal), null when O is not valid
 * @param {{ views?: ("enabled" | "disabled")[], clientPosition?: (b: Buffer) => number }} [options]
 * @returns {{ kind: string, details: string[] }[]} findings
 */
export function checkItem(item, prep, blocks, { views = ["enabled", "disabled"], clientPosition = documentHeadStart } = {}) {
  const findings = [];
  const add = (kind, ...details) => findings.push({ kind, details });
  const { at, full } = prep;
  if (full === null || !fullDocumentAccepted(item, clientPosition)) return findings;
  // Client rule C2 and the manifest element of F.
  const found = findManifestElements(full);
  if (found === null || found.elements.length !== 1 || found.elements[0].start !== at) {
    // An original document that holds a manifest-like element is not valid (M-13): no manifest is sent.
    if (blocks !== null || item.full !== undefined) add("client-elements", `F has not exactly one manifest element at ${at}`);
    return findings;
  }
  const el = found.elements[0];
  if (!clientC2(full, el.start, el.end, clientPosition)) add("client-c2", `the position computed on O is not ${at}`);
  if (blocks === null) return findings;
  // The tree of F (parse5, each view) against O.
  const names = checkViews(item.original, full, views);
  if (!names.ok) add("parse5-no-view", "a view has no tree");
  for (const f of names.failures) add(`parse5-${f}`, "see head-views.mjs");
  // Block boundaries: the candidate blocks on O, shifted by the length of the element.
  const shift = el.end - el.start;
  for (const v of views) {
    const view = parse5View(full, { scripting: v === "enabled" });
    if (!view.ok) continue; // reported as parse5-no-view
    const want = blocks.map((b) => ({ id: b.id, start: b.start >= at ? b.start + shift : b.start, end: b.end >= at ? b.end + shift : b.end }));
    const got = view.blocks.map((b) => ({ id: b.id, start: b.start, end: b.end }));
    const diff = blockMoves(want, got);
    if (diff.length > 0) add(`blocks-${v}`, ...diff);
  }
  return findings;
}

function blockMoves(want, got) {
  if (want.length !== got.length) return [`block count: shifted candidate ${want.length}, parse5 on F ${got.length}`];
  const out = [];
  want.forEach((w, i) => {
    const g = got[i];
    if (w.id !== g.id || w.start !== g.start || w.end !== g.end) out.push(`block ${i} (${show(w.id)}): shifted candidate ${w.start}..${w.end}, parse5 on F ${g.start}..${g.end}`);
  });
  return out;
}

/**
 * Hostile positions: a manifest element at every token boundary of O up to the end of item 1.
 * The client rule must accept the position `at` of the server and no other.
 *
 * @param {{ original: Buffer }} item
 * @param {number} at the server position (-1 for none)
 * @param {{ views?: ("enabled" | "disabled")[], clientAccepts?: (full: Buffer, start: number, end: number) => boolean }} [options]
 * @returns {{ positions: number, accepted: number, findings: { kind: string, details: string[] }[] }}
 */
export function checkHostile(item, at, { views = ["enabled", "disabled"], clientAccepts = clientC2 } = {}) {
  const findings = [];
  let positions = 0;
  let accepted = 0;
  for (const q of boundaries(item.original)) {
    const f = fullDocument(item.original, q, EMPTY_MANIFEST);
    if (f === null) continue;
    const el = findManifestElements(f);
    if (el === null || el.elements.length !== 1) continue;
    positions++;
    if (!clientAccepts(f, el.elements[0].start, el.elements[0].end)) continue;
    accepted++;
    if (q === at) continue; // the server position: checked by checkItem
    findings.push({ kind: "hostile-position", details: [`the client accepts a manifest element at ${q}, the server position is ${at}`] });
    const v = checkViews(item.original, f, views);
    for (const name of v.failures) findings.push({ kind: `hostile-parse5-${name}`, details: [`at ${q}`] });
  }
  return { positions, accepted, findings };
}

/** The sweep. Options: see parseArgs in cli.mjs. `write` receives output lines. */
export async function runHead(o, write, { repoRoot, position = documentHeadStart, clientPosition = documentHeadStart } = {}) {
  const candidates = loadCandidates(o.candidates, o.only);
  if (candidates.length === 0) throw new CrosscheckError("no candidate to run");
  let failed = 0;
  if (o.build) {
    for (const c of candidates) {
      const reason = await buildCandidate(c, { repoRoot, timeoutMs: o.timeoutMs });
      if (reason !== null) {
        failed++;
        write(`candidate ${c.name}: ${reason}`);
      }
    }
  }
  const outDir = o.out ?? path.join(os.tmpdir(), `rqp-crosscheck-head-${o.seed}`);
  const tmp = mkdtempSync(path.join(os.tmpdir(), "rqp-crosscheck-head-"));
  const totals = new Map();
  const kept = new Map();
  const stats = { documents: 0, valid: 0, inserted: 0, negativeFull: 0, hostilePositions: 0, hostileAccepted: 0 };
  const perCandidate = new Map(candidates.map((c) => [c.name, { valid: 0, failed: false }]));
  write(`head seed=${o.seed} count=${o.count} chunk=${o.chunk} scripting=${o.views.join(",")} candidates=${candidates.map((c) => c.name).join(",")}`);

  const record = (item, f) => {
    totals.set(f.kind, (totals.get(f.kind) ?? 0) + 1);
    const count = kept.get(f.kind) ?? 0;
    if (count >= o.keep) return;
    kept.set(f.kind, count + 1);
    for (const d of f.details.slice(0, 6)) write(`  ${f.kind} ${show(item.name)} ${d}`);
    const save = path.join(outDir, `${f.kind}-${count}-${o.seed}`);
    mkdirSync(save, { recursive: true });
    writeFileSync(path.join(save, "input.html"), item.original);
    writeFileSync(path.join(save, "finding.json"), `${JSON.stringify({ seed: o.seed, name: item.name, ...f }, null, 2)}\n`);
  };

  const chunk = async (items) => {
    const preps = items.map((item) => prepare(item, position));
    const docs = [];
    items.forEach((item, k) => {
      docs.push({ name: `o${String(k).padStart(6, "0")}`, bytes: item.original });
      if (preps[k].full !== null) docs.push({ name: `f${String(k).padStart(6, "0")}`, bytes: preps[k].full });
    });
    const dir = path.join(tmp, "batch");
    rmSync(dir, { recursive: true, force: true });
    writeBatchDir(dir, docs);
    const results = new Map();
    for (const c of candidates) {
      const r = await runBatch(c, dir, { repoRoot, timeoutMs: o.timeoutMs });
      if (r.ok) results.set(c.name, r.results);
      else if (!perCandidate.get(c.name).failed) {
        perCandidate.get(c.name).failed = true;
        failed++;
        write(`candidate ${c.name}: ${r.reason}`);
      }
    }
    items.forEach((item, k) => {
      stats.documents++;
      const oName = `o${String(k).padStart(6, "0")}`;
      const fName = `f${String(k).padStart(6, "0")}`;
      const oRes = new Map();
      const fRes = new Map();
      for (const [cand, map] of results) {
        const r = map.get(oName);
        if (r === undefined || resultShapeError(r) !== null) record(item, { kind: "bad-result", details: [`${cand}: no usable result for O`] });
        else oRes.set(cand, r);
        if (preps[k].full !== null) {
          const rf = map.get(fName);
          if (rf === undefined || resultShapeError(rf) !== null) record(item, { kind: "bad-result", details: [`${cand}: no usable result for F`] });
          else fRes.set(cand, rf);
        }
      }
      const mismatch = compareCandidates(oRes);
      if (mismatch.length > 0) record(item, { kind: "candidates", details: mismatch });
      for (const [cand, r] of fRes) if (r.valid) record(item, { kind: "candidate-accepts-f", details: [`${cand} accepts F (markup M-13)`] });
      const valid = oRes.size > 0 && mismatch.length === 0 && [...oRes.values()].every((r) => r.valid === true);
      for (const [cand, r] of oRes) if (r.valid) perCandidate.get(cand).valid++;
      if (valid) stats.valid++;
      const blocks = valid ? [...oRes.values()][0].blocks : null;
      if (preps[k].full !== null && fullDocumentAccepted(item, clientPosition)) stats.inserted++;
      else if (preps[k].full !== null) stats.negativeFull++;
      for (const f of checkItem(item, preps[k], blocks, { views: o.views, clientPosition })) record(item, f);
      if (item.full === undefined) {
        const h = checkHostile(item, preps[k].at, { views: o.views, clientAccepts: (f, s, e) => clientC2(f, s, e, clientPosition) });
        stats.hostilePositions += h.positions;
        stats.hostileAccepted += h.accepted;
        for (const f of h.findings) record(item, f);
      }
    });
  };

  try {
    let buffer = [];
    const flush = async () => {
      if (buffer.length > 0) await chunk(buffer);
      buffer = [];
    };
    for (const item of staticItems(repoRoot)) {
      buffer.push(item);
      if (buffer.length >= o.chunk) await flush();
    }
    await flush();
    for (let first = 0; first < o.count; first += o.chunk) {
      await chunk([...generatedItems(o.seed, first, Math.min(o.count, first + o.chunk))]);
    }
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }

  write("candidate | valid documents");
  for (const [cand, s] of perCandidate) write(`${cand} | ${s.valid}${s.failed ? " (did not run)" : ""}`);
  write(`valid for every candidate: ${stats.valid}; with an insertion point: ${stats.inserted}; full documents with the element elsewhere (negative controls, skipped): ${stats.negativeFull}`);
  write(`hostile positions: ${stats.hostilePositions}, accepted by the client rule: ${stats.hostileAccepted}`);
  const kinds = [...totals.entries()].map(([k, v]) => `${k}=${v}`).join(" ");
  if (totals.size > 0) write(`findings by kind: ${kinds}`);
  if (totals.size > 0) write(`saved inputs: ${outDir}`);
  const count = [...totals.values()].reduce((a, b) => a + b, 0);
  write(`head: ${stats.documents} documents, ${count} findings`);
  return count === 0 && failed === 0 ? 0 : 1;
}
