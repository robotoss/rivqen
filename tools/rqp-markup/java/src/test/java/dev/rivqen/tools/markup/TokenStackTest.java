// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/**
 * The fixed capacity of the token stack (M-26: size <= D <= 256). Rules are tested through the
 * analyzer.
 */
class TokenStackTest {

  @Test
  void capacityIsTheDepthLimit() {
    assertEquals(256, TokenStack.CAPACITY);
    assertEquals(Analyzer.MAX_OPEN_ELEMENTS, TokenStack.CAPACITY);
  }

  @Test
  void stackIsFullAtCapacity() {
    TokenStack s = new TokenStack();
    for (int i = 0; i < TokenStack.CAPACITY; i++) {
      assertFalse(s.full());
      assertEquals(i, s.push("b", Name.B, false, false));
    }
    assertTrue(s.full());
    assertEquals(TokenStack.CAPACITY - 1, s.nearest("b"));
  }

  @Test
  void pushOnAFullStackIsADefect() {
    TokenStack s = new TokenStack();
    for (int i = 0; i < TokenStack.CAPACITY; i++) {
      s.push("x", null, true, false);
    }
    assertThrows(IllegalStateException.class, () -> s.push("x", null, true, false));
    assertEquals(TokenStack.CAPACITY, s.size());
  }

  @Test
  void stackIsNotFullAfterAPop() {
    TokenStack s = new TokenStack();
    for (int i = 0; i < TokenStack.CAPACITY; i++) {
      s.push("b", Name.B, false, false);
    }
    s.pop();
    assertFalse(s.full());
    s.popTo(0);
    assertEquals(0, s.size());
    assertEquals(-1, s.nearest("b"));
  }
}
