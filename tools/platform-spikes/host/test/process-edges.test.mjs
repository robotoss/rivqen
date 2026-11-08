// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Edge behavior of the lab TLS set, host start and stop, and the command line program.

import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { X509Certificate } from "node:crypto";
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import http from "node:http";
import net from "node:net";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { after, before, test } from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";
import { startHost } from "../host.mjs";
import { RENEW_BEFORE_MS, ensureTls } from "../tls.mjs";
import { control, events, get, startRun } from "./helpers.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const MAIN = join(here, "..", "main.mjs");
const dirs = [];
const tempDir = () => {
  const d = mkdtempSync(join(tmpdir(), "rq-spike-edge-"));
  dirs.push(d);
  return d;
};
after(() => dirs.forEach((d) => rmSync(d, { recursive: true, force: true })));

let template;
before(() => {
  template = tempDir();
  ensureTls(template);
});
/** A copy of a complete, fresh TLS set. */
const copyOfSet = () => {
  const d = tempDir();
  cpSync(template, d, { recursive: true });
  return d;
};
const cert = (dir, f) => new X509Certificate(readFileSync(join(dir, f)));

// TLS set

test("tls_set_is_made_again_when_only_a_key_file_is_missing", () => {
  for (const f of ["ca.key", "leaf.key", "bad-leaf.key"]) {
    const dir = copyOfSet();
    rmSync(join(dir, f));
    assert.equal(ensureTls(dir).made, true, f);
    assert.equal(existsSync(join(dir, f)), true, f);
  }
});

test("tls_set_is_kept_while_every_certificate_has_more_than_the_renew_time_left", () => {
  const dir = copyOfSet();
  const end = Math.min(...["ca.pem", "leaf.pem", "bad-leaf.pem"].map((f) => Date.parse(cert(dir, f).validTo)));
  assert.equal(ensureTls(dir, { now: () => end - RENEW_BEFORE_MS - 1000 }).made, false);
});

test("tls_set_is_made_again_when_a_certificate_has_exactly_the_renew_time_left", () => {
  const dir = copyOfSet();
  const end = Math.min(...["ca.pem", "leaf.pem", "bad-leaf.pem"].map((f) => Date.parse(cert(dir, f).validTo)));
  assert.equal(ensureTls(dir, { now: () => end - RENEW_BEFORE_MS }).made, true);
});

test("tls_set_is_made_again_when_the_clock_is_before_the_start_of_a_certificate", () => {
  const dir = copyOfSet();
  const start = Math.min(...["ca.pem", "leaf.pem", "bad-leaf.pem"].map((f) => Date.parse(cert(dir, f).validFrom)));
  assert.equal(ensureTls(dir, { now: () => start - 3_600_000 }).made, true);
});

test("tls_set_is_kept_at_the_second_when_the_last_certificate_starts", () => {
  const dir = copyOfSet();
  const start = Math.max(...["ca.pem", "leaf.pem", "bad-leaf.pem"].map((f) => Date.parse(cert(dir, f).validFrom)));
  assert.equal(ensureTls(dir, { now: () => start }).made, false);
});

test("tls_set_is_made_again_when_any_certificate_file_is_not_a_certificate", () => {
  for (const f of ["ca.pem", "leaf.pem", "bad-leaf.pem"]) {
    const dir = copyOfSet();
    writeFileSync(join(dir, f), "not a certificate");
    assert.equal(ensureTls(dir).made, true, f);
    assert.doesNotThrow(() => cert(dir, f), f);
  }
});

test("tls_set_leaves_no_working_files_and_sets_file_modes", () => {
  const dir = tempDir();
  ensureTls(dir);
  assert.equal(existsSync(join(dir, "openssl.cnf")), false);
  assert.equal(existsSync(join(dir, "leaf.csr")), false);
  for (const f of ["ca.key", "leaf.key", "bad-leaf.key"]) assert.equal(statSync(join(dir, f)).mode & 0o777, 0o600, f);
  for (const f of ["ca.pem", "leaf.pem", "bad-leaf.pem"]) assert.equal(statSync(join(dir, f)).mode & 0o777, 0o644, f);
});

