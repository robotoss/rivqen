// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// UTF-8 decoding and UTF-16 → UTF-8 offset conversion for the parse5 view.

const BOM = [0xef, 0xbb, 0xbf];

/**
 * Decode the input as strict UTF-8 (markup.md M-02).
 * A BOM at offset 0 is removed from the text (the WHATWG decoder removes it)
 * and counted in `bomBytes`, so offsets still refer to the input bytes.
 *
 * @param {Uint8Array} bytes
 * @returns {{ text: string, bomBytes: number } | null} null when the input is not valid UTF-8
 */
export function decodeUtf8(bytes) {
  const bomBytes = bytes.length >= 3 && BOM.every((b, i) => bytes[i] === b) ? 3 : 0;
  try {
    const text = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes.subarray(bomBytes));
    return { text, bomBytes };
  } catch {
    return null;
  }
}

/**
 * Map every UTF-16 offset of `text` (0 … text.length) to a UTF-8 byte offset.
 * An offset between the two units of a surrogate pair maps to the start of the character;
 * parse5 never reports such an offset.
 *
 * @param {string} text well-formed UTF-16 (output of decodeUtf8)
 * @param {number} base byte offset of text[0] in the input (the BOM length)
 * @returns {Uint32Array}
 */
export function utf16ToUtf8Map(text, base = 0) {
  const map = new Uint32Array(text.length + 1);
  let b = base;
  for (let i = 0; i < text.length; i++) {
    map[i] = b;
    const c = text.charCodeAt(i);
    if (c < 0x80) b += 1;
    else if (c < 0x800) b += 2;
    else if (c >= 0xd800 && c <= 0xdbff && i + 1 < text.length) {
      const d = text.charCodeAt(i + 1);
      if (d >= 0xdc00 && d <= 0xdfff) {
        map[i + 1] = b;
        b += 4;
        i++;
      } else {
        b += 3;
      }
    } else b += 3;
  }
  map[text.length] = b;
  return map;
}
