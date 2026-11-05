// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Offline check of a stored client evidence tree (CONTRACT sections 5, 6 and 8, DL-103).
import { readdir, readFile } from 'node:fs/promises';
import path from 'node:path';
import { canonicalLine } from './canon.mjs';
import { LabError } from './errors.mjs';
import { firstDifference, normalizeRun, outcomeRun } from './determinism.mjs';

const isObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
const isInt = Number.isInteger;
const str = (v) => typeof v === 'string';
const strOrNull = (v) => v === null || str(v);
const intOrNull = (v) => v === null || isInt(v);
const bool = (v) => typeof v === 'boolean';
const hex64 = (v) => str(v) && /^[0-9a-f]{64}$/.test(v);
const pairs = (v) => Array.isArray(v) && v.every((p) => Array.isArray(p) && p.length === 2 && str(p[0]) && str(p[1]));
const strings = (v) => Array.isArray(v) && v.every(str);
const KNOWN_FILE_KEYS = ['name', 'size', 'sha256', 'text'];
const oneFile = (f) =>
  isObject(f) && str(f.name) && isInt(f.size) && f.size >= 0 && hex64(f.sha256) &&
  Object.keys(f).every((k) => KNOWN_FILE_KEYS.includes(k)) &&
  (f.name.endsWith('.header') ? str(f.text) : !('text' in f)); // a *.header file also has its text (CONTRACT section 5)
const files = (v) => Array.isArray(v) && v.every(oneFile) && v.every((f, i) => i === 0 || v[i - 1].name < f.name);
const scalar = (v) => str(v) || isInt(v);
const ex = (v) => isInt(v) && v >= 1;

/** Event name -> { src: allowed sources, need: member checks, opt: optional member checks }. */
export const SCHEMA = {
  'run.start': { src: ['conductor'], need: { scenario: str, mode: str, repeat: isInt, origin: str, upstream_sha: str } },
  'step.start': { src: ['conductor'], need: { step: isObject } },
  'step.done': { src: ['conductor', 'harness'], need: { step: isObject }, opt: { error: str } },
  'session.created': { src: ['harness'], need: { session_id: strOrNull } },
  'client.load_url': { src: ['harness'], need: { url: str, extras: strings } },
  'client.load_data': {
    src: ['harness'],
    need: { base_url: str, mime: str, encoding: str, history_url: str, headers: pairs, body_len: isInt, body_sha256: hex64 },
  },
  'client.intercept': {
    src: ['harness'],
    need: { status: isInt, reason: strOrNull, mime: str, encoding: str, headers: pairs, body_len: isInt, body_sha256: hex64, complete: bool },
    opt: { error: strOrNull },
    alternative: { null: (v) => v === true }, // a null stream: { "null": true } and no other member
  },
  'session.result': { src: ['harness'], need: { src_code: isInt, final_code: isInt } },
  'session.message': { src: ['harness'], need: { json: str } },
  'session.callback': { src: ['harness'], need: { name: str, args: Array.isArray } },
  'runtime.cookie_get': { src: ['harness'], need: { url: str, value: strOrNull } },
  // setCookie gets the list of Set-Cookie values of the response (SonicRuntime.setCookie(url, List))
  'runtime.cookie_set': { src: ['harness'], need: { url: str, value: (v) => strOrNull(v) || strings(v) } },
  'cache.snapshot': { src: ['harness'], need: { files, db: (v) => v === null } },
  'proxy.request': { src: ['proxy'], need: { exchange: ex, method: str, target: str, fields: pairs } },
  'proxy.response_head': { src: ['proxy'], need: { exchange: ex, status: isInt, fields: pairs } },
  'proxy.response_end': { src: ['proxy'], need: { exchange: ex, body_len: isInt, body_sha256: hex64, complete: bool } },
  'proxy.cut': { src: ['proxy'], need: { exchange: intOrNull, bytes: isInt } },
  'proxy.drop_field': { src: ['proxy'], need: { exchange: intOrNull, name: str } },
  'proxy.hold': { src: ['proxy'], need: { exchange: ex, point: str }, opt: { bytes: isInt } },
  'proxy.release': { src: ['proxy'], need: { exchange: intOrNull, point: str } },
  'client.clear_history': { src: ['harness'], need: {} },
  'runtime.notify_error': { src: ['harness'], need: { url: str, code: scalar } },
  'runtime.toast': { src: ['harness'], need: { text: str, duration: scalar } },
  'webview.intercept_request': { src: ['harness'], need: { url: str }, opt: { method: str } },
  'webview.page_started': { src: ['harness'], need: { url: str } },
  'webview.page_finished': { src: ['harness'], need: { url: str } },
  'harness.error': { src: ['harness'], need: { error: str } },
  // The conductor writes run.end; the harness sends it with ok false only when it aborts.
  'run.end': { src: ['conductor', 'harness'], need: { ok: bool, error: strOrNull } },
};
const COMMON = ['ev', 'seq', 'src', 't', 'thread', 'hseq', 'session'];
const HOSTS_OK = /^(10\.0\.2\.2|127\.0\.0\.1|localhost|([a-z0-9-]+\.)*example\.(com|org|net))$/i;

