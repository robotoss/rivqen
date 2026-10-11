// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Base64;
import java.util.List;
import java.util.Map;
import java.util.Random;
import java.util.TreeMap;
import java.util.regex.Pattern;
import java.util.stream.Stream;
import org.junit.jupiter.api.Test;

/**
 * Properties on generated input, with fixed seeds (own generator, no extra dependency):
 *
 * <ul>
 *   <li>no exception escapes {@link Analyzer#analyze} for any bytes;
 *   <li>a valid result has ordered, non-overlapping blocks inside the input, and each sha256 and
 *       both revisions equal an independent computation from {@code CONTRACT.md} §4;
 *   <li>replacing the content of every block with plain text keeps the document valid and keeps the
 *       template revision (the template excludes content).
 * </ul>
 */
class PropertyTest {
  private static final Pattern ID = Pattern.compile("^[a-z0-9][a-z0-9_-]{0,63}$");

  /** Fragments that reach many tokenizer states and rules. */
  private static final List<String> FRAGMENTS = fragments();

  private static List<String> fragments() {
    List<String> f = new ArrayList<>();
    for (Name n : Name.values()) {
      f.add("<" + n.tag + ">");
      f.add("</" + n.tag + ">");
      f.add("<" + n.tag + "/>");
    }
    f.addAll(
        List.of(
            "<div data-rq-block=a>",
            "<p data-rq-block=\"b\">",
            "<span data-rq-block='c'>",
            "<title data-rq-block=t>",
            "<script type=application/json data-rq-block=j>",
            "<h1 data-rq-block=h>",
            "<section data-rq-block=s DATA-RQ-BLOCK=x>",
            "<p data-rq-block=a/>",
            "<p data-rq-block>",
            "<p data-rq-block=\"A\">",
            "<script type=\"rivqen-manifest\">",
            "<script type='&amp;'>",
            "<!--",
            "-->",
            "--!>",
            "<!---->",
            "<!-->",
            "<!DOCTYPE html>",
            "<![CDATA[",
            "]]>",
            "<?pi>",
            "<!x>",
            "</ >",
            "</>",
            "<",
            ">",
            "/",
            "=",
            "\"",
            "'",
            "&",
            "&lt;",
            "-",
            "--",
            "!",
            " ",
            "\n",
            "\r\n",
            "x",
            "{",
            "}",
            "[",
            "]",
            "1",
            "\"k\"",
            ":",
            ",",
            "true",
            "\\u00e9",
            "é",
            "😀",
            "\0",
            "<foreignObject>",
            "</foreignObject>",
            "<svg><title>",
            "<math><mi>",
            "<a href=x>",
            "<td>",
            "<tr>",
            "<table>",
            "<!--<script>",
            "</script>",
            "<custom-x>",
            "</custom-x>",
            Docs.BOM,
            "<noscript><!--",
            "<template>",
            "<select><option>"));
    return List.copyOf(f);
  }

  private static byte[] soup(Random r, int maxFragments) {
    StringBuilder s = new StringBuilder();
    int n = r.nextInt(maxFragments);
    for (int i = 0; i < n; i++) {
      s.append(FRAGMENTS.get(r.nextInt(FRAGMENTS.size())));
    }
    return s.toString().getBytes(StandardCharsets.UTF_8);
  }

  private static byte[] randomBytes(Random r) {
    byte[] b = new byte[r.nextInt(300)];
    if (r.nextBoolean()) {
      r.nextBytes(b);
    } else {
      byte[] alphabet = Docs.utf8("<>/!-=\"' ab?[]CDATA&;\né");
      for (int i = 0; i < b.length; i++) {
        b[i] = alphabet[r.nextInt(alphabet.length)];
      }
    }
    return b;
  }

  @Test
  void arbitraryBytesNeverThrow() {
    Random r = new Random(20261009L);
    for (int i = 0; i < 20_000; i++) {
      byte[] in = randomBytes(r);
      checkInvariants(in, Analyzer.analyze(in));
    }
  }

  @Test
  void tokenSoupNeverThrows() {
    Random r = new Random(17L);
    for (int i = 0; i < 30_000; i++) {
      byte[] in = soup(r, 40);
      Result res = Analyzer.analyze(in);
      checkInvariants(in, res);
      if (res.valid() && !res.blocks().isEmpty()) {
        checkContentReplacement(in, res);
      }
    }
  }

