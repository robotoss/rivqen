// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import java.util.Arrays;
import java.util.HashMap;
import java.util.Map;
import org.jspecify.annotations.Nullable;

/**
 * The token stack of {@code markup.md} M-05, with constant-time queries.
 *
 * <p>Every query that a rule needs runs in O(1): the nearest element with a name (a chain of
 * same-name entries), the nearest table-family element (M-09), the foreign region (M-05 step 4) and
 * "is an element with this name open" (counters). Each entry is pushed once and popped once, so the
 * total work is linear in the number of tokens.
 *
 * <p>The counters are arrays indexed by {@link Enum#ordinal()}, as in {@link java.util.EnumMap}:
 * the index never leaves this process and needs no boxing.
 */
@SuppressWarnings("EnumOrdinal")
final class TokenStack {
  private static final int NAMES = Name.values().length;

  private String[] names = new String[16];
  private @Nullable Name[] known = new Name[16];
  private boolean[] foreign = new boolean[16];
  private boolean[] content = new boolean[16];
  private int[] previousSameName = new int[16];
  private int[] tableContext = new int[16];
  private int size;

  private final Map<String, Integer> topByName = new HashMap<>();
  private final int[] openHtml = new int[NAMES];
  private final int[] openInContent = new int[NAMES];
  private int regionRoot = -1;

  int size() {
    return size;
  }

  String name(int index) {
    return names[index];
  }

  @Nullable Name known(int index) {
    return known[index];
  }

  /** True when the element was pushed in a foreign region. */
  boolean foreign(int index) {
    return foreign[index];
  }

  /** Index of the {@code svg} or {@code math} element of the foreign region, or -1. */
  int regionRoot() {
    return regionRoot;
  }

  /** Marks the element at {@code index} as the root of a foreign region (M-05 step 4). */
  void startRegion(int index) {
    regionRoot = index;
  }

  /**
   * Pushes an element.
   *
   * @param name the tag name
   * @param k the known name, or null
   * @param inForeign true when pushed in a foreign region
   * @param inContent true when pushed in the content of an html block
   * @return the index of the new element
   */
  int push(String name, @Nullable Name k, boolean inForeign, boolean inContent) {
    if (size == names.length) {
      grow();
    }
    int i = size++;
    names[i] = name;
    known[i] = k;
    foreign[i] = inForeign;
    content[i] = inContent;
    Integer previous = topByName.put(name, i);
    previousSameName[i] = previous == null ? -1 : previous;
    boolean tableBoundary =
        !inForeign && k != null && (k.inAny(Name.Set.TABLE_FAMILY) || k == Name.TEMPLATE);
    tableContext[i] = tableBoundary ? i : (i == 0 ? -1 : tableContext[i - 1]);
    if (k != null && !inForeign) {
      openHtml[k.ordinal()]++;
      if (inContent) {
        openInContent[k.ordinal()]++;
      }
    }
    return i;
  }

  /** Index of the nearest element (from the top) with this name, or -1. */
  int nearest(String name) {
    Integer i = topByName.get(name);
    return i == null ? -1 : i;
  }

  /** Pops the elements at and above {@code index}. */
  void popTo(int index) {
    while (size > index) {
      pop();
    }
  }

  /** Pops the current node. */
  void pop() {
    int i = --size;
    String name = names[i];
    int previous = previousSameName[i];
    if (previous < 0) {
      topByName.remove(name);
    } else {
      topByName.put(name, previous);
    }
    Name k = known[i];
    if (k != null && !foreign[i]) {
      openHtml[k.ordinal()]--;
      if (content[i]) {
        openInContent[k.ordinal()]--;
      }
    }
    if (i == regionRoot) {
      regionRoot = -1;
    }
    names[i] = "";
    known[i] = null;
  }

  /** Number of open elements with this name that were pushed in HTML context. */
  int open(Name k) {
    return openHtml[k.ordinal()];
  }

  /** Number of open elements with this name that were pushed in the content of an html block. */
  int openInContent(Name k) {
    return openInContent[k.ordinal()];
  }

  /**
   * F of M-09: the nearest element with a name in {@code TABLE_FAMILY}; the search stops at a
   * {@code template} element. Null when there is none.
   */
  @Nullable Name tableFamily() {
    if (size == 0) {
      return null;
    }
    int i = tableContext[size - 1];
    if (i < 0) {
      return null;
    }
    Name k = known[i];
    return k == Name.TEMPLATE ? null : k;
  }

  private void grow() {
    int n = names.length * 2;
    names = Arrays.copyOf(names, n);
    known = Arrays.copyOf(known, n);
    foreign = Arrays.copyOf(foreign, n);
    content = Arrays.copyOf(content, n);
    previousSameName = Arrays.copyOf(previousSameName, n);
    tableContext = Arrays.copyOf(tableContext, n);
  }
}
