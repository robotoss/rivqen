// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Compare a candidate result (CONTRACT.md section 3) with the parse5 view of the same document.
//
// The invariant under test ("no false accept"): when a candidate says that a document is
// valid, the WHATWG tree has exactly the same block elements, all in the HTML namespace,
// each with an explicit end tag, and the content of each block is the same byte range.
// The tree also has no manifest-like script element (M-13).
//
// A candidate that rejects a document is not checked: the rules are stricter than the
// WHATWG parser on purpose (markup.md section 5, item 3).

const MAX_SHOWN = 80;

/** Shorten a value for a message. Never longer than 80 characters. */
export function show(value) {
  const text = JSON.stringify(value) ?? "undefined";
  return text.length <= MAX_SHOWN ? text : `${text.slice(0, MAX_SHOWN - 3)}...`;
}

/**
 * Check the shape of a candidate result line. External data enters as unknown.
 *
 * @param {unknown} r
 * @returns {string | null} a reason when the shape is wrong
 */
export function resultShapeError(r) {
  if (typeof r !== "object" || r === null || Array.isArray(r)) return "result is not an object";
  const o = /** @type {Record<string, unknown>} */ (r);
  if (typeof o.valid !== "boolean") return "field valid is not a boolean";
  if (!Array.isArray(o.blocks)) return "field blocks is not an array";
  for (const b of o.blocks) {
    if (typeof b !== "object" || b === null) return "a block is not an object";
    const blk = /** @type {Record<string, unknown>} */ (b);
    if (typeof blk.id !== "string" || typeof blk.format !== "string") return "a block has no string id or format";
    if (!Number.isSafeInteger(blk.start) || !Number.isSafeInteger(blk.end)) return "a block has no integer start or end";
  }
  if (!o.valid && o.blocks.length > 0) return "an invalid result has blocks";
  return null;
}

/**
 * @param {unknown} result  candidate result object
 * @param {import("./view.mjs").View} view  parse5 view
 * @returns {{ status: "agree" | "disagree" | "rejected" | "bad-result", details: string[], categories: string[] }}
 *   categories (for a disagreement): "manifest" (a manifest-like script in the tree),
 *   "blocks" (block elements or offsets differ), "no-tree" (parse5 has no view)
 */
export function compareWithView(result, view) {
  const shape = resultShapeError(result);
  if (shape !== null) return { status: "bad-result", details: [shape], categories: ["bad-result"] };
  const r = /** @type {{ valid: boolean, blocks: { id: string, format: string, start: number, end: number }[] }} */ (
    result
  );
  if (!r.valid) return { status: "rejected", details: [], categories: [] };
  if (!view.ok) {
    return { status: "disagree", details: [`candidate valid, but parse5 view has no tree: ${view.reason}`], categories: ["no-tree"] };
  }

  const details = [];
  const categories = [];
  if (view.manifests > 0) {
    details.push(`the tree has ${view.manifests} manifest-like script element(s)`);
    categories.push("manifest");
  }
  const blockDetails = blockDifferences(r.blocks, view.blocks);
  if (blockDetails.length > 0) categories.push("blocks");
  details.push(...blockDetails);
  return { status: details.length > 0 ? "disagree" : "agree", details, categories };
}

/**
 * @param {{ id: string, format: string, start: number, end: number }[]} blocks candidate blocks
 * @param {import("./view.mjs").ViewBlock[]} want parse5 blocks
 * @returns {string[]}
 */
function blockDifferences(blocks, want) {
  const details = [];
  if (want.length !== blocks.length) {
    details.push(`block count: candidate ${blocks.length}, parse5 ${want.length}`);
  }
  const n = Math.min(want.length, blocks.length);
  for (let i = 0; i < n; i++) {
    const c = blocks[i];
    const p = want[i];
    const at = `block ${i} (${show(c.id)})`;
    if (p.id !== c.id) details.push(`${at} id: parse5 ${show(p.id)}`);
    if (!p.html) details.push(`${at}: parse5 element <${show(p.tag).slice(1, -1)}> is not in the HTML namespace`);
    if (p.start !== c.start) details.push(`${at} start: candidate ${c.start}, parse5 ${p.start}`);
    if (p.end === null) details.push(`${at} end: candidate ${c.end}, parse5 has no end tag (implied or EOF)`);
    else if (p.end !== c.end) details.push(`${at} end: candidate ${c.end}, parse5 ${p.end}`);
    const format = p.tag === "script" ? "json" : "html";
    if (format !== c.format) details.push(`${at} format: candidate ${show(c.format)}, parse5 element ${show(p.tag)}`);
  }
  for (let i = n; i < want.length; i++) {
    details.push(`parse5 block ${i} (${show(want[i].id)}) at ${want[i].start} is not in the candidate result`);
  }
  return details;
}

/**
 * Compare the results of several candidates for one document.
 * Error codes are not compared: a document with two errors can give either code (CONTRACT.md section 5).
 *
 * @param {Map<string, unknown>} byCandidate name → result
 * @returns {string[]} differences (empty when all agree)
 */
export function compareCandidates(byCandidate) {
  const entries = [...byCandidate.entries()].filter(([, r]) => resultShapeError(r) === null);
  if (entries.length < 2) return [];
  const key = (r) => {
    const o = /** @type {{ valid: boolean, blocks: { id: string, format: string, start: number, end: number }[] }} */ (r);
    return o.valid ? `valid ${o.blocks.map((b) => `${b.id}/${b.format}/${b.start}/${b.end}`).join(",")}` : "invalid";
  };
  const groups = new Map();
  for (const [name, r] of entries) {
    const k = key(r);
    if (!groups.has(k)) groups.set(k, []);
    groups.get(k).push(name);
  }
  if (groups.size === 1) return [];
  return [...groups.entries()].map(([k, names]) => `${names.join(",")}: ${show(k).slice(1, -1)}`);
}