  @Test
  void generatedDocumentsAreValidAndConsistent() {
    Random r = new Random(4242L);
    for (int i = 0; i < 5_000; i++) {
      byte[] in = Docs.utf8(new DocumentGenerator(r).document());
      Result res = Analyzer.analyze(in);
      assertEquals(
          null, res.error(), () -> res.rule() + ": " + new String(in, StandardCharsets.UTF_8));
      checkInvariants(in, res);
      checkContentReplacement(in, res);
    }
  }

  @Test
  void editedGeneratedDocumentsNeverThrow() {
    Random r = new Random(77L);
    int validWithBlocks = 0;
    for (int i = 0; i < 20_000; i++) {
      byte[] in = edit(Docs.utf8(new DocumentGenerator(r).document()), r);
      Result res = Analyzer.analyze(in);
      checkInvariants(in, res);
      if (res.valid() && !res.blocks().isEmpty()) {
        validWithBlocks++;
        checkContentReplacement(in, res);
      }
    }
    // The edits must leave many documents valid, else the property says little.
    assertTrue(validWithBlocks > 2_000, "valid with blocks " + validWithBlocks);
  }

  @Test
  void editedFixturesNeverThrow() throws IOException {
    Random r = new Random(99L);
    try (Stream<Path> dirs = Files.list(FixturesTest.fixturesDir())) {
      for (Path d : dirs.sorted().toList()) {
        Path p = d.resolve("input.html");
        if (!Files.exists(p) || Files.size(p) > 100_000) {
          continue;
        }
        byte[] base = Files.readAllBytes(p);
        Result original = Analyzer.analyze(base);
        if (original.valid()) {
          checkContentReplacement(base, original);
        }
        for (int i = 0; i < 200; i++) {
          byte[] in = edit(base, r);
          checkInvariants(in, Analyzer.analyze(in));
        }
      }
    }
  }

  /**
   * M-26 against a model stack. Each document is a random walk around the depth limit: divs opened
   * before, then HTML context, block content or a foreign region with start tags that M-05 pushes,
   * matching end tags, and tokens that M-05 does not push (void elements, self-closing svg, math
   * and foreign elements, text, comments). The document is valid when the model stack never has
   * more than 256 elements; else it is RQP_MARKUP_LIMIT with rule M-26.
   */
  @Test
  void depthLimitMatchesAModelStack() {
    Random r = new Random(256L);
    int above = 0;
    int atLimit = 0;
    for (int i = 0; i < 3_000; i++) {
      int context = r.nextInt(3); // 0: HTML context, 1: block content, 2: foreign region
      List<String> pushed =
          List.of(
                  List.of("b", "i", "span/", "div"),
                  List.of("b", "i", "div"),
                  List.of("g", "text", "mrow"))
              .get(context);
      List<String> notPushed =
          List.of(
                  List.of("<br>", "<img src=x>", "<svg/>", "<math />", "x", "<!--c-->"),
                  List.of("<br>", "<img src=x>", "<wbr>", "x", "<!--c-->"),
                  List.of("<rect/>", "<g/>", "x", "<!--c-->"))
              .get(context);
      StringBuilder s = new StringBuilder();
      ArrayDeque<String> stack = new ArrayDeque<>();
      // Half of the walks start near the limit, so that a maximum of exactly 256 is frequent.
      int prefix =
          r.nextBoolean()
              ? r.nextInt(Analyzer.MAX_OPEN_ELEMENTS)
              : Analyzer.MAX_OPEN_ELEMENTS - 1 - r.nextInt(64);
      for (int j = 0; j < prefix; j++) {
        s.append("<div>");
        stack.push("div");
      }
      if (context == 1) {
        s.append("<div data-rq-block=a>");
        stack.push("div");
      } else if (context == 2) {
        s.append("<svg>");
        stack.push("svg");
      }
      int base = stack.size();
      int max = base;
      // A capped walk never pushes on a full model stack, so it often stays exactly at the limit.
      boolean capped = r.nextInt(3) == 0;
      int steps = 100 + r.nextInt(400);
      for (int j = 0; j < steps; j++) {
        int k = r.nextInt(20);
        if (k < 8 && !(capped && stack.size() >= Analyzer.MAX_OPEN_ELEMENTS)) {
          String name = pushed.get(r.nextInt(pushed.size()));
          boolean selfClosing = name.endsWith("/");
          String tag = selfClosing ? name.substring(0, name.length() - 1) : name;
          s.append('<').append(tag).append(selfClosing ? "/>" : ">");
          stack.push(tag);
        } else if (k < 15 && stack.size() > base) {
          s.append("</").append(stack.pop()).append('>');
        } else {
          s.append(notPushed.get(r.nextInt(notPushed.size())));
        }
        max = Math.max(max, stack.size());
      }
      while (!stack.isEmpty()) {
        s.append("</").append(stack.pop()).append('>');
      }
      byte[] in = Docs.utf8(s.toString());
      Result res = Analyzer.analyze(in);
      int depth = max;
      if (max > Analyzer.MAX_OPEN_ELEMENTS) {
        above++;
        assertEquals(ErrorCode.LIMIT, res.error(), () -> res.rule() + " at max depth " + depth);
        assertEquals("M-26", res.rule());
      } else {
        atLimit += max == Analyzer.MAX_OPEN_ELEMENTS ? 1 : 0;
        assertEquals(null, res.error(), () -> res.rule() + " at max depth " + depth);
        assertEquals(context == 1 ? 1 : 0, res.blocks().size());
      }
      checkInvariants(in, res);
    }
    // Both sides of the limit and the limit itself must occur often, else the property says little.
    assertTrue(above > 300, "above the limit " + above);
    assertTrue(atLimit > 100, "at the limit " + atLimit);
  }

