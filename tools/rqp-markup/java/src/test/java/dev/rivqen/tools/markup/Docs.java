// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.List;

/** Helpers for tests on small documents. */
final class Docs {
  /** U+FEFF, the byte order mark (kept out of the source text: it is invisible). */
  static final String BOM = String.valueOf((char) 0xFEFF);

  private Docs() {}

  static byte[] utf8(String s) {
    return s.getBytes(StandardCharsets.UTF_8);
  }

  static Result analyze(String html) {
    return Analyzer.analyze(utf8(html));
  }

  /** Asserts that the document is invalid with this code and rule. */
  static void assertInvalid(String html, ErrorCode code, String rule) {
    Result r = analyze(html);
    assertEquals(code, r.error(), () -> "error for " + html);
    assertEquals(rule, r.rule(), () -> "rule for " + html);
    assertTrue(r.blocks().isEmpty());
    assertEquals(null, r.templateRevision());
    assertEquals(null, r.pageRevision());
  }

  /** Asserts that the document is invalid with RQP_MARKUP_STRUCTURE and this rule. */
  static void assertStructure(String html, String rule) {
    assertInvalid(html, ErrorCode.STRUCTURE, rule);
  }

  /** Asserts that the document is valid and returns its blocks as "id:format:content". */
  static List<String> blocks(String html) {
    byte[] in = utf8(html);
    Result r = Analyzer.analyze(in);
    assertEquals(null, r.error(), () -> "expected valid (" + r.rule() + "): " + html);
    return r.blocks().stream()
        .map(
            b ->
                b.id()
                    + ":"
                    + b.format()
                    + ":"
                    + new String(
                        Arrays.copyOfRange(in, b.start(), b.end()), StandardCharsets.UTF_8))
        .toList();
  }

  /** Asserts that the document is valid. */
  static void assertValid(String html) {
    blocks(html);
  }

  /** A body with the given markup. */
  static String body(String markup) {
    return "<!DOCTYPE html><html><head><title>T</title></head><body>" + markup + "</body></html>";
  }
}
