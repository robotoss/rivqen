// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from "node:assert/strict";
import { X509Certificate } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";
import { TLS_FILES, ensureTls } from "../tls.mjs";
import { get, startTestHost, tempTls } from "./helpers.mjs";

const dirs = [];
after(() => dirs.forEach((d) => rmSync(d, { recursive: true, force: true })));
const fresh = () => {
  const d = mkdtempSync(join(tmpdir(), "rq-spike-tls-"));
  dirs.push(d);
  return d;
};

test("tls_files_made_once", () => {
  const dir = fresh();
  assert.equal(ensureTls(dir).made, true);
  const before = TLS_FILES.map((f) => readFileSync(join(dir, f), "utf8"));
  assert.equal(ensureTls(dir).made, false);
  assert.deepEqual(TLS_FILES.map((f) => readFileSync(join(dir, f), "utf8")), before);

  const ca = new X509Certificate(readFileSync(join(dir, "ca.pem")));
  const leaf = new X509Certificate(readFileSync(join(dir, "leaf.pem")));
  const bad = new X509Certificate(readFileSync(join(dir, "bad-leaf.pem")));
  assert.equal(ca.subject, "CN=Rivqen WP-04 lab CA");
  assert.equal(ca.ca, true);
  assert.ok(leaf.verify(ca.publicKey), "leaf is signed by the CA");
  assert.equal(leaf.issuer, ca.subject);
  assert.equal(leaf.ca, false);
  for (const c of [leaf, bad]) {
    const san = c.subjectAltName ?? "";
    assert.match(san, /DNS:localhost/);
    assert.match(san, /IP Address:127\.0\.0\.1/);
    assert.match(san, /IP Address:10\.0\.2\.2/);
  }
  assert.ok(bad.verify(bad.publicKey), "bad leaf is self-signed");
  assert.ok(!bad.verify(ca.publicKey));
  const days = (c) => (Date.parse(c.validTo) - Date.parse(c.validFrom)) / 86_400_000;
  for (const c of [ca, leaf, bad]) assert.ok(days(c) >= 29.9 && days(c) <= 30.1, `valid ${days(c)} days`);
  for (const f of ["ca.key", "leaf.key", "bad-leaf.key"]) assert.equal(statSync(join(dir, f)).mode & 0o077, 0, `${f} is private`);
});

test("tls_partial_set_is_replaced_as_a_whole", () => {
  const dir = fresh();
  ensureTls(dir);
  const oldCa = readFileSync(join(dir, "ca.pem"), "utf8");
  rmSync(join(dir, "leaf.pem"));
  assert.equal(ensureTls(dir).made, true);
  assert.notEqual(readFileSync(join(dir, "ca.pem"), "utf8"), oldCa);
  const ca = new X509Certificate(readFileSync(join(dir, "ca.pem")));
  assert.ok(new X509Certificate(readFileSync(join(dir, "leaf.pem"))).verify(ca.publicKey));
});

test("tls_failure_is_a_typed_error_and_leaves_no_files", () => {
  const dir = fresh();
  const exec = () => { throw new Error("boom"); };
  assert.throws(() => ensureTls(dir, { exec }), (e) => e.code === "TLS_SETUP" && e.cause instanceof Error);
  assert.equal(TLS_FILES.some((f) => statSync(join(dir, f), { throwIfNoEntry: false }) !== undefined), false);
});

test("tls_good_and_bad", async (t) => {
  const tls = tempTls();
  const host = await startTestHost(tls);
  t.after(async () => { await host.close(); tls.cleanup(); });
  const ok = await get(host.ports.good, "/s/echo", { ca: tls.ca });
  assert.equal(ok.status, 200);
  await assert.rejects(get(host.ports.bad, "/s/echo", { ca: tls.ca }), (e) => /SELF_SIGNED|UNABLE_TO|CERT/.test(e.code));
  // The same client reaches the site by the IP form too (SAN has 127.0.0.1).
  const ip = await get(host.ports.good, "/s/echo", { ca: tls.ca, host: `127.0.0.1:${host.ports.good}` });
  assert.equal(ip.status, 200);
});

test("tls_renewed_before_expiry", () => {
  const dir = fresh();
  assert.equal(ensureTls(dir).made, true);
  const ca = new X509Certificate(readFileSync(join(dir, "ca.pem")));
  const end = Date.parse(ca.validTo);
  // Three days before the end the set is kept; one day before the end it is made again.
  assert.equal(ensureTls(dir, { now: () => end - 3 * 24 * 3600 * 1000 }).made, false);
  assert.equal(ensureTls(dir, { now: () => end - 1 * 24 * 3600 * 1000 }).made, true);
  assert.notEqual(new X509Certificate(readFileSync(join(dir, "ca.pem"))).serialNumber, ca.serialNumber);
});
