// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Reader of the stored upstream server traces (evidence/traces/server/<server>/*.http)
// and the comparison of an expected response with a trace (fixtures/legacy/README.md,
// generator check 2).
//
// Trace layout: the request line "<method> <path> HTTP/1.1", the request field lines
// (a case request sent through the lab control endpoint, S22 and later, has "Host" as
// its first line), an empty line, "--- response ---", "HTTP <status>", the response
// field lines, an empty line, "[body decoded from content-encoding=<coding>]", then the
// decoded body bytes.
//
// Every function here is linear in the size of its input. Only parseTrace throws, and
// only TraceError, for a file without the layout.

import { isDeepStrictEqual } from 'node:util';
import { asciiLower, trimOws, valuesOf } from './fields.mjs';

/** The fields that a trace comparison looks at (besides the status and the body). */
export const TRACE_FIELDS = Object.freeze(['etag', 'template-tag', 'template-change', 'cache-offline', 'cache-control', 'sonic-etag-key', 'content-type']);

/** The fields that only legacy processing adds; one of them in a trace shows legacy processing (U-13). */
const LEGACY_FIELDS = Object.freeze(['etag', 'template-tag', 'template-change', 'cache-offline']);

/** The unsafe upstream behaviors that a profile fixture can mark as not reproduced (each has a check below). */
export const NOT_REPRODUCIBLE = Object.freeze(['U-11', 'U-13']);

