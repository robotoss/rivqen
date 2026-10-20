// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;
import java.util.Random;
import org.junit.jupiter.api.Test;

/**
 * H-85a property (markup.md M-09, "Implied closure"): the token stack reads a document D as it
 * reads D', the same document with the omitted end tags written. D' comes from a random table tree
 * with every end tag; D leaves out a whole run of end tags when the closure of the next table part
 * start tag supplies exactly that run.
 */
class ImpliedClosurePropertyTest {
  private static final String P = "<p data-rq-block=\"a\">x</p>";

  /** A token of the tree: a start tag, an end tag of a table-family element, or other markup. */
  private record Token(String text, String end, String start) {
    static Token start(String name) {
      return new Token("<" + name + ">", null, name);
    }

    static Token end(String name) {
      return new Token("</" + name + ">", name, null);
    }

    static Token other(String text) {
      return new Token(text, null, null);
    }

    boolean closableEnd() {
      return end != null && !end.equals("table");
    }
  }

  private static void table(Random r, List<Token> out, int depth) {
    out.add(Token.start("table"));
    if (r.nextInt(4) == 0) {
      out.add(Token.start("caption"));
      out.add(Token.other("c"));
      out.add(Token.end("caption"));
    }
    if (r.nextInt(4) == 0) {
      out.add(Token.start("colgroup"));
      for (int i = r.nextInt(3); i > 0; i--) {
        out.add(Token.start("col"));
      }
      out.add(Token.end("colgroup"));
    }
    for (int g = r.nextInt(4); g > 0; g--) {
      String group = new String[] {"tbody", "thead", "tfoot", "tr"}[r.nextInt(4)];
      if (group.equals("tr")) {
        row(r, out, depth);
        continue;
      }
      out.add(Token.start(group));
      for (int i = r.nextInt(3); i > 0; i--) {
        row(r, out, depth);
      }
      out.add(Token.end(group));
    }
    out.add(Token.end("table"));
  }

  private static void row(Random r, List<Token> out, int depth) {
    out.add(Token.start("tr"));
    for (int c = r.nextInt(4); c > 0; c--) {
      String cell = r.nextBoolean() ? "td" : "th";
      out.add(Token.start(cell));
      switch (r.nextInt(depth < 2 ? 4 : 3)) {
        case 0 -> out.add(Token.other("t"));
        case 1 -> out.add(Token.other("<div>x</div>"));
        case 2 -> out.add(Token.other("<b>y</b>"));
        default -> table(r, out, depth + 1);
      }
      out.add(Token.end(cell));
    }
    out.add(Token.end("tr"));
  }

  /** Whether F allows the start tag (the M-09 table). */
  private static boolean allows(String start, String f) {
    return switch (start) {
      case "caption", "colgroup", "tbody", "thead", "tfoot" -> f.equals("table");
      case "col" -> f.equals("table") || f.equals("colgroup");
      case "tr" -> List.of("table", "tbody", "thead", "tfoot").contains(f);
      default -> List.of("table", "tbody", "thead", "tfoot", "tr").contains(f);
    };
  }

  private static boolean tablePart(String name) {
    return List.of("caption", "colgroup", "col", "tbody", "thead", "tfoot", "tr", "td", "th")
        .contains(name);
  }

  /** D: drops a run of end tags that the closure of the start tag after it supplies exactly. */
  private static String omit(List<Token> tokens, Random r) {
    StringBuilder d = new StringBuilder();
    int i = 0;
    while (i < tokens.size()) {
      int j = i;
      while (j < tokens.size() && tokens.get(j).closableEnd()) {
        j++;
      }
      if (j > i && j < tokens.size() && j - i <= 3) {
        Token next = tokens.get(j);
        if (next.start() != null && tablePart(next.start()) && r.nextBoolean()) {
          int k = i;
          while (k < j && !allows(next.start(), tokens.get(k).end())) {
            k++;
          }
          if (k == j) {
            i = j; // the closure closes the whole run
            continue;
          }
        }
      }
      d.append(tokens.get(i).text());
      i++;
    }
    return d.toString();
  }

  private static String page(String table) {
    return "<!DOCTYPE html><html><head><title>T</title></head><body>"
        + table
        + P
        + "</body></html>";
  }

  @Test
  void omittingTheEndTagsOfTheClosureKeepsTheResult() {
    Random r = new Random(85L);
    int shorter = 0;
    for (int n = 0; n < 4_000; n++) {
      List<Token> tokens = new ArrayList<>();
      table(r, tokens, 0);
      String explicit = tokens.stream().map(Token::text).reduce("", String::concat);
      String implied = omit(tokens, r);
      Result before = Docs.analyze(page(explicit));
      assertNull(before.error(), () -> before.rule() + ": " + explicit);
      Result after = Docs.analyze(page(implied));
      assertNull(after.error(), () -> after.rule() + ": " + implied + " from " + explicit);
      assertEquals(Docs.blocks(page(explicit)), Docs.blocks(page(implied)));
      if (implied.length() < explicit.length()) {
        shorter++;
      }
    }
    assertTrue(shorter > 1_000, "documents with an omitted end tag: " + shorter);
  }
}