  /**
   * M-06 items 1 to 4, M-09 conditions 1 and 2 (H-35, H-37), M-10 item 2 (H-36) and M-26 (D with
   * the dirty mark and implied parts, H-31) against {@link StackModel}, a plain implementation from
   * the rule text. Each document is a random walk with crossings, exact pops, stray end tags, table
   * parts, templates and forms in HTML context, block content (exact pops only) or a foreign
   * region. Divs opened before put D near the limit in half of the walks. The walk stops at the
   * first error of the model; the analyzer must report the same rule, or the document is valid.
   */
  @Test
  void depthAndMarksMatchAModel() {
    Random r = new Random(32L);
    Map<String, Integer> byItem = new TreeMap<>(); // "M-26", "M-06 item 1", …
    int valid = 0;
    int validNearLimit = 0;
    for (int doc = 0; doc < 12_000; doc++) {
      StackModel m = new StackModel();
      StringBuilder s = new StringBuilder();
      int prefix = r.nextBoolean() ? r.nextInt(32) : 192 + r.nextInt(64);
      for (int j = 0; j < prefix && m.error() == null; j++) {
        s.append("<div>");
        m.start("div", false, true);
      }
      int context = r.nextInt(3);
      int blocks = 0;
      if (context == 0) {
        htmlWalk(r, m, s);
      } else if (context == 1) {
        blocks = blockWalk(r, m, s);
      } else {
        foreignWalk(r, m, s);
      }
      byte[] in = Docs.utf8(s.toString());
      Result res = Analyzer.analyze(in);
      String expected = m.error();
      if (expected == null) {
        valid++;
        validNearLimit += m.maxDepth() >= StackModel.LIMIT - 8 ? 1 : 0;
        assertEquals(null, res.error(), () -> res.rule() + " for " + s);
        assertEquals(blocks, res.blocks().size());
      } else {
        byItem.merge(expected + (m.item() > 0 ? " item " + m.item() : ""), 1, Integer::sum);
        ErrorCode code = expected.equals("M-26") ? ErrorCode.LIMIT : ErrorCode.STRUCTURE;
        assertEquals(code, res.error(), () -> "item " + m.item() + " for " + s);
        assertEquals(expected, res.rule());
      }
      checkInvariants(in, res);
    }
    // Each outcome must occur often, else the property says little.
    assertTrue(valid > 1_000, "valid " + valid);
    assertTrue(validNearLimit > 200, "valid near the limit " + validNearLimit);
    List<String> outcomes =
        List.of( // in the order of the keys of byItem
            "M-06 item 1",
            "M-06 item 2",
            "M-06 item 3",
            "M-06 item 4",
            "M-09 item 1",
            "M-09 item 2",
            "M-10",
            "M-26");
    assertEquals(outcomes, List.copyOf(byItem.keySet()), byItem::toString);
    for (String outcome : outcomes) {
      assertTrue(byItem.get(outcome) > 100, byItem::toString);
    }
  }

  /**
   * Start tags of the HTML walk. {@code template}, {@code form} and {@code col} are there twice:
   * M-09 condition 1 and M-10 item 2 need two of these tags in one walk.
   */
  private static final List<String> HTML_NAMES =
      List.of(
          ("b i em a span q div p ul li body html table tbody tr td col br colgroup template form"
                  + " template form col")
              .split(" "));

