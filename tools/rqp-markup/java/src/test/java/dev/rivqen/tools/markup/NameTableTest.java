// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertSame;

import org.junit.jupiter.api.Test;

/** Interned tag names of one document (security review F-2). */
class NameTableTest {

  @Test
  void knownNameIsTheConstantOfTheEnum() {
    NameTable t = new NameTable();
    assertSame(Name.DIV.tag, t.intern(new String("div"), Name.DIV));
    assertEquals(0, t.size());
  }

  @Test
  void equalUnknownNamesShareOneString() {
    NameTable t = new NameTable();
    String first = t.intern(new String("x-y"), null);
    assertSame(first, t.intern(new String("x-y"), null));
    assertEquals(1, t.size());
  }

  @Test
  void tableKeepsAtMostItsCapacity() {
    NameTable t = new NameTable();
    String[] kept = new String[NameTable.CAPACITY];
    for (int i = 0; i < NameTable.CAPACITY; i++) {
      kept[i] = t.intern("e" + i, null);
    }
    assertEquals(NameTable.CAPACITY, t.size());
    // A new name after the table is full is returned as it is and not kept.
    String late = new String("late");
    assertSame(late, t.intern(late, null));
    assertEquals(NameTable.CAPACITY, t.size());
    // The names in the table stay interned.
    assertSame(kept[0], t.intern(new String("e0"), null));
    assertSame(kept[NameTable.CAPACITY - 1], t.intern("e" + (NameTable.CAPACITY - 1), null));
  }
}
