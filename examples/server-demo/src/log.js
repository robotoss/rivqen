// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Structured, redacted logging (docs/engineering/architecture/errors-logging.md).
// One JSON object per line. Never logs bodies, cookies, auth headers or query strings.

const LEVELS = { debug: 10, info: 20, warn: 30, error: 40 };
const threshold = LEVELS[process.env.LOG_LEVEL ?? 'info'] ?? LEVELS.info;
const SAFE_DETAIL = new Set(['id', 'parent', 'element', 'code', 'bytes', 'blocks', 'operations', 'base', 'revision', 'template', 'status', 'mode', 'duration_ms']);

/** Keep origin + path only. */
export function safeUrl(url) {
  try {
    const u = new URL(url, 'http://localhost');
    return u.pathname;
  } catch {
    return '<invalid-url>';
  }
}

function clean(detail = {}) {
  const out = {};
  for (const [k, v] of Object.entries(detail)) if (SAFE_DETAIL.has(k)) out[k] = v;
  return out;
}

export function log(level, event, fields = {}) {
  if (LEVELS[level] < threshold) return;
  const line = { ts: new Date().toISOString(), level, event, ...fields };
  if (fields.detail) line.detail = clean(fields.detail);
  process.stdout.write(JSON.stringify(line) + '\n');
}

/** Log an error once, with its code, safe detail and cause chain (messages only). */
export function logError(event, err, fields = {}) {
  const chain = [];
  for (let e = err; e; e = e.cause) chain.push(`${e.name ?? 'Error'}: ${e.message}`);
  log('error', event, { ...fields, code: err?.code ?? 'INTERNAL_ERROR', detail: err?.detail, chain });
}
