// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Runs every check of the platform spike evidence (CONTRACT sections 5, 7, 8 and 9).
import { catalogProblems } from './catalog.mjs';
import { evidenceProblems } from './evidence.mjs';

/** `catalog` is the parsed catalog; `root` is the tree that the catalog paths are relative to. */
export async function runCheck({ root, catalog }) {
  const catalogErrors = catalogProblems(catalog);
  const evidence = await evidenceProblems(catalog, root);
  const capabilities = Array.isArray(catalog) ? catalog.length : 0;
  return {
    errors: [...catalogErrors, ...evidence.problems],
    counts: { capabilities, ...evidence.counts },
  };
}