  private static final List<String> STRAY_NAMES =
      List.of("x", "td", "tbody", "tr", "table", "i", "b");

  /** An end tag for the element {@code offset} below the current node (0: an exact pop). */
  private static int endOffset(Random r) {
    int k = r.nextInt(10);
    return k < 6 ? 0 : 1 + r.nextInt(k < 9 ? 2 : 6);
  }

  /**
   * HTML context: crossings, table parts (only where the table of M-09 allows them; conditions 1
   * and 2 are left to the model), templates, forms, html and body.
   */
  private static void htmlWalk(Random r, StackModel m, StringBuilder s) {
    int steps = 50 + r.nextInt(250);
    for (int j = 0; j < steps && m.error() == null; j++) {
      int k = r.nextInt(100);
      if (k < 45) {
        String name = HTML_NAMES.get(r.nextInt(HTML_NAMES.size()));
        String f = m.tableFamily();
        boolean allowed =
            switch (name) {
              case "tbody", "colgroup" -> f == null || f.equals("table");
              case "tr" -> f == null || f.equals("table") || f.equals("tbody");
              case "td" -> f == null || f.equals("table") || f.equals("tbody") || f.equals("tr");
              case "col" -> f == null || f.equals("table") || f.equals("colgroup");
              default -> true;
            };
        if (allowed) {
          s.append('<').append(name).append('>');
          m.start(name, false, !name.equals("col") && !name.equals("br"));
        } else {
          s.append('x');
        }
      } else if (k < 90 && m.size() > 0) {
        int index = Math.max(0, m.size() - 1 - endOffset(r));
        String name = m.name(index);
        s.append("</").append(name).append('>');
        m.end(name);
      } else if (k < 95) {
        String name = STRAY_NAMES.get(r.nextInt(STRAY_NAMES.size()));
        s.append("</").append(name).append('>');
        m.end(name);
      } else {
        s.append(r.nextBoolean() ? "x" : "<!--c-->");
      }
    }
  }

  /**
   * Block content: elements that leave units before the block, then exact pushes and pops in the
   * content (M-23), then the block end tag and the end tags of the elements around the block.
   *
   * @return the number of blocks of a valid document
   */
  private static int blockWalk(Random r, StackModel m, StringBuilder s) {
    int crossings = r.nextInt(4);
    for (int j = 0; j < crossings && m.error() == null; j++) {
      s.append("<span><div></span>");
      m.start("span", false, true);
      m.start("div", false, true);
      m.end("span");
    }
    s.append("<div data-rq-block=a>");
    m.start("div", false, true);
    int block = m.size() - 1;
    List<String> names = List.of("b", "i", "span", "q", "div", "p");
    int steps = 20 + r.nextInt(300);
    for (int j = 0; j < steps && m.error() == null; j++) {
      int k = r.nextInt(100);
      if (k < 45) {
        String name = names.get(r.nextInt(names.size()));
        s.append('<').append(name).append('>');
        m.start(name, false, true);
      } else if (k < 90 && m.size() - 1 > block) {
        String name = m.name(m.size() - 1);
        s.append("</").append(name).append('>');
        m.end(name);
      } else {
        s.append(r.nextBoolean() ? "<br>" : "x");
        m.start("br", false, false);
      }
    }
    while (m.error() == null && m.size() > 0) {
      String name = m.name(m.size() - 1);
      s.append("</").append(name).append('>');
      m.end(name);
    }
    return 1;
  }

  /**
   * A foreign region: crossings of foreign elements (also by the formatting end tag {@code </a>}),
   * self-closing elements, sometimes a region end that crosses. A {@code b} below the region shows
   * the mark that passes down to it.
   */
  private static void foreignWalk(Random r, StackModel m, StringBuilder s) {
    boolean b = r.nextBoolean();
    if (b) {
      s.append("<b>");
      m.start("b", false, true);
    }
    s.append("<svg>");
    m.start("svg", false, true);
    List<String> names = List.of("g", "a", "path", "tspan");
    int steps = 20 + r.nextInt(200);
    for (int j = 0; j < steps && m.error() == null && m.regionRoot() >= 0; j++) {
      int root = m.regionRoot();
      int k = r.nextInt(100);
      if (k < 40) {
        String name = names.get(r.nextInt(names.size()));
        boolean selfClosing = r.nextInt(6) == 0;
        s.append('<').append(name).append(selfClosing ? "/>" : ">");
        m.start(name, true, !selfClosing);
      } else if (k < 85 && m.size() - 1 > root) {
        int index = Math.max(root + 1, m.size() - 1 - endOffset(r));
        String name = m.name(index);
        s.append("</").append(name).append('>');
        m.end(name);
      } else if (k < 88) {
        s.append("</svg>");
        m.end("svg");
      } else {
        s.append('x');
      }
    }
    // Sometimes the input ends in the region (E2); else close everything from the top.
    if (r.nextInt(4) > 0) {
      while (m.error() == null && m.size() > 0) {
        String name = m.name(m.size() - 1);
        s.append("</").append(name).append('>');
        m.end(name);
      }
    }
  }

