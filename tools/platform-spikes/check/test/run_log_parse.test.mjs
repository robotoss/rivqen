// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import test from 'node:test';
import { normalizeRunLog, parseRunLog } from '../src/runs.mjs';

const START = '{"ev":"run.start","run":"r-1","seq":0,"src":"script","t":0}';
const END = '{"ev":"run.end","ok":true,"seq":1,"src":"script","t":9}';
const log = (...lines) => `${lines.join('\n')}\n`;

test('run_log_parse: a log with only run.start and run.end has no problems', () => {
  assert.deepEqual(parseRunLog(log(START, END)).problems, []);
});

for (const [ev, src] of [
  ['app.ssl_error', 'app'],
  ['app.error', 'app'],
  ['host.cut', 'host'],
]) {
  test(`run_log_parse: ${ev} is a known event written by ${src}`, () => {
    const line = `{"ev":"${ev}","seq":0,"src":"${src}"}`;
    assert.deepEqual(parseRunLog(log(START, line, END)).problems, []);
  });
  test(`run_log_parse: ${ev} with another src is reported`, () => {
    const line = `{"ev":"${ev}","seq":0,"src":"script"}`;
    assert.deepEqual(parseRunLog(log(START, line, END)).problems, [`line 2: src "script" is wrong for ${ev}`]);
  });
}

test('run_log_parse: a CR in the log is reported', () => {
  assert.deepEqual(parseRunLog(`${START}\r\n${END}\r\n`).problems, ['contains CR (the log is LF only)']);
});

test('run_log_parse: an empty log is reported and nothing else', () => {
  assert.deepEqual(parseRunLog('').problems, ['empty log']);
});

test('run_log_parse: a last line without a final newline is still read', () => {
  const parsed = parseRunLog(`${START}\n${END}`);
  assert.deepEqual(parsed.problems, []);
  assert.equal(parsed.events.length, 2);
});

for (const [name, text] of [['an array', '[1]'], ['a number', '5'], ['null', 'null'], ['a string', '"x"']]) {
  test(`run_log_parse: a line that is ${name} is reported and not kept as an event`, () => {
    const parsed = parseRunLog(log(START, text, END));
    assert.deepEqual(parsed.problems, ['line 2: not a JSON object']);
    assert.equal(parsed.events.length, 2);
  });
}

test('run_log_parse: a blank line in the middle is not JSON', () => {
  assert.deepEqual(parseRunLog(`${START}\n\n${END}\n`).problems, ['line 2: not JSON']);
});

for (const [name, seq] of [['negative', '-1'], ['fractional', '1.5'], ['a string', '"0"'], ['null', 'null']]) {
  test(`run_log_parse: a seq that is ${name} is reported`, () => {
    const line = `{"ev":"app.error","seq":${seq},"src":"app"}`;
    assert.deepEqual(parseRunLog(log(START, line, END)).problems, ['line 2: seq must be an integer >= 0']);
  });
}

test('run_log_parse: a seq of 0 is accepted', () => {
  assert.deepEqual(parseRunLog(log(START, '{"ev":"app.error","seq":0,"src":"app"}', END)).problems, []);
});

for (const [name, bytes] of [['missing', ''], ['negative', ',"bytes":-1'], ['fractional', ',"bytes":1.5'], ['a string', ',"bytes":"9"']]) {
  test(`run_log_parse: app.stream.read with bytes ${name} is reported`, () => {
    // keys must stay sorted: bytes first
    const line = bytes === '' ? '{"ev":"app.stream.read","seq":0,"src":"app"}' : `{${bytes.slice(1)},"ev":"app.stream.read","seq":0,"src":"app"}`;
    assert.deepEqual(parseRunLog(log(START, line, END)).problems, ['line 2: app.stream.read bytes must be an integer >= 0']);
  });
}

test('run_log_parse: app.stream.read with 0 bytes is accepted', () => {
  assert.deepEqual(parseRunLog(log(START, '{"bytes":0,"ev":"app.stream.read","seq":0,"src":"app"}', END)).problems, []);
});

test('run_log_parse: an event other than app.stream.read needs no bytes', () => {
  assert.deepEqual(parseRunLog(log(START, '{"ev":"app.fetch.end","seq":0,"src":"app"}', END)).problems, []);
});

test('run_log_parse: a log that starts with another event reports only the first-event problem', () => {
  const line = '{"ev":"app.error","seq":0,"src":"app"}';
  assert.deepEqual(parseRunLog(log(line, END)).problems, ['the first event is not run.start']);
});

for (const [name, runField] of [['missing', ''], ['empty', '"run":"",'], ['a number', '"run":5,']]) {
  test(`run_log_parse: run.start whose run id is ${name} is reported`, () => {
    const start = `{"ev":"run.start",${runField}"seq":0,"src":"script","t":0}`;
    assert.deepEqual(parseRunLog(log(start, END)).problems, ['run.start has no run id']);
  });
}

test('run_log_parse: a log that ends with another event is reported', () => {
  assert.deepEqual(parseRunLog(log(START, '{"ev":"app.error","seq":0,"src":"app"}')).problems, ['the last event is not run.end']);
});

test('run_log_parse: a log of one run.end line is reported as not starting with run.start', () => {
  assert.deepEqual(parseRunLog(log(END)).problems, ['the first event is not run.start']);
});

test('run_log_parse: a line that is not JSON is skipped, the other lines still count', () => {
  const parsed = parseRunLog(log(START, '{oops', END));
  assert.deepEqual(parsed.problems, ['line 2: not JSON']);
  assert.equal(parsed.events.length, 2);
});

// normalizeRunLog

const norm = (events) => normalizeRunLog(events);
const start = (extra = {}) => ({ ev: 'run.start', run: 'r-1', seq: 0, src: 'script', t: 0, k: 1, ...extra });

