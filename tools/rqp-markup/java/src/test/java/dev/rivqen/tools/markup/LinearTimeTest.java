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

  /** {@code head} + {@code unit} repeated to almost 5 MiB + {@code tail}. */
  private static byte[] repeat(String head, String unit, String tail) {
    int n = (SIZE - head.length() - tail.length()) / unit.length();
    String s = head + unit.repeat(n) + tail;
    byte[] b = s.getBytes(StandardCharsets.UTF_8);
    assertTrue(b.length <= Analyzer.MAX_INPUT_BYTES);
    return b;
  }

  static Stream<Arguments> inputs() {
    String half = "<div>".repeat(SIZE / 10);
    return Stream.of(
        Arguments.of("deep nesting", repeat("", "<div>", ""), null),
        Arguments.of(
            "stray end tags over a deep stack",
            (half + "</x>".repeat(SIZE / 8)).getBytes(StandardCharsets.US_ASCII),
            null),
        Arguments.of("one end tag pops a deep stack", repeat("<div>", "<b>", "</div>"), null),
        Arguments.of("unique names", uniqueNames(), null),
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
        Arguments.of("svg region", repeat("<svg>", "<g>", "</svg>"), null),
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

  @ParameterizedTest(name = "{0}")
  @MethodSource("inputs")
  void finishesQuickly(String name, byte[] input, ErrorCode expected) {
    Result r = assertTimeoutPreemptively(Duration.ofSeconds(20), () -> Analyzer.analyze(input));
    if (expected != null) {
      assertEquals(expected, r.error());
    }
    PropertyTest.checkInvariants(input, r);
  }
}
