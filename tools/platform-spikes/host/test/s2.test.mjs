// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// S2 routes (CONTRACT.md section 3): sw.js, sw-page, sw-probe, csp, hist, cookie/js.

import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import vm from "node:vm";
import { SW_SOURCE } from "../s2.mjs";
import { get, startTestHost, tempTls } from "./helpers.mjs";

let tls;
let host;
let port;
before(async () => {
  tls = tempTls();
  host = await startTestHost(tls);
  port = host.ports.good;
});
after(async () => {
  await host.close();
  tls.cleanup();
});
const g = (path, opts = {}) => get(port, path, { ca: tls.ca, ...opts });
const text = async (path) => (await g(path)).body.toString();

/** The text of the script elements of a page, in order, with their attribute text. */
function scripts(html) {
  return [...html.matchAll(/<script([^>]*)>([\s\S]*?)<\/script>/g)].map((m) => ({ attrs: m[1], code: m[2] }));
}

/** Run page code in a sandbox that records the probes. */
function runPage(code, sandbox) {
  const probes = [];
  const win = { rqLab: { probe: (n, v) => probes.push([n, v]) } };
  const ctx = vm.createContext({ window: win, ...sandbox });
  vm.runInContext(code, ctx);
  return { probes, ctx };
}

const flush = () => new Promise((r) => setImmediate(r));

test("sw_script_shape", async () => {
  const r = await g("/s/sw.js");
  assert.equal(r.status, 200);
  assert.equal(r.headers["content-type"], "text/javascript; charset=utf-8");
  assert.equal(r.headers["service-worker-allowed"], "/s/");
  assert.equal(r.headers["cache-control"], "no-store");
  assert.equal(r.body.toString(), SW_SOURCE);

  // The worker logic, run with a small fake of the worker scope.
  const handlers = {};
  const self = { addEventListener: (n, f) => { handlers[n] = f; }, skipWaiting: () => { self.skipped = true; },
    clients: { claim: () => "claimed" } };
  class Response {
    constructor(body, init) { this.body = body; this.init = init; }
  }
  vm.runInNewContext(r.body.toString(), { self, URL, Response });
  assert.deepEqual(Object.keys(handlers).sort(), ["activate", "fetch", "install"]);
  handlers.install();
  assert.equal(self.skipped, true);
  let waited;
  handlers.activate({ waitUntil: (v) => { waited = v; } });
  assert.equal(waited, "claimed");

  let answered;
  handlers.fetch({ request: { url: "https://localhost:1/s/sw-probe?run=x" }, respondWith: (v) => { answered = v; } });
  assert.equal(answered.body, '{"sw":"worker"}');
  assert.equal(answered.init.status, 200);
  assert.equal(answered.init.headers["Content-Type"], "application/json");
  answered = undefined;
  handlers.fetch({ request: { url: "https://localhost:1/s/echo" }, respondWith: (v) => { answered = v; } });
  assert.equal(answered, undefined);
});

test("sw_probe_answers_network", async () => {
  const r = await g("/s/sw-probe?run=a");
  assert.equal(r.status, 200);
  assert.equal(r.headers["content-type"], "application/json");
  assert.equal(r.body.toString(), '{"sw":"network"}');
});

function swSandbox({ controlled, again, registerError, fetchSw }) {
  const calls = { register: [], replaced: [], fetched: [], cleared: [] };
  const sw = {
    register: (url, opts) => { calls.register.push([url, opts]); return registerError ? Promise.reject({ name: registerError }) : Promise.resolve({}); },
    ready: Promise.resolve({}),
    controller: controlled ? {} : null,
  };
  return {
    calls,
    sandbox: {
      navigator: { serviceWorker: sw },
      location: { pathname: "/s/sw-page", search: again ? "?run=r1&again=1" : "?run=r1", replace: (u) => calls.replaced.push(u) },
      fetch: (u) => { calls.fetched.push(u); return Promise.resolve({ json: () => Promise.resolve({ sw: fetchSw }) }); },
      setTimeout: () => 7,
      clearTimeout: (id) => { calls.cleared.push(id); },
      Promise,
    },
  };
}

test("sw_page_probes", async () => {
  const html = await text("/s/sw-page?run=r1");
  const [{ code }] = scripts(html);

  const none = runPage(code, { navigator: {}, location: { search: "" }, setTimeout: () => 0, clearTimeout: () => {}, Promise });
  assert.deepEqual(none.probes, [["sw", "unsupported"]]);

  const worker = swSandbox({ controlled: true, fetchSw: "worker" });
  const w = runPage(code, worker.sandbox);
  await flush();
  assert.equal(JSON.stringify(worker.calls.register), '[["/s/sw.js",{"scope":"/s/"}]]');
  assert.deepEqual(worker.calls.fetched, ["/s/sw-probe?run=r1"]);
  assert.deepEqual(w.probes, [["sw", "worker"]]);

  const n = runPage(code, swSandbox({ controlled: false, again: true, fetchSw: "network" }).sandbox);
  await flush();
  assert.deepEqual(n.probes, [["sw", "network"]]);

  const first = swSandbox({ controlled: false, fetchSw: "worker" });
  const f = runPage(code, first.sandbox);
  await flush();
  assert.deepEqual(first.calls.replaced, ["/s/sw-page?run=r1&again=1"]);
  assert.deepEqual(first.calls.fetched, []);
  assert.deepEqual(f.probes, []);

  const bad = runPage(code, swSandbox({ registerError: "SecurityError" }).sandbox);
  await flush();
  assert.deepEqual(bad.probes, [["sw", "SecurityError"]]);
});

