// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import org.jspecify.annotations.Nullable;

/**
 * A second, plain implementation of the token stack of {@code markup.md} M-05 with the depth
 * counter D of M-26 (dirty mark, implied parts) and M-06 items 1 to 4 (guarded elements, formatting
 * end tags, stray table end tags, special-crossed mark). It is written from the rule text only,
 * with lists and linear searches, and knows only the few names that {@link PropertyTest} generates.
 * It is a test oracle; it does not share code with {@link Analyzer}.
 */
final class StackModel {
  static final int LIMIT = 256;

  /** The names of markup.md §2.1 that the generated documents use. */
  private static final Set<String> SPECIAL =
      Set.of("body", "div", "html", "li", "p", "table", "tbody", "td", "tr", "ul");

  private static final Set<String> FORMATTING = Set.of("a", "b", "em", "i");
  private static final Set<String> GUARDED = Set.of("a", "table", "tbody", "td", "tr");
  private static final Set<String> TABLE_PART = Set.of("tbody", "td", "tr");
  private static final Set<String> TABLE_FAMILY = Set.of("table", "tbody", "td", "tr");
  private static final Set<String> STRAY_TABLE_END = Set.of("table", "tbody", "tr");

  /** One element of the token stack. */
  private static final class Entry {
    final String name;
    final boolean foreign;
    boolean dirty;
    boolean marked;
    final List<String> parts = new ArrayList<>();

    Entry(String name, boolean foreign) {
      this.name = name;
      this.foreign = foreign;
    }
  }

  private final List<Entry> stack = new ArrayList<>();
  private int d;
  private int maxD;

  /** The rule of the first error, or null. */
  private @Nullable String error;

  /** The item of M-06 of the first error (1 to 4), or 0. */
  private int item;

  int depth() {
    return d;
  }

  int maxDepth() {
    return maxD;
  }

  @Nullable String error() {
    return error;
  }

  int item() {
    return item;
  }

  int size() {
    return stack.size();
  }

  String name(int index) {
    return stack.get(index).name;
  }

  /** Index of the svg element of the foreign region, or -1. */
  int regionRoot() {
    for (int i = 0; i < stack.size(); i++) {
      if (!stack.get(i).foreign && stack.get(i).name.equals("svg")) {
        return i;
      }
    }
    return -1;
  }

  /** F of M-09 (no template in these documents), or null. */
  @Nullable String tableFamily() {
    int f = tableFamilyIndex();
    return f < 0 ? null : stack.get(f).name;
  }

  private int tableFamilyIndex() {
    for (int i = stack.size() - 1; i >= 0; i--) {
      Entry e = stack.get(i);
      if (!e.foreign && TABLE_FAMILY.contains(e.name)) {
        return i;
      }
    }
    return -1;
  }

  private void add(int units) {
    for (int i = 0; i < units && error == null; i++) {
      d++;
      maxD = Math.max(maxD, d);
      if (d > LIMIT) {
        error = "M-26";
        item = 0;
      }
    }
  }

  /**
   * A start tag in HTML context ({@code foreign} false) or in a foreign region. {@code pushed} is
   * false for void elements and self-closing foreign elements.
   */
  void start(String name, boolean foreign, boolean pushed) {
    if (error != null) {
      return;
    }
    if (!foreign) {
      int f = tableFamilyIndex();
      if (f >= 0) {
        Entry fe = stack.get(f);
        List<String> implied = new ArrayList<>();
        if (name.equals("tr") && fe.name.equals("table")) {
          implied.add("row group");
        } else if (name.equals("td") && fe.name.equals("table")) {
          implied.add("row group");
          implied.add("row");
        } else if (name.equals("td") && fe.name.equals("tbody")) {
          implied.add("row");
        } else if (name.equals("col") && fe.name.equals("table")) {
          implied.add("column group");
        }
        for (String part : implied) {
          if (!fe.parts.contains(part)) {
            fe.parts.add(part);
            add(1);
          }
        }
      }
    }
    if (pushed && error == null) {
      add(1);
      stack.add(new Entry(name, foreign));
    }
  }

  /** An end tag (M-05 step 3) with M-06 items 1 to 4 and the change of D and of the marks. */
  void end(String name) {
    if (error != null) {
      return;
    }
    boolean htmlContext = regionRoot() < 0;
    int m = -1;
    for (int i = stack.size() - 1; i >= 0; i--) {
      if (stack.get(i).name.equals(name)) {
        m = i;
        break;
      }
    }
    if (m < 0) {
      if (htmlContext && STRAY_TABLE_END.contains(name)) {
        error = "M-06"; // item 3
        item = 3;
      }
      return;
    }
    int top = stack.size() - 1;
    boolean setMark = false;
    for (int i = top; i >= m; i--) {
      Entry e = stack.get(i);
      boolean crossed = i > m;
      if (crossed
          && !e.foreign
          && GUARDED.contains(e.name)
          && !(TABLE_FAMILY.contains(name) && TABLE_PART.contains(e.name))) {
        error = "M-06"; // item 1
        item = 1;
        return;
      }
      boolean specialOrForeign = e.foreign || SPECIAL.contains(e.name);
      if (FORMATTING.contains(name) && crossed && specialOrForeign) {
        error = "M-06"; // item 2
        item = 2;
        return;
      }
      if (FORMATTING.contains(name) && e.marked) {
        error = "M-06"; // item 4
        item = 4;
        return;
      }
      setMark |= (crossed && specialOrForeign) || e.marked;
    }
    Entry matched = stack.get(m);
    boolean exact = m == top && !matched.dirty && !name.equals("html") && !name.equals("body");
    if (exact) {
      d -= 1 + matched.parts.size();
    }
    while (stack.size() > m) {
      stack.remove(stack.size() - 1);
    }
    if (!stack.isEmpty()) {
      Entry current = stack.get(stack.size() - 1);
      if (!exact) {
        current.dirty = true;
      }
      if (setMark) {
        current.marked = true;
      }
    }
  }
}
