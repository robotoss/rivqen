// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The Rivqen canonical legacy client: profile `legacy-canonical` of the client fixtures
// (DL-111). Every decision cites its rule CL-nn of docs/engineering/protocol/legacy-client.md
// section 9. Clean-room (R-3): the model is written from those rules and their observable
// effects (requests, web view calls, messages, codes, cookie calls, the abstract cache record);
// it has no upstream state names, flags or call structure.
//
// Terms (legacy client section 9): the *document* is the HTML that the session gives to the web
// view; *delivered* means that the web view got it (load_data or an intercept answer); a
// *normal load* means that the web view loads the URL itself; a *visible entry* is a record of
// the session key that passes CL-12 and CL-40.

import { isUtf8 } from 'node:buffer';
import { caseError, sha1hex, sha256hex } from './client-engine.mjs';
import { DEFAULT_APP } from './client-case.mjs';
import { sortRecords, sortedFields } from './client-trace.mjs';
import { asciiLower, parseContentType, trimOws, valuesOf } from './fields.mjs';
import { respond } from './model.mjs';
import { rebuild, split } from './split.mjs';

// --- response fields (CL-14, CL-15, CL-16, CL-21, CL-12) ---

/** CL-14: the name of the etag field: the value of `sonic-etag-key` when present and not empty, else `etag`. */
export function etagFieldName(headers) {
  const key = valuesOf(headers, 'sonic-etag-key');
  return key.length > 0 && key[0] !== '' ? asciiLower(key[0]) : 'etag';
}

/** CL-14: each of the four fields has a line, every line has one value, and the value is not empty. */
export function isLegacyResponse(headers) {
  for (const name of [etagFieldName(headers), 'template-tag', 'template-change', 'cache-offline']) {
    const values = valuesOf(headers, name);
    if (values.length === 0 || values[0] === '' || values.some((v) => v !== values[0])) return false;
  }
  return true;
}

/** CL-16: the charset parameter of Content-Type (ASCII lowercase), default utf-8. */
export function charsetOf(contentType) {
  if (typeof contentType !== 'string') return 'utf-8';
  const value = parseContentType(contentType).params.get('charset');
  return value === undefined ? 'utf-8' : asciiLower(value);
}

/** CL-16: a utf-8 body that is valid UTF-8. */
export function isUtf8Text(headers, body) {
  const types = valuesOf(headers, 'content-type');
  return charsetOf(types[0]) === 'utf-8' && isUtf8(body);
}

/** CL-15: one leading `W/` (exact case), then one pair of outer quotes. */
export function bareEtag(value) {
  let v = value;
  if (v.startsWith('W/')) v = v.slice(2);
  if (v.length >= 2 && v.startsWith('"') && v.endsWith('"')) v = v.slice(1, -1);
  return v;
}

/** CL-15: `template-change` exactly `false` or `0` is a data update. */
export function isDataUpdateValue(value) {
  return value === 'false' || value === '0';
}

function hasNoStore(headers) {
  for (const v of valuesOf(headers, 'cache-control')) {
    for (const item of v.split(',')) if (asciiLower(trimOws(item.split('=')[0])) === 'no-store') return true;
  }
  return false;
}

/** CL-15: the cache-offline value (`true`, `false`, `store`, `http`); `no-store` counts as `false`, `http` wins. */
export function offlineOf(headers) {
  const raw = valuesOf(headers, 'cache-offline')[0] ?? '';
  const v = asciiLower(raw);
  if (v === 'http') return 'http';
  if (hasNoStore(headers)) return 'false';
  return v === 'true' || v === 'store' ? v : 'false';
}

const HOP_BY_HOP = ['connection', 'keep-alive', 'proxy-connection', 'te', 'trailer', 'transfer-encoding', 'upgrade'];

/** CL-21: the page fields of a response (lowercase names, sorted by name, stable). */
export function pageFields(headers) {
  const drop = new Set([
    'set-cookie', 'set-cookie2', ...HOP_BY_HOP, 'cache-control', 'expires', 'content-encoding', 'content-length',
    etagFieldName(headers), 'sonic-etag-key', 'template-tag', 'template-change', 'cache-offline', 'vary', 'cdn-cache-control', 'surrogate-control',
  ]);
  for (const v of valuesOf(headers, 'connection')) for (const t of v.split(',')) if (trimOws(t) !== '') drop.add(asciiLower(trimOws(t)));
  return sortedFields(headers.filter(([n]) => !drop.has(asciiLower(n))));
}

