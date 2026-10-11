// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTimeoutPreemptively;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.charset.StandardCharsets;
import java.time.Duration;
import java.util.stream.Stream;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.Arguments;
import org.junit.jupiter.params.provider.MethodSource;

/**
 * Worst-case inputs of almost 5 MiB. A quadratic step would take hours on these inputs; the linear
 * algorithm takes well under a second. The time bound is generous so that the test does not depend
 * on the machine.
 */
class LinearTimeTest {
  private static final int SIZE = Analyzer.MAX_INPUT_BYTES - 64;
  private static final int MAX = Analyzer.MAX_OPEN_ELEMENTS;

  /** {@code head} + {@code unit} repeated to almost 5 MiB + {@code tail}. */
  private static byte[] repeat(String head, String unit, String tail) {
    int n = (SIZE - head.length() - tail.length()) / unit.length();
    String s = head + unit.repeat(n) + tail;
    byte[] b = s.getBytes(StandardCharsets.UTF_8);
    assertTrue(b.length <= Analyzer.MAX_INPUT_BYTES);
    return b;
  }

  /** {@code tag} start tags, {@code n} times. */
  private static String open(String tag, int n) {
    return ("<" + tag + ">").repeat(n);
  }

  /** {@code tag} end tags, {@code n} times. */
  private static String close(String tag, int n) {
    return ("</" + tag + ">").repeat(n);
  }

  static Stream<Arguments> inputs() {
    return Stream.of(
        Arguments.of("deep nesting", repeat("", "<div>", ""), ErrorCode.LIMIT),
        Arguments.of("unique names", uniqueNames(), ErrorCode.LIMIT),
        Arguments.of("many attributes", repeat("<p ", "a=1 ", ">"), null),
        Arguments.of("long attribute value", repeat("<p title=\"", "x", "\">"), null),
        Arguments.of("long tag name", repeat("<p", "x", ">"), null),
        Arguments.of("comment dashes", repeat("<!--", "-", ""), null),
        Arguments.of("comment end bang", repeat("<!--", "--!x", "-->"), null),
        Arguments.of("rcdata near misses", repeat("<title>", "</titlex", "</title>"), null),
        Arguments.of(
            "script escapes", repeat("<script>", "<!--<script>--></scrip", "</script>"), null),
        Arguments.of(
            "script double escape",
            repeat("<script><!--", "<script></script>", "--></script>"),
            null),
        Arguments.of("noscript content", repeat("<noscript>", "<img src=x>", "</noscript>"), null),
        Arguments.of("svg region", repeat("<svg>", "<g>", "</svg>"), ErrorCode.LIMIT),
        Arguments.of(
            "table rows",
            repeat("<table><tbody>", "<tr><td>x</td></tr>", "</tbody></table>"),
            null),
        Arguments.of(
            "many blocks then duplicate",
            repeat("", "<p data-rq-block=a>x</p>", ""),
            ErrorCode.DUPLICATE),
        Arguments.of(
            "big block", repeat("<div data-rq-block=a>", "<b>x</b>", "</div>"), ErrorCode.LIMIT),
        Arguments.of("big content in many blocks", manyBlocks(), null),
        Arguments.of("big json", jsonBlock(), null),
        Arguments.of("bogus comments", repeat("", "<?x>", ""), null),
        Arguments.of("cdata", repeat("<svg>", "<![CDATA[x]]>", "</svg>"), null));
  }

  private static byte[] uniqueNames() {
    StringBuilder s = new StringBuilder();
    for (int i = 0; s.length() < SIZE - 16; i++) {
      s.append("<e").append(Integer.toString(i, 36)).append('>');
    }
    return s.toString().getBytes(StandardCharsets.US_ASCII);
  }

  private static byte[] manyBlocks() {
    StringBuilder s = new StringBuilder();
    String filler = "<b>y</b>".repeat((SIZE / 256 - 40) / 8);
    for (int i = 0; i < 256; i++) {
      s.append("<div data-rq-block=b").append(i).append('>').append(filler).append("</div>");
    }
    return s.toString().getBytes(StandardCharsets.US_ASCII);
  }

  private static byte[] jsonBlock() {
    String head = "<script type=application/json data-rq-block=j>[";
    String body =
        "[[{\"a\":-1.5e3,\"b\":\"\\u00e9\"}]],".repeat((Analyzer.MAX_BLOCK_BYTES - 16) / 28);
    return (head + body + "0]</script>").getBytes(StandardCharsets.US_ASCII);
  }

