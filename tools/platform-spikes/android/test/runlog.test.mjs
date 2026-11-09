// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Offline tests of the run sets and the run acceptance of runlog.mjs (what an ok run needs before
// it may go into evidence/). Run: node --test "tools/platform-spikes/android/test/*.test.mjs"
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";
import { readFileSync } from "node:fs";
import { SETS, byteChecks, pageUrl, sameAsStored, setOf, sortedJson } from "../runlog.mjs";

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

function cutRun({ after = 3000, read = 3000, outcome = "error", second = { decision: "pass", reason: "claimed" }, fallback = true, kind = "cut", beacon = [] } = {}) {
  const sha = createHash("sha256").update("x").digest("hex");
  const events = [
    app("app.intercept", { decision: "stream" }),
    app("app.stream.read", { bytes: read }),
    ...(fallback ? [app("app.probe", { name: "native.step", value: "fallback-load" })] : []),
    ...(second ? [app("app.intercept", second)] : []),
    app("app.fetch.end", { bytes: 3000, complete: false }),
    ...beacon,
    host("host.cut", { after }),
  ];
  const metrics = { fetch: { sha256: sha }, stream: { outcome, bytes: read, sha256: sha }, dropped_events: 0 };
  return byteChecks(events, metrics, null, kind).problems;
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

test("partial_set_holds_the_cut_and_delivers_more_than_one_read_block_on_api_29", () => {
  const q37 = new URL(pageUrl("CAP-A-08", "r", undefined, "partial", "37")).searchParams;
  const q29 = new URL(pageUrl("CAP-A-08", "r", undefined, "partial", "29")).searchParams;
  assert.equal(setOf("CAP-A-08", "partial").scenario, "cut-partial");
  assert.equal(setOf("CAP-A-08", "partial").kind, "partial");
  for (const q of [q37, q29]) {
    assert.ok(Number(q.get("hold")) >= 10000 && Number(q.get("hold")) <= 60000, "hold covers a cold WebView start, within the host limit");
    assert.ok(Number(q.get("after")) < Number(q.get("size")));
  }
  assert.ok(Number(q29.get("after")) > 65536, "API 29: more than one 64 KiB read block before the cut");
  // The catalog set keeps the S2 parameters (no hold), so its stored runs stay reproducible.
  assert.equal(new URL(pageUrl("CAP-A-08", "r")).searchParams.get("hold"), null);
});

test("partial_run_needs_the_beacon_of_chunk_0_before_the_cut", () => {
  const beacon0 = host("host.beacon", { k: 0 });
  assert.deepEqual(cutRun({ kind: "partial", beacon: [beacon0] }), []);
  assert.match(cutRun({ kind: "partial" }).join(), /no beacon of chunk 0 before the cut/);
  assert.match(cutRun({ kind: "partial", beacon: [host("host.beacon", { k: 1 })] }).join(), /no beacon of chunk 0/);
  // A beacon after the cut does not count.
  const late = [app("app.intercept", { decision: "stream" }), host("host.cut", { after: 3000 }), beacon0];
  const metrics = { fetch: { sha256: "a" }, stream: { outcome: "error", bytes: 0, sha256: "a" }, dropped_events: 0 };
  assert.match(byteChecks(late, metrics, null, "partial").problems.join(), /no beacon of chunk 0/);
  // The cut checks still apply.
  assert.match(cutRun({ kind: "partial", beacon: [beacon0], outcome: "end" }).join(), /outcome end/);
});

test("a fourth run with another run id has the log of the stored runs", () => {
  // The recording mode (run.sh SPIKE_RECORD=1) records a run k=4 outside evidence/ and keeps the clip
  // only when that run has the normalized log of the stored runs.
  const dir = new URL("../../../../evidence/spikes/wp-04/android/CAP-A-08/37/partial/", import.meta.url);
  const stored = [1, 2, 3].map((k) => readFileSync(new URL(`run-${k}.jsonl`, dir), "utf8"));
  const fourth = stored[0].replaceAll("CAP-A-08-android-37-1", "CAP-A-08-android-37-4").replace('"k":1,', '"k":4,');
  assert.notEqual(fourth, stored[0]);
  assert.deepEqual(sameAsStored(stored, fourth), { same: true, reason: null, note: null });
  const other = fourth.replace("net::ERR_FAILED", "net::ERR_ABORTED");
  assert.deepEqual(sameAsStored(stored, other), { same: false, reason: "the run differs from the stored runs", note: null });
  assert.match(sameAsStored(stored.slice(0, 2), fourth).reason, /at least 3/);
  assert.match(sameAsStored([stored[0], stored[1], other], fourth).reason, /not stable/);
});

test("runs_of_the_s1_app_are_compared_without_the_s2_intercept_members", () => {
  // CAP-A-02/37 was stored with the S1 app: app.intercept has no reason and redirect members.
  const dir = new URL("../../../../evidence/spikes/wp-04/android/CAP-A-02/37/", import.meta.url);
  const stored = [1, 2, 3].map((k) => readFileSync(new URL(`run-${k}.jsonl`, dir), "utf8"));
  assert.ok(!stored[0].includes('"reason"'));
  const now = stored[0].split("\n").map((line) => {
    if (line === "") return line;
    const e = JSON.parse(line);
    if (e.ev === "app.intercept") Object.assign(e, { reason: null, redirect: false });
    return sortedJson(e);
  }).join("\n");
  assert.notEqual(now, stored[0]);
  const r = sameAsStored(stored, now);
  assert.equal(r.same, true);
  assert.match(r.note, /S1 app/);
  // Any other difference still counts.
  assert.equal(sameAsStored(stored, now.replace('"decision":"stream"', '"decision":"pass"')).same, false);
});
