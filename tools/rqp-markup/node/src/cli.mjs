#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Command line of the Node.js RQP markup parser candidate
// (tools/rqp-markup/CONTRACT.md section 2):
//
//   node src/cli.mjs <path-to-input.html>
//   node src/cli.mjs --batch <fixtures-dir>
//
// Exit codes: 0 when results are printed (valid and invalid documents),
// 2 for a usage error or an I/O error, 70 when an internal defect was found
// (the result for that input is printed and fails closed).
// Document content is never printed to stderr.

import { closeSync, existsSync, openSync, readSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { ErrorCode, LIMITS, analyze, invalid } from './markup.mjs';

export const EXIT_OK = 0;
export const EXIT_USAGE = 2;
export const EXIT_INTERNAL = 70;

const USAGE = 'usage: node src/cli.mjs <input.html> | node src/cli.mjs --batch <fixtures-dir>';
const CHUNK = 64 * 1024;

/** A usage or I/O error. */
export class CliError extends Error {
  constructor(message, options) {
    super(message, options);
    this.name = 'CliError';
    this.code = 'RQP_CLI_USAGE';
  }
}

/**
 * Read at most `limit + 1` bytes of a file. A longer file gives `limit + 1`
 * bytes, which is enough for the size check of M-01. Memory stays bounded for
 * any file, also a device or a FIFO.
 */
export function readBounded(file, limit = LIMITS.maxDocumentBytes) {
  let fd;
  try {
    fd = openSync(file, 'r');
  } catch (cause) {
    throw new CliError(`cannot open ${file} (${cause.code ?? 'error'})`, { cause });
  }
  const chunks = [];
  let total = 0;
  try {
    while (total <= limit) {
      const chunk = Buffer.alloc(Math.min(CHUNK, limit + 1 - total));
      const n = readSync(fd, chunk, 0, chunk.length, null);
      if (n === 0) break;
      chunks.push(n === chunk.length ? chunk : chunk.subarray(0, n));
      total += n;
    }
  } catch (cause) {
    throw new CliError(`cannot read ${file} (${cause.code ?? 'error'})`, { cause });
  } finally {
    closeSync(fd);
  }
  return Buffer.concat(chunks, total);
}

/**
 * Analyze one document. An exception is a defect: report it without content,
 * and fail closed (the document is not used for RQP).
 */
function analyzeSafely(bytes, label, io) {
  try {
    return { result: io.analyze(bytes), defect: false };
  } catch (error) {
    const kind = error instanceof Error ? error.name : typeof error;
    io.stderr(`rqp-markup: internal error (${kind}) for ${label}; the result fails closed\n`);
    return { result: invalid(ErrorCode.STRUCTURE), defect: true };
  }
}

/** Subdirectory names in byte order of their UTF-8 names (CONTRACT.md section 2). */
export function sortedSubdirs(dir) {
  let names;
  try {
    names = readdirSync(dir);
  } catch (cause) {
    throw new CliError(`cannot read directory ${dir} (${cause.code ?? 'error'})`, { cause });
  }
  names.sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)));
  return names.filter((name) => {
    try {
      return statSync(path.join(dir, name)).isDirectory();
    } catch (cause) {
      throw new CliError(`cannot read ${path.join(dir, name)} (${cause.code ?? 'error'})`, { cause });
    }
  });
}

function runSingle(file, io) {
  const { result, defect } = analyzeSafely(readBounded(file), 'the input', io);
  io.stdout(`${JSON.stringify(result)}\n`);
  return defect ? EXIT_INTERNAL : EXIT_OK;
}

function runBatch(dir, io) {
  let code = EXIT_OK;
  for (const name of sortedSubdirs(dir)) {
    const input = path.join(dir, name, 'input.html');
    if (!existsSync(input)) continue;
    const { result, defect } = analyzeSafely(readBounded(input), `fixture ${name}`, io);
    if (defect) code = EXIT_INTERNAL;
    io.stdout(`${JSON.stringify({ fixture: name, ...result })}\n`);
  }
  return code;
}

/**
 * Run the command. Returns the exit code.
 *
 * @param {string[]} argv arguments after the script name
 * @param {{ stdout: (s: string) => void, stderr: (s: string) => void }} streams
 * @param {(bytes: Buffer) => object} analyzer replaceable for tests of the defect path
 */
export function main(argv, streams, analyzer = analyze) {
  const io = { ...streams, analyze: analyzer };
  try {
    if (argv.length === 2 && argv[0] === '--batch') return runBatch(argv[1], io);
    if (argv.length === 1 && !argv[0].startsWith('--')) return runSingle(argv[0], io);
    throw new CliError(USAGE);
  } catch (error) {
    if (!(error instanceof CliError)) throw error;
    io.stderr(`rqp-markup: ${error.message}\n`);
    return EXIT_USAGE;
  }
}

if (process.argv[1] !== undefined && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.stdout.on('error', (error) => {
    // A closed pipe: nothing more can be printed.
    process.stderr.write(`rqp-markup: cannot write output (${error.code ?? 'error'})\n`);
    process.exit(EXIT_USAGE);
  });
  process.exitCode = main(process.argv.slice(2), {
    stdout: (s) => process.stdout.write(s),
    stderr: (s) => process.stderr.write(s),
  });
}
