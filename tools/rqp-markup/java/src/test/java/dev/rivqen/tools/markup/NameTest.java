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
  void formattingSetIsTheListOfTheRules() {
    names(FORMATTING).forEach(n -> assertNotNull(Name.of(n), n));
    assertEquals(names(FORMATTING), withFlag(Name.Set.FORMATTING));
  }
}
