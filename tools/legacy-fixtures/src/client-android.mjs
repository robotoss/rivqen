// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Profile `legacy-android` of the client fixtures (DL-111): the canonical session of
// src/client-canonical.mjs with the named differences that the FACT rows of
// docs/engineering/protocol/legacy-client.md (sections 1 to 3: F1-F14, Q1-Q22, S1-S18, M-01-M-16),
// the divergences D-20-D-52 and the stored Android traces (legacy-client-traces.md, CS-01-CS-27;
// S1 and S2) prove. Each difference is a method or a function below with its row in the comment
// (A-nn). The traces are in evidence/traces/client/android/<scenario>/<mode>/.
//
// Clean-room (R-1, R-3): written from those rows and from the observable effects in the traces.
// The model reproduces what the client did (requests, web view calls, codes, messages, cookie
// calls, cache files), not how the upstream SDK is built.
//
// A branch that no FACT row or trace settles is not guessed: it is a ClientCaseError with the
// code CLIENT_NOT_MODELED and the row, so a fixture cannot get an unproven Android result.

import { createHash } from 'node:crypto';
import { notModeled, sha1hex } from './client-engine.mjs';
import { CanonicalSession, objectText } from './client-canonical.mjs';
import { sortRecords, sortedFields } from './client-trace.mjs';
import { asciiLower, valuesOf } from './fields.mjs';
import { rebuild, findBlocks } from './split.mjs';

// --- pure functions (D-20 to D-31, D-26) ---

const md5hex = (text) => createHash('md5').update(Buffer.from(text, 'utf8')).digest('hex');

function decodeQuery(text) {
  try {
    return decodeURIComponent(text.replace(/\+/g, ' '));
  } catch {
    return text;
  }
}

/**
 * A-01 (section 1, D-20, D-21; function cases FN-ID-*): the Android session id. Base: the
 * authority (host and port, case kept) and the path; scheme, fragment and other query
 * parameters are not part of it. Kept parameters: the names that a non-empty
 * `sonic_remain_params` lists, else the names that start with `sonic_`; sorted; each as the
 * decoded name and its first decoded value (a name whose raw form differs from the decoded
 * one reads the value `null`, FN-ID-ORD-05). MD5 of the UTF-8 text, lowercase hex; with
 * `account_related` the prefix `<account>_` (`null_` for no account, FN-ID-09).
 */