const LEGACY_REQUEST = new Set(['accept-diff', 'if-none-match', 'template-tag']);

/** CL-12: the Vary names that count, and whether a `*` forbids the commit. */
export function varyNames(headers) {
  const names = new Set();
  let star = false;
  for (const v of valuesOf(headers, 'vary')) {
    for (const item of v.split(',')) {
      const name = asciiLower(trimOws(item));
      if (name === '*') star = true;
      else if (name !== '' && !LEGACY_REQUEST.has(name)) names.add(name);
    }
  }
  return { star, names: [...names].sort() };
}

/** The value of a request field (first line) or null. */
function requestValue(fields, name) {
  const v = valuesOf(fields, name);
  return v.length === 0 ? null : v[0];
}

// --- functions (CL-41, CL-42, CL-24) ---

/** CL-41: the client split, as the canonical server split. Returns { template, data: [[k, v]] } or null. */
export function canonicalSplit(text) {
  const parts = split('legacy-canonical', text);
  return parts.ok ? { template: parts.template, data: [...parts.data] } : null;
}

/** CL-42: the diff of the server data against the stored data, in server order. Null: a value is not a string. */
export function canonicalDiff(local, server) {
  const stored = new Map(local ?? []);
  for (const [, v] of stored) if (typeof v !== 'string') return null;
  const out = [];
  for (const [k, v] of server) {
    if (typeof v !== 'string') return null;
    if (!stored.has(k) || stored.get(k) !== v) out.push([k, v]);
  }
  return out;
}

/** Compact JSON object text with the members in the given order (CL-24, CL-38). */
export function objectText(entries, str = JSON.stringify) {
  return `{${entries.map(([k, v]) => `${str(k)}:${str(v)}`).join(',')}}`;
}

/**
 * CL-24: the page message text (without `local_refresh_time`).
 * `headers` are the fields of the response that set the result; `diff` the entries or null.
 */
export function canonicalMessage({ src, final, diff, headers, reload }) {
  const parts = [];
  if (final === 200 && diff !== null && diff.length > 0) parts.push(`"result":${JSON.stringify(objectText(diff))}`);
  parts.push(`"code":${final}`, `"srcCode":${src}`);
  const extra = [];
  const etagName = etagFieldName(headers);
  const keyName = valuesOf(headers, 'sonic-etag-key')[0];
  const etagKey = keyName === undefined || keyName === '' ? 'eTag' : keyName;
  for (const [name, key] of [[etagName, etagKey], ['template-tag', 'template-tag'], ['cache-offline', 'cache-offline']]) {
    const v = valuesOf(headers, name);
    if (v.length > 0) extra.push([key, v[0]]);
  }
  extra.push(['isReload', reload ? 'true' : 'false']);
  parts.push(`"extra":${objectText(extra)}`);
  return `{${parts.join(',')}}`;
}

// --- the cache (CL-38) ---

function recordView(r) {
  return {
    partition: r.partition,
    url: r.url,
    vary: r.vary,
    etag: r.etag,
    template_tag: r.template_tag,
    page_sha256: r.page === null ? null : sha256hex(r.page),
    template_sha256: r.template === null ? null : sha256hex(r.template),
    data_sha256: r.data === null ? null : sha256hex(objectText(r.data)),
    fields: sortedFields(r.fields),
  };
}

const sameVary = (a, b) => JSON.stringify(a) === JSON.stringify(b);

/** The abstract cache of CL-38: at most one record for each key (partition, URL, Vary values). */
export class CanonicalStore {
  constructor(sim) {
    this.sim = sim;
    this.records = [];
  }

  /** CL-12, CL-40: the visible entry for a request, or null; an entry that fails CL-40 is removed. */
  find(partition, url, requestFields) {
    const r = this.records.find((x) => x.partition === partition && x.url === url && x.vary.every(([n, v]) => requestValue(requestFields, n) === v));
    if (r === undefined) return null;
    if (sha1hex(r.page) !== r.etag || r.template === null || sha1hex(r.template) !== r.template_tag) {
      this.remove(r);
      return null;
    }
    return r;
  }