test("sw_page_without_run_has_no_run_parameter", async () => {
  const [{ code }] = scripts(await text("/s/sw-page"));
  assert.ok(code.includes('"/s/sw-probe?"'));
  assert.ok(code.includes('"again=1"'));
  assert.equal((await g("/s/sw-page?run=bad%20id")).status, 400);
});

test("csp_modes", async () => {
  const policy = "script-src 'nonce-run_1'";
  const meta = `<meta http-equiv="Content-Security-Policy" content="${policy}">`;
  const header = await g("/s/csp?run=run.1&mode=header");
  assert.equal(header.status, 200);
  assert.equal(header.headers["content-security-policy"], policy);
  assert.ok(!header.body.toString().includes("http-equiv"));

  const m = await g("/s/csp?run=run.1&mode=meta");
  assert.equal(m.headers["content-security-policy"], undefined);
  assert.ok(m.body.toString().startsWith(`<!doctype html><html><head>${meta}`));

  const both = await g("/s/csp?run=run.1&mode=both");
  assert.equal(both.headers["content-security-policy"], policy);
  assert.ok(both.body.toString().includes(`<head>${meta}`));

  assert.equal((await g("/s/csp?run=run.1")).headers["content-security-policy"], policy);
  assert.equal((await g("/s/csp?run=run.1&mode=x")).status, 400);

  const [withNonce, inline] = scripts(both.body.toString());
  assert.equal(withNonce.attrs, ' nonce="run_1"');
  assert.equal(inline.attrs, "");
  const a = runPage(withNonce.code, { eval: () => 1 });
  assert.deepEqual(a.probes, [["csp.nonce", true], ["csp.eval", "allowed"]]);
  const b = runPage(withNonce.code, { eval: () => { throw new Error("blocked"); } });
  assert.deepEqual(b.probes, [["csp.nonce", true], ["csp.eval", "blocked"]]);
  assert.deepEqual(runPage(inline.code, {}).probes, [["csp.inline", true]]);
});

test("csp_without_run_uses_norun_nonce", async () => {
  const r = await g("/s/csp?mode=header");
  assert.equal(r.headers["content-security-policy"], "script-src 'nonce-norun'");
});

test("hist_steps", async () => {
  for (const step of [0, 5, 8]) {
    const html = await text(`/s/hist?run=h1&step=${step}`);
    assert.ok(html.includes(`<a id="next" href="/s/hist?run=h1&step=${step + 1}">`));
    assert.ok(html.includes(`<p>step ${step}</p>`));
    const [{ code }] = scripts(html);
    const r = runPage(code, { history: { length: 3 }, document: { location: { href: "https://x/" } } });
    assert.deepEqual(r.probes, [["history", 3], ["location", "https://x/"]]);
  }
  assert.ok((await text("/s/hist")).includes("<p>step 0</p>"));
  assert.ok((await text("/s/hist?step=2")).includes('href="/s/hist?step=3"'));
  for (const bad of ["10", "-1", "a", ""]) assert.equal((await g(`/s/hist?step=${bad}`)).status, 400);
});

test("cookie_js_rules", async () => {
  const ok = await g("/s/cookie/js?run=c1&name=rq-a&value=v1&attrs=Secure,SameSite=Lax,Max-Age=60");
  assert.equal(ok.status, 200);
  assert.equal(ok.headers["set-cookie"], undefined);
  const [{ code }] = scripts(ok.body.toString());
  const set = [];
  const doc = { set cookie(v) { set.push(v); }, get cookie() { return "rq-a=v1"; } };
  const r = runPage(code, { document: doc });
  assert.deepEqual(set, ["rq-a=v1; Path=/; Secure; SameSite=Lax; Max-Age=60"]);
  assert.deepEqual(r.probes, [["cookie", "rq-a=v1"]]);

  const plain = scripts(await text("/s/cookie/js?name=a&value=b"))[0].code;
  assert.ok(plain.startsWith('document.cookie="a=b; Path=/";'));

  for (const q of ["name=a&value=b&attrs=HttpOnly", "name=A&value=b", "name=a&value=b%3Bx", "name=a", "value=b",
    "name=a&value=b&attrs=Secure,Secure", "name=a&value=b&attrs=Domain=x", "name=a&value=b&attrs=Max-Age=x"]) {
    assert.equal((await g(`/s/cookie/js?${q}`)).status, 400, q);
  }
  assert.equal((await g("/s/cookie/set?name=a&value=b&attrs=HttpOnly")).status, 200);
});

test("hist_last_step_has_no_link", async () => {
  const html = await text("/s/hist?run=h1&step=9");
  assert.ok(html.includes("<p>step 9</p>"));
  assert.ok(!html.includes('id="next"'));
  assert.ok((await text("/s/hist?run=h1&step=8")).includes('<a id="next" href="/s/hist?run=h1&step=9">'));
});

test("sw_page_clears_its_time_limit_when_ready_wins", async () => {
  const html = await text("/s/sw-page?run=r1");
  const [{ code }] = scripts(html);
  const worker = swSandbox({ controlled: true, fetchSw: "worker" });
  runPage(code, worker.sandbox);
  await new Promise((r) => setTimeout(r, 20));
  assert.deepEqual(worker.calls.cleared, [7]);
});