/** Problems of one event object (without its position). */
export function eventProblems(e) {
  if (!isObject(e)) return ['not an object'];
  const out = [];
  if (!str(e.ev)) return ['ev is not a string'];
  if (!isInt(e.seq) || e.seq < 1) out.push('seq is not a positive integer');
  if (!['harness', 'proxy', 'conductor'].includes(e.src)) out.push('src is not harness, proxy or conductor');
  if (!isInt(e.t) || e.t < 0) out.push('t is not a non-negative integer');
  const spec = Object.hasOwn(SCHEMA, e.ev) ? SCHEMA[e.ev] : null;
  if (!spec) return [...out, `unknown event ${e.ev}`];
  if (!spec.src.includes(e.src)) out.push(`${e.ev} must come from ${spec.src.join(' or ')}`);
  if (e.src === 'harness' && (!str(e.thread) || e.thread === '')) out.push('harness event without thread');
  if (e.src === 'harness' && !(isInt(e.hseq) && e.hseq >= 1)) out.push('harness event without hseq');
  if (e.src !== 'harness' && e.thread !== undefined) out.push('thread only on harness events');
  if (e.src !== 'harness' && e.hseq !== undefined) out.push('hseq only on harness events');
  if (e.session !== undefined && !(e.src === 'harness' && isInt(e.session) && e.session >= 1)) out.push('session must be a positive integer on a harness event');
  const known = new Set([...COMMON, ...Object.keys(spec.need), ...Object.keys(spec.opt ?? {}), ...Object.keys(spec.alternative ?? {})]);
  if (spec.alternative && Object.entries(spec.alternative).every(([k, test]) => test(e[k]))) {
    const extra = Object.keys(e).filter((k) => !COMMON.includes(k) && !(k in spec.alternative));
    return [...out, ...extra.map((k) => `${e.ev}: member ${k} with the alternative form`)];
  }
  for (const [k, test] of Object.entries(spec.need)) {
    if (!(k in e)) out.push(`${e.ev}: missing ${k}`);
    else if (!test(e[k])) out.push(`${e.ev}: bad ${k}`);
  }
  for (const [k, test] of Object.entries(spec.opt ?? {})) {
    if (k in e && !test(e[k])) out.push(`${e.ev}: bad ${k}`);
  }
  for (const k of Object.keys(e)) if (!known.has(k)) out.push(`${e.ev}: unknown member ${k}`);
  return out;
}