export function androidSessionId(url, account, accountRelated) {
  const text = String(url);
  const at = text.indexOf('://');
  const rest = at < 0 ? text : text.slice(at + 3);
  const end = rest.search(/[/?#]/);
  const authority = end < 0 ? rest : rest.slice(0, end);
  let tail = end < 0 ? '' : rest.slice(end);
  const hash = tail.indexOf('#');
  if (hash >= 0) tail = tail.slice(0, hash);
  const q = tail.indexOf('?');
  const path = q < 0 ? tail : tail.slice(0, q);
  const params = [];
  for (const part of (q < 0 ? '' : tail.slice(q + 1)).split('&')) {
    if (part === '') continue;
    const eq = part.indexOf('=');
    const raw = eq < 0 ? part : part.slice(0, eq);
    params.push({ raw, name: decodeQuery(raw), value: eq < 0 ? '' : decodeQuery(part.slice(eq + 1)) });
  }
  const valueOf = (name) => {
    const p = params.find((x) => x.raw === name);
    return p === undefined ? 'null' : p.value;
  };
  const names = [...new Set(params.map((p) => p.name))];
  const remain = params.find((p) => p.raw === 'sonic_remain_params');
  const listed = remain === undefined || remain.value === '' ? null : new Set(remain.value.split(';'));
  const kept = names.filter((n) => (listed === null ? n.startsWith('sonic_') : listed.has(n))).sort();
  const digest = md5hex(`${authority}${path}${kept.map((n) => `${n}${valueOf(n)}`).join('')}`);
  return accountRelated ? `${account === null ? 'null' : account}_${digest}` : digest;
}

/**
 * A-02 (D-22, D-23, D-24; FN-SPLIT-*): the Android split. Case-sensitive markers; the key is
 * the name of the opening marker, `{null}` for an unnamed one; the last value of a repeated
 * name wins and every place keeps its placeholder. After the blocks, the first literal
 * `<title>` element of the template becomes `{title}`. No block: an empty template and no data.
 * Returns { template, data: [[key, value], ...] }. Linear: the block scan of src/split.mjs
 * with the text itself as the comparison text (no case folding).
 */
export function androidSplit(text) {
  const blocks = findBlocks(text, text);
  if (blocks.length === 0) return { template: '', data: [] };
  const data = new Map();
  const pieces = [];
  let last = 0;
  for (const b of blocks) {
    const key = `{${b.name === '' ? 'null' : b.name}}`;
    data.set(key, text.slice(b.start, b.end));
    pieces.push(text.slice(last, b.start), key);
    last = b.end;
  }
  pieces.push(text.slice(last));
  let template = pieces.join('');
  const open = template.indexOf('<title>');
  const close = open < 0 ? -1 : template.indexOf('</title>', open + 7);
  const out = [];
  if (close >= 0) {
    out.push(['{title}', template.slice(open, close + 8)]);
    template = `${template.slice(0, open)}{title}${template.slice(close + 8)}`;
  }
  return { template, data: [...out, ...data] };
}

/**
 * A-03 (D-25, D-35; FN-DIFF-*): every server key whose value as a string differs from the
 * local value; a missing local value counts as ''. `local` null gives null (FN-DIFF-NEW-03).
 */
export function androidDiff(local, server) {
  if (local === null) return null;
  const stored = new Map(local);
  const out = [];
  for (const [k, v] of server) {
    const value = String(v);
    const before = stored.has(k) ? String(stored.get(k)) : '';
    if (value !== before) out.push([k, value]);
  }
  return out;
}

/** A-04 (D-31; FN-ETAG-*): a value that starts with `w/` in any case is lowercased and loses every `w/` and `"`. */
export function androidEtag(value) {
  const v = String(value);
  if (!asciiLower(v).startsWith('w/')) return v;
  return asciiLower(v).split('w/').join('').split('"').join('');
}

/** A-05 (D-29): `template-change` exactly `false` or `0` is a data update. */
export function androidTemplateChange(value) {
  return !(value === 'false' || value === '0');
}

/** org.json strings (legacy client section 5.2 rule 5): `/` is written as `\/`. */
const androidString = (s) => JSON.stringify(String(s)).replace(/\//g, '\\/');

/** A field value as the Android HTTP stack gives it: the lines of one name joined with `,` (D-33). */
function joined(headers, name) {
  const v = valuesOf(headers, name);
  return v.length === 0 ? null : v.join(',');
}

/**
 * A-06 (D-26; section 5.2 rule 5; legacy-client-traces section 6 item 7): the Android page
 * message without `local_refresh_time`. Keys: `result` (final 200 with a diff that is not
 * empty), `code`, `srcCode`, `extra` (the etag field under `eTag` or the name that
 * `sonic-etag-key` gives, `template-tag`, `cache-offline` of the response, each only when
 * present; `isReload`). Returns null when Android sends nothing: final -1, or final 200
 * with an empty diff (section 2.4 items 1 and 2).
 */
export function androidMessage({ src, final, diff, headers, reload }) {
  if (final === -1) return null;
  if (final === 200 && (diff === null || diff.length === 0)) return null;
  const parts = [];
  if (final === 200) parts.push(`"result":${androidString(objectText(diff, androidString))}`);
  parts.push(`"code":${final}`, `"srcCode":${src}`);
  const keyName = joined(headers, 'sonic-etag-key');
  const etagName = keyName === null || keyName === '' ? 'etag' : keyName;
  const extra = [];
  for (const [name, key] of [[etagName, keyName === null || keyName === '' ? 'eTag' : keyName], ['template-tag', 'template-tag'], ['cache-offline', 'cache-offline']]) {
    const v = joined(headers, name);
    if (v !== null) extra.push([key, v]);
  }
  extra.push(['isReload', reload ? 'true' : 'false']);
  parts.push(`"extra":${objectText(extra, androidString)}`);
  return `{${parts.join(',')}}`;
}

// --- fields (section 2.9, legacy-client-traces section 6 items 4 and 5) ---

const WEB_DROP = new Set(['set-cookie', 'cache-control', 'expires', 'etag']);

/**
 * A-07 (section 2.9 "Headers given to the WebView"; CS-03, CS-08, CS-09): the fields of a
 * document for the web view: lowercase names without `Set-Cookie`, `Cache-Control`,
 * `Expires`, `Etag` and every name that has more than one line.
 */
export function webFields(lines) {
  const lower = lines.map(([n, v]) => [asciiLower(n), v]);
  const count = new Map();
  for (const [n] of lower) count.set(n, (count.get(n) ?? 0) + 1);
  return sortedFields(lower.filter(([n]) => !WEB_DROP.has(n) && count.get(n) === 1));
}

// --- the cache (D-42 not reproduced: a record per session id) ---

/**
 * A-08 (section 8; CS-01, CS-04, CS-08): one record per session id with the page, the template,
 * the data and the header lines. A first-load and a template-change save add the line
 * `sonic-html-sha1`; a data-update save does not (CS-04, CS-05). Standard writes a data update
 * that F12 refuses without a template (legacy-client-traces section 6 item 3).
 */
export class AndroidStore {
  constructor(sim) {
    this.sim = sim;
    this.records = new Map();
  }

  get(id) {
    return this.records.get(id) ?? null;
  }

  write(id, record) {
    this.records.set(id, record);
  }

  remove(id) {
    this.records.delete(id);
  }

  snapshot() {
    const first = (lines, name) => lines.find(([n]) => n === name)?.[1] ?? null;
    return sortRecords([...this.records.values()].map((r) => ({
      partition: r.partition,
      url: r.url,
      vary: [],
      etag: first(r.header, 'etag'),
      template_tag: first(r.header, 'template-tag'),
      page_sha256: r.page === null ? null : createHash('sha256').update(r.page).digest('hex'),
      template_sha256: r.template === null ? null : createHash('sha256').update(r.template).digest('hex'),
      data_sha256: r.data === null ? null : createHash('sha256').update(JSON.stringify(r.data)).digest('hex'),
      fields: sortedFields(r.header),
    })));
  }

  seed() {
    throw notModeled('DL-112', 'cache_before under legacy-android (a trace-backed fixture starts with an empty app state)');
  }
}

// --- the session ---

function webDoc(kind, bytes, fieldsOf, ex = null) {
  return {
    kind,
    ex,
    status: 0, // A-09 (CONTRACT section 5, every intercept of CS-01-CS-18): API 28 reports status 0
    bytes,
    get fields() {
      return fieldsOf();
    },
    get complete() {
      return ex === null ? true : ex.ended && ex.complete;
    },
  };
}

const lowerLines = (headers) => headers.map(([n, v]) => [asciiLower(n), v]);

/** F12: the cache is written only when cache-offline is exactly `true` or `store`. */
const writeAllowed = (offline) => offline === 'true' || offline === 'store';

export class AndroidSession extends CanonicalSession {
  constructor(sim, index, partition, url) {
    super(sim, index, partition, url);
    this.id = androidSessionId(url, partition, sim.config.account_related);
    this.running = true;
    this.slot = 'preload'; // Quick: the one held message until client_ready (section 3.1 gate, M-06)
    this.slotDoc = null;
    this.interceptCalls = 0;
    this.interceptInvoked = false;
    this.loadDataInvoked = false;
    this.loadUrlInvoked = false;
    this.netStream = null; // Quick: the page of the response for the first intercept (Q1, Q2, Q12, Q14)
    this.netGiven = false;
    this.pending = null; // Standard: the stream for the intercept (S5-S11)
    this.pendingGiven = false;
    this.mode = 'read';
    this.sent200 = false;
    this.cookie = null;
    this.requestEtag = ''; // F4: the etag of the last request
    this.lastResponse = null; // the headers of the last complete template change (A-17)
    this.save = null; // the delayed save of A-13: { timer, doc }
    this.lastMessageAt = null; // A-17: the clock of the last page message
    this.delayedMessage = null; // A-17: the spaced message timer of a refresh
  }

  // A-10 (section 2.9, CS-12, CS-17): the cookie is read when the session is created.
  start() {
    if (this.sim.config.get_cookie_when_session_create) {
      this.sim.cookieGet(this.url);
      this.cookie = this.sim.runtime.cookie;
    }
    const record = this.sim.store.get(this.id);
    // F1: the cache is used only with a page and its etag.
    this.entry = record !== null && record.page !== null && record.etag !== null ? record : null;
    if (this.entry !== null) {
      const store = this.sim.store;
      const id = this.id;
      const page = Buffer.from(this.entry.page, 'utf8');
      this.cachedPage = webDoc('cached', page, () => webFields(store.get(id)?.header ?? []));
      this.pending = this.cachedPage; // S5-S15: the cached page is the pending stream at once
    }
    if (!this.sim.runtime.network) {
      // F3, Q21, S18: no request; only the preload acts; codes -1.
      this.running = false;
      return;
    }
    this.send();
  }

  // A-11 (CL-09 note, CS-01, CS-17): empty validators without a cache, `sonic-sdk-version`, the cookie read at creation.
  // The validators are those of the cache; a refresh gives its own (A-17).
  send(etag = this.entry?.etag ?? '', tag = this.entry?.template_tag ?? '') {
    if (!this.sim.config.get_cookie_when_session_create) {
      this.sim.cookieGet(this.url);
      this.cookie = this.sim.runtime.cookie;
    }
    this.requestEtag = etag;
    const fields = [
      ['accept-diff', this.sim.config.accept_diff ? 'true' : 'false'],
      ['if-none-match', etag],
      ['template-tag', tag],
      ['sonic-sdk-version', 'Sonic/2.0.0'],
    ];
    if (this.cookie !== null && this.cookie !== '') fields.push(['cookie', this.cookie]);
    fields.push(['accept-encoding', 'gzip']);
    this.requestFields = fields;
    const ex = this.sim.send(this, this.gen, fields);
    if (ex === null) this.androidError(); // F5: a connection error takes the error branch
  }

  // --- classification (section 2.2: F4-F9) ---
  androidClassify(ex) {
    const h = ex.headers;
    const etag = joined(h, 'etag');
    // F4: a 200 whose etag equals the request etag (the cache etag, or the refresh validator of A-17).
    if (ex.status === 304 || (ex.status === 200 && this.entry !== null && etag !== null && androidEtag(etag) === this.requestEtag)) {
      if (this.entry === null) throw notModeled('F4', 'a 304 without a cache');
      return 'not-modified';
    }
    if (ex.status !== 200) return 'error';
    const offline = joined(h, 'cache-offline');
    if (offline !== null && asciiLower(offline) === 'http') return 'http';
    if (this.entry === null) return 'first-load';
    if (offline === null || offline === '' || asciiLower(offline) === 'false') return 'remove'; // F7
    if (etag === null || joined(h, 'template-change') === null) return 'remove'; // F8
    return androidTemplateChange(joined(h, 'template-change')) ? 'template-change' : 'data-update';
  }

  onHead(ex) {
    if (!this.open) return;
    this.cookies(ex); // F10: Set-Cookie of every status first
    if (ex.gen !== this.gen) return;
    this.kind = this.androidClassify(ex);
    // A-17: a refresh is settled only for a 304 (CS-27); Q19 and S16 (a template change) and the other outcomes are not.
    if (this.refreshed && this.kind !== 'not-modified') throw notModeled('Q19, S16', `a ${this.kind} response to a refresh`);
    switch (this.kind) {
      case 'not-modified':
        this.setAndroidResult(304, 304, null, ex.headers, true); // Q7, S5
        return this.flowEnd();
      case 'error':
        return this.androidError();
      case 'http':
        // F6, Q5, Q16, S4: the id is unavailable; the cache is kept.
        this.sim.disabledIds ??= [];
        this.sim.disabledIds.push({ id: this.id, until: this.sim.clock + this.sim.config.unavailable_time_ms });
        if (this.sim.mode === 'quick') {
          if (this.presented && this.entry !== null) this.webLoadUrl(); // Q16: loadUrl once
          else this.slot = 'unavailable';
        }
        if (this.entry === null) this.pending = null;
        return this.flowEnd();
      case 'remove':
        this.sim.store.remove(this.id); // F7, F8: removed, no update; the cached page stays (Q17, Q18, S14, S15)
        return this.flowEnd();
      case 'first-load':
        if (this.waiting > 0) {
          // Q2, S1 (intercept first), Q22, S17: the waiting intercept gets the bridge stream at the head.
          this.mode = 'bridge';
          this.waiting -= 1;
          this.giveAndroid(webDoc('response', ex.body, () => webFields(ex.headers), ex));
        } else {
          this.mode = 'read'; // the session reads the body itself (CS-18: an intercept then waits for the end)
        }
        return undefined;
      default:
        return undefined; // data update and template change: at the end of the body
    }
  }

  androidError() {
    // F5, Q4, Q15, S3, S13: codes -1; the cached page stays (Quick gives it at client_ready, CS-11).
    if (this.entry === null) this.pending = null;
    this.flowEnd();
  }

  /** The flow of the session thread ends: a waiting intercept gets its answer (M-04). */
  flowEnd() {
    this.running = false;
    while (this.waiting > 0) {
      this.waiting -= 1;
      this.answerIntercept();
    }
  }

  onEnd(ex) {
    if (!this.open) return this.finishStreams(ex);
    if (ex.gen !== this.gen) return this.finishStreams(ex);
    switch (this.kind) {
      case 'first-load':
        return this.androidFirstLoadEnd(ex);
      case 'template-change':
        if (!ex.complete) throw notModeled('Q12, S9', 'a template change whose body ends with an error');
        return this.androidTemplateChangeEnd(ex);
      case 'data-update':
        if (!ex.complete) throw notModeled('Q8, S6', 'a data update whose body ends with an error');
        return this.androidDataUpdateEnd(ex);
      default:
        return this.finishStreams(ex);
    }
  }

  androidFirstLoadEnd(ex) {
    const doc = webDoc('response', ex.body, () => webFields(ex.headers), ex);
    const empty = ex.complete && ex.body.length === 0;
    if (this.mode === 'bridge') {
      if (empty) throw notModeled('Q2, S1', 'an empty first-load body through the bridge stream');
      this.finishStreams(ex);
      if (!ex.complete) {
        this.setAndroidResult(1000, 1000, null, ex.headers, true); // Q22, S17: 1000 / 1000, no cache
        return this.flowEnd();
      }
      this.setAndroidResult(1000, 1000, null, ex.headers, true); // Q2, S1 intercept first
      this.scheduleSave(ex, true);
      return this.flowEnd();
    }
    if (this.waiting > 0) {
      if (!ex.complete) return this.flowEnd();
      throw notModeled('S1, Q2', 'an intercept that arrives while the session reads the first-load body');
    }
    if (!ex.complete) return this.flowEnd(); // Q20, S2: a read error before any intercept: no document
    if (empty) return this.androidEmptyFirstLoad(ex, doc);
    if (this.sim.mode === 'quick') {
      this.netStream = doc;
      if (this.loadUrlInvoked) {
        this.setAndroidResult(1000, 1000, null, ex.headers, true); // Q2: the intercept gets the full body
      } else {
        this.slot = 'page'; // Q1: the page as data at client_ready, 1000 / 304, silent
        this.slotDoc = doc;
        this.setAndroidResult(1000, 304, null, ex.headers, false);
      }
    } else {
      this.pending = doc; // S1 body first: 1000 / 304, notify
      this.pendingGiven = false;
      this.setAndroidResult(1000, 304, null, ex.headers, true);
    }
    this.scheduleSave(ex, true, doc);
    return this.flowEnd();
  }

  /**
   * A-16 (Q3, S1; trace report section 5.2 "Quick empty 200 body" and section 6 item 10;
   * evidence/traces/client/android/CS-20/quick, CS-20/standard): a complete first-load body of
   * 0 bytes, read before any intercept.
   * Quick (Q3, probable upstream defect confirmed): no load call at client_ready; the intercept
   * gets the empty stream; codes stay -1 / -1, so no message.
   * Standard (S1 with an empty body): the empty response is the pending stream; 1000 / 1000, notify.
   * Nothing is saved (A-15). Quick after the preload loadUrl (Q3 note 2) is not settled.
   */
  androidEmptyFirstLoad(ex, doc) {
    if (this.sim.mode === 'quick') {
      if (this.loadUrlInvoked) throw notModeled('Q3', 'an empty first-load body after the preload loadUrl');
      this.slot = 'none';
      this.netStream = doc;
    } else {
      this.pending = doc;
      this.pendingGiven = false;
      this.setAndroidResult(1000, 1000, null, ex.headers, true);
    }
    return this.flowEnd();
  }

  androidTemplateChangeEnd(ex) {
    const h = ex.headers;
    const offline = joined(h, 'cache-offline');
    const doc = webDoc('response', ex.body, () => webFields(h), ex);
    // A-16 (Q14, S12; CS-21): an empty new page. A-15: it is not saved.
    const empty = ex.body.length === 0;
    if (this.sim.mode === 'quick') {
      this.netStream = doc;
      if (this.loadDataInvoked) {
        if (empty) throw notModeled('Q12, Q13', 'a template change with an empty body after the cached page was given as data');
        if (offline === 'store') {
          this.setAndroidResult(2000, 304, null, h, true); // Q13
        } else {
          this.callback = false; // Q12 note 5, M-16: the handler drops the page callback
          this.webLoadData(doc);
          this.setAndroidResult(2000, 2000, null, h, false); // Q12: silent
        }
      } else if (!this.presented) {
        // Q14: the new page as data, 2000 / 304, silent. An empty page also takes the data path
        // (CS-21 quick: 0 bytes as data, not the loadUrl of the row text).
        this.slot = 'page';
        this.slotDoc = doc;
        this.setAndroidResult(2000, 304, null, h, false);
      } else {
        throw notModeled('Q12, Q14', 'a Quick template change after a client_ready without a data load');
      }
    } else if (this.interceptInvoked) {
      if (empty) throw notModeled('S9, S10', 'a template change with an empty body after the intercept');
      if (offline === 'store') {
        this.setAndroidResult(2000, 304, null, h, true); // S10
      } else {
        this.webLoadUrl(); // S9: loadUrl again; the second intercept gets the new response
        this.pending = doc;
        this.pendingGiven = false;
        this.setAndroidResult(2000, 2000, null, h, true);
      }
    } else if (empty) {
      // S12 (CS-21 standard): no result (-1 / -1, no message); the pending stream is the empty
      // body with the header lines of the cached record (the trace intercept has the
      // `sonic-html-sha1` and the `template-tag` of the cached page, and 0 bytes).
      const store = this.sim.store;
      const id = this.id;
      this.pending = webDoc('response', ex.body, () => webFields(store.get(id)?.header ?? []), ex);
      this.pendingGiven = false;
    } else {
      this.pending = doc; // S11: the pending stream is now the new response
      this.pendingGiven = false;
      this.setAndroidResult(2000, 304, null, h, true);
    }
    this.lastResponse = h;
    this.scheduleSave(ex, true, doc);
    return this.flowEnd();
  }

  androidDataUpdateEnd(ex) {
    const h = ex.headers;
    const offline = joined(h, 'cache-offline');
    let body;
    try {
      body = JSON.parse(ex.body.toString('utf8'));
    } catch {
      body = null;
    }
    const data = body !== null && typeof body === 'object' && body.data !== null && typeof body.data === 'object' && !Array.isArray(body.data) ? Object.entries(body.data) : null;
    if (data === null || this.entry.template === null) throw notModeled('Q11', 'a data update with no data or no stored template');
    const rebuilt = rebuild(this.entry.template, new Map(data.map(([k, v]) => [k, String(v)])));
    if (sha1hex(rebuilt) !== body['html-sha1']) {
      if (this.sim.mode !== 'quick') throw notModeled('Q11', 'a Standard data update whose rebuilt page fails the html-sha1 check');
      this.sim.store.remove(this.id); // Q11: loadUrl, 200 / 1000, silent, removed
      this.webLoadUrl();
      this.setAndroidResult(200, 1000, null, h, false);
      return this.flowEnd();
    }
    // A-12 (legacy-client-traces section 6 item 1): the first-load data file is not a flat block
    // map, so the diff after a first load holds every block.
    const local = this.entry.data === null ? null : this.entry.data.form === 'flat' ? this.entry.data.entries : [];
    const diff = androidDiff(local ?? [], data);
    const doc = webDoc('rebuilt', Buffer.from(rebuilt, 'utf8'), () => webFields(h));
    if (this.sim.mode === 'quick') {
      if (this.loadDataInvoked) {
        if (offline === 'store') this.setAndroidResult(200, 304, null, h, true); // Q9
        else this.setAndroidResult(200, 200, diff, h, true); // Q8
      } else if (!this.presented) {
        this.slot = 'page'; // Q10: the rebuilt page as data, 200 / 304, silent
        this.slotDoc = doc;
        this.setAndroidResult(200, 304, null, h, false);
      } else {
        throw notModeled('Q8, Q10', 'a Quick data update after a client_ready without a data load');
      }
      // Q8 note 4, M-12: Quick writes when F12 allows it, else removes the cache.
      if (writeAllowed(offline)) this.writeDataUpdate(ex, rebuilt, data, true);
      else this.sim.store.remove(this.id);
    } else {
      if (offline === 'true') {
        if (this.interceptInvoked) {
          this.setAndroidResult(200, 200, diff, h, true); // S6
        } else {
          this.pending = doc; // S7: the rebuilt page replaces the pending stream
          this.pendingGiven = false;
          this.setAndroidResult(200, 304, null, h, true);
        }
      }
      // S8: `store` or another value: no message, codes -1, the cached page stays the pending stream.
      // S6/S8 note 2, M-12: Standard removes the cache when F12 refuses, then writes it without a template.
      this.writeDataUpdate(ex, rebuilt, data, writeAllowed(offline));
    }
    return this.flowEnd();
  }

  writeDataUpdate(ex, rebuilt, data, allowed) {
    const old = this.entry;
    this.sim.store.write(this.id, {
      partition: this.partition,
      url: this.url,
      etag: joined(ex.headers, 'etag'),
      template_tag: joined(ex.headers, 'template-tag'),
      page: rebuilt,
      template: allowed ? old.template : null,
      data: { form: 'flat', entries: data },
      header: lowerLines(ex.headers),
    });
  }

  /**
   * A-13 (section 2.6 item 6, M-13; CS-01, CS-02, CS-18): the save of a first load or template
   * change runs 1.5 s after the end of the body; a destroy before it cancels it. F12 applies.
   * A-15 (CS-20 and CS-21, both modes): a body of 0 bytes is never saved (no delayed save and
   * no stream-close save; the cache stays as it was).
   * `doc` is the document of the response that an intercept can read (A-17 needs its close).
   */
  scheduleSave(ex, withSha1, doc = null) {
    const offline = joined(ex.headers, 'cache-offline');
    if (!writeAllowed(offline) || !ex.complete || ex.body.length === 0) return;
    const page = ex.body.toString('utf8');
    const record = {
      partition: this.partition,
      url: this.url,
      etag: joined(ex.headers, 'etag'),
      template_tag: joined(ex.headers, 'template-tag'),
      page,
      template: androidSplit(page).template,
      data: { form: 'wrapped', entries: [] },
      header: [...lowerLines(ex.headers), ...(withSha1 ? [['sonic-html-sha1', sha1hex(ex.body)]] : [])],
    };
    const write = () => this.sim.store.write(this.id, record);
    this.save = { timer: this.sim.timer(this.sim.clock + 1500, this, write), doc, write };
  }

  // --- results and messages (section 2.4) ---
  setAndroidResult(src, final, diff, headers, notify) {
    this.result = { src, final, diff, headers, time: this.sim.clock };
    if (notify && this.callback) this.sendMessage();
  }

  /** A-14 (section 2.4; legacy-client-traces section 6 item 2): after a final-200 message the page gets `{}`. */
  sendMessage() {
    const r = this.result;
    if (r === null || !this.open) return;
    const text = this.androidText(r);
    if (text === null) return;
    // A-17 (section 2.4 item 3, D-37; evidence/traces/client/android/CS-27): after a refresh, two
    // page messages are at least 2 s apart. A closer one waits; a destroy cancels it (CS-27: the
    // 304 message 2 s after the result message, and no message for the get_diff 0.5 s after it).
    if (this.refreshed && this.lastMessageAt !== null && this.sim.clock - this.lastMessageAt < 2000) {
      if (this.delayedMessage !== null) throw notModeled('section 2.4 item 3', 'a second page message while a spaced one waits');
      this.delayedMessage = this.sim.timer(this.lastMessageAt + 2000, this, () => {
        this.delayedMessage = null;
        this.sendMessage();
      });
      return;
    }
    this.sim.message(this, text);
    this.lastMessageAt = this.sim.clock;
    if (r.final === 200) this.sent200 = true;
  }

  /** The text of the next page message, or null (A-06, A-14, section 2.4 item 2). */
  androidText(r) {
    if (this.sent200) return '{}';
    if (r.final === 200 && this.sim.clock - r.time > 30_000) return null; // section 2.4 item 2
    return androidMessage({ src: r.src, final: r.final, diff: r.diff, headers: r.headers, reload: this.refreshed });
  }

  getDiff() {
    if (!this.open) return;
    this.callback = true;
    this.sendMessage();
  }

  // --- the web view (sections 3.1 and 3.2) ---
  present() {
    if (!this.bound || !this.ready_ || this.presented) return;
    this.presented = true;
    if (this.sim.mode !== 'quick') return this.webLoadUrl(); // M-01: Standard loadUrl only
    // Quick: the held message decides (section 3.1 gate).
    if (this.slot === 'page') return this.webLoadData(this.slotDoc); // Q1, Q10, Q14
    if (this.slot === 'none') return undefined; // A-16, Q3: an empty first load loads nothing (CS-20)
    if (this.slot === 'unavailable' || this.entry === null) return this.webLoadUrl(); // Q4, Q5, Q16, Q20
    return this.webLoadData(this.cachedPage); // Q7, Q15, Q17, Q18: the cached page as data (CS-03, CS-09, CS-11)
  }

  webLoadUrl() {
    if (!this.bound || !this.ready_ || !this.open) return;
    this.sim.loadUrl(this);
    this.loadUrlInvoked = true;
  }

  webLoadData(doc) {
    if (!this.bound || !this.ready_ || !this.open) return;
    this.sim.loadData(this, doc);
    this.loadDataInvoked = true;
  }

  giveAndroid(doc) {
    if (doc.complete) {
      this.sim.interceptBody(this, doc.status, doc.fields, doc.bytes, true);
      doc.closedAt = this.sim.clock; // the web view read the stream to the end (A-17)
    } else {
      this.streams.push({ doc });
    }
  }

  /** The answer of an intercept when the session does not run (M-04, M-05). */
  answerIntercept() {
    if (this.sim.mode === 'quick') {
      if (this.netStream !== null && !this.netGiven) {
        this.netGiven = true;
        return this.giveAndroid(this.netStream); // CS-01, CS-06, CS-07: the page of the response
      }
      return this.sim.interceptNull(this); // CS-03, CS-04, CS-05
    }
    if (this.pending !== null && !this.pendingGiven) {
      this.pendingGiven = true;
      return this.giveAndroid(this.pending);
    }
    return this.sim.interceptNull(this);
  }

  intercept() {
    // Section 2.3: refresh() resets the intercept flag; what a later intercept gets is not settled.
    if (this.open && this.refreshed) throw notModeled('Q19, S16', 'an intercept after a refresh');
    this.interceptCalls += 1;
    this.interceptInvoked = true;
    if (!this.open) return this.sim.interceptNull(this);
    if (this.sim.mode === 'quick') {
      if (this.interceptCalls > 1) return this.sim.interceptNull(this); // M-05
      if (this.running) {
        this.waiting += 1; // M-04: up to 30 s while the session runs
        return undefined;
      }
      return this.answerIntercept();
    }
    if (this.pending !== null && !this.pendingGiven) return this.answerIntercept(); // M-04: no wait with a cache
    if (this.running && this.interceptCalls === 1) {
      this.waiting += 1;
      return undefined;
    }
    return this.sim.interceptNull(this);
  }

  /**
   * A-17 (section 2.3 `session.refresh()`, section 2.4 item 3, D-26, D-37; trace report section 4
   * note 7, section 5.2 and section 6 item 12; evidence/traces/client/android/CS-27/quick and
   * CS-27/standard). Settled only for a refresh after a complete template change, when the flow
   * has ended, with a connection:
   * - the codes go back to -1 / -1 and `isReload` becomes "true";
   * - the request has the etag and the template tag of the template-change response (not those of
   *   the cache: the delayed save has not written them yet) and no new cookie read (A-10);
   * - no load call; a 304 gives 304 / 304, notify, with the message spacing of sendMessage;
   * - the delayed save of the template change does not write (the SDK reports error -1005). The
   *   web view read the new page to the end, so the stream-close save writes it 1.5 s after that
   *   close. A stream-close save blocks destroy (section 2.6 items 3 and 6): a destroy does not
   *   cancel it.
   * Every other refresh is CLIENT_NOT_MODELED (Q19, S16).
   */
  refresh() {
    if (!this.open) throw notModeled('Q19, S16', 'a refresh after destroy');
    if (this.running) throw notModeled('Q19, S16', 'a refresh while the session runs');
    if (this.kind !== 'template-change') throw notModeled('Q19, S16', `a refresh after a ${this.kind ?? 'session without a response'} (CS-27 settles the validators after a template change only)`);
    if (!this.sim.runtime.network || this.sim.serverDown) throw notModeled('Q19, S16', 'a refresh without a connection');
    if (this.save !== null && !this.save.timer.done) {
      const closed = this.save.doc?.closedAt;
      if (closed === undefined) throw notModeled('Q19, S16', 'a refresh before the delayed save of a new page that no intercept read (trace report section 5.2)');
      this.sim.cancelTimer(this.save.timer);
      this.sim.timer(closed + 1500, { closeSaveOf: this }, this.save.write);
    }
    this.refreshed = true;
    this.result = null;
    this.gen += 1;
    this.running = true;
    this.kind = null;
    this.send(joined(this.lastResponse, 'etag') ?? '', joined(this.lastResponse, 'template-tag') ?? '');
  }

  destroy() {
    if (!this.open) return;
    const r = this.result;
    this.sim.result(this, r === null ? -1 : r.src, r === null ? -1 : r.final);
    this.stop(); // section 2.6: a waiting intercept gets null; the delayed save stops (item 6)
    this.open = false;
  }
}

export const ANDROID_PROFILE = Object.freeze({
  name: 'legacy-android',
  createStore: (sim) => new AndroidStore(sim),
  createSession: (sim, index, partition, url) => new AndroidSession(sim, index, partition, url),
  canOpen(sim, partition, url) {
    // Section 2.7: a session of the same id that is not destroyed refuses a new one (CS-15); F6: unavailable id.
    const id = androidSessionId(url, partition, sim.config.account_related);
    if (sim.sessions.some((s) => s.open && s.id === id)) return false;
    return !(sim.disabledIds ?? []).some((d) => d.id === id && d.until > sim.clock);
  },
  accountChanged() {}, // Section 2.8: no code detects an account change (CS-16)
});
