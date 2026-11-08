// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Offline tests of the run sets and the run acceptance of runlog.mjs (what an ok run needs before
// it may go into evidence/). Run: node --test tools/platform-spikes/android/test
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";
import { SETS, byteChecks, pageUrl, setOf } from "../runlog.mjs";

const EMPTY = createHash("sha256").digest("hex");
const RUN = "CAP-A-04-android-29-1";

const app = (ev, extra = {}) => ({ ev, src: "app", seq: 0, t: 0, ...extra });
const host = (ev, extra = {}) => ({ ev, src: "host", seq: 0, t: 0, ...extra });

test("set_urls_are_canonical_and_carry_the_run_id", () => {
  for (const [cap, sets] of Object.entries(SETS)) {
    for (const name of Object.keys(sets)) {
      for (const api of ["37", "29"]) {
        const run = `${cap}-android-${api}-1`;
        const url = pageUrl(cap, run, undefined, name, api);
        assert.equal(new URL(url).searchParams.get("run"), run, `${cap}/${name}`);
        assert.match(url, /^[A-Za-z0-9.:/?&=%-]+$/, `${cap}/${name}: run.sh quotes the URL with single quotes`);
        assert.ok(url.length <= 2048, `${cap}/${name}: RunParams limit`);
      }
    }
  }
});

test("api_29_stream_sets_use_the_64_kib_read_block", () => {
  for (const cap of ["CAP-A-01", "CAP-A-02", "CAP-A-03"]) {
    assert.equal(new URL(pageUrl(cap, "r", undefined, "", "29")).searchParams.get("size"), "65536", cap);
    assert.equal(new URL(pageUrl(cap, "r", undefined, "", "37")).searchParams.get("size"), "2048", cap);
  }
  assert.equal(new URL(pageUrl("CAP-A-01", "r", undefined, "small-2048", "29")).searchParams.get("size"), "2048");
});

test("redirect_sets_end_on_the_echo_page_with_the_run_id", () => {
  const url = new URL(pageUrl("CAP-A-04", RUN, undefined, "cross-site"));
  assert.equal(url.pathname, "/s/redirect");
  assert.equal(url.searchParams.get("alt"), "10.0.2.2");
  assert.equal(url.searchParams.get("to"), `/s/echo?html=1&run=${RUN}`);
});

test("tls_set_uses_site_b_on_the_bad_port", () => {
  assert.equal(new URL(pageUrl("CAP-A-05", "CAP-A-05-android-37-1")).origin, "https://10.0.2.2:18444");
});

test("unknown_set_and_override_values_are_refused", () => {
  assert.throws(() => setOf("CAP-A-04", "nope"), /unknown set/);
  assert.throws(() => setOf("CAP-A-99"), /unknown capability/);
  assert.throws(() => pageUrl("CAP-A-01", "r", "chunks=1/2"), /bad parameter/);
  assert.equal(setOf("CAP-A-04", "wrr-302").scenario, "redirect-3xx");
  assert.equal(setOf("CAP-A-08").scenario, "cut-range");
  assert.equal(setOf("CAP-A-01").scenario, "stream");
});

const passMetrics = { stream: { outcome: "cancel", bytes: 0, sha256: EMPTY }, dropped_events: 0 };

test("pass_run_is_ok_when_nothing_of_the_fetch_reached_the_webview", () => {
  const events = [app("app.intercept", { decision: "pass", reason: "redirect-status" })];
  assert.deepEqual(byteChecks(events, passMetrics, null, "pass").problems, []);
});

test("pass_run_fails_when_a_main_frame_was_streamed_or_bytes_were_read", () => {
  const streamed = [app("app.intercept", { decision: "stream" })];
  assert.match(byteChecks(streamed, passMetrics, null, "pass").problems.join(), /decisions stream/);
  const read = [app("app.intercept", { decision: "pass" }), app("app.stream.read", { bytes: 9 })];
  assert.match(byteChecks(read, passMetrics, null, "pass").problems.join(), /read 9 bytes/);
  const ended = { ...passMetrics, stream: { ...passMetrics.stream, outcome: "end" } };
  assert.match(byteChecks([app("app.intercept", { decision: "pass" })], ended, null, "pass").problems.join(), /outcome end/);
  assert.match(byteChecks([], passMetrics, null, "pass").problems.join(), /no main-frame decision/);
});

