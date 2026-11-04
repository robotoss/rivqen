// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

/** Copy of a JSON value with the keys of every object sorted (UTF-16 code unit order). Arrays keep their order. */
export function sortKeys(value) {
  if (Array.isArray(value)) return value.map(sortKeys);
  if (value !== null && typeof value === 'object') {
    const out = {};
    for (const key of Object.keys(value).sort()) out[key] = sortKeys(value[key]);
    return out;
  }
  return value;
}

/** One line of an event log (CONTRACT section 5): JSON with sorted keys, no trailing newline. */
export function canonicalLine(value) {
  return JSON.stringify(sortKeys(value));
}
