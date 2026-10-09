// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import org.jspecify.annotations.Nullable;

/** Writes the result object of {@code CONTRACT.md} §3 as one line of JSON. */
final class ResultJson {
  private ResultJson() {}

  /**
   * Formats a result.
   *
   * @param r the result
   * @param fixture the fixture name for batch mode, or null in single mode
   * @return one JSON object without a line feed
   */
  static String format(Result r, @Nullable String fixture) {
    StringBuilder s = new StringBuilder(128 + r.blocks().size() * 128);
    s.append('{');
    if (fixture != null) {
      s.append("\"fixture\":");
      string(s, fixture);
      s.append(',');
    }
    ErrorCode error = r.error();
    s.append("\"valid\":").append(error == null);
    s.append(",\"error\":");
    if (error == null) {
      s.append("null");
    } else {
      string(s, error.wire());
    }
    s.append(",\"blocks\":[");
    boolean first = true;
    for (Result.Block b : r.blocks()) {
      if (!first) {
        s.append(',');
      }
      first = false;
      s.append("{\"id\":");
      string(s, b.id());
      s.append(",\"format\":");
      string(s, b.format());
      s.append(",\"start\":").append(b.start());
      s.append(",\"end\":").append(b.end());
      s.append(",\"sha256\":");
      string(s, b.sha256());
      s.append('}');
    }
    s.append("],\"template_revision\":");
    nullableString(s, r.templateRevision());
    s.append(",\"page_revision\":");
    nullableString(s, r.pageRevision());
    s.append('}');
    return s.toString();
  }

  private static void nullableString(StringBuilder s, @Nullable String v) {
    if (v == null) {
      s.append("null");
    } else {
      string(s, v);
    }
  }

  /** A JSON string (RFC 8259 §7): quotes, backslash and control characters are escaped. */
  static void string(StringBuilder s, String v) {
    s.append('"');
    for (int i = 0; i < v.length(); i++) {
      char c = v.charAt(i);
      switch (c) {
        case '"' -> s.append("\\\"");
        case '\\' -> s.append("\\\\");
        case '\n' -> s.append("\\n");
        case '\r' -> s.append("\\r");
        case '\t' -> s.append("\\t");
        default -> {
          if (c < 0x20 || (Character.isSurrogate(c) && !validSurrogate(v, i))) {
            s.append(String.format("\\u%04x", (int) c));
          } else {
            s.append(c);
          }
        }
      }
    }
    s.append('"');
  }

  /** True when the surrogate at {@code i} is part of a well-formed pair. */
  private static boolean validSurrogate(String v, int i) {
    char c = v.charAt(i);
    if (Character.isHighSurrogate(c)) {
      return i + 1 < v.length() && Character.isLowSurrogate(v.charAt(i + 1));
    }
    return i > 0 && Character.isHighSurrogate(v.charAt(i - 1));
  }
}
