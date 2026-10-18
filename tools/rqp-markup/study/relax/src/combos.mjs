// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Helpers for the relaxation combinations of the H-40 study.

import { COMBOS } from './markup-relax.mjs';

/** The fields of the result object of CONTRACT.md section 3 (no study fields). */
export function contractFields(r) {
  return {
    valid: r.valid,
    error: r.error,
    blocks: r.blocks,
    template_revision: r.template_revision,
    page_revision: r.page_revision,
  };
}

/** A short key of a result: "invalid" or the blocks with their offsets. */
export function resultKey(r) {
  if (!r.valid) return 'invalid';
  return `valid ${r.blocks.map((b) => `${b.id}/${b.format}/${b.start}/${b.end}`).join(',')}`;
}

/** The relaxations of a combination name, as a set ("r3w" counts as r3w). */
export function partsOf(combo) {
  return combo === 'base' ? new Set() : new Set(combo.split('+'));
}

/**
 * The combinations of COMBOS that are a proper subset of `combo` (base is a
 * subset of every other combination). r3 is a subset of r3w: the scope
 * "region" allows every crossing that the scope "root" allows.
 */
export function subsetsOf(combo) {
  const want = partsOf(combo);
  const covers = (part) => want.has(part) || (part === 'r3' && want.has('r3w'));
  return COMBOS.filter((c) => c !== combo && [...partsOf(c)].every(covers));
}
