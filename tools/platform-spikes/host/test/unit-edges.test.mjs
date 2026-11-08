// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Edge behavior of the pure parts of the spike host: limits, parameter parsing, run log rules, beacon gate,
// response head rules, field filtering, and the command line.

import assert from "node:assert/strict";
import { getEventListeners } from "node:events";
import { test } from "node:test";
import { RequestContext, fieldsForLog } from "../context.mjs";
import { parseConfig } from "../main.mjs";
import { endToEndFields } from "../page.mjs";
import { BeaconGate, RunLogs } from "../runlog.mjs";
import { buildChunk, nonceOf } from "../stream.mjs";
import { HostError, enumParam, intParam, loggableValue, runOf, siteOf, sleep, stableStringify } from "../util.mjs";

const q = (s) => new URLSearchParams(s);
const meta = (run, extra = {}) => ({ run, capability: "CAP-X", platform: "ios", os: "27.0", k: 1, ...extra });
const code = (c, status) => (e) => e instanceof HostError && e.code === c && (status === undefined || e.status === status);

// util

test("host_error_carries_status_code_and_a_name", () => {
  const e = new HostError(418, "TEAPOT", "short", { cause: "c" });
  assert.deepEqual([e.name, e.status, e.code, e.message, e.cause], ["HostError", 418, "TEAPOT", "short", "c"]);
});

test("site_of_refuses_text_before_the_host_name", () => {
  assert.equal(siteOf("evil localhost"), null);
  assert.equal(siteOf("evil 127.0.0.1:5"), null);
});

test("run_of_gives_null_when_neither_the_field_nor_the_parameter_is_set", () => {
  assert.equal(runOf(undefined, undefined), null);
  assert.equal(runOf(undefined, null), null);
});

test("int_param_takes_the_default_and_requires_a_value_when_there_is_none", () => {
  assert.equal(intParam(q(""), "n", { def: 7, min: 0, max: 9 }), 7);
  assert.throws(() => intParam(q(""), "n", { min: 0, max: 9 }), code("BAD_PARAM", 400));
});

test("int_param_accepts_the_bounds_and_refuses_values_beside_them", () => {
  const spec = { min: 2, max: 9 };
  assert.equal(intParam(q("n=2"), "n", spec), 2);
  assert.equal(intParam(q("n=9"), "n", spec), 9);
  assert.throws(() => intParam(q("n=1"), "n", spec), code("BAD_PARAM", 400));
  assert.throws(() => intParam(q("n=10"), "n", spec), code("BAD_PARAM", 400));
});

test("int_param_refuses_anything_but_one_to_nine_digits", () => {
  const spec = { min: 0, max: 999_999_999 };
  assert.equal(intParam(q("n=999999999"), "n", spec), 999_999_999);
  for (const v of ["x5", "5x", "-1", "1.5", "", " 5", "1000000000", "+5"]) {
    assert.throws(() => intParam(q(`n=${encodeURIComponent(v)}`), "n", spec), code("BAD_PARAM", 400), JSON.stringify(v));
  }
});

test("enum_param_takes_the_default_and_refuses_values_outside_the_list", () => {
  assert.equal(enumParam(q(""), "e", ["a", "b"], "b"), "b");
  assert.equal(enumParam(q("e=a"), "e", ["a", "b"], "b"), "a");
  assert.throws(() => enumParam(q("e=c"), "e", ["a", "b"], "b"), code("BAD_PARAM", 400));
  assert.throws(() => enumParam(q("e="), "e", ["a", "b"], "b"), code("BAD_PARAM", 400));
});

test("stable_stringify_writes_an_undefined_array_item_as_null", () => {
  assert.equal(stableStringify([undefined, 1]), "[null,1]");
});

test("cookie_log_rule_keeps_an_empty_name_and_ignores_spaces_around_a_value", () => {
  assert.equal(loggableValue("cookie", "=b"), "=b");
  assert.equal(loggableValue("cookie", "a= b"), "a= b");
  assert.equal(loggableValue("cookie", "a=b ;c=d"), "a=b ;c=d");
});

test("sleep_leaves_no_abort_listener_behind", async () => {
  const done = new AbortController();
  assert.equal(await sleep(0, done.signal), true);
  assert.equal(getEventListeners(done.signal, "abort").length, 0);
  const cut = new AbortController();
  const p = sleep(60_000, cut.signal);
  assert.equal(getEventListeners(cut.signal, "abort").length, 1);
  cut.abort();
  assert.equal(await p, false);
  assert.equal(getEventListeners(cut.signal, "abort").length, 0);
});

