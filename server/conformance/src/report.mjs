// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The report of CONTRACT.md section 4: one text line per case, the summary
// line, and the JSON form. A result is { id, level: "must" | "should",
// status: "pass" | "fail" | "skip", problems: [text] }. The problems hold
// only fixture values and response values; the runner sends no secret.

// C0 and C1 control characters: a problem text can hold response bytes
// (latin1), and a terminal must not act on them.
const CONTROL = /[\u0000-\u001f\u007f-\u009f]/g;
const escape = (s) => s.replace(CONTROL, (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, '0')}`);

/** The text line of one result. */
export function formatLine({ id, level, status, problems }) {
  const text = escape(problems.join('; '));
  if (status === 'pass') return `PASS ${id}`;
  if (status === 'skip') return `SKIP ${id}: ${text}`;
  return `FAIL ${level === 'should' ? '[SHOULD] ' : ''}${id}: ${text}`;
}

export function summarize(results) {
  const count = (f) => results.filter(f).length;
  return {
    passed: count((r) => r.status === 'pass'),
    failed: count((r) => r.status === 'fail' && r.level === 'must'),
    should_failed: count((r) => r.status === 'fail' && r.level === 'should'),
    skipped: count((r) => r.status === 'skip'),
  };
}

export function formatSummary(s) {
  return `${s.passed} passed, ${s.failed} failed, ${s.should_failed} SHOULD failed, ${s.skipped} skipped`;
}

/** The JSON report. */
export function toJson(opts, results, summary) {
  return { target: opts.target.origin, suite: opts.suite, profile: opts.profile ?? null, results, summary };
}
