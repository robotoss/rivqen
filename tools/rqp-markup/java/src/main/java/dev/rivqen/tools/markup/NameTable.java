// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import java.util.HashMap;
import java.util.Map;
import org.jspecify.annotations.Nullable;

/**
 * Tag names of one document, interned (security review F-2). A known name is the constant of {@link
 * Name}. An unknown name is one string for the document while the table has room for it, so the
 * stacks keep references to shared strings and not one string for each tag.
 *
 * <p>The table has at most {@link #CAPACITY} names. A later new name is returned as the tokenizer
 * gave it and is not kept: the table never holds the names of a whole 5 MiB input. The keys are tag
 * names from the input; {@link HashMap} keeps a large bucket of {@link String} keys as a tree, so
 * names with one hash code cost O(log {@link #CAPACITY}) comparisons.
 */
final class NameTable {
  /** The maximum number of unknown names in the table. */
  static final int CAPACITY = 1024;

  private final Map<String, String> names = new HashMap<>();

  /**
   * Returns the string to keep for a tag name.
   *
   * @param raw the name from the tokenizer
   * @param k the known name for {@code raw}, or null
   * @return {@code k.tag}, the interned string equal to {@code raw}, or {@code raw}
   */
  String intern(String raw, @Nullable Name k) {
    if (k != null) {
      return k.tag;
    }
    String interned = names.get(raw);
    if (interned != null) {
      return interned;
    }
    if (names.size() < CAPACITY) {
      names.put(raw, raw);
    }
    return raw;
  }

  /** The number of unknown names in the table. */
  int size() {
    return names.size();
  }
}
