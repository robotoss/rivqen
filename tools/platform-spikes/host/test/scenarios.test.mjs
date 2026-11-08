// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { createHash } from "node:crypto";
import { labBytes, parseRange, partialHead } from "../scenarios.mjs";
import { events, get, open, startRun, startTestHost, tempTls, wait } from "./helpers.mjs";

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
const o = (path, opts = {}) => open(port, path, { ca: tls.ca, ...opts });

test("stream_gate_beacon", async () => {
  await startRun(host, "gate-1");
  const s = o("/s/stream?run=gate-1&chunks=3&size=400&gate=beacon&timeout=20000");
  await s.until(/data-k="0"/);
  await wait(60); // chunk 1 would be written at once if the gate did not hold it
  assert.ok(!/data-k="1"/.test(s.text), "chunk 1 waits for the beacon");
  assert.equal((await g("/s/beacon?run=gate-1&k=0")).status, 204);
  await s.until(/data-k="1"/);
  await wait(60);
  assert.ok(!/data-k="2"/.test(s.text), "chunk 2 waits for the beacon of chunk 1");
  await g("/s/beacon?run=gate-1&k=1");
  await s.done;
  assert.match(s.text, /data-k="2"/);
  const ev = await events(host, "gate-1");
  assert.deepEqual(ev.filter((e) => e.ev === "host.chunk").map((e) => e.k), [0, 1, 2]);
  assert.deepEqual(ev.filter((e) => e.ev === "host.beacon").map((e) => e.k), [0, 1]);
  assert.equal(ev.some((e) => e.ev === "host.gate_timeout"), false);
});

test("stream_gate_timeout", async () => {
  await startRun(host, "gate-2");
  const s = o("/s/stream?run=gate-2&chunks=2&size=400&gate=beacon&timeout=150");
  const t0 = await s.until(/data-k="0"/);
  const t1 = await s.until(/data-k="1"/);
  assert.ok(t1 - t0 >= 140, `chunk 1 came ${t1 - t0} ms after chunk 0`);
  await s.done;
  const ev = await events(host, "gate-2");
  assert.deepEqual(ev.filter((e) => e.ev === "host.gate_timeout").map((e) => e.k), [0]);
});

test("stream_delay_and_first", async () => {
  const s = o("/s/stream?chunks=2&size=300&first=120&delay=120");
  const t0 = await s.until(/data-k="0"/);
  const t1 = await s.until(/data-k="1"/);
  assert.ok(t0 >= 110, `head came after ${t0} ms`);
  assert.ok(t1 - t0 >= 110, `second chunk ${t1 - t0} ms later`);
  await s.done;
});

test("stream_chunk_shape", async () => {
  await startRun(host, "shape-1");
  const r = await g("/s/stream?run=shape-1&chunks=2&size=2000&delay=0");
  assert.equal(r.status, 200);
  assert.equal(r.headers["content-type"], "text/html; charset=utf-8");
  assert.equal(r.headers["cache-control"], "no-store");
  assert.equal(r.headers["transfer-encoding"], "chunked");
  assert.equal(r.headers["content-security-policy"], undefined);
  const body = r.body.toString();
  assert.match(body, /<img src="\/s\/beacon\?run=shape-1&k=0" hidden>/);
  assert.match(body, /<img src="\/s\/beacon\?run=shape-1&k=1" hidden>/);
  assert.match(body, /rqLab\.probe\("chunk",1\)/);
  assert.equal(body.match(/<script>/g).length, 2);
  const chunks = (await events(host, "shape-1")).filter((e) => e.ev === "host.chunk");
  assert.deepEqual(chunks.map((e) => e.bytes), [2000, 2000]);
});

test("stream_csp_nonce", async () => {
  const r = await g("/s/stream?run=CAP-X-01-ios-27.1-1&chunks=3&size=500&csp=1");
  assert.equal(r.headers["content-security-policy"], "script-src 'nonce-CAP-X-01-ios-27_1-1'");
  const body = r.body.toString();
  const scripts = body.match(/<script[^>]*>/g);
  assert.equal(scripts.length, 3);
  for (const s of scripts) assert.equal(s, '<script nonce="CAP-X-01-ios-27_1-1">');
});

test("stream_rejects_bad_limits", async () => {
  for (const q of ["chunks=0", "chunks=1025", "size=0", "size=8388609", "chunks=1024&size=8388608", "chunks=x",
    "gate=other", "csp=2", "first=60001", "chunks=%C3%A9"]) {
    const r = await g(`/s/stream?${q}`);
    assert.equal(r.status, 400, q);
  }
  assert.equal((await g("/s/stream?chunks=1&size=8388608")).status, 200);
});