test("probe_run_allows_a_stream_decision_but_no_fetched_bytes", () => {
  const events = [app("app.intercept", { decision: "stream", status: 302 })];
  assert.deepEqual(byteChecks(events, passMetrics, null, "probe").problems, []);
});

function cutRun({ after = 3000, read = 3000, outcome = "error", second = { decision: "pass", reason: "claimed" }, fallback = true } = {}) {
  const sha = createHash("sha256").update("x").digest("hex");
  const events = [
    app("app.intercept", { decision: "stream" }),
    app("app.stream.read", { bytes: read }),
    ...(fallback ? [app("app.probe", { name: "native.step", value: "fallback-load" })] : []),
    ...(second ? [app("app.intercept", second)] : []),
    app("app.fetch.end", { bytes: 3000, complete: false }),
    host("host.cut", { after }),
  ];
  const metrics = { fetch: { sha256: sha }, stream: { outcome, bytes: read, sha256: sha }, dropped_events: 0 };
  return byteChecks(events, metrics, null, "cut").problems;
}

test("cut_run_is_ok_when_the_webview_got_the_bytes_before_the_cut_and_the_fallback_passed", () => {
  assert.deepEqual(cutRun(), []);
});

test("fetched_run_needs_the_whole_body_delivered_unchanged", () => {
  const sha = createHash("sha256").update("page").digest("hex");
  const events = (read) => [
    app("app.intercept", { decision: "stream" }),
    app("app.stream.read", { bytes: read }),
    app("app.fetch.end", { bytes: 230, complete: true }),
  ];
  const metrics = (outcome, s = sha) => ({ fetch: { sha256: sha }, stream: { outcome, bytes: 230, sha256: s }, dropped_events: 0 });
  assert.deepEqual(byteChecks(events(230), metrics("end"), null, "fetched").problems, []);
  assert.match(byteChecks(events(100), metrics("end"), null, "fetched").problems.join(), /read 100 bytes/);
  assert.match(byteChecks(events(230), metrics("cancel"), null, "fetched").problems.join(), /outcome cancel/);
  assert.match(byteChecks(events(230), metrics("end", EMPTY), null, "fetched").problems.join(), /read 230 bytes/);
  assert.match(byteChecks([app("app.intercept", { decision: "pass" })], metrics("end"), null, "fetched").problems.join(), /not streamed/);
});

test("step_run_is_ok_only_when_the_last_step_is_done_without_errors", () => {
  const step = (value) => app("app.probe", { name: "native.step", value });
  const ok = { dropped_events: 0 };
  assert.deepEqual(byteChecks([step("clear"), step("done")], ok, null, "steps").problems, []);
  assert.match(byteChecks([step("clear")], ok, null, "steps").problems.join(), /steps did not end/);
  assert.match(byteChecks([step("done"), app("app.error", { error: "x" })], ok, null, "steps").problems.join(), /app errors/);
  assert.match(byteChecks([step("done")], { dropped_events: 2 }, null, "steps").problems.join(), /dropped/);
});

test("cookie_sets_carry_their_scenarios_and_seed", () => {
  assert.equal(setOf("CAP-A-06").scenario, "cookie-on");
  assert.equal(setOf("CAP-A-06", "manual").scenario, "cookie-manual");
  assert.equal(setOf("CAP-A-07", "persist").seed, "cookie-seed");
  assert.equal(setOf("CAP-A-07").seed, undefined);
  assert.equal(new URL(pageUrl("CAP-A-06", "r")).searchParams.get("attrs"), "Secure,SameSite=Lax");
});

test("cut_run_fails_on_each_broken_condition", () => {
  assert.match(cutRun({ after: 2000 }).join(), /did not end at the cut/);
  assert.match(cutRun({ read: 2048 }).join(), /read 2048 bytes/);
  assert.match(cutRun({ outcome: "end" }).join(), /outcome end/);
  assert.match(cutRun({ second: null }).join(), /no fallback request/);
  assert.match(cutRun({ second: { decision: "pass", reason: "url" } }).join(), /no fallback request/);
  assert.match(cutRun({ fallback: false }).join(), /no fallback load/);
});