  /** CL-26: a commit replaces the entry of its key at once. */
  commit(record) {
    this.records = this.records.filter((x) => !(x.partition === record.partition && x.url === record.url && sameVary(x.vary, record.vary)));
    this.records.push(record);
  }

  remove(record) {
    this.records = this.records.filter((x) => x !== record);
  }

  removePartition(partition) {
    this.records = this.records.filter((x) => x.partition !== partition);
  }

  snapshot() {
    return sortRecords(this.records.map(recordView));
  }

  /** README "cache_before entry": the record of a canonical first load of the page, then the overrides. */
  seed(entry) {
    const sim = this.sim;
    const bytes = sim.pageBytes(entry.page);
    const request = [['accept-diff', 'true'], ['accept-encoding', 'gzip']];
    const app = { status: DEFAULT_APP.status, headers: DEFAULT_APP.headers.map(([n, v]) => [n, v]) };
    const response = respond(sim.b.server_profile, { method: 'GET', headers: request }, app, bytes);
    const body = response.body === null ? Buffer.alloc(0) : Buffer.from(response.body);
    const record = response.status === 200 && isLegacyResponse(response.headers) && isUtf8Text(response.headers, body)
      ? commitRecord({ partition: sim.config.account, url: sim.url }, response.headers, body, request)
      : null;
    if (record === null) throw caseError('CASE_CACHE_BEFORE', `cache_before: a canonical first load of page ${entry.page} from ${sim.b.server_profile} commits no record (CL-25)`);
    if (Object.hasOwn(entry, 'partition')) record.partition = entry.partition;
    if (entry.url !== undefined) record.url = entry.url;
    if (entry.vary !== undefined) record.vary = entry.vary.map(([n, v]) => [asciiLower(n), v]).sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));
    if (entry.fields !== undefined) record.fields = entry.fields.map(([n, v]) => [asciiLower(n), v]);
    if (entry.etag !== undefined) record.etag = entry.etag;
    if (entry.template_tag !== undefined) record.template_tag = entry.template_tag;
    this.commit(record);
  }
}

/**
 * CL-25 and CL-12: the record that a complete first load or template change commits, or null
 * when the commit is not allowed (cache-offline not true or store, no-store, Vary `*`,
 * SHA-1 of the page not the etag, SHA-1 of the client template not the template tag).
 */
export function commitRecord(key, headers, body, requestFields) {
  const offline = offlineOf(headers);
  if (offline !== 'true' && offline !== 'store') return null;
  const vary = varyNames(headers);
  if (vary.star) return null;
  const etag = bareEtag(valuesOf(headers, etagFieldName(headers))[0] ?? '');
  const tag = valuesOf(headers, 'template-tag')[0] ?? '';
  if (!isUtf8(body)) return null;
  const page = body.toString('utf8');
  const parts = canonicalSplit(page);
  if (sha1hex(body) !== etag || parts === null || sha1hex(parts.template) !== tag) return null;
  return {
    partition: key.partition,
    url: key.url,
    vary: vary.names.map((n) => [n, requestValue(requestFields, n)]),
    etag,
    template_tag: tag,
    page,
    template: parts.template,
    data: parts.data,
    fields: pageFields(headers),
  };
}

// --- documents ---

function cachedDoc(record) {
  return {
    kind: 'cached',
    status: 200,
    complete: true,
    get bytes() {
      return Buffer.from(record.page, 'utf8');
    },
    get fields() {
      return sortedFields(record.fields);
    },
  };
}

function responseDoc(ex) {
  return {
    kind: 'response',
    ex,
    status: ex.status,
    bytes: ex.body,
    fields: pageFields(ex.headers),
    get complete() {
      return ex.ended && ex.complete;
    },
  };
}

function textDoc(text, headers) {
  return { kind: 'rebuilt', status: 200, complete: true, bytes: Buffer.from(text, 'utf8'), fields: pageFields(headers) };
}