  @Test
  void resultIsDeterministic() {
    Random r = new Random(5L);
    for (int i = 0; i < 500; i++) {
      byte[] in = soup(r, 30);
      assertEquals(Analyzer.analyze(in), Analyzer.analyze(in));
    }
  }

  private static byte[] edit(byte[] base, Random r) {
    ByteArrayOutputStream out = new ByteArrayOutputStream();
    int at = base.length == 0 ? 0 : r.nextInt(base.length);
    int cut = Math.min(base.length - at, r.nextInt(8));
    out.write(base, 0, at);
    out.writeBytes(soup(r, 4));
    out.write(base, at + cut, base.length - at - cut);
    return out.toByteArray();
  }

  /** Invariants of every result (CONTRACT.md §3, §4). */
  static void checkInvariants(byte[] in, Result r) {
    assertNotNull(r);
    assertFalse(r.internalError(), r::rule);
    if (!r.valid()) {
      assertNotNull(r.rule());
      assertTrue(r.blocks().isEmpty());
      assertNull(r.templateRevision());
      assertNull(r.pageRevision());
      return;
    }
    int previousEnd = 0;
    ByteArrayOutputStream template = new ByteArrayOutputStream();
    StringBuilder page = new StringBuilder();
    List<String> ids = new ArrayList<>();
    for (Result.Block b : r.blocks()) {
      assertTrue(b.start() >= previousEnd && b.start() <= b.end() && b.end() <= in.length);
      assertTrue(ID.matcher(b.id()).matches(), b.id());
      assertTrue(!ids.contains(b.id()));
      ids.add(b.id());
      assertTrue(b.format().equals("html") || b.format().equals("json"));
      byte[] content = Arrays.copyOfRange(in, b.start(), b.end());
      assertEquals(b64(sha256(content)), b.sha256());
      template.write(in, previousEnd, b.start() - previousEnd);
      previousEnd = b.end();
      page.append(b.id()).append('\t').append(b.format()).append('\t').append(b.sha256());
      page.append('\n');
    }
    assertTrue(r.blocks().size() <= Analyzer.MAX_BLOCKS);
    template.write(in, previousEnd, in.length - previousEnd);
    String t = "t1." + b64(sha256(concat(Docs.utf8("rqp-t1\n"), template.toByteArray())));
    assertEquals(t, r.templateRevision());
    String p = "r1." + b64(sha256(Docs.utf8("rqp-r1\n" + t + "\n" + page)));
    assertEquals(p, r.pageRevision());
  }

  /** Replace each block content with plain text: still valid, same blocks, same template. */
  private static void checkContentReplacement(byte[] in, Result r) {
    ByteArrayOutputStream out = new ByteArrayOutputStream();
    int at = 0;
    for (Result.Block b : r.blocks()) {
      out.write(in, at, b.start() - at);
      out.writeBytes(Docs.utf8(b.format().equals("json") ? "[0]" : "plain text"));
      at = b.end();
    }
    out.write(in, at, in.length - at);
    byte[] replaced = out.toByteArray();
    Result r2 = Analyzer.analyze(replaced);
    assertEquals(null, r2.error(), () -> "after replacement: " + r2.rule());
    assertEquals(r.templateRevision(), r2.templateRevision());
    assertArrayEquals(
        r.blocks().stream().map(Result.Block::id).toArray(),
        r2.blocks().stream().map(Result.Block::id).toArray());
  }

  private static byte[] concat(byte[] a, byte[] b) {
    byte[] c = Arrays.copyOf(a, a.length + b.length);
    System.arraycopy(b, 0, c, a.length, b.length);
    return c;
  }

  private static byte[] sha256(byte[] b) {
    try {
      return MessageDigest.getInstance("SHA-256").digest(b);
    } catch (NoSuchAlgorithmException e) {
      throw new AssertionError(e);
    }
  }

  private static String b64(byte[] b) {
    return Base64.getUrlEncoder().withoutPadding().encodeToString(b);
  }
}
