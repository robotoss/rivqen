// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Reference implementation of the Rivqen protocol (RQP) server side, as
// specified in docs/engineering/protocol/{markup,negotiation,manifest-patch}.md.
// It is a demo, not the production SDK: clarity over speed.

import { createHash } from 'node:crypto';
import { parse } from 'parse5';

export const RQP_VERSION = 1;
export const MANIFEST_TYPE = 'application/rivqen-manifest+json';
export const DELTA_TYPE = 'application/vnd.rivqen.delta+json';

const BLOCK_ATTR = 'data-rq-block';
const ID_RE = /^[a-z0-9][a-z0-9_-]{0,63}$/;
const FORBIDDEN = new Set(['style', 'template', 'iframe', 'textarea']);
const VOID = new Set(['area', 'base', 'br', 'col', 'embed', 'hr', 'img', 'input', 'link', 'meta', 'source', 'track', 'wbr']);

/** Error with a stable code from the Rivqen error catalog. */
export class RqpError extends Error {
  constructor(code, message, detail = {}) {
    super(message);
    this.name = 'RqpError';
    this.code = code;
    this.detail = detail; // safe fields only: ids, offsets, counts
  }
}

export const sha256 = (s) => createHash('sha256').update(s, 'utf8').digest('hex');

function attr(node, name) {
  return node.attrs?.find((a) => a.name === name)?.value;
}

/**
 * Find all data blocks in an HTML document.
 * Returns blocks in document order with UTF-16 offsets of their inner content.
 * Throws RqpError for invalid markup (duplicate, nested, forbidden, bad id).
 */
export function findBlocks(html) {
  const doc = parse(html, { sourceCodeLocationInfo: true });
  const blocks = [];
  const seen = new Set();

  const walk = (node, insideBlock) => {
    const id = node.tagName ? attr(node, BLOCK_ATTR) : undefined;
    let inside = insideBlock;
    if (id !== undefined) {
      if (!ID_RE.test(id)) throw new RqpError('RQP_MARKUP_INVALID_ID', 'invalid block id', { id });
      if (seen.has(id)) throw new RqpError('RQP_MARKUP_DUPLICATE', 'duplicate block id', { id });
      if (insideBlock) throw new RqpError('RQP_MARKUP_NESTED', 'nested block', { id, parent: insideBlock });
      if (FORBIDDEN.has(node.tagName) || VOID.has(node.tagName)) {
        throw new RqpError('RQP_MARKUP_FORBIDDEN_ELEMENT', 'element cannot be a block', { id, element: node.tagName });
      }
      let format = 'html';
      if (node.tagName === 'script') {
        if (attr(node, 'type') !== 'application/json') {
          throw new RqpError('RQP_MARKUP_FORBIDDEN_ELEMENT', 'only application/json scripts can be blocks', { id });
        }
        format = 'json';
      }
      const loc = node.sourceCodeLocation;
      if (!loc?.startTag || !loc?.endTag) {
        throw new RqpError('RQP_MARKUP_UNCLOSED', 'block element must have an explicit end tag', { id });
      }
      seen.add(id);
      blocks.push({ id, format, start: loc.startTag.endOffset, end: loc.endTag.startOffset });
      inside = id;
    }
    // <template> content lives in node.content in parse5.
    for (const child of node.childNodes ?? []) walk(child, inside);
    if (node.content) for (const child of node.content.childNodes ?? []) walk(child, inside);
  };
  walk(doc, null);
  blocks.sort((a, b) => a.start - b.start);
  return blocks;
}

/**
 * Split a document into template + blocks and compute revisions.
 * The template is the document with every block's inner content removed.
 */
export function analyze(html) {
  const found = findBlocks(html);
  let template = '';
  let last = 0;
  const blocks = [];
  for (const b of found) {
    const content = html.slice(b.start, b.end);
    if (b.format === 'json') {
      try {
        JSON.parse(content);
      } catch {
        throw new RqpError('RQP_MARKUP_BAD_JSON', 'json block is not valid JSON', { id: b.id });
      }
    }
    template += html.slice(last, b.start);
    last = b.end;
    blocks.push({ id: b.id, format: b.format, content, sha256: sha256(content) });
  }
  template += html.slice(last);
  const templateRevision = 'tpl-' + sha256(template).slice(0, 16);
  const pageRevision =
    'rev-' + sha256(templateRevision + '\n' + blocks.map((b) => `${b.id}:${b.sha256}`).join('\n')).slice(0, 16);
  return { template, templateRevision, pageRevision, blocks };
}

/** Build the page manifest (no content, no secrets). */
export function manifest({ origin, path, analysis, policy }) {
  return {
    protocol: `rqp/${RQP_VERSION}`,
    origin,
    path,
    template_revision: analysis.templateRevision,
    page_revision: analysis.pageRevision,
    blocks: analysis.blocks.map((b) => ({ id: b.id, format: b.format, sha256: b.sha256 })),
    policy,
    limits: { max_patch_bytes: 262144 },
  };
}

/** Insert the manifest script right before </head>. Escapes "<" to keep the script inert. */
export function injectManifest(html, m) {
  const json = JSON.stringify(m).replace(/</g, '\\u003c');
  const tag = `<script type="${MANIFEST_TYPE}" id="rq-manifest">${json}</script>`;
  const i = html.search(/<\/head\s*>/i);
  if (i < 0) throw new RqpError('RQP_NO_HEAD', 'document has no </head>');
  return html.slice(0, i) + tag + html.slice(i);
}

/**
 * Build a patch envelope from a base analysis to the current analysis.
 * Returns null when a patch is not possible (different template).
 */
export function buildPatch({ pageId, base, current, sequence }) {
  if (base.templateRevision !== current.templateRevision) return null;
  const old = new Map(base.blocks.map((b) => [b.id, b]));
  const operations = [];
  for (const b of current.blocks) {
    const prev = old.get(b.id);
    if (!prev || prev.sha256 !== b.sha256) {
      operations.push({ op: 'replace_block', block_id: b.id, format: b.format, content: b.content, sha256: b.sha256 });
    }
  }
  return {
    type: 'rivqen.patch',
    protocol: RQP_VERSION,
    page_id: pageId,
    base_revision: base.pageRevision,
    next_revision: current.pageRevision,
    template_revision: current.templateRevision,
    sequence,
    operations,
  };
}

/** Parse the request headers that matter for RQP. Header names are case-insensitive in Node. */
export function readRequest(headers) {
  const version = Number.parseInt(headers['rq-version'] ?? '', 10);
  const caps = (headers['rq-capabilities'] ?? '').replace(/[()]/g, ' ').split(/\s+/).filter(Boolean);
  const base = (headers['rq-base-revision'] ?? '').replace(/^"|"$/g, '') || null;
  const inm = (headers['if-none-match'] ?? '').split(',').map((s) => s.trim().replace(/^W\//, '').replace(/^"|"$/g, ''));
  return { isRqp: version === RQP_VERSION, caps: new Set(caps), base, ifNoneMatch: new Set(inm.filter(Boolean)) };
}