/** The canonical session (rules CL-05 to CL-37). */
export class CanonicalSession {
  constructor(sim, index, partition, url) {
    this.sim = sim;
    this.index = index;
    this.partition = partition;
    this.url = url;
    this.open = true; // until destroy (CL-07, CL-34)
    this.stopped = false; // CL-06, CL-34: no later call of any kind
    this.bound = false;
    this.ready_ = false;
    this.presented = false;
    this.gen = 0;
    this.refreshed = false; // CL-35: the document of the old generation counts as delivered
    this.entry = null;
    this.doc = null; // the current document (CL-20 item 1)
    this.noDoc = false; // a normal load ended the current document (CL-20 item 6)
    this.decided = false; // the outcome is known: a waiting intercept is answered
    this.armed = true; // the next intercept gets the current document (CL-20 items 4 and 5)
    this.waiting = 0;
    this.streams = [];
    this.firstDelivery = null;
    this.cachedGiven = false;
    this.result = null;
    this.callback = false;
    this.kind = null;
    this.pendingNew = null;
    this.requestFields = [];
  }

  // --- request (CL-09, CL-10, CL-11) ---
  buildRequest() {
    const fields = [['accept-diff', this.sim.config.accept_diff ? 'true' : 'false']];
    if (this.entry !== null) fields.push(['if-none-match', this.entry.etag], ['template-tag', this.entry.template_tag]);
    const cookie = this.sim.runtime.cookie;
    if (cookie !== null && cookie !== '') fields.push(['cookie', cookie]);
    fields.push(['accept-encoding', 'gzip']);
    return fields;
  }

  lookup() {
    const cookie = this.sim.runtime.cookie;
    const values = [['accept-encoding', 'gzip'], ...(cookie !== null && cookie !== '' ? [['cookie', cookie]] : [])];
    this.entry = this.sim.store.find(this.partition, this.url, values);
    if (this.entry !== null) this.doc = cachedDoc(this.entry);
  }

  start() {
    this.lookup();
    this.send();
  }

  send() {
    if (!this.sim.runtime.network) {
      // CL-10: no request and no cookie call; the cached page stays the document, else a normal load.
      this.decided = true;
      if (this.entry === null) this.normalLoad(false);
      this.resolveWaiting();
      return;
    }
    this.sim.cookieGet(this.url); // CL-11 item 1
    this.requestFields = this.buildRequest();
    const ex = this.sim.send(this, this.gen, this.requestFields);
    if (ex === null) this.error(); // CL-13 item 1: a refused connection
  }

  // --- response (CL-13) ---
  cookies(ex) {
    const values = ex.headers.filter(([n]) => ['set-cookie', 'set-cookie2'].includes(asciiLower(n))).map(([, v]) => v);
    if (values.length > 0) this.sim.cookieSet(this.url, values); // CL-11 item 2
  }

  classify(ex) {
    const h = ex.headers;
    const etag = valuesOf(h, etagFieldName(h))[0];
    const sameEtag = ex.status === 200 && etag !== undefined && this.entry !== null && bareEtag(etag) === this.entry.etag;
    if (this.entry !== null && (ex.status === 304 || sameEtag)) return 'not-modified';
    if (ex.status !== 200) return 'error';
    if (!isLegacyResponse(h) || !isUtf8Text(h, ex.body) || ex.body.length === 0) return 'normal'; // CL-14, CL-16, CL-32
    if (offlineOf(h) === 'http') return 'http';
    if (this.entry === null) return 'first-load';
    return isDataUpdateValue(valuesOf(h, 'template-change')[0]) ? 'data-update' : 'template-change';
  }

  current(ex) {
    return ex.gen === this.gen && this.open && !this.stopped;
  }

  onHead(ex) {
    if (!this.open || this.stopped) return; // CL-06, CL-34, CL-11 item 4
    this.cookies(ex);
    if (ex.gen !== this.gen) return; // CL-35: a response of the old generation is ignored
    this.kind = this.classify(ex);
    switch (this.kind) {
      case 'not-modified':
        return this.notModified(ex);
      case 'error':
        return this.error();
      case 'normal':
        return this.normalLoad(true);
      case 'http':
        return this.http();
      case 'first-load':
        this.decided = true;
        this.doc = responseDoc(ex);
        return this.resolveWaiting();
      case 'template-change':
        if (this.delivered()) this.pendingNew = responseDoc(ex);
        else this.doc = responseDoc(ex);
        this.decided = true;
        return this.resolveWaiting();
      default:
        return undefined; // data update: decided at the end of the body (CL-27, CL-29)
    }
  }

