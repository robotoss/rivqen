// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Lab TLS files (CONTRACT.md section 2, DL-134): a CA, a leaf signed by it, and a self-signed
// "bad" leaf, made with the system openssl when missing. The keys stay in the lab directory.

import { execFileSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import { chmodSync, existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
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

/**
 * Make the TLS files in `dir` when any is missing. A complete set is kept.
 * @param {string} dir
 * @param {{ openssl?: string, exec?: (file: string, args: string[], opts: object) => unknown }} [opts]
 * @returns {{ made: boolean, dir: string }}
 */
export function ensureTls(dir, { openssl = process.env.OPENSSL ?? "openssl", exec = execFileSync } = {}) {
  if (TLS_FILES.every((f) => existsSync(join(dir, f)))) return { made: false, dir };
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