test("redirect_chain", async () => {
  for (const code of [301, 302, 303, 307, 308]) {
    const r = await g(`/s/redirect?n=3&code=${code}`);
    assert.equal(r.status, code);
    assert.equal(r.headers.location, `/s/redirect?n=2&code=${code}&to=%2Fs%2Fecho`);
  }
  const last = await g("/s/redirect?n=1&code=307&to=%2Fs%2Fecho%3Fa%3D1");
  assert.equal(last.headers.location, "/s/echo?a=1");
  assert.equal((await g("/s/redirect")).headers.location, "/s/echo");

  // cross=1: every hop goes to the other site
  const h1 = await g("/s/redirect?run=r1&n=3&code=302&cross=1", { host: `localhost:${port}` });
  assert.equal(h1.headers.location, `https://127.0.0.1:${port}/s/redirect?run=r1&n=2&code=302&to=%2Fs%2Fecho&cross=1&alt=127.0.0.1`);
  const h2 = await g("/s/redirect?run=r1&n=2&code=302&to=%2Fs%2Fecho&cross=1&alt=127.0.0.1", { host: `127.0.0.1:${port}` });
  assert.equal(h2.headers.location, `https://localhost:${port}/s/redirect?run=r1&n=1&code=302&to=%2Fs%2Fecho&cross=1&alt=127.0.0.1`);
  const h3 = await g("/s/redirect?n=1&cross=1&alt=10.0.2.2", { host: `localhost:${port}` });
  assert.equal(h3.headers.location, `https://10.0.2.2:${port}/s/echo`);
  const h4 = await g("/s/redirect?n=1&cross=1&alt=10.0.2.2", { host: `10.0.2.2:${port}` });
  assert.equal(h4.headers.location, `https://localhost:${port}/s/echo`);
});

test("redirect_rejects_bad_input", async () => {
  for (const q of ["n=0", "n=21", "code=200", "code=304", "to=http%3A%2F%2Fevil.example", "to=%2F%2Fevil.example",
    "to=%2Fa%5Cb", "to=%2Fa%0D%0Ab", "cross=2", "alt=8.8.8.8"]) {
    assert.equal((await g(`/s/redirect?${q}`)).status, 400, q);
  }
});

test("cookie_set_attrs", async () => {
  const r = await g("/s/cookie/set?name=sid&value=abc-123&attrs=HttpOnly,Secure,SameSite%3DLax,Max-Age%3D60");
  assert.equal(r.status, 200);
  assert.equal(r.headers["set-cookie"].length, 1);
  assert.equal(r.headers["set-cookie"][0], "sid=abc-123; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=60");
  assert.match(r.body.toString(), /rqLab\.probe\("cookie",document\.cookie\)/);
  const plain = await g("/s/cookie/set?name=a&value=b");
  assert.deepEqual(plain.headers["set-cookie"], ["a=b; Path=/"]);
  for (const q of ["name=a&value=B", "name=a&value=a%3Bb", "name=A&value=b", "name=&value=b", "name=a", "value=b",
    "name=a&value=b&attrs=Domain%3Dx", "name=a&value=b&attrs=SameSite%3DOther", "name=a&value=b&attrs=Max-Age%3D-1",
    "name=a&value=b&attrs=Secure,Secure", "name=a&value=b&attrs=HttpOnly,,Secure"]) {
    const bad = await g(`/s/cookie/set?${q}`);
    assert.equal(bad.status, 400, q);
    assert.equal(bad.headers["set-cookie"], undefined);
  }
});

test("echo_site_and_cookie", async () => {
  const a = JSON.parse((await g("/s/echo?x=1", { headers: { Cookie: "sid=abc-123; k=v" } })).body);
  assert.equal(a.cookie, "sid=abc-123; k=v");
  assert.equal(a.site, "A");
  assert.equal(a.method, "GET");
  assert.equal(a.target, "/s/echo?x=1");
  assert.ok(a.fields.includes("cookie") && a.fields.includes("host"));
  assert.deepEqual(a.fields, [...a.fields].sort());
  const b = JSON.parse((await g("/s/echo", { host: `10.0.2.2:${port}` })).body);
  assert.equal(b.site, "B");
  assert.equal(b.cookie, null);
  assert.equal(JSON.parse((await g("/s/echo", { host: `127.0.0.1:${port}` })).body).site, "B");
  const html = await g("/s/echo?html=1&q=%3Cb%3E", { headers: { Cookie: "a=b" } });
  assert.equal(html.headers["content-type"], "text/html; charset=utf-8");
  const m = /<pre id="echo">(.*)<\/pre>/.exec(html.body.toString());
  assert.ok(m && !m[1].includes("<b>"));
  assert.equal(JSON.parse(m[1].replaceAll("&lt;", "<").replaceAll("&gt;", ">").replaceAll("&amp;", "&")).cookie, "a=b");
});

