// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// node main.mjs [--port N] [--bad-port N] [--control-port N] [--site-port N] [--tls-dir DIR]
// Defaults come from the environment of lab-env.sh (LAB_HOST_PORT, LAB_HOST_BAD_PORT,
// LAB_HOST_CONTROL_PORT, LAB_SITE_PORT, LAB_TLS). Prints "listening <good> <bad> <control>".

import { parseArgs } from "node:util";
import { startHost } from "./host.mjs";

/**
 * @param {string[]} argv @param {NodeJS.ProcessEnv} env
 * @returns {{ tlsDir: string, ports: { good: number, bad: number, control: number }, sitePort: number }}
 */
export function parseConfig(argv, env) {
  const { values } = parseArgs({
    args: argv,
    options: {
      port: { type: "string" },
      "bad-port": { type: "string" },
      "control-port": { type: "string" },
      "site-port": { type: "string" },
      "tls-dir": { type: "string" },
    },
    strict: true,
  });
  const port = (v, fallback, name) => {
    const text = v ?? fallback;
    if (text === undefined || !/^\d{1,5}$/.test(text) || Number(text) > 65535) throw new Error(`${name} must be a port number`);
    return Number(text);
  };
  const tlsDir = values["tls-dir"] ?? env.LAB_TLS;
  if (tlsDir === undefined || tlsDir === "") throw new Error("LAB_TLS or --tls-dir is required (source tools/platform-spikes/lab-env.sh)");
  return {
    tlsDir,
    ports: {
      good: port(values.port, env.LAB_HOST_PORT ?? "18443", "port"),
      bad: port(values["bad-port"], env.LAB_HOST_BAD_PORT ?? "18444", "bad-port"),
      control: port(values["control-port"], env.LAB_HOST_CONTROL_PORT ?? "18445", "control-port"),
    },
    sitePort: port(values["site-port"], env.LAB_SITE_PORT ?? "18480", "site-port"),
  };
}

async function main() {
  let config;
  try {
    config = parseConfig(process.argv.slice(2), process.env);
  } catch (err) {
    process.stderr.write(`spike host: ${err instanceof Error ? err.message : String(err)}\n`);
    process.exit(2);
  }
  let host;
  try {
    host = await startHost(config);
  } catch (err) {
    process.stderr.write(`spike host: cannot start: ${err instanceof Error ? err.message : String(err)}\n`);
    process.exit(1);
  }
  // Install the signal handlers before the ready line, so a stop right after "listening" exits 0.
  const stop = () => {
    host.close().then(() => process.exit(0), () => process.exit(1));
  };
  process.on("SIGINT", stop);
  process.on("SIGTERM", stop);
  process.stdout.write(`listening ${host.ports.good} ${host.ports.bad} ${host.ports.control}\n`);
}

import { pathToFileURL } from "node:url";
if (process.argv[1] !== undefined && import.meta.url === pathToFileURL(process.argv[1]).href) await main();