  /**
   * Valid inputs that keep the token stack at or near its limit of 256 elements (M-26) for the
   * whole input, and pop it with one end tag at a time or with one end tag for many elements. A
   * push, a pop or a stack query that costs time in proportion to the stack size (an array shift, a
   * scan or a rebuild of the name index) makes these inputs about 256 times slower; a cost in
   * proportion to the number of names seen so far makes them quadratic. The block content cases
   * also do this for the end tags in content (M-23) and for the list queries of M-24 c and j. The
   * expected block count makes sure that the analysis reads the whole input and does not stop
   * early.
   */
  static Stream<Arguments> deepStacks() {
    return Stream.of(
        Arguments.of(
            "full stack closed by matching end tags",
            repeat("", open("b", MAX) + close("b", MAX), ""),
            0),
        Arguments.of(
            "mixed names closed by matching end tags",
            repeat("", "<b><i>".repeat(MAX / 2) + "</i></b>".repeat(MAX / 2), ""),
            0),
        Arguments.of(
            "pushes and pops on a full stack", repeat(open("div", MAX - 1), "<b></b>", ""), 0),
        Arguments.of("stray end tags over a full stack", repeat(open("div", MAX), "</x>", ""), 0),
        Arguments.of(
            "one end tag pops a full stack",
            repeat("", "<div>" + open("b", MAX - 1) + "</div>", ""),
            0),
        Arguments.of(
            "nested lists in block content",
            blocks("", 5, "<ul><li>".repeat(127) + "</li></ul>".repeat(127)),
            5),
        Arguments.of(
            "nested elements in block content closed by matching end tags",
            blocks("", 5, open("b", MAX - 1) + close("b", MAX - 1)),
            5),
        Arguments.of(
            "nested lists in block content over lists opened before the block",
            blocks("<ul><li>".repeat(64), 5, "<ul><li>".repeat(63) + "</li></ul>".repeat(63)),
            5),
        Arguments.of(
            "svg region at the depth limit",
            repeat("<svg>", open("g", MAX - 1) + close("g", MAX - 1), "</svg>"),
            0),
        Arguments.of("unique names within the depth limit", uniqueNamesInGroups(MAX), 0),
        Arguments.of("names with one hash code within the depth limit", collidingNames(MAX), 0));
  }

  /**
   * Deep inputs of the same shapes without the limit: they push more than 256 elements. The
   * analysis stops at the 257th push (M-26).
   */
  static Stream<Arguments> deepStacksAboveTheLimit() {
    return Stream.of(
        Arguments.of("deep nesting closed by matching end tags", balanced("<b>", "</b>")),
        Arguments.of("mixed names closed by matching end tags", balanced("<b><i>", "</i></b>")),
        Arguments.of("one end tag pops a deep stack", repeat("<div>", "<b>", "</div>")),
        Arguments.of(
            "stray end tags over a deep stack",
            ("<div>".repeat(SIZE / 10) + "</x>".repeat(SIZE / 8))
                .getBytes(StandardCharsets.US_ASCII)),
        Arguments.of("nested lists in block content", deepBlocks("", 5, "<ul><li>", "</li></ul>")),
        Arguments.of(
            "nested elements in block content closed by matching end tags",
            deepBlocks("", 5, "<b>", "</b>")),
        Arguments.of(
            "nested lists in block content over a deep stack",
            deepBlocks(
                "<b>".repeat((SIZE - Analyzer.MAX_BLOCK_BYTES) / 3), 1, "<ul><li>", "</li></ul>")),
        Arguments.of("svg region closed by matching end tags", balanced("<svg><g>", "</g></svg>")),
        Arguments.of(
            "names with one hash code closed by matching end tags", collidingNames(1 << 16)));
  }

  /**
   * Unique tag names in groups: {@code group} names opened, then closed in reverse order, to almost
   * 5 MiB. Each name is new, so the name index adds and removes a key for each element.
   */
  private static byte[] uniqueNamesInGroups(int group) {
    StringBuilder s = new StringBuilder();
    int next = 0;
    while (true) {
      StringBuilder opens = new StringBuilder();
      StringBuilder closes = new StringBuilder();
      for (int i = 0; i < group; i++) {
        String n = "e" + Integer.toString(next++, 36);
        opens.append('<').append(n).append('>');
        closes.insert(0, "</" + n + ">");
      }
      if (s.length() + opens.length() + closes.length() > SIZE) {
        break;
      }
      s.append(opens).append(closes);
    }
    return s.toString().getBytes(StandardCharsets.US_ASCII);
  }