// stream chunk

test("chunk_nonce_is_norun_without_a_run_and_has_no_dots_with_one", () => {
  assert.equal(nonceOf(null), "norun");
  assert.equal(nonceOf("a.b.c-1"), "a_b_c-1");
});

test("chunk_after_the_first_starts_with_its_paragraph", () => {
  assert.ok(buildChunk({ k: 5, size: 100, run: null, nonce: null }).startsWith('<p data-k="5">'));
  assert.ok(buildChunk({ k: 0, size: 100, run: "r", nonce: "n" }).startsWith('<!doctype html><html><head><meta charset="utf-8"><title>stream</title></head><body><p data-k="0">'));
});

test("chunk_script_carries_the_nonce_only_when_one_is_given", () => {
  assert.ok(buildChunk({ k: 1, size: 400, run: "r", nonce: "n1" }).includes('<script nonce="n1">'));
  assert.ok(buildChunk({ k: 1, size: 400, run: "r", nonce: null }).includes("<script>"));
});

// run log

test("run_log_start_refuses_a_body_that_is_not_an_object", () => {
  const logs = new RunLogs();
  for (const body of [null, [], "x", 5]) assert.throws(() => logs.start(body), code("BAD_RUN_START", 400));
});

test("run_log_start_refuses_a_bad_run_id_or_safe_token", () => {
  const logs = new RunLogs();
  assert.throws(() => logs.start(meta("bad id")), code("BAD_RUN_START", 400));
  assert.throws(() => logs.start(meta("a".repeat(129))), code("BAD_RUN_START", 400));
  assert.equal(logs.start(meta("a".repeat(128))).length, 128);
  for (const key of ["capability", "platform", "os"]) {
    for (const bad of [undefined, "", "a/b", "a".repeat(65), 5]) {
      assert.throws(() => logs.start(meta("r-1", { [key]: bad })), code("BAD_RUN_START", 400), `${key} ${bad}`);
    }
    assert.equal(logs.start(meta(`r-${key}`, { [key]: "a".repeat(64) })), `r-${key}`);
  }
});

test("run_log_start_takes_k_from_zero_to_below_one_million_or_a_safe_token", () => {
  const logs = new RunLogs();
  for (const [i, k] of [0, 1, 999_999, "k1", "A.b-9"].entries()) assert.equal(logs.start(meta(`k-ok-${i}`, { k })), `k-ok-${i}`);
  for (const k of [-1, 1_000_000, 1.5, null, undefined, "", "a b", "a".repeat(65), true]) {
    assert.throws(() => logs.start(meta("k-bad", { k })), code("BAD_RUN_START", 400), String(k));
  }
});

test("run_log_start_refuses_a_run_that_is_open", () => {
  const logs = new RunLogs();
  logs.start(meta("dup-1"));
  assert.throws(() => logs.start(meta("dup-1")), code("RUN_EXISTS", 409));
});

test("run_log_keeps_old_runs_while_there_is_room", () => {
  let now = 0;
  const logs = new RunLogs({ now: () => now, maxRuns: 3, maxAgeMs: 10 });
  logs.start(meta("old-1"));
  now = 1_000;
  logs.start(meta("new-1"));
  assert.equal(logs.has("old-1"), true);
});

test("run_log_drops_only_the_runs_that_are_older_than_the_age_limit_when_full", () => {
  let now = 1000;
  const logs = new RunLogs({ now: () => now, maxRuns: 2, maxAgeMs: 100 });
  logs.start(meta("a-1"));
  now = 1050;
  logs.start(meta("a-2"));
  now = 1100; // a-1 is exactly 100 old: not stale
  assert.throws(() => logs.start(meta("a-3")), code("TOO_MANY_RUNS", 429));
  now = 1101; // a-1 is stale, a-2 is 51 old
  assert.equal(logs.start(meta("a-3")), "a-3");
  assert.deepEqual([logs.has("a-1"), logs.has("a-2"), logs.has("a-3")], [false, true, true]);
});

test("run_log_defaults_are_16_open_runs_and_15_minutes", () => {
  let now = 1000;
  const logs = new RunLogs({ now: () => now });
  for (let i = 0; i < 16; i += 1) logs.start(meta(`d-${i}`));
  now = 1000 + 15 * 60 * 1000;
  assert.throws(() => logs.start(meta("d-16")), code("TOO_MANY_RUNS", 429));
  now += 1;
  assert.equal(logs.start(meta("d-16")), "d-16");
});