test('run_log_normalize: events are grouped by source and keep their order, t and seq are removed', () => {
  const out = norm([
    start(),
    { ev: 'host.request', seq: 0, src: 'host', t: 1, method: 'GET' },
    { ev: 'app.nav', seq: 0, src: 'app', t: 2, url: 'a' },
    { ev: 'host.response', seq: 1, src: 'host', t: 3, status: 200 },
    { ev: 'run.end', seq: 1, src: 'script', t: 4, ok: true },
  ]);
  assert.deepEqual(out, {
    app: [{ ev: 'app.nav', src: 'app', url: 'a' }],
    host: [
      { ev: 'host.request', src: 'host', method: 'GET' },
      { ev: 'host.response', src: 'host', status: 200 },
    ],
    script: [
      { ev: 'run.start', run: '<run>', src: 'script' },
      { ev: 'run.end', src: 'script', ok: true },
    ],
    stream_bytes: 0,
  });
});

test('run_log_normalize: k is removed from run.start only', () => {
  const out = norm([start(), { ev: 'app.nav', seq: 0, src: 'app', t: 1, k: 7 }]);
  assert.equal('k' in out.script[0], false);
  assert.equal(out.app[0].k, 7);
});

test('run_log_normalize: app.stream.read events are replaced by the sum of their bytes', () => {
  const out = norm([
    start(),
    { bytes: 100, ev: 'app.stream.read', seq: 1, src: 'app', t: 1 },
    { bytes: 50, ev: 'app.stream.read', seq: 2, src: 'app', t: 2 },
  ]);
  assert.equal(out.stream_bytes, 150);
  assert.deepEqual(out.app, []);
});

test('run_log_normalize: metric probes are removed, other probes are kept', () => {
  const out = norm([
    start(),
    { ev: 'app.probe', name: 'metric.first_paint', seq: 1, src: 'app', t: 1, value: 1 },
    { ev: 'app.probe', name: 'chunk', seq: 2, src: 'app', t: 2, value: 0 },
    { ev: 'app.probe', name: 'x.metric.y', seq: 3, src: 'app', t: 3, value: 0 },
    { ev: 'app.probe', seq: 4, src: 'app', t: 4, value: 0 },
    { ev: 'app.nav', name: 'metric.not_a_probe', seq: 5, src: 'app', t: 5 },
  ]);
  assert.deepEqual(
    out.app.map((e) => e.name),
    ['chunk', 'x.metric.y', undefined, 'metric.not_a_probe'],
  );
});

test('run_log_normalize: an event without fields gets no fields member', () => {
  const out = norm([start(), { ev: 'app.nav', seq: 0, src: 'app', t: 1 }]);
  assert.equal('fields' in out.app[0], false);
});

test('run_log_normalize: Date values are blanked in every shape of fields, in any letter case', () => {
  const event = (fields) => norm([start(), { ev: 'host.response', fields, seq: 0, src: 'host', t: 1 }]).host[0].fields;
  assert.deepEqual(event([['Date', 'x'], ['DATE', 'y']]), [['Date', ''], ['DATE', '']]);
  assert.deepEqual(event([{ name: 'date', value: 'x' }]), [{ name: 'date', value: '' }]);
  assert.deepEqual(event([{ Date: 'x', Host: 'h' }]), [{ Date: '', Host: 'h' }]);
  assert.deepEqual(event({ DaTe: 'x', Host: 'h' }), { DaTe: '', Host: 'h' });
});

test('run_log_normalize: fields other than Date are kept as they are', () => {
  const event = (fields) => norm([start(), { ev: 'host.response', fields, seq: 0, src: 'host', t: 1 }]).host[0].fields;
  assert.deepEqual(event([['Host', 'x']]), [['Host', 'x']]);
  assert.deepEqual(event([{ name: 'Host', value: 'x' }]), [{ name: 'Host', value: 'x' }]);
  assert.deepEqual(event(['Host', 5, null]), ['Host', 5, null]);
  assert.deepEqual(event('text'), 'text');
  assert.deepEqual(event(null), null);
});

test('run_log_normalize: the run id and its nonce form are replaced in nested strings', () => {
  const out = norm([
    start({ run: 'cap-27.0-1' }),
    { ev: 'host.request', fields: ['a-cap-27.0-1', { v: 'cap-27_0-1' }], seq: 0, src: 'host', t: 1, target: '/s?run=cap-27.0-1&n=cap-27_0-1' },
  ]);
  assert.equal(out.script[0].run, '<run>');
  assert.equal(out.host[0].target, '/s?run=<run>&n=<run>');
  assert.deepEqual(out.host[0].fields, ['a-<run>', { v: '<run>' }]);
});

test('run_log_normalize: non-string values are not changed by run id replacement', () => {
  const out = norm([start(), { ev: 'host.response', seq: 0, src: 'host', status: 200, t: 1, flag: true, none: null }]);
  assert.deepEqual(out.host[0], { ev: 'host.response', flag: true, none: null, src: 'host', status: 200 });
});

test('run_log_normalize: a first event without a string run id leaves values as they are', () => {
  const out = norm([{ ev: 'run.start', seq: 0, src: 'script', t: 0 }, { ev: 'app.nav', seq: 0, src: 'app', t: 1, url: 'abc' }]);
  assert.equal(out.app[0].url, 'abc');
});

test('run_log_normalize: an empty run id does not split the strings', () => {
  const out = norm([start({ run: '' }), { ev: 'app.nav', seq: 0, src: 'app', t: 1, url: 'abc' }]);
  assert.equal(out.app[0].url, 'abc');
});

test('run_log_normalize: no events give empty groups', () => {
  assert.deepEqual(norm([]), { app: [], host: [], script: [], stream_bytes: 0 });
});
