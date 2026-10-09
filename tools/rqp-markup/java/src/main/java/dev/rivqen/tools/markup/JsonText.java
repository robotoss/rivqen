// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

/**
 * Validation of a JSON text by RFC 8259 §2 ({@code markup.md} M-25), without building values.
 *
 * <p>The input range must be valid UTF-8 (checked before by M-02). The validator has no recursion:
 * it keeps the open arrays and objects in a fixed array of {@link #MAX_DEPTH} entries. Numbers have
 * no size limit. Duplicate member names are allowed. A {@code \}{@code u} escape of a surrogate
 * must form a pair.
 */
final class JsonText {
  /** Maximum nesting depth of arrays and objects (M-25). */
  static final int MAX_DEPTH = 64;

  private final byte[] in;
  private final int limit;
  private int pos;

  private JsonText(byte[] in, int from, int to) {
    this.in = in;
    this.pos = from;
    this.limit = to;
  }

  /** Returns true when {@code in[from, to)} is one JSON text. */
  static boolean isValid(byte[] in, int from, int to) {
    return new JsonText(in, from, to).text();
  }

  private boolean text() {
    // true = object, false = array, for each open container.
    boolean[] open = new boolean[MAX_DEPTH];
    int depth = 0;
    while (true) {
      // A value is expected.
      skipSpace();
      if (pos >= limit) {
        return false;
      }
      byte b = in[pos];
      if (b == '{' || b == '[') {
        if (depth == MAX_DEPTH) {
          return false;
        }
        boolean object = b == '{';
        open[depth++] = object;
        pos++;
        skipSpace();
        if (pos < limit && in[pos] == (object ? (byte) '}' : (byte) ']')) {
          pos++;
          depth--;
        } else if (object) {
          if (!memberName()) {
            return false;
          }
          continue;
        } else {
          continue;
        }
      } else if (!scalar(b)) {
        return false;
      }
      // After a value: close containers or read the next element.
      while (true) {
        skipSpace();
        if (depth == 0) {
          return pos == limit;
        }
        if (pos >= limit) {
          return false;
        }
        byte c = in[pos++];
        boolean object = open[depth - 1];
        if (c == ',') {
          if (object && !memberName()) {
            return false;
          }
          break;
        }
        if (c != (object ? (byte) '}' : (byte) ']')) {
          return false;
        }
        depth--;
      }
    }
  }

  /** Reads {@code string ws ':'} of an object member. */
  private boolean memberName() {
    skipSpace();
    if (pos >= limit || in[pos] != '"' || !string()) {
      return false;
    }
    skipSpace();
    if (pos >= limit || in[pos] != ':') {
      return false;
    }
    pos++;
    return true;
  }

  private boolean scalar(byte b) {
    return switch (b) {
      case '"' -> string();
      case 't' -> literal("true");
      case 'f' -> literal("false");
      case 'n' -> literal("null");
      default -> number();
    };
  }

  private boolean literal(String word) {
    int n = word.length();
    if (limit - pos < n) {
      return false;
    }
    for (int i = 0; i < n; i++) {
      if (in[pos + i] != word.charAt(i)) {
        return false;
      }
    }
    pos += n;
    return true;
  }

  /** {@code [ minus ] int [ frac ] [ exp ]}. */
  private boolean number() {
    if (pos < limit && in[pos] == '-') {
      pos++;
    }
    if (pos >= limit) {
      return false;
    }
    if (in[pos] == '0') {
      pos++;
    } else if (!digits()) {
      return false;
    }
    if (pos < limit && in[pos] == '.') {
      pos++;
      if (!digits()) {
        return false;
      }
    }
    if (pos < limit && (in[pos] == 'e' || in[pos] == 'E')) {
      pos++;
      if (pos < limit && (in[pos] == '+' || in[pos] == '-')) {
        pos++;
      }
      return digits();
    }
    return true;
  }

  /** One or more digits. */
  private boolean digits() {
    int from = pos;
    while (pos < limit && in[pos] >= '0' && in[pos] <= '9') {
      pos++;
    }
    return pos > from;
  }

  /** A string from its opening quote at {@code pos}. */
  private boolean string() {
    pos++;
    while (pos < limit) {
      int c = in[pos++] & 0xFF;
      if (c == '"') {
        return true;
      }
      if (c < 0x20) {
        return false;
      }
      if (c == '\\' && !escape()) {
        return false;
      }
    }
    return false;
  }

  private boolean escape() {
    if (pos >= limit) {
      return false;
    }
    byte e = in[pos++];
    return switch (e) {
      case '"', '\\', '/', 'b', 'f', 'n', 'r', 't' -> true;
      case 'u' -> unicodeEscape();
      default -> false;
    };
  }

  /** The four hex digits after {@code \}{@code u}; a surrogate must form a pair. */
  private boolean unicodeEscape() {
    int unit = hex4();
    if (unit < 0 || (unit >= 0xDC00 && unit <= 0xDFFF)) {
      return false;
    }
    if (unit < 0xD800 || unit > 0xDBFF) {
      return true;
    }
    if (limit - pos < 2 || in[pos] != '\\' || in[pos + 1] != 'u') {
      return false;
    }
    pos += 2;
    int low = hex4();
    return low >= 0xDC00 && low <= 0xDFFF;
  }

  /** Four hex digits as a value, or -1. */
  private int hex4() {
    if (limit - pos < 4) {
      return -1;
    }
    int v = 0;
    for (int i = 0; i < 4; i++) {
      int d = Character.digit(in[pos + i], 16);
      if (d < 0) {
        return -1;
      }
      v = (v << 4) | d;
    }
    pos += 4;
    return v;
  }

  private void skipSpace() {
    while (pos < limit) {
      byte b = in[pos];
      if (b != ' ' && b != '\t' && b != '\n' && b != '\r') {
        return;
      }
      pos++;
    }
  }
}
