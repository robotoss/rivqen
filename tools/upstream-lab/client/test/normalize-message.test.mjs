// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import test from 'node:test';
import { normalizeEvent } from '../src/determinism.mjs';

const message = (json) => normalizeEvent({ ev: 'session.message', json }).json;

test('normalize_message_keeps_value_types', () => {
  // A parse and stringify would turn 1.0 into 1 and the string "5" would stay apart only by luck; the text must stay.
  assert.equal(message('{"a":1.0,"local_refresh_time":9,"b":"5"}'), '{"a":1.0,"b":"5"}');
  assert.notEqual(message('{"a":1.0}'), message('{"a":1}'));
  assert.notEqual(message('{"a":"1","local_refresh_time":9}'), message('{"a":1,"local_refresh_time":9}'));
  assert.notEqual(message('{"a":1e2,"local_refresh_time":9}'), message('{"a":100,"local_refresh_time":9}'));
  assert.notEqual(message('{"a":12345678901234567890,"local_refresh_time":9}'), message('{"a":12345678901234567891,"local_refresh_time":9}'));
});

test('normalize_message_removes_only_the_top_level_member', () => {
  assert.equal(message('{"local_refresh_time":1}'), '{}');
  assert.equal(message('{"x":1,"local_refresh_time":1}'), '{"x":1}');
  assert.equal(message('{"local_refresh_time":{"k":[1,2,{"a":","}]},"x":1}'), '{"x":1}');
  assert.equal(message('{"n":{"local_refresh_time":5},"x":"a,\\"b","local_refresh_time":"t"}'), '{"n":{"local_refresh_time":5},"x":"a,\\"b"}');
  assert.equal(message('{"x":1}'), '{"x":1}');
  assert.equal(message('{"a":"local_refresh_time","local_refresh_time":2}'), '{"a":"local_refresh_time"}');
});