const RESPONSE_MARK = '\n\n--- response ---\n';
const BODY_MARK = '\n[body decoded from content-encoding=';
const REQUEST_LINE = /^([!#$%&'*+.^_`|~0-9A-Za-z-]+) (\S+) HTTP\/1\.1$/;

/** A trace file that does not have the layout. */
export class TraceError extends Error {
  constructor(message) {
    super(message);
    this.name = 'TraceError';
    this.code = 'LEGACY_TRACE_FORMAT';
  }
}

function fieldLines(lines, what) {
  return lines.map((line) => {
    const colon = line.indexOf(': ');
    if (colon <= 0) throw new TraceError(`${what}: not a field line: ${JSON.stringify(line)}`);
    return [line.slice(0, colon), line.slice(colon + 2)];
  });
}

/**
 * Parse a trace. Returns { request: { line, method, path, host, headers }, status,
 * headers, encoding, body }. `host` is the value of a `Host` first request line (the
 * layout of a case request), else null; that line is not in `request.headers`.
 *
 * @param {Buffer} bytes
 */
export function parseTrace(bytes) {
  const split = bytes.indexOf(RESPONSE_MARK);
  if (split < 0) throw new TraceError('missing "--- response ---"');
  const requestLines = bytes.subarray(0, split).toString('utf8').split('\n');
  const requestLine = REQUEST_LINE.exec(requestLines[0]);
  if (requestLine === null) throw new TraceError(`bad request line: ${JSON.stringify(requestLines[0].slice(0, 80))}`);
  const responseStart = split + RESPONSE_MARK.length;
  const bodyMark = bytes.indexOf(BODY_MARK, responseStart);
  if (bodyMark < 0) throw new TraceError('missing the body marker line');
  const markerEnd = bytes.indexOf('\n', bodyMark + 1);
  if (markerEnd < 0) throw new TraceError('the body marker line has no end');
  const encoding = bytes.subarray(bodyMark + BODY_MARK.length, markerEnd).toString('utf8').replace(/\]$/, '');
  const head = bytes.subarray(responseStart, bodyMark).toString('utf8').split('\n');
  head.pop(); // the text ends with the line end
  const statusLine = /^HTTP (\d{3})$/.exec(head[0] ?? '');
  if (statusLine === null) throw new TraceError(`bad status line: ${JSON.stringify(head[0])}`);
  let headers = fieldLines(requestLines.slice(1), 'request');
  let host = null;
  if (headers.length > 0 && asciiLower(headers[0][0]) === 'host') {
    host = headers[0][1];
    headers = headers.slice(1);
  }
  return {
    request: { line: requestLines[0], method: requestLine[1], path: requestLine[2], host, headers },
    status: Number(statusLine[1]),
    headers: fieldLines(head.slice(1), 'response'),
    encoding,
    body: bytes.subarray(markerEnd + 1),
  };
}

/** The authority and the path (with the query) of an absolute URL; null when it is not one. */
function authorityAndPath(target) {
  const m = /^[A-Za-z][A-Za-z0-9+.-]*:\/\/([^/?#@]+)((?:[/?][^#]*)?)$/.exec(target);
  if (m === null) return null;
  const path = m[2] === '' ? '/' : m[2].startsWith('?') ? `/${m[2]}` : m[2];
  return { authority: m[1], path };
}

/**
 * Is the fixture request the request of the trace? Always the method and the field
 * lines. The authority and the path only when the trace has a `Host` first line (a case
 * request: the lab served the fixture target); the older traces used the lab route
 * /p/<page>, so their path is not the fixture target.
 *
 * @param {{ method: string, target: string, headers: Array<[string, string]> }} request resolved fixture request
 */
export function sameRequest(request, trace) {
  if (request.method !== trace.request.method) return false;
  if (!isDeepStrictEqual(request.headers, trace.request.headers)) return false;
  if (trace.request.host === null) return true;
  const target = authorityAndPath(request.target);
  return target !== null && target.authority === trace.request.host && target.path === trace.request.path;
}

/** The shape of a body: 'none', 'data' (a legacy data object) or 'page'. */
export function bodyKindOf(body) {
  if (body.length === 0) return 'none';
  try {
    const value = JSON.parse(body.toString('utf8'));
    if (value !== null && typeof value === 'object' && !Array.isArray(value) && 'data' in value && 'html-sha1' in value) return 'data';
  } catch {
    // not JSON: a page
  }
  return 'page';
}

function expectedDataObject(body) {
  return { data: body.data, 'template-tag': body.template_tag, 'html-sha1': body.html_sha1, diff: body.diff };
}

/**
 * A content-type value in a form where equal media types give equal strings (RFC 9110
 * section 8.3.1): the type, the parameter names and the charset value in lower case, no
 * white space around ";" and "=", one pair of quotes removed from a value, the order of
 * the parameters kept. The upstream servers write the same application value in other
 * spellings (Tomcat: "text/html;charset=utf-8"; the PHP default: "charset=UTF-8"), so a
 * byte comparison would report differences that no client sees.
 */
export function mediaTypeKey(value) {
  const parts = value.split(';');
  const out = [asciiLower(trimOws(parts[0]))];
  for (const part of parts.slice(1)) {
    const eq = part.indexOf('=');
    if (eq < 0) {
      out.push(asciiLower(trimOws(part)));
      continue;
    }
    const name = asciiLower(trimOws(part.slice(0, eq)));
    let val = trimOws(part.slice(eq + 1));
    if (val.length >= 2 && val.startsWith('"') && val.endsWith('"')) val = val.slice(1, -1);
    out.push(`${name}=${name === 'charset' ? asciiLower(val) : val}`);
  }
  return out.join(';');
}

function comparable(name, values) {
  return name === 'content-type' ? values.map(mediaTypeKey) : values;
}

/**
 * Compare an expected response with a trace. Returns a list of differences (empty
 * when equal). `pageBytes` are the bytes of a `page` body. With `skip` (a Set of
 * field names) the named fields and the body content are not compared; the body
 * kind still is. `content-type` lines compare as media types (mediaTypeKey).
 */
export function compareToTrace(expected, trace, pageBytes, skip = new Set()) {
  const out = [];
  if (expected.status !== trace.status) out.push(`status: expected ${expected.status}, trace ${trace.status}`);
  for (const name of TRACE_FIELDS) {
    if (skip.has(name)) continue;
    const want = expected.fields[name];
    const got = valuesOf(trace.headers, name);
    if (!isDeepStrictEqual(comparable(name, want), comparable(name, got))) out.push(`${name}: expected ${JSON.stringify(want)}, trace ${JSON.stringify(got)}`);
  }
  const kind = expected.body.kind;
  const traceKind = bodyKindOf(trace.body);
  if (kind !== traceKind) {
    out.push(`body: expected ${kind}, trace ${traceKind}`);
  } else if (skip.size === 0) {
    if (kind === 'page' && !trace.body.equals(pageBytes)) out.push('body: page bytes differ');
    if (kind === 'data' && !isDeepStrictEqual(JSON.parse(trace.body.toString('utf8')), expectedDataObject(expected.body))) {
      out.push('body: data differs');
    }
  }
  if (skip.has('template-tag')) {
    const want = expected.fields['template-tag'];
    if (isDeepStrictEqual(want, valuesOf(trace.headers, 'template-tag'))) out.push('template-tag: the trace does not show the documented difference');
  }
  return out;
}

/**
 * U-13 (legacy processing outside the C-01 scope) is not reproduced. The fixture must be
 * outside the scope (not a GET, or an application status `appStatus` other than 200); its expected
 * response must have no legacy processing (no split, no data body, no template-change);
 * and the trace must show legacy processing: a 304 for another status, a data body, or
 * a legacy field (etag, template-tag, template-change, cache-offline) that the expected
 * response does not have.
 */
function compareU13(expected, trace, appStatus) {
  const out = [];
  if (expected.request.method === 'GET' && appStatus === 200) {
    out.push('U-13: the request and the application response are in the C-01 scope (GET, status 200)');
  }
  if (expected.split.applied || expected.body.kind === 'data' || expected.fields['template-change'].length > 0) {
    out.push('U-13: the expected response has legacy processing');
  }
  let shown = trace.status === 304 && expected.status !== 304;
  if (bodyKindOf(trace.body) === 'data') shown = true;
  for (const name of LEGACY_FIELDS) {
    if (expected.fields[name].length === 0 && valuesOf(trace.headers, name).length > 0) shown = true;
  }
  if (!shown) out.push('U-13: the trace does not show the documented difference (no 304, data body or legacy field)');
  return out;
}

/**
 * Compare the expected response of a profile fixture that has `not_reproduced` with its
 * trace. U-11: compareToTrace without the template-tag and the body content, and the
 * template-tag of the trace must differ. U-13: compareU13 (`appStatus`: the status of the
 * application response). Any other value gives one difference (loadCase accepts only
 * NOT_REPRODUCIBLE).
 */
export function compareNotReproduced(item, expected, trace, pageBytes, appStatus) {
  if (item === 'U-11') return compareToTrace(expected, trace, pageBytes, new Set(['template-tag']));
  if (item === 'U-13') return compareU13(expected, trace, appStatus);
  return [`not_reproduced ${JSON.stringify(item)} has no check`];
}