  onBody() {}

  onEnd(ex) {
    if (!this.current(ex)) return this.finishStreams(ex);
    if (!ex.complete) return this.cut(ex);
    switch (this.kind) {
      case 'first-load':
        return this.firstLoadEnd(ex);
      case 'template-change':
        return this.templateChangeEnd(ex);
      case 'data-update':
        return this.dataUpdateEnd(ex);
      default:
        return this.finishStreams(ex);
    }
  }

  /** The cached page (or, in a refresh, the old document) was delivered (CL-27, CL-31, CL-35). */
  delivered() {
    return this.cachedGiven || this.refreshed;
  }

  /** CL-22: 304 when the web view has no document yet, or its first document is the current one and was complete. */
  finalOf(src) {
    const d = this.firstDelivery;
    if (d === null) return 304;
    if (d.doc === this.doc && d.complete) return 304;
    return src;
  }

  // --- outcomes ---
  notModified(ex) {
    // CL-17: the cached page is the document; the page fields of the response replace the stored lines of their names.
    this.decided = true;
    const update = pageFields(ex.headers);
    const names = new Set(update.map(([n]) => n));
    this.entry.fields = [...this.entry.fields.filter(([n]) => !names.has(asciiLower(n))), ...update];
    this.setResult(304, 304, null, ex.headers);
    this.resolveWaiting();
  }

  error() {
    // CL-18: with a visible entry the cached page stays and the entry stays; else a normal load.
    this.decided = true;
    if (this.entry === null) this.normalLoad(false);
    this.resolveWaiting();
  }

  http() {
    // CL-19: the entry removed, the key disabled for unavailable_time_ms, a normal load.
    this.sim.disabled ??= [];
    this.sim.disabled.push({ partition: this.partition, url: this.url, until: this.sim.clock + this.sim.config.unavailable_time_ms });
    this.normalLoad(true);
  }

  /** CL-20 item 6, CL-14, CL-19, CL-29, CL-32: a normal load ends the current document. */
  normalLoad(removeEntry) {
    this.decided = true;
    if (removeEntry && this.entry !== null) this.sim.store.remove(this.entry);
    const wasDelivered = this.delivered();
    this.doc = null;
    this.pendingNew = null;
    this.noDoc = true;
    this.callback = false; // CL-23 item 5
    this.resolveWaiting();
    if (wasDelivered) this.webLoadUrl();
  }

  firstLoadEnd(ex) {
    this.finishStreams(ex);
    const record = commitRecord(this, ex.headers, ex.body, this.requestFields); // CL-25, CL-26
    if (record !== null) this.sim.store.commit(record);
    this.setResult(1000, this.finalOf(1000), null, ex.headers);
  }

  templateChangeEnd(ex) {
    const h = ex.headers;
    const offline = offlineOf(h);
    const record = commitRecord(this, h, ex.body, this.requestFields);
    if (this.pendingNew !== null && offline === 'store') {
      // CL-30: no reload; final 304.
      this.pendingNew = null;
      this.setResult(2000, 304, null, h);
    } else if (this.pendingNew !== null) {
      // CL-31: reload after the complete body; the callback is dropped (CL-23 item 5).
      this.doc = this.pendingNew;
      this.pendingNew = null;
      this.callback = false;
      if (this.sim.mode === 'quick') this.webLoadData(this.doc);
      else this.webLoadUrl();
      this.setResult(2000, 2000, null, h);
    } else {
      this.setResult(2000, this.finalOf(2000), null, h);
    }
    this.finishStreams(ex); // an intercept that got the new document as a stream (not delivered case)
    if (offline === 'false') this.sim.store.remove(this.entry); // CL-31: deliver, then remove the entry
    else if (record !== null) this.sim.store.commit(record);
  }

