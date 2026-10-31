// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Control endpoint of the upstream lab (server/conformance/CONTRACT.md section 2, with the
// lab exception of DL-081). TEST CODE ONLY: it runs in a lab container whose port is
// published on 127.0.0.1 of the host. Pure functions; the Koa adapter is in app.js.
'use strict';

const CONTROL_PREFIX = '/__rivqen/conformance/';
const INFO_PATH = CONTROL_PREFIX + 'info';
const CASE_PATH = CONTROL_PREFIX + 'case';
const MAX_CASE_BYTES = 8 * 1024 * 1024;

const isObject = (v) => typeof v === 'object' && v !== null && !Array.isArray(v);
// RFC 9110 section 5.6.2 token; section 5.5 field value: HTAB, SP, VCHAR, obs-text.
const TOKEN = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;
const FIELD_VALUE = /^[\t\x20-\x7e\x80-\xff]*$/;
const isWirePairList = (v) =>
  Array.isArray(v) &&
  v.every((p) => Array.isArray(p) && p.length === 2 && typeof p[0] === 'string' && typeof p[1] === 'string' && TOKEN.test(p[0]) && FIELD_VALUE.test(p[1]));

const lower = (s) => s.replace(/[A-Z]/g, (c) => c.toLowerCase());

/** Host of a control request: a loopback name with the PUBLISHED port of the lab (DL-081). Fails closed. */
function isControlHost(host, port) {
  if (typeof host !== 'string' || !/^[0-9]{1,5}$/.test(String(port))) return false;
  const h = lower(host);
  return h === `127.0.0.1:${port}` || h === `localhost:${port}` || h === `[::1]:${port}`;
}

/** Media type application/json (ASCII case-insensitive), parameters allowed. */
function isJsonContentType(value) {
  if (typeof value !== 'string') return false;
  return lower(value.split(';')[0].replace(/^[ \t]+|[ \t]+$/g, '')) === 'application/json';
}

function decodeBase64(text) {
  if (typeof text !== 'string' || text.length % 4 !== 0 || !/^[A-Za-z0-9+/]*={0,2}$/.test(text)) return null;
  return Buffer.from(text, 'base64');
}

/**
 * Parse a case body. Returns { case } or { error: 'BAD_CASE' | 'UNSUPPORTED' }.
 * BAD_CASE (invalid JSON or member) is checked before UNSUPPORTED. This lab server serves one
 * legacy profile; any other suite or profile is UNSUPPORTED.
 */
function parseCase(text, profile) {
  let json;
  try {
    json = JSON.parse(text);
  } catch {
    return { error: 'BAD_CASE' };
  }
  if (!isObject(json)) return { error: 'BAD_CASE' };
  if (json.suite !== 'rqp' && json.suite !== 'legacy') return { error: 'BAD_CASE' };
  if (typeof json.case_id !== 'string') return { error: 'BAD_CASE' };
  if (json.suite === 'legacy' && typeof json.profile !== 'string') return { error: 'BAD_CASE' };
  const page = decodeBase64(json.page_base64);
  if (page === null) return { error: 'BAD_CASE' };
  const app = json.app;
  if (!isObject(app) || !Number.isInteger(app.status) || app.status < 100 || app.status > 599 || !isWirePairList(app.headers)) {
    return { error: 'BAD_CASE' };
  }
  if (json.server_config !== undefined && !isObject(json.server_config)) return { error: 'BAD_CASE' };
  if (json.suite !== 'legacy' || json.profile !== profile) return { error: 'UNSUPPORTED' };
  return { case: { caseId: json.case_id, page, status: app.status, headers: app.headers } };
}

/**
 * Group the field lines by name (case-insensitive) in order of first appearance, so that a
 * repeated name becomes one array value (Node writes one line per value). Interleaved repeats
 * (a, b, a) therefore come out as a, a, b: NOT exact; the lab records this limit.
 */
function groupHeaders(headers) {
  const order = [];
  const byName = new Map();
  for (const [name, value] of headers) {
    const key = lower(name);
    if (!byName.has(key)) {
      byName.set(key, { name, values: [] });
      order.push(key);
    }
    byName.get(key).values.push(value);
  }
  return order.map((key) => {
    const { name, values } = byName.get(key);
    return [name, values.length === 1 ? values[0] : values];
  });
}

module.exports = {
  CONTROL_PREFIX, INFO_PATH, CASE_PATH, MAX_CASE_BYTES,
  isControlHost, isJsonContentType, parseCase, groupHeaders,
};
