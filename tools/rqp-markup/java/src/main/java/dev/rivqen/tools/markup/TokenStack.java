// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import java.util.HashMap;
import java.util.Map;
import org.jspecify.annotations.Nullable;

/**
 * The token stack of {@code markup.md} M-05, with constant-time queries.
 *
 * <p>Every query that a rule needs runs in O(1): the nearest element with a name (a chain of
 * same-name entries), the nearest table-family, list and list-item element (M-09, M-24 c and j; one
 * "nearest at or below" index per entry), the foreign region (M-05 step 4) and "is an element with
 * this name open" (counters). Each entry is pushed once and popped once, so the total work is
 * linear in the number of tokens.
 *
 * <p>Each entry also keeps the state of the depth counter D of M-26 (a dirty flag and its implied
 * parts) and the special-crossed mark of M-06 item 4. The rules that use this state are in {@link
 * Analyzer}.
 *
 * <p>The capacity is fixed at {@link #CAPACITY} entries. Every element on the stack holds at least
 * 1 of D (M-26), and the caller rejects the document before D becomes larger than 256, so the stack
 * never has more than 256 elements (H-25, H-31). A push on a full stack is a defect of the caller
 * and throws {@link IllegalStateException}. So the memory of the stack does not depend on the
 * input, and the name index has at most {@link #CAPACITY} entries.
 *
 * <p>The counters are arrays indexed by {@link Enum#ordinal()}, as in {@link java.util.EnumMap}:
 * the index never leaves this process and needs no boxing.
 */
@SuppressWarnings("EnumOrdinal")
final class TokenStack {
  /** M-26: the maximum number of elements ({@link Analyzer#MAX_OPEN_ELEMENTS}). */
  static final int CAPACITY = Analyzer.MAX_OPEN_ELEMENTS;

  private static final int NAMES = Name.values().length;

  /** M-26 implied part: a row group ({@code tbody}). */
  static final int ROW_GROUP = 1;

  /** M-26 implied part: a row ({@code tr}). */
  static final int ROW = 1 << 1;

  /** M-26 implied part: a column group ({@code colgroup}). */
  static final int COLUMN_GROUP = 1 << 2;

  private final String[] names = new String[CAPACITY];
  private final @Nullable Name[] known = new Name[CAPACITY];
  private final boolean[] foreign = new boolean[CAPACITY];
  private final boolean[] content = new boolean[CAPACITY];
  private final int[] previousSameName = new int[CAPACITY];
  private final int[] tableContext = new int[CAPACITY];
  private final int[] listContext = new int[CAPACITY];
  private final int[] listItemContext = new int[CAPACITY];
  private final boolean[] dirty = new boolean[CAPACITY];
  private final boolean[] marked = new boolean[CAPACITY];
  private final byte[] parts = new byte[CAPACITY];
  private int size;

  private final Map<String, Integer> topByName = new HashMap<>();
  private final int[] openHtml = new int[NAMES];
  private final int[] openInContent = new int[NAMES];
  private int regionRoot = -1;

  int size() {
    return size;
  }

  /** True when a push would make the stack larger than {@link #CAPACITY} elements (M-26). */
  boolean full() {
    return size >= CAPACITY;
  }

  /** True when the element at {@code index} was pushed in a foreign region. */
  boolean foreign(int index) {
    return foreign[index];
  }

  /** M-26: true when the element at {@code index} has the dirty mark. */
  boolean dirty(int index) {
    return dirty[index];
  }

  /** M-26: sets the dirty mark of the element at {@code index}. It stays until the pop. */
  void setDirty(int index) {
    dirty[index] = true;
  }

  /** M-06 item 4: true when the element at {@code index} has the special-crossed mark. */
  boolean marked(int index) {
    return marked[index];
  }

  /** M-06 item 4: sets the special-crossed mark of the element at {@code index}. */
  void mark(int index) {
    marked[index] = true;
  }

  /**
   * M-26 item 3: gives the implied parts {@code bits} to the element at {@code index}.
   *
   * @param bits a combination of {@link #ROW_GROUP}, {@link #ROW} and {@link #COLUMN_GROUP}
   * @return the number of parts that the element did not have before (each adds 1 to D)
   */
  int addParts(int index, int bits) {
    int added = bits & ~parts[index];
    parts[index] = (byte) (parts[index] | added);
    return Integer.bitCount(added);
  }

  /** M-26 item 4: the number of implied parts of the element at {@code index}. */
  int partCount(int index) {
    return Integer.bitCount(parts[index]);
  }

  /** True when the element at {@code index} was pushed in the content of an html block. */
  boolean inContent(int index) {
    return content[index];
  }

  String name(int index) {
    return names[index];
  }

  @Nullable Name known(int index) {
    return known[index];
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
   * Pushes an element. The caller checks {@link #full()} first (M-26).
   *
   * @param name the tag name
   * @param k the known name, or null
   * @param inForeign true when pushed in a foreign region
   * @param inContent true when pushed in the content of an html block
   * @return the index of the new element
   * @throws IllegalStateException when the stack is full (a defect of the caller)
   */
  int push(String name, @Nullable Name k, boolean inForeign, boolean inContent) {
    if (full()) {
      throw new IllegalStateException("token stack is full");
    }
    int i = size++;
    names[i] = name;
    known[i] = k;
    foreign[i] = inForeign;
    content[i] = inContent;
    dirty[i] = false;
    marked[i] = false;
    parts[i] = 0;
    Integer previous = topByName.put(name, i);
    previousSameName[i] = previous == null ? -1 : previous;
    chain(
        tableContext,
        i,
        !inForeign && (Name.isAny(k, Name.Set.TABLE_FAMILY) || k == Name.TEMPLATE));
    // List and list-item names are in BREAKOUT: M-07 rejects them in a foreign region before a
    // push, so every such entry was pushed in HTML context.
    chain(listContext, i, Name.is(k, Name.Set.LIST));
    chain(listItemContext, i, Name.is(k, Name.Set.LIST_ITEM));
    if (k != null && !inForeign) {
      openHtml[k.ordinal()]++;
      if (inContent) {
        openInContent[k.ordinal()]++;
      }
    }
    return i;
  }

  /** Sets {@code context[i]} to {@code i} on a hit, else to the value of the entry below. */
  private static void chain(int[] context, int i, boolean hit) {
    context[i] = hit ? i : (i == 0 ? -1 : context[i - 1]);
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
    int i = tableFamilyIndex();
    return i < 0 ? null : known[i];
  }

  /** Index of F of M-09 (see {@link #tableFamily()}), or -1 when there is none. */
  int tableFamilyIndex() {
    if (size == 0) {
      return -1;
    }
    int i = tableContext[size - 1];
    if (i < 0 || known[i] == Name.TEMPLATE) {
      return -1;
    }
    return i;
  }

  /**
   * Index of the nearest {@code ul}, {@code ol}, {@code menu} or {@code dl} element pushed in HTML
   * context (M-24 c, j), or -1.
   */
  int nearestList() {
    return size == 0 ? -1 : listContext[size - 1];
  }

  /**
   * Index of the nearest {@code li}, {@code dd} or {@code dt} element pushed in HTML context (M-24
   * j), or -1.
   */
  int nearestListItem() {
    return size == 0 ? -1 : listItemContext[size - 1];
  }
}
