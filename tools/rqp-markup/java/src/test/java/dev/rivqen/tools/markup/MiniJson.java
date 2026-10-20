// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * A small JSON reader for test data (expected.json, fixture.json, command output). Objects become
 * {@link Map}, arrays {@link List}, numbers {@link Long}, null becomes {@link #NULL}.
 */
final class MiniJson {
  /** The JSON null value. */
  static final Object NULL = new Object();

  private final String s;
  private int i;

  private MiniJson(String s) {
    this.s = s;
  }

  static Object parse(String s) {
    MiniJson p = new MiniJson(s);
    Object v = p.value();
    p.space();
    if (p.i != s.length()) {
      throw new IllegalArgumentException("trailing data at " + p.i);
    }
    return v;
  }

  @SuppressWarnings("unchecked")
  static Map<String, Object> object(String s) {
    return (Map<String, Object>) parse(s);
  }

  private Object value() {
    space();
    char c = s.charAt(i);
    switch (c) {
      case '{' -> {
        i++;
        Map<String, Object> m = new LinkedHashMap<>();
        space();
        if (s.charAt(i) == '}') {
          i++;
          return m;
        }
        while (true) {
          space();
          String k = string();
          space();
          expect(':');
          m.put(k, value());
          space();
          if (s.charAt(i++) == '}') {
            return m;
          }
        }
      }
      case '[' -> {
        i++;
        List<Object> l = new ArrayList<>();
        space();
        if (s.charAt(i) == ']') {
          i++;
          return l;
        }
        while (true) {
          l.add(value());
          space();
          if (s.charAt(i++) == ']') {
            return l;
          }
        }
      }
      case '"' -> {
        return string();
      }
      case 't' -> {
        i += 4;
        return true;
      }
      case 'f' -> {
        i += 5;
        return false;
      }
      case 'n' -> {
        i += 4;
        return NULL;
      }
      default -> {
        int from = i;
        while (i < s.length() && "+-0123456789.eE".indexOf(s.charAt(i)) >= 0) {
          i++;
        }
        return Long.parseLong(s.substring(from, i));
      }
    }
  }

  private String string() {
    expect('"');
    StringBuilder b = new StringBuilder();
    while (true) {
      char c = s.charAt(i++);
      if (c == '"') {
        return b.toString();
      }
      if (c != '\\') {
        b.append(c);
        continue;
      }
      char e = s.charAt(i++);
      switch (e) {
        case 'n' -> b.append('\n');
        case 'r' -> b.append('\r');
        case 't' -> b.append('\t');
        case 'b' -> b.append('\b');
        case 'f' -> b.append('\f');
        case 'u' -> {
          b.append((char) Integer.parseInt(s.substring(i, i + 4), 16));
          i += 4;
        }
        default -> b.append(e);
      }
    }
  }

  private void expect(char c) {
    if (s.charAt(i++) != c) {
      throw new IllegalArgumentException("expected " + c + " at " + (i - 1));
    }
  }

  private void space() {
    while (i < s.length() && Character.isWhitespace(s.charAt(i))) {
      i++;
    }
  }
}