/** Hosts of http(s) URLs in a text that are not lab or example.* hosts. */
export function foreignHosts(text) {
  const found = new Set();
  for (const m of text.matchAll(/https?:\/\/([^/\s:?#"'<>\\]+)/gi)) if (!HOSTS_OK.test(m[1])) found.add(m[1]);
  return [...found];
}

/** Parse and check the text of one run-<k>.jsonl. Returns { events, errors }. */
export function checkRunText(text) {
  const errors = [];
  const events = [];
  if (text === '') return { events, errors: ['empty event log'], warnings: [] };
  if (!text.endsWith('\n')) errors.push('the last line has no LF');
  const lines = text.split('\n');
  lines.pop();
  lines.forEach((line, i) => {
    const at = `line ${i + 1}`;
    let value;
    try {
      value = JSON.parse(line);
    } catch {
      errors.push(`${at}: not JSON`);
      return;
    }
    if (isObject(value) && canonicalLine(value) !== line) errors.push(`${at}: keys not sorted or not canonical JSON`);
    for (const p of eventProblems(value)) errors.push(`${at}: ${p}`);
    if (isObject(value) && isInt(value.seq) && value.seq !== i + 1) errors.push(`${at}: seq ${value.seq}, expected ${i + 1}`);
    if (isObject(value)) events.push(value);
  });
  const t = events.map((e) => e.t);
  if (t.some((v, i) => i > 0 && v < t[i - 1])) errors.push('t goes back');
  if (events.length > 0 && events[0].ev !== 'run.start') errors.push('the first event is not run.start');
  const last = events[events.length - 1];
  if (events.length > 0 && !(last.ev === 'run.end' && last.src === 'conductor')) errors.push('the last event is not run.end of the conductor');
  else if (last && last.ok !== true) errors.push(`the run did not succeed: ${last.error}`);
  if (events.filter((e) => e.ev === 'run.start').length > 1 || events.filter((e) => e.ev === 'run.end' && e.src === 'conductor').length > 1) {
    errors.push('more than one run.start or run.end');
  }
  if (events.some((e) => e.ev === 'run.end' && e.src === 'harness')) errors.push('the harness aborted the run');
  const hseqs = events.filter((e) => e.src === 'harness' && isInt(e.hseq)).map((e) => e.hseq).sort((a, b) => a - b);
  if (hseqs.some((v, i) => v !== i + 1)) errors.push('hseq of the harness events is not 1..n without gaps or repeats');
  const warnings = events.filter((e) => e.ev === 'harness.error').map((e) => `harness.error at seq ${e.seq}: ${e.error}`);
  const heads = new Set();
  const ends = new Set();
  const reqs = new Set();
  for (const e of events) {
    if (e.ev === 'proxy.request') reqs.add(e.exchange);
    if (e.ev === 'proxy.response_head') heads.add(e.exchange);
    if (e.ev === 'proxy.response_end') ends.add(e.exchange);
  }
  for (const n of reqs) if (!heads.has(n) || !ends.has(n)) errors.push(`exchange ${n} has no response_head or response_end`);
  for (const n of [...heads, ...ends]) if (!reqs.has(n)) errors.push(`exchange ${n} has no request`);
  for (const h of foreignHosts(text)) errors.push(`foreign host in the log: ${h}`);
  return { events, errors, warnings };
}

const RUN_JSON_KEYS = ['date', 'tools', 'emulator_image', 'upstream_sha', 'scenarios', 'summary'];

/** Problems of a parsed run.json (CONTRACT section 8). */
export function runJsonProblems(j) {
  if (!isObject(j)) return ['run.json is not an object'];
  const out = [];
  for (const k of RUN_JSON_KEYS) if (!(k in j)) out.push(`run.json: missing ${k}`);
  if ('date' in j && !(str(j.date) && /^\d{4}-\d{2}-\d{2}$/.test(j.date))) out.push('run.json: date must be yyyy-mm-dd');
  if ('tools' in j && !(isObject(j.tools) && Object.values(j.tools).every(str))) out.push('run.json: tools must map names to version strings');
  if ('emulator_image' in j && !str(j.emulator_image)) out.push('run.json: emulator_image must be a string');
  if ('upstream_sha' in j && !(str(j.upstream_sha) && /^[0-9a-f]{8,40}$/.test(j.upstream_sha))) out.push('run.json: upstream_sha must be a hex SHA');
  if ('scenarios' in j && !(Array.isArray(j.scenarios) && j.scenarios.every((s) => isObject(s) && str(s.id) && isObject(s.modes)))) {
    out.push('run.json: scenarios must be a list of { id, modes }');
  }
  if ('functions' in j) {
    const f = j.functions;
    if (!isObject(f) || !Object.keys(f).every((k) => PLATFORMS.includes(k))) out.push('run.json: functions must map android or ios to a meta object');
    else for (const [k, v] of Object.entries(f)) for (const p of metaProblems(v, k)) out.push(`run.json: functions.${k}: ${p}`);
  }
  if ('summary' in j) {
    const s = j.summary;
    const ok = isObject(s) && Array.isArray(s.stable) && Array.isArray(s.unstable) &&
      s.stable.every((x) => isObject(x) && str(x.scenario) && str(x.mode)) &&
      s.unstable.every((x) => isObject(x) && str(x.scenario) && str(x.mode) && str(x.reason) && x.reason !== '' &&
        (!('outcome' in x) || x.outcome === 'stable' || x.outcome === 'unstable'));
    if (!ok) out.push('run.json: summary needs stable [{scenario, mode}] and unstable [{scenario, mode, reason, outcome?: stable|unstable}]');
  }
  return out;
}

/** Parsed JSON of a file, or undefined when the file does not exist. Fails with RUN_JSON on invalid JSON. */
export async function readOptionalJson(file) {
  let text;
  try {
    text = await readFile(file, 'utf8');
  } catch (cause) {
    if (cause.code === 'ENOENT') return undefined;
    throw cause;
  }
  try {
    return JSON.parse(text);
  } catch (cause) {
    throw new LabError('BAD_JSON', `${path.basename(file)} is not valid JSON`, { cause });
  }
}

export const PLATFORMS = ['android', 'ios'];

/** Problems of the meta file that a function runner writes next to functions.jsonl (CONTRACT section 7). */
export function metaProblems(meta, platform) {
  if (!isObject(meta)) return ['meta file is not an object'];
  const out = [];
  if ('platform' in meta && meta.platform !== platform) out.push(`platform must be ${platform}`);
  if ('non_public_calls' in meta && !strings(meta.non_public_calls)) out.push('non_public_calls must be a list of strings');
  for (const k of ['macos_version', 'sdk_version', 'upstream_sha', 'compiler']) if (k in meta && !str(meta[k])) out.push(`${k} must be a string`);
  if ('cases' in meta && !(isInt(meta.cases) && meta.cases >= 0)) out.push('cases must be a non-negative integer');
  return out;
}

export async function listDir(dir) {
  try {
    return await readdir(dir, { withFileTypes: true });
  } catch (cause) {
    if (cause.code === 'ENOENT') return [];
    throw cause;
  }
}

/**
 * Check the evidence tree at `root` (the directory with android/ and run.json).
 * @param {string} root
 * @param {{ catalog?: object[] }} [options]  scenarios.json content: also checks modes and run counts
 * @returns {Promise<{ errors: string[], warnings: string[], unstable: { scenario: string, mode: string, difference: object, outcome: 'stable'|'unstable' }[], runs: number }>}
 */
export async function checkTree(root, { catalog } = {}) {
  const errors = [];
  const warnings = [];
  let runs = 0;
  let runJson = null;
  try {
    runJson = JSON.parse(await readFile(path.join(root, 'run.json'), 'utf8'));
    errors.push(...runJsonProblems(runJson));
  } catch (cause) {
    errors.push(cause.code === 'ENOENT' ? 'run.json is missing' : 'run.json is not valid JSON');
  }

  const observed = new Map(); // scenario -> Map mode -> normalized runs
  const scenarioDirs = (await listDir(path.join(root, 'android'))).filter((d) => d.isDirectory() && d.name.startsWith('CS-'));
  for (const sd of scenarioDirs) {
    const modes = new Map();
    observed.set(sd.name, modes);
    for (const md of await listDir(path.join(root, 'android', sd.name))) {
      const where = `${sd.name}/${md.name}`;
      if (!md.isDirectory() || !['quick', 'standard'].includes(md.name)) {
        errors.push(`${sd.name}/${md.name}: unexpected entry`);
        continue;
      }
      const names = (await listDir(path.join(root, 'android', where))).map((d) => d.name).sort();
      const jsonl = names.filter((n) => /^run-\d+\.jsonl$/.test(n));
      for (const n of names) if (!/^run-\d+\.jsonl$/.test(n) && !/^run-\d+-x\d+\.http$/.test(n)) errors.push(`${where}/${n}: unexpected file`);
      const normalized = [];
      const outcomes = [];
      for (const name of jsonl) {
        const k = Number(name.match(/^run-(\d+)/)[1]);
        const text = await readFile(path.join(root, 'android', where, name), 'utf8');
        const res = checkRunText(text);
        runs += 1;
        for (const e of res.errors) errors.push(`${where}/${name}: ${e}`);
        for (const w of res.warnings) warnings.push(`${where}/${name}: ${w}`);
        const wanted = new Set(res.events.filter((e) => e.ev === 'proxy.request').map((e) => e.exchange));
        for (const n of wanted) if (!names.includes(`run-${k}-x${n}.http`)) errors.push(`${where}/${name}: missing run-${k}-x${n}.http`);
        for (const n of names) {
          const m = n.match(new RegExp(`^run-${k}-x(\\d+)\\.http$`));
          if (m && !wanted.has(Number(m[1]))) errors.push(`${where}/${n}: no proxy.request for this exchange`);
          if (m) {
            const http = await readFile(path.join(root, 'android', where, n), 'utf8');
            if (!/^[A-Z]+ \S+ HTTP\/1\.1\n/.test(http) || !http.includes('\n--- response ---\n')) errors.push(`${where}/${n}: not the .http format`);
            for (const h of foreignHosts(http)) errors.push(`${where}/${n}: foreign host ${h}`);
          }
        }
        normalized.push(normalizeRun(res.events));
        outcomes.push(outcomeRun(res.events));
      }
      modes.set(md.name, { normalized, outcomes });
    }
  }

  const unstable = [];
  const listedUnstable = (runJson?.summary?.unstable ?? []).filter(isObject);
  const reasons = new Map(listedUnstable.map((u) => [`${u.scenario}/${u.mode}`, u.reason]));
  const listedOutcome = new Map(listedUnstable.filter((u) => 'outcome' in u).map((u) => [`${u.scenario}/${u.mode}`, u.outcome]));
  const stableListed = new Set((runJson?.summary?.stable ?? []).filter(isObject).map((u) => `${u.scenario}/${u.mode}`));
  for (const [scenario, modes] of observed) {
    for (const [mode, { normalized, outcomes }] of modes) {
      if (normalized.length === 0) continue;
      const difference = firstDifference(normalized);
      const key = `${scenario}/${mode}`;
      if (difference) {
        const outcome = firstDifference(outcomes) ? 'unstable' : 'stable';
        unstable.push({ scenario, mode, difference, outcome });
        if (!reasons.has(key)) errors.push(`${key}: runs differ (run ${difference.run + 1} at line ${difference.index + 1}) and run.json gives no reason`);
        if (stableListed.has(key)) errors.push(`${key}: run.json says stable, the runs differ`);
        if (listedOutcome.has(key) && listedOutcome.get(key) !== outcome) errors.push(`${key}: run.json says outcome ${listedOutcome.get(key)}, the outcome projection is ${outcome}`);
      } else {
        if (reasons.has(key)) errors.push(`${key}: run.json says unstable, the runs are equal`);
        if (runJson && !stableListed.has(key)) errors.push(`${key}: not listed in run.json summary`);
      }
    }
  }
  if (runJson && Array.isArray(runJson.scenarios)) {
    const listed = new Set(runJson.scenarios.map((s) => s.id));
    for (const id of observed.keys()) if (!listed.has(id)) errors.push(`${id}: not listed in run.json scenarios`);
    for (const id of listed) if (!observed.has(id)) errors.push(`${id}: listed in run.json but has no runs`);
  }
  for (const platform of PLATFORMS) {
    const file = path.join(root, platform, 'functions.jsonl.meta.json');
    let meta;
    try {
      meta = await readOptionalJson(file);
    } catch (cause) {
      if (!(cause instanceof LabError)) throw cause;
      errors.push(`${platform}/functions.jsonl.meta.json: not valid JSON`);
      continue;
    }
    if (meta === undefined) continue;
    for (const p of metaProblems(meta, platform)) errors.push(`${platform}/functions.jsonl.meta.json: ${p}`);
    if (runJson && canonicalLine(runJson.functions?.[platform] ?? null) !== canonicalLine(meta)) {
      errors.push(`run.json: functions.${platform} does not match ${platform}/functions.jsonl.meta.json`);
    }
  }
  if (catalog) {
    for (const s of catalog) {
      const modes = observed.get(s.id);
      if (!modes) continue;
      for (const mode of s.modes) {
        if ((modes.get(mode)?.normalized.length ?? 0) < s.repeat) errors.push(`${s.id}/${mode}: fewer than ${s.repeat} runs`);
      }
      for (const mode of modes.keys()) if (!s.modes.includes(mode)) errors.push(`${s.id}/${mode}: mode is not in scenarios.json`);
    }
    const known = new Set(catalog.map((s) => s.id));
    for (const id of observed.keys()) if (!known.has(id)) errors.push(`${id}: not in scenarios.json`);
  }
  return { errors, warnings, unstable, runs };
}

/** Problems of a functions.jsonl text against the case list (CONTRACT section 7). */
export function functionsProblems(text, platform, cases) {
  const out = [];
  const ids = new Set(cases.map((c) => c.id));
  const seen = new Set();
  text.split('\n').filter((l) => l !== '').forEach((line, i) => {
    let v;
    try {
      v = JSON.parse(line);
    } catch {
      out.push(`line ${i + 1}: not JSON`);
      return;
    }
    if (!isObject(v) || !str(v.id) || !str(v.fn) || v.platform !== platform) {
      out.push(`line ${i + 1}: needs id, fn and platform ${platform}`);
      return;
    }
    if (!ids.has(v.id)) out.push(`line ${i + 1}: unknown case ${v.id}`);
    if (seen.has(v.id)) out.push(`line ${i + 1}: duplicate case ${v.id}`);
    seen.add(v.id);
    const forms = ['result', 'error', 'unsupported'].filter((k) => k in v);
    if (forms.length !== 1) out.push(`line ${i + 1}: exactly one of result, error, unsupported`);
    if ('unsupported' in v && !(str(v.unsupported) && v.unsupported !== '')) out.push(`line ${i + 1}: unsupported needs a reason`);
  });
  for (const id of ids) if (!seen.has(id)) out.push(`case ${id} has no result`);
  return out;
}
