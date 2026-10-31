// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The report of CONTRACT.md section 4: one text line per case, the summary
// line, and the JSON form. A result is { id, level: "must" | "should",
// status: "pass" | "fail" | "skip", problems: [text] }. The problems hold
// only fixture values and response values; the runner sends no secret.

import { escapeText } from './escape.mjs';

/** At most this many problems are kept per case (DL-078, L3). */
export const MAX_PROBLEMS = 50;

/** The first MAX_PROBLEMS problems and one `… N more` line when there are more. */
export function capProblems(problems) {
  if (problems.length <= MAX_PROBLEMS) return problems;
  return [...problems.slice(0, MAX_PROBLEMS), `… ${problems.length - MAX_PROBLEMS} more`];
}

/** The text line of one result, escaped for a terminal. */
export function formatLine({ id, level, status, problems }) {
  const text = escapeText(problems.join('; '));
  const name = escapeText(id);
  if (status === 'pass') return `PASS ${name}`;
  if (status === 'skip') return `SKIP ${name}: ${text}`;
  return `FAIL ${level === 'should' ? '[SHOULD] ' : ''}${name}: ${text}`;
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