  /**
   * 2^16 different tag names with the same {@link String#hashCode()}. They are opened in groups of
   * {@code group} names, and each group is closed in reverse order. The pairs {@code "a~"} and
   * {@code "b_"} have the same hash code, so every name of the same number of pairs has the same
   * hash code. All open names go to one bucket of the name index.
   */
  private static byte[] collidingNames(int group) {
    int bits = 16;
    String[] names = new String[1 << bits];
    for (int i = 0; i < names.length; i++) {
      StringBuilder n = new StringBuilder("x");
      for (int b = 0; b < bits; b++) {
        n.append(((i >> b) & 1) == 0 ? "a~" : "b_");
      }
      names[i] = n.toString();
      assertEquals(names[0].hashCode(), names[i].hashCode());
    }
    StringBuilder s = new StringBuilder();
    for (int from = 0; from < names.length; from += group) {
      int to = Math.min(names.length, from + group);
      for (int i = from; i < to; i++) {
        s.append('<').append(names[i]).append('>');
      }
      for (int i = to - 1; i >= from; i--) {
        s.append("</").append(names[i]).append('>');
      }
    }
    byte[] b = s.toString().getBytes(StandardCharsets.US_ASCII);
    assertTrue(b.length <= Analyzer.MAX_INPUT_BYTES);
    return b;
  }

  /** {@code open} repeated n times, then {@code close} repeated n times, to almost 5 MiB. */
  private static byte[] balanced(String open, String close) {
    int n = SIZE / (open.length() + close.length());
    return (open.repeat(n) + close.repeat(n)).getBytes(StandardCharsets.US_ASCII);
  }

  /**
   * {@code before}, then {@code count} html blocks. The content of each block is {@code unit}
   * repeated, just under the limit of M-26 and the input limit of M-01. The elements that {@code
   * before} opens stay open below the blocks.
   */
  private static byte[] blocks(String before, int count, String unit) {
    int content = Math.min(Analyzer.MAX_BLOCK_BYTES, (SIZE - before.length()) / count - 32);
    String body = unit.repeat(content / unit.length());
    assertTrue(body.length() <= Analyzer.MAX_BLOCK_BYTES);
    StringBuilder s = new StringBuilder(before);
    for (int i = 0; i < count; i++) {
      s.append("<div data-rq-block=b").append(i).append('>').append(body).append("</div>");
    }
    byte[] b = s.toString().getBytes(StandardCharsets.US_ASCII);
    assertTrue(b.length <= Analyzer.MAX_INPUT_BYTES);
    return b;
  }

  /**
   * {@code before}, then {@code count} html blocks whose content is {@code open} repeated n times,
   * then {@code close} repeated n times, just under the block and input limits.
   */
  private static byte[] deepBlocks(String before, int count, String open, String close) {
    int content = Math.min(Analyzer.MAX_BLOCK_BYTES, (SIZE - before.length()) / count - 32);
    int n = content / (open.length() + close.length());
    return blocks(before, count, open.repeat(n) + close.repeat(n));
  }

  @ParameterizedTest(name = "{0}")
  @MethodSource("inputs")
  void finishesQuickly(String name, byte[] input, ErrorCode expected) {
    Result r = analyzeQuickly(input);
    if (expected != null) {
      assertEquals(expected, r.error());
    }
    PropertyTest.checkInvariants(input, r);
  }

  @ParameterizedTest(name = "{0}")
  @MethodSource("deepStacks")
  void popsDeepStackQuickly(String name, byte[] input, int blocks) {
    // More than 4 MiB: the stack work covers almost the whole input limit.
    assertTrue(input.length > Analyzer.MAX_INPUT_BYTES - Analyzer.MAX_BLOCK_BYTES);
    Result r = analyzeQuickly(input);
    assertEquals(null, r.error(), r::rule);
    assertEquals(blocks, r.blocks().size());
    PropertyTest.checkInvariants(input, r);
  }

  @ParameterizedTest(name = "{0}")
  @MethodSource("deepStacksAboveTheLimit")
  void rejectsTooDeepStackQuickly(String name, byte[] input) {
    Result r = analyzeQuickly(input);
    assertEquals(ErrorCode.LIMIT, r.error(), r::rule);
    assertEquals("M-26", r.rule());
  }

  private static Result analyzeQuickly(byte[] input) {
    return assertTimeoutPreemptively(Duration.ofSeconds(20), () -> Analyzer.analyze(input));
  }
}