test("run_log_times_an_event_from_the_start_of_its_run", () => {
  let now = 500;
  const logs = new RunLogs({ now: () => now });
  logs.start(meta("t-1"));
  now = 542.4;
  logs.emit("t-1", "host.beacon", { k: 0 });
  assert.equal(JSON.parse(logs.text("t-1")).t, 42);
});

test("run_log_text_and_has_tell_unknown_runs_apart", () => {
  const logs = new RunLogs();
  assert.equal(logs.has(null), false);
  assert.equal(logs.has("none"), false);
  assert.equal(logs.text("none"), null);
  logs.start(meta("known-1"));
  assert.equal(logs.has("known-1"), true);
  assert.equal(logs.text("known-1"), "");
});

// beacon gate

test("beacon_gate_does_not_mix_a_run_less_waiter_with_runs_named_like_its_key", async () => {
  const gate = new BeaconGate();
  const waiter = gate.expect(null, 0);
  let released = false;
  waiter.arrived.then(() => { released = true; });
  for (const run of ["null", "", "undefined"]) gate.arrive(run, 0);
  await new Promise((r) => setImmediate(r));
  assert.equal(released, false);
  gate.arrive(null, 0);
  await waiter.arrived;
});

test("beacon_gate_keeps_other_waiters_of_a_chunk_when_one_cancels", async () => {
  const gate = new BeaconGate();
  const a = gate.expect("r", 0);
  const b = gate.expect("r", 0);
  a.cancel();
  gate.arrive("r", 0);
  await b.arrived;
});

test("beacon_gate_forgets_a_chunk_when_its_last_waiter_cancels_or_the_beacon_arrives", () => {
  const gate = new BeaconGate();
  gate.expect("r", 0).cancel();
  assert.equal(gate.waiters.size, 0);
  gate.expect("r", 1);
  gate.arrive("r", 1);
  assert.equal(gate.waiters.size, 0);
});

test("beacon_gate_late_cancel_of_a_released_waiter_does_not_drop_a_newer_waiter", async () => {
  const gate = new BeaconGate();
  const first = gate.expect("r", 0);
  gate.arrive("r", 0);
  const second = gate.expect("r", 0);
  first.cancel();
  gate.arrive("r", 0);
  await second.arrived;
});

// response head

function fakeContext() {
  const calls = [];
  const res = { writeHead: (status, flat) => calls.push({ status, flat }), end: (body) => calls.push({ body }) };
  const logs = new RunLogs();
  const rc = new RequestContext({ req: {}, res, q: q(""), path: "/", run: null, site: { site: "A", host: "localhost", port: null },
    ctx: { logs, beacons: new BeaconGate(), upstream: { host: "127.0.0.1", port: 1 } }, signal: new AbortController().signal });
  return { rc, calls };
}

test("head_adds_cache_control_no_store_once", () => {
  const { rc, calls } = fakeContext();
  rc.head(200, [["Content-Type", "text/plain"]]);
  assert.deepEqual(calls[0].flat, ["Content-Type", "text/plain", "Cache-Control", "no-store"]);
});

test("head_keeps_a_cache_control_field_of_any_letter_case", () => {
  for (const name of ["cache-control", "CACHE-CONTROL", "Cache-Control"]) {
    const { rc, calls } = fakeContext();
    rc.head(200, [[name, "max-age=5"]]);
    assert.deepEqual(calls[0].flat, [name, "max-age=5"], name);
  }
});

test("head_adds_no_cache_control_when_asked_not_to", () => {
  const { rc, calls } = fakeContext();
  rc.head(200, [["X-A", "1"]], { noStore: false });
  assert.deepEqual(calls[0].flat, ["X-A", "1"]);
});

test("send_counts_the_bytes_of_a_text_body_and_of_a_binary_body", () => {
  const text = fakeContext();
  text.rc.send(200, [], "é");
  assert.deepEqual(text.calls[0].flat.slice(0, 2), ["Content-Length", "2"]);
  const bin = fakeContext();
  bin.rc.send(200, [], new Uint8Array(7));
  assert.deepEqual(bin.calls[0].flat.slice(0, 2), ["Content-Length", "7"]);
  const empty = fakeContext();
  empty.rc.send(416, [], "");
  assert.deepEqual(empty.calls[0].flat.slice(0, 2), ["Content-Length", "0"]);
  assert.equal(empty.calls[1].body, "");
});

