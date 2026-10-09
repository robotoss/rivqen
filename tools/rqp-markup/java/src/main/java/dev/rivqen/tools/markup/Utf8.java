// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

/** Strict UTF-8 validation by RFC 3629 ({@code markup.md} M-02). */
final class Utf8 {
  private Utf8() {}

  /**
   * Returns true when {@code b} is valid UTF-8: no overlong form, no surrogate code point, no code
   * point above U+10FFFF, no truncated sequence and no stray continuation byte.
   */
  static boolean isValid(byte[] b) {
    int n = b.length;
    int i = 0;
    while (i < n) {
      int c = b[i] & 0xFF;
      if (c < 0x80) {
        i++;
        continue;
      }
      int need;
      int lo = 0x80;
      int hi = 0xBF;
      if (c < 0xC2) {
        // A continuation byte, or C0/C1 (overlong two-byte form).
        return false;
      } else if (c < 0xE0) {
        need = 1;
      } else if (c < 0xF0) {
        need = 2;
        if (c == 0xE0) {
          lo = 0xA0; // overlong three-byte form
        } else if (c == 0xED) {
          hi = 0x9F; // surrogates U+D800..U+DFFF
        }
      } else if (c < 0xF5) {
        need = 3;
        if (c == 0xF0) {
          lo = 0x90; // overlong four-byte form
        } else if (c == 0xF4) {
          hi = 0x8F; // above U+10FFFF
        }
      } else {
        return false;
      }
      if (n - i <= need) {
        return false;
      }
      int c1 = b[i + 1] & 0xFF;
      if (c1 < lo || c1 > hi) {
        return false;
      }
      for (int k = 2; k <= need; k++) {
        int ck = b[i + k] & 0xFF;
        if (ck < 0x80 || ck > 0xBF) {
          return false;
        }
      }
      i += need + 1;
    }
    return true;
  }
}
