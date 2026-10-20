// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from "node:assert/strict";
import { test } from "node:test";
import { isHeaderName, isHeaderValue, parseCase } from "../lab.mjs";

const withHeader = (name, value) =>
  parseCase({ id: "A", question: "Q-20", source: "x.html", headers: [[name, value]], views: ["on"], probe: null }, "A.json");

test("a header value with a code unit above 0xFF is rejected, also when its low byte is CR or LF", () => {
  // U+010D has the low byte 0x0D (CR), U+010A has 0x0A (LF): latin1 encoding would turn them into a split.
  assert.throws(() => withHeader("Content-Type", "text/htmlčĊSet-Cookie: x"), { code: "CASE_INVALID" });
  assert.throws(() => withHeader("Content-Type", "Ā"), { code: "CASE_INVALID" });
  assert.throws(() => withHeader("Content-Type", "\u{1F600}"), { code: "CASE_INVALID" });
});

test("a header name with a code unit above 0xFF is rejected", () => {
  assert.throws(() => withHeader("X-AŁ", "v"), { code: "CASE_INVALID" });
  assert.equal(isHeaderName("X-AŁ"), false);
});

test("a header value with CR, LF, NUL or another control byte is rejected; HTAB is allowed", () => {
  for (const bad of ["a\rb", "a\nb", "a\0b", "a\x01b", "a\x7fb"]) assert.equal(isHeaderValue(bad), false, JSON.stringify(bad));
  assert.equal(isHeaderValue("a\tb"), true);
  assert.equal(isHeaderValue(""), true);
});

test("a Latin-1 header value passes", () => {
  assert.deepEqual(withHeader("X-Name", "café ÿ\u0080").headers, [["X-Name", "café ÿ\u0080"]]);
});

test("a header name must be non-empty token bytes", () => {
  assert.equal(isHeaderName(""), false);
  assert.equal(isHeaderName("Bad Name"), false);
  assert.equal(isHeaderName("X:y"), false);
  assert.equal(isHeaderName("Content-Type"), true);
});
