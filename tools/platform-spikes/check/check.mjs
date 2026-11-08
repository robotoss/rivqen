#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Offline check of the platform spike catalog and evidence: node check.mjs [--root <dir>] [--catalog <path>]
// Exit code 0: the evidence is valid. 1: findings, one line each. 2: bad use or unreadable input.
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { runCheck } from './src/check.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const say = (text) => process.stdout.write(`${text}\n`);
const USAGE = 'usage: node check.mjs [--root <dir>] [--catalog <path>]\n';

/** Parses `--root <dir>` and `--catalog <path>`. Returns null for bad use. */
function parseArgs(args) {
  const opts = {};
  for (let i = 0; i < args.length; i += 2) {
    const flag = args[i];
    const value = args[i + 1];
    if ((flag !== '--root' && flag !== '--catalog') || value === undefined || value.startsWith('--')) return null;
    const key = flag.slice(2);
    if (opts[key] !== undefined) return null;
    opts[key] = value;
  }
  return opts;
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts === null) {
    process.stderr.write(USAGE);
    return 2;
  }
  const root = path.resolve(opts.root ?? path.join(here, '..', '..', '..'));
  const catalogPath = path.resolve(opts.catalog ?? path.join(root, 'tools', 'platform-spikes', 'capabilities.json'));
  const catalog = JSON.parse(await readFile(catalogPath, 'utf8'));
  const { errors, counts } = await runCheck({ root, catalog });
  for (const e of errors) say(`ERROR ${e}`);
  if (errors.length > 0) return 1;
  say(
    `ok: ${counts.capabilities} capabilities, ${counts.results} results, ${counts.runs} run logs, ${counts.recordings} recordings`,
  );
  return 0;
}

main().then(
  (code) => {
    process.exitCode = code;
  },
  (e) => {
    process.stderr.write(`check failed: ${e.code ?? e.message}\n`);
    process.exitCode = 2;
  },
);