  /** CL-29: the rebuild check. Returns { rebuilt, data } or null. */
  rebuildCheck(ex) {
    const h = ex.headers;
    if ((valuesOf(h, 'template-tag')[0] ?? null) !== this.entry.template_tag) return null;
    let body;
    try {
      body = JSON.parse(ex.body.toString('utf8'));
    } catch {
      return null;
    }
    if (body === null || typeof body !== 'object' || Array.isArray(body)) return null;
    const data = body.data;
    if (data === null || typeof data !== 'object' || Array.isArray(data)) return null;
    const entries = Object.entries(data);
    if (!entries.every(([, v]) => typeof v === 'string')) return null;
    const rebuilt = rebuild(this.entry.template, new Map(entries));
    const sha = sha1hex(rebuilt);
    const etag = bareEtag(valuesOf(h, etagFieldName(h))[0] ?? '');
    if (sha !== body['html-sha1'] || sha !== etag) return null;
    return { rebuilt, data: entries };
  }

  dataUpdateEnd(ex) {
    const h = ex.headers;
    const check = this.rebuildCheck(ex);
    if (check === null) return this.normalLoad(true); // CL-29
    const offline = offlineOf(h);
    if (this.delivered()) {
      if (offline === 'store') {
        this.setResult(200, 304, null, h); // CL-30
      } else {
        const diff = canonicalDiff(this.entry.data, check.data); // CL-27, CL-42
        if (diff.length === 0) this.setResult(200, 304, null, h); // CL-28
        else this.setResult(200, 200, diff, h);
      }
    } else {
      this.doc = textDoc(check.rebuilt, h);
      this.setResult(200, this.finalOf(200), null, h);
    }
    const vary = varyNames(h);
    if (offline === 'false' || vary.star) {
      this.sim.store.remove(this.entry); // CL-27 (D-46), CL-12
      return undefined;
    }
    this.sim.store.commit({
      partition: this.partition,
      url: this.url,
      vary: vary.names.map((n) => [n, requestValue(this.requestFields, n)]),
      etag: bareEtag(valuesOf(h, etagFieldName(h))[0]),
      template_tag: valuesOf(h, 'template-tag')[0],
      page: check.rebuilt,
      template: this.entry.template,
      data: check.data,
      fields: pageFields(h),
    });
    return undefined;
  }

  cut(ex) {
    // CL-33: no commit; a stream in delivery ends incomplete, then a normal load.
    this.decided = true;
    const streaming = this.streams.some((s) => s.doc.ex === ex);
    this.finishStreams(ex);
    this.callback = false;
    this.pendingNew = null;
    if (streaming) {
      this.doc = null;
      this.noDoc = true;
      this.webLoadUrl();
    } else if (this.entry !== null) {
      this.doc = cachedDoc(this.entry); // CL-18
    } else {
      this.normalLoad(false);
    }
    this.resolveWaiting();
  }

  // --- results and messages (CL-22, CL-23, CL-24, CL-28) ---
  setResult(src, final, diff, headers) {
    this.result = { src, final, diff, headers, time: this.sim.clock, expired: false, diffSent: false };
    if (this.callback) this.sendMessage();
  }

  messageText(r) {
    return canonicalMessage({ src: r.src, final: r.final, diff: r.diff, headers: r.headers, reload: this.refreshed });
  }

  sendMessage() {
    const r = this.result;
    if (r === null || r.final === -1 || r.expired || this.stopped || !this.open) return;
    if (r.final === 200 && r.diff !== null && this.sim.clock - r.time > 30_000) {
      r.expired = true; // CL-28: an old diff is not sent, and no later message follows
      if (!r.diffSent) r.final = 304;
      return;
    }
    this.sim.message(this, this.messageText(r));
    if (r.diff !== null) r.diffSent = true;
  }

  getDiff() {
    if (!this.open || this.stopped) return;
    this.callback = true; // CL-23 item 2
    this.sendMessage();
  }

  // --- the web view (CL-20) ---
  bind() {
    if (!this.open || this.stopped) return;
    this.bound = true;
    this.present();
  }

  ready() {
    if (!this.open || this.stopped) return;
    this.ready_ = true;
    this.present();
  }

  /** CL-20 items 2, 3 and 7: the state when the client is bound and ready decides the first call. */
  present() {
    if (!this.bound || !this.ready_ || this.presented) return;
    this.presented = true;
    if (this.sim.mode === 'quick' && this.doc !== null && this.doc.complete) this.webLoadData(this.doc);
    else this.webLoadUrl();
  }

