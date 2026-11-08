// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from "node:assert/strict";
import { test } from "node:test";
import { BeaconGate, RunLogs } from "../runlog.mjs";
import { buildChunk } from "../stream.mjs";
import { loggableValue, runOf, siteOf, sleep, stableStringify } from "../util.mjs";

test("stable_stringify_sorts_keys_at_every_level", () => {
  assert.equal(stableStringify({ b: 1, a: { d: [{ y: 1, x: 2 }], c: null }, u: undefined }), '{"a":{"c":null,"d":[{"x":2,"y":1}]},"b":1}');
});

test("site_of_maps_hosts_to_sites", () => {
  assert.deepEqual(siteOf("localhost:18443"), { site: "A", host: "localhost", port: "18443" });
  assert.deepEqual(siteOf("LOCALHOST"), { site: "A", host: "localhost", port: null });
  assert.equal(siteOf("127.0.0.1:1")?.site, "B");
  assert.equal(siteOf("10.0.2.2:1")?.site, "B");
  for (const h of [undefined, "", "10.0.2.3", "localhost:123456", "localhost:", "a b", "[::1]"]) assert.equal(siteOf(h), null, String(h));
});

test("run_id_comes_from_the_field_first_and_is_validated", () => {
  assert.equal(runOf("a", "b"), "a");
  assert.equal(runOf(undefined, "b"), "b");
  assert.equal(runOf(undefined, null), null);
  assert.throws(() => runOf("bad id", null), (e) => e.status === 400 && e.code === "BAD_RUN");
  assert.throws(() => runOf(undefined, ""), (e) => e.status === 400);
  assert.throws(() => runOf("a".repeat(129), null), (e) => e.status === 400);
  assert.equal(runOf("a".repeat(128), null)?.length, 128);
});

test("cookie_values_are_redacted_unless_synthetic", () => {
  assert.equal(loggableValue("set-cookie", "a=b-1; Path=/; Max-Age=5"), "a=b-1; Path=/; Max-Age=5");
  assert.equal(loggableValue("set-cookie", "a=B; Path=/"), "[redacted]");
  assert.equal(loggableValue("set-cookie", "novalue"), "[redacted]");
  assert.equal(loggableValue("cookie", "a=b; c=d"), "a=b; c=d");
  assert.equal(loggableValue("cookie", "a=b; c=D"), "[redacted]");
  assert.equal(loggableValue("x-other", "A=B"), "A=B");
});

test("run_log_drops_events_of_unknown_runs_and_caps_events", () => {
  let now = 0;
  const logs = new RunLogs({ now: () => now, maxEvents: 2 });
  logs.emit("nope", "host.beacon", { k: 1 });
  logs.emit(null, "host.beacon", { k: 1 });
  assert.equal(logs.text("nope"), null);
  logs.start({ run: "r", capability: "C", platform: "p", os: "1", k: 1 });
  now = 10.4;
  logs.emit("r", "host.beacon", { k: 1, ev: "forged", seq: 9 });
  logs.emit("r", "host.beacon", { k: 2 });
  logs.emit("r", "host.beacon", { k: 3 });
  assert.equal(logs.end("r"), '{"ev":"host.beacon","k":1,"seq":0,"src":"host","t":10}\n{"ev":"host.beacon","k":2,"seq":1,"src":"host","t":10}\n');
  assert.equal(logs.end("r"), null);
});

test("beacon_gate_releases_only_the_matching_waiter", async () => {
  const gate = new BeaconGate();
  const a = gate.expect("r", 0);
  const b = gate.expect("r", 1);
  const c = gate.expect(null, 0);
  gate.arrive("r", 0);
  await a.arrived;
  let other = false;
  b.arrived.then(() => { other = true; });
  c.arrived.then(() => { other = true; });
  await new Promise((r) => setImmediate(r));
  assert.equal(other, false);
  b.cancel();
  c.cancel();
  gate.arrive("r", 1); // nobody waits: no error
});

test("sleep_ends_early_on_abort", async () => {
  const ac = new AbortController();
  const p = sleep(60_000, ac.signal);
  ac.abort();
  assert.equal(await p, false);
  assert.equal(await sleep(0, new AbortController().signal), true);
  assert.equal(await sleep(0, AbortSignal.abort()), false);
});

test("chunk_is_the_requested_size_and_ascii", () => {
  const c = buildChunk({ k: 5, size: 5000, run: null, nonce: null });
  assert.equal(c.length, 5000);
  assert.ok(/^[\x20-\x7e]*$/.test(c));
  assert.ok(c.includes('src="/s/beacon?k=5"'));
  assert.ok(!c.includes("<!doctype"), "only chunk 0 has the document head");
  assert.ok(buildChunk({ k: 0, size: 1, run: null, nonce: null }).startsWith("<!doctype html>"));
});