test("unknown_host_is_refused", async () => {
  for (const h of ["evil.example", "evil.example:1", "[::1]:1", "localhost.evil.example"]) {
    assert.equal((await g("/s/echo", { host: h })).status, 400, h);
  }
});

test("range_206_416", async () => {
  const full = await g("/s/range?size=1000");
  assert.equal(full.status, 200);
  assert.equal(full.headers["accept-ranges"], "bytes");
  assert.deepEqual(full.body, labBytes(0, 1000));
  const part = await g("/s/range?size=1000", { headers: { Range: "bytes=10-19" } });
  assert.equal(part.status, 206);
  assert.equal(part.headers["content-range"], "bytes 10-19/1000");
  assert.equal(part.headers["content-length"], "10");
  assert.deepEqual(part.body, labBytes(10, 20));
  const open = await g("/s/range?size=1000", { headers: { Range: "bytes=990-" } });
  assert.equal(open.status, 206);
  assert.equal(open.headers["content-range"], "bytes 990-999/1000");
  assert.deepEqual(open.body, labBytes(990, 1000));
  const tail = await g("/s/range?size=1000", { headers: { Range: "bytes=-5" } });
  assert.equal(tail.status, 206);
  assert.equal(tail.headers["content-range"], "bytes 995-999/1000");
  assert.deepEqual(tail.body, labBytes(995, 1000));
  const clamp = await g("/s/range?size=1000", { headers: { Range: "bytes=995-5000" } });
  assert.equal(clamp.headers["content-range"], "bytes 995-999/1000");
  for (const r of ["bytes=1000-", "bytes=20-10", "bytes=-0", "bytes=0-1,3-4", "items=0-1", "bytes=a-b", "bytes=1"]) {
    const bad = await g("/s/range?size=1000", { headers: { Range: r } });
    assert.equal(bad.status, 416, r);
    assert.equal(bad.headers["content-range"], "bytes */1000");
  }
  assert.equal((await g("/s/range?size=0")).status, 400);
});

test("range_parse_boundaries", () => {
  assert.deepEqual(parseRange("bytes=0-0", 1), { from: 0, to: 0 });
  assert.deepEqual(parseRange("bytes=0-", 1), { from: 0, to: 0 });
  assert.equal(parseRange("bytes=1-", 1), "bad");
  assert.equal(parseRange("bytes=5-4", 10), "bad");
  assert.equal(parseRange(undefined, 10), null);
});

test("lab_bytes_follow_the_contract_formula", () => {
  assert.deepEqual([...labBytes(0, 3)], [7, 38, 69]);
  assert.equal(labBytes(8, 9)[0], (8 * 31 + 7) % 251);
  assert.equal(labBytes(300, 301)[0], (300 * 31 + 7) % 251);
});

test("cut_after_bytes", async () => {
  await startRun(host, "cut-1");
  const r = await g("/s/cut?run=cut-1&after=100&size=1000").catch((e) => e);
  // The client sees the head with the full length, then a body that stops after 100 bytes.
  assert.ok(!(r instanceof Error), r instanceof Error ? r.message : "");
  assert.equal(r.status, 200);
  assert.equal(r.headers["content-length"], "1000");
  assert.equal(r.body.length, 100);
  assert.equal(r.aborted, true);
  const ev = await events(host, "cut-1");
  assert.deepEqual(ev.filter((e) => e.ev === "host.cut").map((e) => e.after), [100]);
  assert.equal((await g("/s/cut?after=1000&size=1000")).status, 400);
  assert.equal((await g("/s/cut")).status, 400);
});

test("cut_default_size_is_4096", async () => {
  const r = await g("/s/cut?after=0");
  assert.equal(r.headers["content-length"], "4096");
  assert.equal(r.body.length, 0);
});

