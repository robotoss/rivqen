// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Lab TLS files (CONTRACT.md section 2, DL-134): a CA, a leaf signed by it, and a self-signed
// "bad" leaf, made with the system openssl when missing. The keys stay in the lab directory.

import { execFileSync } from "node:child_process";
import { X509Certificate, randomBytes } from "node:crypto";
import { chmodSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { HostError } from "./util.mjs";

export const TLS_FILES = Object.freeze(["ca.pem", "ca.key", "leaf.pem", "leaf.key", "bad-leaf.pem", "bad-leaf.key"]);
export const SAN = "DNS:localhost,IP:127.0.0.1,IP:10.0.2.2";
const DAYS = "30";

// One config works with OpenSSL 3 and with LibreSSL: extensions come from named sections. The subject
// is in the config because LibreSSL 3.3 ignores -subj when the config has a [dn] section.
const config = (cn) => `[req]
distinguished_name = dn
prompt = no
[dn]
CN = ${cn}
[ca_ext]
basicConstraints = critical,CA:TRUE,pathlen:0
keyUsage = critical,keyCertSign,cRLSign
[leaf_ext]
basicConstraints = critical,CA:FALSE
keyUsage = critical,digitalSignature,keyEncipherment
extendedKeyUsage = serverAuth
subjectAltName = ${SAN}
`;

/** Renew the set when a certificate ends within this time (a run must not meet an expired lab CA). */
export const RENEW_BEFORE_MS = 2 * 24 * 60 * 60 * 1000;

/** True when every certificate of a complete set is valid for at least RENEW_BEFORE_MS more. */
function setIsFresh(dir, nowMs) {
  try {
    return ["ca.pem", "leaf.pem", "bad-leaf.pem"].every((f) => {
      const cert = new X509Certificate(readFileSync(join(dir, f)));
      return Date.parse(cert.validTo) - nowMs > RENEW_BEFORE_MS && Date.parse(cert.validFrom) <= nowMs;
    });
  } catch {
    return false; // an unreadable certificate makes the set incomplete
  }
}

/**
 * Make the TLS files in `dir` when any is missing or a certificate ends within RENEW_BEFORE_MS.
 * A complete, fresh set is kept. After a renewal the lab simulators must trust the new CA again
 * (`lab/sim.sh trust`) and the Android app must be built again.
 * @param {string} dir
 * @param {{ openssl?: string, exec?: (file: string, args: string[], opts: object) => unknown, now?: () => number }} [opts]
 * @returns {{ made: boolean, dir: string }}
 */
export function ensureTls(dir, { openssl = process.env.OPENSSL ?? "openssl", exec = execFileSync, now = Date.now } = {}) {
  if (TLS_FILES.every((f) => existsSync(join(dir, f))) && setIsFresh(dir, now())) return { made: false, dir };
  mkdirSync(dir, { recursive: true, mode: 0o700 });
  // A partial set is replaced as a whole: a leaf must match its CA.
  for (const f of TLS_FILES) rmSync(join(dir, f), { force: true });
  const cfg = "openssl.cnf";
  const run = (args) => {
    try {
      exec(openssl, args, { cwd: dir, stdio: ["ignore", "ignore", "pipe"] });
    } catch (cause) {
      throw new HostError(500, "TLS_SETUP", `openssl ${args[0]} failed`, { cause });
    }
  };
  try {
    const setSubject = (cn) => writeFileSync(join(dir, cfg), config(cn), { mode: 0o600 });
    setSubject("Rivqen WP-04 lab CA");
    run(["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", "ca.key", "-out", "ca.pem", "-days", DAYS,
      "-config", cfg, "-extensions", "ca_ext"]);
    setSubject("localhost");
    run(["req", "-new", "-newkey", "rsa:2048", "-nodes", "-keyout", "leaf.key", "-out", "leaf.csr",
      "-config", cfg]);
    const serial = randomBytes(8);
    serial[0] &= 0x7f; // positive
    run(["x509", "-req", "-in", "leaf.csr", "-CA", "ca.pem", "-CAkey", "ca.key", "-set_serial",
      `0x${serial.toString("hex")}`, "-out", "leaf.pem", "-days", DAYS, "-extfile", cfg, "-extensions", "leaf_ext"]);
    run(["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", "bad-leaf.key", "-out", "bad-leaf.pem",
      "-days", DAYS, "-config", cfg, "-extensions", "leaf_ext"]);
  } catch (err) {
    for (const f of TLS_FILES) rmSync(join(dir, f), { force: true });
    throw err;
  } finally {
    for (const f of [cfg, "leaf.csr"]) rmSync(join(dir, f), { force: true });
  }
  for (const f of TLS_FILES) chmodSync(join(dir, f), f.endsWith(".key") ? 0o600 : 0o644);
  return { made: true, dir };
}
