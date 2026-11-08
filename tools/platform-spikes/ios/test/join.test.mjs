// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from "node:assert/strict";
import { test } from "node:test";
import { joinRun, parseLines, stableStringify } from "../join.mjs";

const meta = {
  run: "CAP-I-01-ios-27.0-1", capability: "CAP-I-01", os: "27.0", k: 1, device: "iPhone 16 Pro",
  prototype: "IOS-P1", ok: true, error: null, t_end: 900,
};

test("join_order_and_sorted_keys", () => {
  const app = '{"ev":"app.fetch.start","seq":0,"src":"app","t":1,"url":"u"}\n';
  const host = '{"ev":"host.request","seq":0,"src":"host","t":2,"target":"/s/echo"}\n';
  const lines = joinRun(meta, app, host).split("\n");
  assert.equal(lines.at(-1), "");
  const evs = lines.slice(0, -1).map((l) => JSON.parse(l));
  assert.deepEqual(evs.map((e) => e.ev), ["run.start", "app.fetch.start", "host.request", "run.end"]);
  for (const l of lines.slice(0, -1)) {
    const keys = Object.keys(JSON.parse(l));
    assert.deepEqual(keys, [...keys].sort());
  }
  assert.equal(evs[0].webview, null);
  assert.equal(evs[0].platform, "ios");
  assert.equal(evs[0].k, 1);
  assert.deepEqual([evs[3].ok, evs[3].error, evs[3].t, evs[3].seq], [true, null, 900, 1]);
});

test("join_refuses_wrong_source", () => {
  assert.throws(() => parseLines('{"ev":"host.request","src":"app"}\n', "host"), /src is "app"/);
  assert.throws(() => parseLines("[1]\n", "app"), /not an object/);
  assert.deepEqual(parseLines("", "app"), []);
});

test("stable_stringify_sorts_nested_keys", () => {
  assert.equal(stableStringify({ b: [{ d: 1, c: 2 }], a: null }), '{"a":null,"b":[{"c":2,"d":1}]}');
});