test("fields_for_log_lowers_names_and_gathers_repeated_fields_in_order", () => {
  assert.deepEqual(fieldsForLog([["A", "1"], ["a", "2"], ["A", "3"], ["B", "4"]]), { a: ["1", "2", "3"], b: "4" });
  assert.deepEqual(fieldsForLog([["X", "1"], ["x", "2"]]), { x: ["1", "2"] });
});

// page field filter

test("end_to_end_fields_drop_every_hop_by_hop_field_in_any_letter_case", () => {
  const hop = ["Connection", "Keep-Alive", "Proxy-Authenticate", "Proxy-Authorization", "Proxy-Connection", "TE", "Trailer", "Transfer-Encoding", "Upgrade"];
  for (const name of hop) assert.deepEqual(endToEndFields([name, "x", "X-Keep", "y"]), [["X-Keep", "y"]], name);
  for (const name of hop) assert.deepEqual(endToEndFields([name.toLowerCase(), "x"]), [], name);
});

test("end_to_end_fields_drop_the_fields_that_connection_names_in_any_letter_case", () => {
  assert.deepEqual(endToEndFields(["Connection", " X-Drop ,Other", "x-drop", "1", "OTHER", "2", "X-Keep", "3"]), [["X-Keep", "3"]]);
  assert.deepEqual(endToEndFields(["CONNECTION", "x-drop", "X-DROP", "1"]), []);
});

test("end_to_end_fields_keep_a_field_that_only_another_field_value_names", () => {
  assert.deepEqual(endToEndFields(["X-Foo", "bar", "bar", "keep"]), [["X-Foo", "bar"], ["bar", "keep"]]);
});

// command line

test("config_takes_every_port_from_its_option_before_the_environment", () => {
  const env = { LAB_TLS: "/x", LAB_HOST_PORT: "9", LAB_HOST_BAD_PORT: "9", LAB_HOST_CONTROL_PORT: "9", LAB_SITE_PORT: "9" };
  const c = parseConfig(["--port", "1", "--bad-port", "2", "--control-port", "3", "--site-port", "4"], env);
  assert.deepEqual(c.ports, { good: 1, bad: 2, control: 3 });
  assert.equal(c.sitePort, 4);
});

test("config_takes_every_port_from_the_environment_when_there_is_no_option", () => {
  const c = parseConfig([], { LAB_TLS: "/x", LAB_HOST_PORT: "5", LAB_HOST_BAD_PORT: "6", LAB_HOST_CONTROL_PORT: "7", LAB_SITE_PORT: "8" });
  assert.deepEqual(c, { tlsDir: "/x", ports: { good: 5, bad: 6, control: 7 }, sitePort: 8 });
});

test("config_accepts_port_65535_and_zero_and_refuses_values_beside_the_range", () => {
  assert.equal(parseConfig(["--port", "65535"], { LAB_TLS: "/x" }).ports.good, 65535);
  assert.equal(parseConfig(["--port", "0"], { LAB_TLS: "/x" }).ports.good, 0);
  for (const v of ["65536", "100000", "123456", "x80", "80x", "", "8 0", "1e3"]) {
    assert.throws(() => parseConfig(["--port", v], { LAB_TLS: "/x" }), /port must be a port number/, JSON.stringify(v));
  }
});

test("config_names_the_option_whose_port_is_wrong", () => {
  const env = { LAB_TLS: "/x" };
  assert.throws(() => parseConfig(["--bad-port", "x"], env), /bad-port must be/);
  assert.throws(() => parseConfig(["--control-port", "x"], env), /control-port must be/);
  assert.throws(() => parseConfig(["--site-port", "x"], env), /site-port must be/);
  assert.throws(() => parseConfig([], { ...env, LAB_SITE_PORT: "70000" }), /site-port must be/);
});

test("config_needs_a_tls_directory_that_is_not_empty", () => {
  assert.throws(() => parseConfig([], {}), /LAB_TLS/);
  assert.throws(() => parseConfig([], { LAB_TLS: "" }), /LAB_TLS/);
  assert.throws(() => parseConfig(["--tls-dir", ""], {}), /LAB_TLS/);
  assert.equal(parseConfig(["--tls-dir", "/y"], {}).tlsDir, "/y");
});