test("tls_leaf_serial_is_a_positive_number_below_2_pow_63", () => {
  const dir = tempDir();
  ensureTls(dir);
  const serial = BigInt(`0x${cert(dir, "leaf.pem").serialNumber}`);
  assert.ok(serial < 1n << 63n, serial.toString(16));
});

test("tls_openssl_gets_the_lab_directory_and_a_pipe_for_errors", () => {
  const dir = tempDir();
  const calls = [];
  const exec = (file, args, opts) => {
    calls.push({ file, args, opts });
    throw new Error("stop");
  };
  assert.throws(() => ensureTls(dir, { exec, openssl: "my-openssl" }));
  assert.equal(calls[0].file, "my-openssl");
  assert.equal(calls[0].opts.cwd, dir);
  assert.deepEqual(calls[0].opts.stdio, ["ignore", "ignore", "pipe"]);
});

// host start and stop

const startOn = (tlsDir, extra = {}) => startHost({ tlsDir, ports: { good: 0, bad: 0, control: 0 }, sitePort: 1, ...extra });

test("host_start_makes_the_tls_files_when_the_directory_is_empty", async () => {
  const dir = tempDir();
  const h = await startOn(dir);
  try {
    assert.equal(existsSync(join(dir, "leaf.pem")), true);
  } finally {
    await h.close();
  }
});

test("host_start_stamps_events_with_the_clock_that_it_is_given", async () => {
  let now = 5000;
  const dir = copyOfSet();
  const h = await startOn(dir, { now: () => now });
  try {
    await startRun(h, "clock-1");
    now = 5042;
    await get(h.ports.good, "/s/echo?run=clock-1", { ca: readFileSync(join(dir, "ca.pem")) });
    const [req] = await events(h, "clock-1");
    assert.equal(req.t, 42);
  } finally {
    await h.close();
  }
});

test("host_start_fails_with_the_listen_error_and_frees_the_ports_that_it_took", async () => {
  const taken = net.createServer();
  await new Promise((r) => taken.listen(0, "127.0.0.1", r));
  const busy = taken.address().port;
  try {
    await assert.rejects(startOn(copyOfSet(), { ports: { good: 0, bad: busy, control: 0 } }), { code: "EADDRINUSE" });
  } finally {
    await new Promise((r) => taken.close(r));
  }
});

test("host_close_ends_while_a_client_keeps_its_connection_open", { timeout: 15_000 }, async () => {
  const h = await startOn(copyOfSet());
  const agent = new http.Agent({ keepAlive: true });
  await new Promise((resolve, reject) => {
    const req = http.request({ host: "127.0.0.1", port: h.ports.control, path: "/run/log?run=x", agent, headers: { Host: `127.0.0.1:${h.ports.control}` } }, (res) => {
      res.resume();
      res.on("end", resolve);
    });
    req.on("error", reject);
    req.end();
  });
  await h.close();
  agent.destroy();
});

// the program

/** Start main.mjs with ports 0 and wait for its first line. */
async function startProgram(tlsDir, extraArgs = []) {
  const child = spawn(process.execPath, [MAIN, "--port", "0", "--bad-port", "0", "--control-port", "0", "--site-port", "1", "--tls-dir", tlsDir, ...extraArgs],
    { stdio: ["ignore", "pipe", "pipe"] });
  const exit = new Promise((resolve) => child.on("exit", (code, signal) => resolve({ code, signal })));
  let err = "";
  child.stderr.on("data", (d) => { err += d; });
  const line = await new Promise((resolve, reject) => {
    let out = "";
    child.stdout.on("data", (d) => { out += d; if (out.includes("\n")) resolve(out.trim()); });
    exit.then((e) => reject(new Error(`exited early: ${JSON.stringify(e)} ${err}`)));
  });
  // The program prints the line before it installs its signal handlers, in the same turn of the event loop.
  // An answer from the control port proves that the turn is over.
  const controlPort = Number(line.split(" ")[3]);
  await control(controlPort, "GET", "/run/log?run=ready");
  return { child, exit, stderr: () => err };
}

