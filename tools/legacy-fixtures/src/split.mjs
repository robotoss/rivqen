// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Template and data split of a legacy page (docs/engineering/protocol/legacy-markers.md
// sections 1 to 4, legacy-wire.md rules C-06 to C-08). The split is textual and pure.
//
// split never throws for a string input. Profiles:
//   legacy-canonical  title once; empty or longer than 128 names stay in the template;
//                     a repeated name makes the split fail (DL-075)
//   legacy-java       empty name gives the key {}; the last value of a repeated name wins
//   legacy-php        as legacy-java
//   legacy-node       empty names get {auto0}, {auto1}, ...; the last value of a repeated name wins
// No profile reproduces the several-title defect of Java and PHP (U-11, DL-069).

export const MAX_NAME = 128;

const TITLE = /<title([^\n]*?)<\/title>/i;
const BLOCK = /<!--sonicdiff-?(\w*)-->([\s\S]+?)<!--sonicdiff-?(\w*)-end-->/gi;
const PLACEHOLDER = /\{\w*\}/g;

/**
 * Split page text. Returns { ok: true, template, data, blocks } where data is a
 * Map of key (with braces) to value, {title} first, then the blocks in document
 * order, and blocks is the number of extracted blocks; or { ok: false, reason }.
 *
 * @param {string} profile
 * @param {string} text
 */
export function split(profile, text) {
  const data = new Map();
  const title = TITLE.exec(text);
  let template = text;
  if (title !== null) template = text.slice(0, title.index) + '{title}' + text.slice(title.index + title[0].length);
  data.set('{title}', title === null ? '' : title[0]);

  const canonical = profile === 'legacy-canonical';
  let blocks = 0;
  let auto = 0;
  let failed = false;
  template = template.replace(BLOCK, (whole, name) => {
    let key;
    if (name === '' && profile === 'legacy-node') {
      key = `{auto${auto++}}`;
    } else if (canonical && (name === '' || name.length > MAX_NAME)) {
      return whole;
    } else {
      key = `{${name}}`;
    }
    if (canonical && data.has(key)) failed = true;
    data.set(key, whole);
    blocks++;
    return key;
  });
  if (failed) return { ok: false, reason: 'DUPLICATE_BLOCK_NAME' };
  return { ok: true, template, data, blocks };
}

/**
 * Rebuild a page: each placeholder of the template is replaced once, in template
 * order, by its value (legacy-markers.md section 5). A placeholder without a value
 * and a second occurrence stay as they are.
 *
 * @param {string} template
 * @param {Map<string, string>} data
 */
export function rebuild(template, data) {
  const used = new Set();
  return template.replace(PLACEHOLDER, (placeholder) => {
    if (!data.has(placeholder) || used.has(placeholder)) return placeholder;
    used.add(placeholder);
    return data.get(placeholder);
  });
}
