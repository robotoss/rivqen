// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The option combinations of the S5 study (flags of src/markup-s5.mjs) and their
// order: each combination accepts every document that a smaller one accepts, with
// the same result (checked by the sweep as "monotonic").

/** name -> { template, noscript, control } */
export const COMBOS = Object.freeze({
  base: { template: 'off', noscript: 'off', control: false },
  t1: { template: 't1', noscript: 'off', control: false },
  t2: { template: 't2', noscript: 'off', control: false },
  tu: { template: 'tu', noscript: 'off', control: true },
  n1: { template: 'off', noscript: 'n1', control: false },
  n2: { template: 'off', noscript: 'n2', control: false },
  nu: { template: 'off', noscript: 'nu', control: true },
  t1n1: { template: 't1', noscript: 'n1', control: false },
  t2n1: { template: 't2', noscript: 'n1', control: false },
  t2n2: { template: 't2', noscript: 'n2', control: false },
});

export const COMBO_NAMES = Object.freeze(Object.keys(COMBOS));

/** Pairs [smaller, larger]: every document that `smaller` accepts, `larger` accepts with the same result. */
export const SUBSETS = Object.freeze([
  ['base', 't2'], ['t2', 't1'], ['t1', 'tu'], ['base', 'n2'], ['n2', 'n1'], ['n1', 'nu'],
  ['t1', 't1n1'], ['n1', 't1n1'], ['t2', 't2n2'], ['n2', 't2n2'], ['t2n2', 't2n1'], ['t2', 't2n1'], ['n1', 't2n1'],
  ['t2n1', 't1n1'],
]);

/** The key of a result for the cache of the oracle: validity and blocks. */
export function resultKey(r) {
  return r.valid ? `valid ${r.blocks.map((b) => `${b.id}/${b.format}/${b.start}/${b.end}`).join(',')}` : `invalid ${r.error}`;
}
