// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Reader of the stored upstream server traces (evidence/traces/server/<server>/*.http)
// and the comparison of an expected response with a trace (fixtures/legacy/README.md,
// generator check 2).
//
// Trace layout: the request line, the request field lines, an empty line,
// "--- response ---", "HTTP <status>", the response field lines, an empty line,
// "[body decoded from content-encoding=<coding>]", then the decoded body bytes.

import { isDeepStrictEqual } from 'node:util';
import { valuesOf } from './fields.mjs';

/** The fields that a trace comparison looks at (besides the status and the body). */
export const TRACE_FIELDS = Object.freeze(['etag', 'template-tag', 'template-change', 'cache-offline', 'cache-control', 'sonic-etag-key']);

const RESPONSE_MARK = '\n\n--- response ---\n';
const BODY_MARK = '\n[body decoded from content-encoding=';

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
 * Parse a trace. Returns { request: { line, headers }, status, headers, encoding, body }.
 *
 * @param {Buffer} bytes
 */
export function parseTrace(bytes) {
  const split = bytes.indexOf(RESPONSE_MARK);
  if (split < 0) throw new TraceError('missing "--- response ---"');
  const requestLines = bytes.subarray(0, split).toString('utf8').split('\n');
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
  return {
    request: { line: requestLines[0], headers: fieldLines(requestLines.slice(1), 'request') },
    status: Number(statusLine[1]),
    headers: fieldLines(head.slice(1), 'response'),
    encoding,
    body: bytes.subarray(markerEnd + 1),
  };
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
 * Compare an expected response with a trace. Returns a list of differences (empty
 * when equal). `pageBytes` are the bytes of a `page` body. With `skip` (a Set of
 * field names) the named fields and the body content are not compared; the body
 * kind still is.
 */
export function compareToTrace(expected, trace, pageBytes, skip = new Set()) {
  const out = [];
  if (expected.status !== trace.status) out.push(`status: expected ${expected.status}, trace ${trace.status}`);
  for (const name of TRACE_FIELDS) {
    if (skip.has(name)) continue;
    const want = expected.fields[name];
    const got = valuesOf(trace.headers, name);
    if (!isDeepStrictEqual(want, got)) out.push(`${name}: expected ${JSON.stringify(want)}, trace ${JSON.stringify(got)}`);
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
