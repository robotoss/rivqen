#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// rivqen-conformance: tests an RQP or legacy server by URL (server/conformance/CONTRACT.md).
// Usage and exit codes: server/conformance/README.md.

import { main } from '../src/runner.mjs';

try {
  process.exitCode = await main(process.argv.slice(2));
} catch (error) {
  // A defect of the runner itself: no result can be trusted.
  process.stderr.write(`rivqen-conformance: internal error (${error instanceof Error ? `${error.name}: ${error.message}` : typeof error})\n`);
  process.exitCode = 2;
}