  webLoadUrl() {
    if (!this.bound || !this.ready_ || this.stopped || !this.open) return;
    this.sim.loadUrl(this);
    this.armed = true;
  }

  webLoadData(doc) {
    if (!this.bound || !this.ready_ || this.stopped || !this.open) return;
    this.sim.loadData(this, doc);
    this.noteDelivery(doc, true);
    this.armed = false;
  }

  noteDelivery(doc, complete) {
    if (this.firstDelivery === null) this.firstDelivery = { doc, complete };
    if (doc.kind === 'cached') this.cachedGiven = true;
  }

  intercept() {
    if (!this.open || this.stopped || !this.armed) {
      this.sim.interceptNull(this); // CL-20 item 5
      return;
    }
    this.armed = false;
    if (this.doc !== null) this.give(this.doc);
    else if (this.noDoc || this.decided) this.sim.interceptNull(this);
    else this.waiting += 1; // CL-20 item 4: wait for the response head
  }

  give(doc) {
    const complete = doc.complete;
    this.noteDelivery(doc, complete);
    if (complete) this.sim.interceptBody(this, doc.status, doc.fields, doc.bytes, true);
    else this.streams.push({ doc });
  }

  resolveWaiting() {
    while (this.waiting > 0) {
      this.waiting -= 1;
      if (this.doc !== null && !this.noDoc) this.give(this.doc);
      else this.sim.interceptNull(this);
    }
  }

  /** The intercepts that receive the body of `ex` as a stream end now (complete or not). */
  finishStreams(ex) {
    const done = this.streams.filter((s) => s.doc.ex === ex);
    this.streams = this.streams.filter((s) => s.doc.ex !== ex);
    for (const s of done) {
      const bytes = ex.complete ? ex.body : ex.body.subarray(0, ex.given);
      this.sim.interceptBody(this, s.doc.status, s.doc.fields, bytes, ex.complete);
    }
  }

  pageFinished() {} // CL-36

  refresh() {
    // CL-35: a new generation with the validators of the visible entry and a new cookie_get.
    if (!this.open || this.stopped) return;
    this.gen += 1;
    this.refreshed = true;
    this.result = null;
    this.kind = null;
    this.pendingNew = null;
    this.noDoc = false;
    this.decided = false;
    this.armed = false;
    const old = this.doc ?? { kind: 'old' };
    this.firstDelivery = { doc: old, complete: true };
    while (this.waiting > 0) {
      this.waiting -= 1;
      this.sim.interceptNull(this);
    }
    this.lookup();
    if (this.entry === null) this.doc = old.kind === 'old' ? null : old;
    this.send();
  }

  /** CL-06, CL-34: the session stops at once. */
  stop() {
    this.stopped = true;
    while (this.waiting > 0) {
      this.waiting -= 1;
      this.sim.interceptNull(this);
    }
    for (const s of this.streams) this.sim.interceptBody(this, s.doc.status, s.doc.fields, s.doc.ex.body.subarray(0, s.doc.ex.given), false);
    this.streams = [];
    this.sim.cancelTimers(this);
  }

  destroy() {
    if (!this.open) return;
    const r = this.result;
    this.sim.result(this, r === null ? -1 : r.src, r === null ? -1 : r.final); // CL-23 item 8
    if (!this.stopped) this.stop();
    this.open = false;
  }
}

/** The profile object of the engine. */
export const CANONICAL_PROFILE = Object.freeze({
  name: 'legacy-canonical',
  createStore: (sim) => new CanonicalStore(sim),
  createSession: (sim, index, partition, url) => new CanonicalSession(sim, index, partition, url),
  canOpen(sim, partition, url) {
    // CL-07: one open session for a key; CL-08: no session during the http period of the key.
    if (sim.sessions.some((s) => s.open && s.partition === partition && s.url === url)) return false;
    return !(sim.disabled ?? []).some((d) => d.partition === partition && d.url === url && d.until > sim.clock);
  },
  accountChanged(sim, from) {
    // CL-06: stop every open session of the old partition and remove its entries.
    for (const s of sim.sessions) if (s.open && !s.stopped && s.partition === from) s.stop();
    sim.store.removePartition(from);
  },
});
