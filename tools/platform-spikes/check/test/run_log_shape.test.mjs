// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import test from 'node:test';
import { copyFixture, expectProblem, problemsOf, rewriteRun } from './helpers.mjs';

test('run_log_shape: a line that is not JSON is reported', async (t) => {
  const dir = await copyFixture(t);
  await rewriteRun(dir, 'run-2.jsonl', (lines) => [...lines, 'not json']);
  expectProblem(await problemsOf(dir), /run-2\.jsonl: line 9: not JSON/);
});

test('run_log_shape: keys that are not sorted are reported', async (t) => {
  const dir = await copyFixture(t);
  await rewriteRun(dir, 'run-3.jsonl', (lines) => {
    lines[1] = '{"seq":0,"ev":"app.probe","name":"chunk","src":"app","t":8,"value":0}';
    return lines;
  });
  expectProblem(await problemsOf(dir), /run-3\.jsonl: line 2: keys are not sorted/);
});

test('run_log_shape: an unknown ev is reported', async (t) => {
  const dir = await copyFixture(t);
  await rewriteRun(dir, 'run-3.jsonl', (lines) => lines.map((l) => l.replace('"app.probe","name":"chunk"', '"app.other","name":"chunk"')));
  expectProblem(await problemsOf(dir), /unknown ev "app\.other"/);
});

test('run_log_shape: a src that does not match its ev is reported', async (t) => {
  const dir = await copyFixture(t);
  await rewriteRun(dir, 'run-3.jsonl', (lines) => lines.map((l) => l.replace('"src":"app","t":8', '"src":"host","t":8')));
  expectProblem(await problemsOf(dir), /src "host" is wrong for app\.probe/);
});

test('run_log_shape: a line without seq is reported', async (t) => {
  const dir = await copyFixture(t);
  await rewriteRun(dir, 'run-3.jsonl', (lines) => lines.map((l) => l.replace('"seq":1,"src":"app","t":9', '"src":"app","t":9')));
  expectProblem(await problemsOf(dir), /line 3: seq must be an integer/);
});

test('run_log_shape: a log whose first event is not run.start is reported', async (t) => {
  const dir = await copyFixture(t);
  await rewriteRun(dir, 'run-1.jsonl', (lines) => lines.slice(1));
  expectProblem(await problemsOf(dir), /first event is not run\.start/);
});

test('run_log_shape: a log whose last event is not run.end is reported', async (t) => {
  const dir = await copyFixture(t);
  await rewriteRun(dir, 'run-1.jsonl', (lines) => lines.slice(0, -1));
  expectProblem(await problemsOf(dir), /last event is not run\.end/);
});

test('run_log_shape: a run that did not end ok is reported', async (t) => {
  const dir = await copyFixture(t);
  await rewriteRun(dir, 'run-2.jsonl', (lines) => lines.map((l) => l.replace('"error":null,"ev":"run.end","ok":true', '"error":"app timeout","ev":"run.end","ok":false')));
  expectProblem(await problemsOf(dir), /run-2\.jsonl: run\.end is not ok/);
});

test('run_log_shape: a run of another capability or version is reported', async (t) => {
  const dir = await copyFixture(t);
  await rewriteRun(dir, 'run-1.jsonl', (lines) => lines.map((l) => l.replace('"capability":"CAP-F-01"', '"capability":"CAP-F-02"').replace('"os":"host-jvm"', '"os":"host-swift"')));
  const problems = await problemsOf(dir);
  expectProblem(problems, /run-1\.jsonl: run\.start capability "CAP-F-02" is not "CAP-F-01"/);
  expectProblem(problems, /run-1\.jsonl: run\.start os "host-swift" is not "host-jvm"/);
});
