// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;

import java.util.Arrays;
import java.util.Set;
import java.util.TreeSet;
import java.util.stream.Collectors;
import org.junit.jupiter.api.Test;

/** The name sets SPECIAL and FORMATTING of {@code markup.md} §2.1, copied from the rule text. */
class NameTest {
  private static final String SPECIAL =
      "address applet area article aside base basefont bgsound blockquote body br button caption"
          + " center col colgroup dd details dir div dl dt embed fieldset figcaption figure footer"
          + " form frame frameset h1 h2 h3 h4 h5 h6 head header hgroup hr html iframe img input"
          + " keygen li link listing main marquee menu meta nav noembed noframes noscript object ol"
          + " p param plaintext pre script search section select source style summary table tbody"
          + " td template textarea tfoot th thead title tr track ul wbr xmp";

  /** FORBIDDEN_CONTENT of markup.md section 2.1 (H-78). */
  private static final String FORBIDDEN_CONTENT =
      "body frame frameset head html noembed noframes optgroup option plaintext select"
          + " textarea title xmp";

  private static final String FORMATTING = "a b big code em font i nobr s small strike strong tt u";

  private static Set<String> names(String list) {
    return new TreeSet<>(Arrays.asList(list.split(" ")));
  }

  private static Set<String> withFlag(int flag) {
    return Arrays.stream(Name.values())
        .filter(n -> n.in(flag))
        .map(n -> n.tag)
        .collect(Collectors.toCollection(TreeSet::new));
  }

  @Test
  void specialSetIsTheListOfTheRules() {
    names(SPECIAL).forEach(n -> assertNotNull(Name.of(n), n));
    assertEquals(names(SPECIAL), withFlag(Name.Set.SPECIAL));
  }

  @Test
  void forbiddenContentIsTheListOfTheRules() {
    names(FORBIDDEN_CONTENT).forEach(n -> assertNotNull(Name.of(n), n));
    assertEquals(names(FORBIDDEN_CONTENT), withFlag(Name.Set.FORBIDDEN_CONTENT));
  }

  @Test
  void svgAndMathAreNotForbiddenInContent() {
    // H-73: they start a region in the content; M-22 item 2 does not reject them.
    assertEquals(false, Name.is(Name.SVG, Name.Set.FORBIDDEN_CONTENT));
    assertEquals(false, Name.is(Name.MATH, Name.Set.FORBIDDEN_CONTENT));
    // H-94, H-95: M-22 item 6, M-24 item g and M-11 item 10 make them safe in the content.
    assertEquals(false, Name.is(Name.NOSCRIPT, Name.Set.FORBIDDEN_CONTENT));
    assertEquals(false, Name.is(Name.TEMPLATE, Name.Set.FORBIDDEN_CONTENT));
  }

  @Test
  void formattingSetIsTheListOfTheRules() {
    names(FORMATTING).forEach(n -> assertNotNull(Name.of(n), n));
    assertEquals(names(FORMATTING), withFlag(Name.Set.FORMATTING));
  }
}