for (const signal of ["SIGTERM", "SIGINT"]) {
  test(`program_stops_with_exit_code_0_on_${signal}`, { timeout: 20_000 }, async () => {
    const p = await startProgram(copyOfSet());
    p.child.kill(signal);
    assert.deepEqual(await p.exit, { code: 0, signal: null });
  });
}

test("program_prints_a_listening_line_with_the_three_bound_ports", { timeout: 20_000 }, async () => {
  const child = spawn(process.execPath, [MAIN, "--port", "0", "--bad-port", "0", "--control-port", "0", "--site-port", "1", "--tls-dir", copyOfSet()], { stdio: ["ignore", "pipe", "ignore"] });
  const line = await new Promise((resolve) => {
    let out = "";
    child.stdout.on("data", (d) => { out += d; if (out.includes("\n")) resolve(out.trim()); });
  });
  child.kill("SIGTERM");
  assert.match(line, /^listening [1-9]\d* [1-9]\d* [1-9]\d*$/);
});

test("program_exits_with_code_2_and_a_message_when_the_tls_directory_is_missing", { timeout: 20_000 }, async () => {
  const env = { ...process.env };
  delete env.LAB_TLS;
  const child = spawn(process.execPath, [MAIN], { env, stdio: ["ignore", "pipe", "pipe"] });
  let err = "";
  child.stderr.on("data", (d) => { err += d; });
  const code = await new Promise((resolve) => child.on("exit", resolve));
  assert.equal(code, 2);
  assert.match(err, /^spike host: LAB_TLS or --tls-dir is required/);
});

test("program_exits_with_code_2_on_a_bad_port", { timeout: 20_000 }, async () => {
  const child = spawn(process.execPath, [MAIN, "--port", "70000", "--tls-dir", "/nowhere"], { stdio: ["ignore", "pipe", "pipe"] });
  let err = "";
  child.stderr.on("data", (d) => { err += d; });
  const code = await new Promise((resolve) => child.on("exit", resolve));
  assert.equal(code, 2);
  assert.match(err, /^spike host: port must be a port number\n$/);
});

test("program_exits_with_code_1_when_a_port_is_taken", { timeout: 20_000 }, async () => {
  const taken = net.createServer();
  await new Promise((r) => taken.listen(0, "127.0.0.1", r));
  try {
    const child = spawn(process.execPath, [MAIN, "--port", String(taken.address().port), "--bad-port", "0", "--control-port", "0", "--site-port", "1", "--tls-dir", copyOfSet()], { stdio: ["ignore", "pipe", "pipe"] });
    let err = "";
    child.stderr.on("data", (d) => { err += d; });
    const code = await new Promise((resolve) => child.on("exit", resolve));
    assert.equal(code, 1);
    assert.match(err, /^spike host: cannot start: .*EADDRINUSE/);
  } finally {
    await new Promise((r) => taken.close(r));
  }
});

test("importing_main_does_not_start_the_program", { timeout: 20_000 }, async () => {
  const env = { ...process.env };
  delete env.LAB_TLS;
  const child = spawn(process.execPath, ["--input-type=module", "-e", `await import(${JSON.stringify(pathToFileURL(MAIN).href)});`], { env, stdio: ["ignore", "pipe", "pipe"] });
  let err = "";
  child.stderr.on("data", (d) => { err += d; });
  const code = await new Promise((resolve) => child.on("exit", resolve));
  assert.equal(code, 0);
  assert.equal(err, "");
});
