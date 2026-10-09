// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from "node:assert/strict";
import { test } from "node:test";
import { decodeUtf8, utf16ToUtf8Map } from "../src/utf8.mjs";

test("decodeUtf8 keeps ASCII and has no BOM", () => {
  assert.deepEqual(decodeUtf8(Buffer.from("<p>a</p>")), { text: "<p>a</p>", bomBytes: 0 });
});

test("decodeUtf8 removes a BOM at offset 0 and counts its 3 bytes", () => {
  const r = decodeUtf8(Buffer.from([0xef, 0xbb, 0xbf, 0x61]));
  assert.deepEqual(r, { text: "a", bomBytes: 3 });
});

test("decodeUtf8: an input that is only a BOM is empty text", () => {
  assert.deepEqual(decodeUtf8(Buffer.from([0xef, 0xbb, 0xbf])), { text: "", bomBytes: 3 });
  assert.deepEqual(decodeUtf8(Buffer.from([0xef, 0xbb])), null);
});

test("decodeUtf8 keeps U+FEFF that is not at offset 0", () => {
  const r = decodeUtf8(Buffer.from([0x61, 0xef, 0xbb, 0xbf]));
  assert.deepEqual(r, { text: "a﻿", bomBytes: 0 });
});

test("decodeUtf8 rejects invalid, overlong, surrogate and truncated UTF-8", () => {
  for (const bytes of [[0xc3, 0x28], [0xc0, 0xaf], [0xed, 0xa0, 0x80], [0xe2, 0x82]]) {
    assert.equal(decodeUtf8(Buffer.from(bytes)), null, `bytes ${bytes}`);
  }
});

test("utf16ToUtf8Map counts 1, 2, 3 and 4 byte characters", () => {
  const map = utf16ToUtf8Map("aé€\u{1F600}b");
  // a=0 é=1 €=3 😀=6 (two units) b=10 end=11
  assert.deepEqual([...map], [0, 1, 3, 6, 6, 10, 11]);
});

test("utf16ToUtf8Map adds the base offset (BOM)", () => {
  assert.deepEqual([...utf16ToUtf8Map("ab", 3)], [3, 4, 5]);
});

test("utf16ToUtf8Map: the boundaries of the 1, 2 and 3 byte forms", () => {
  // U+007F 1 byte, U+0080 2 bytes, U+07FF 2 bytes, U+0800 3 bytes, U+FFFF 3 bytes
  assert.deepEqual([...utf16ToUtf8Map("\u007f\u0080߿ࠀ￿")], [0, 1, 3, 5, 8, 11]);
});

test("utf16ToUtf8Map counts a lone surrogate as 3 bytes", () => {
  assert.deepEqual([...utf16ToUtf8Map("\ud800a")], [0, 3, 4]);
  // A high surrogate followed by a character above the low surrogates is not a pair.
  assert.deepEqual([...utf16ToUtf8Map("\ud800")], [0, 3, 6]);
});