test("cut_hold", async () => {
  // S3 partial-page case: the bytes go out at once, the close comes `hold` ms later.
  await startRun(host, "hold-1");
  const t0 = Date.now();
  const s = o("/s/cut?run=hold-1&after=600&size=2000&hold=300");
  const got = await s.until(/k=0" hidden>/);
  await s.done.catch(() => undefined); // the close before Content-Length is an error for the client
  const closedAfter = Date.now() - t0;
  assert.ok(got < 250, `the delivered bytes came after ${got} ms, not before the hold`);
  assert.ok(closedAfter >= 295, `the connection closed after ${closedAfter} ms`);
  assert.equal(Buffer.byteLength(s.text), 600);
  assert.ok(s.text.startsWith(partialHead("hold-1")));
  const ev = await events(host, "hold-1");
  const response = ev.find((e) => e.ev === "host.response");
  const cut = ev.find((e) => e.ev === "host.cut");
  assert.equal(response.fields["content-length"], "2000");
  assert.deepEqual(ev.filter((e) => e.ev === "host.cut").map((e) => e.after), [600]);
  assert.ok(cut.t - response.t >= 295, `host.cut ${cut.t - response.t} ms after the head`);
});

test("cut_hold_body_has_the_chunk_0_probe", async () => {
  await startRun(host, "hold-2");
  const r = await g("/s/cut?run=hold-2&after=1000&size=1001&hold=1");
  const body = r.body.toString();
  assert.equal(r.aborted, true);
  assert.equal(body.length, 1000);
  assert.match(body, /^<!doctype html><meta charset="utf-8"><title>cut<\/title><p data-k="0">/);
  assert.match(body, /<img src="\/s\/beacon\?run=hold-2&k=0" hidden>/);
  assert.match(body, /<script>window\.rqLab&&window\.rqLab\.probe\("chunk",0\)<\/script><p>x+$/);
  // Without a run the beacon has no run parameter.
  assert.match(partialHead(null), /<img src="\/s\/beacon\?k=0" hidden>/);
});

test("cut_without_hold_keeps_the_s2_body", async () => {
  // The stored CAP-A-08 runs (after=3000&size=8192) must stay reproducible: same bytes as in S2.
  const head = '<!doctype html><meta charset="utf-8"><title>cut</title><p>';
  const expected = createHash("sha256").update(head + "x".repeat(3000 - head.length)).digest("hex");
  for (const q of ["", "&hold=0"]) {
    const r = await g(`/s/cut?after=3000&size=8192${q}`);
    assert.equal(createHash("sha256").update(r.body).digest("hex"), expected, q);
    assert.equal(r.aborted, true);
  }
});

test("cut_hold_bounds", async () => {
  for (const h of ["60001", "-1", "1.5", "x", ""]) assert.equal((await g(`/s/cut?after=1&size=4096&hold=${h}`)).status, 400, h);
  // The partial-page start must fit: size at least its length + 1.
  const n = partialHead("hold-3").length;
  assert.equal((await g(`/s/cut?run=hold-3&after=${n}&size=${n}&hold=1`)).status, 400);
  assert.equal((await g(`/s/cut?run=hold-3&after=${n}&size=${n + 1}&hold=1`)).status, 200);
  // With hold the cut must not fall inside the partial markup: after >= its length.
  assert.equal((await g(`/s/cut?run=hold-3&after=${n - 1}&size=${n + 10}&hold=1`)).status, 400);
  // The S2 start is shorter, so the same size is fine without hold.
  assert.equal((await g(`/s/cut?after=0&size=${n}`)).status, 200);
});

test("cut_hold_ends_when_the_client_closes_first", async () => {
  // hold=60000 (the maximum): the client closes during the hold; no host.cut, and no timer is left
  // (the test host closes at once in after()).
  await startRun(host, "hold-4");
  const s = o(`/s/cut?run=hold-4&after=${partialHead("hold-4").length}&size=400&hold=60000`);
  s.done.catch(() => undefined);
  await s.until(/<title>/);
  s.abort();
  await wait(100);
  const ev = await events(host, "hold-4");
  assert.equal(ev.some((e) => e.ev === "host.response"), true);
  assert.equal(ev.some((e) => e.ev === "host.cut"), false);
  assert.equal((await g("/s/echo")).status, 200, "the host still answers");
});

test("unknown_routes_are_404", async () => {
  assert.equal((await g("/s/nothing")).status, 404);
  assert.equal((await g("/s/constructor")).status, 404);
});

test("only_get_with_origin_form_and_ascii_query", async () => {
  const post = await new Promise((resolve, reject) => {
    import("node:https").then(({ default: https }) => {
      const req = https.request({ host: "127.0.0.1", port, path: "/s/echo", method: "POST", ca: tls.ca, servername: "localhost", agent: false,
        headers: { Host: `localhost:${port}` } }, (res) => { res.resume(); resolve(res.statusCode); });
      req.on("error", reject);
      req.end();
    });
  });
  assert.equal(post, 405);
  assert.equal((await g("/s/echo?run=bad%20id")).status, 400);
  assert.equal((await g("/s/echo?x=%FF")).status, 400);
  assert.equal((await g("/s/echo?x=%C3%A9")).status, 400);
});

test("cache_control_is_no_store_on_scenario_routes", async () => {
  for (const p of ["/s/echo", "/s/range?size=5", "/s/beacon?k=0", "/s/redirect", "/s/cookie/set?name=a&value=b", "/nothing"]) {
    assert.equal((await g(p)).headers["cache-control"], "no-store", p);
  }
});

test("range_suffix", () => {
  assert.deepEqual(parseRange("bytes=-100", 1000), { from: 900, to: 999 });
  assert.deepEqual(parseRange("bytes=-5000", 1000), { from: 0, to: 999 });
  assert.equal(parseRange("bytes=-0", 1000), "bad");
  assert.equal(parseRange("bytes=-", 1000), "bad");
});
