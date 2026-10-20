// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Map;
import java.util.stream.Stream;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.MethodSource;

/**
 * Every golden fixture of {@code fixtures/rqp/markup}: the result equals {@code expected.json} (the
 * same comparison as {@code tools/rqp-markup/diff.mjs}). For an invalid fixture, the rule that the
 * parser reports is one of the rules that {@code fixture.json} names.
 */
class FixturesTest {

  static Path fixturesDir() {
    String p = System.getProperty("rqp.fixtures");
    Path dir =
        p != null
            ? Path.of(p)
            : Path.of(
                System.getProperty("user.dir"), "..", "..", "..", "fixtures", "rqp", "markup");
    return dir.normalize();
  }

  static Stream<String> fixtures() throws IOException {
    try (Stream<Path> s = Files.list(fixturesDir())) {
      List<String> names =
          s.filter(d -> Files.exists(d.resolve("input.html")))
              .map(d -> d.getFileName().toString())
              .sorted()
              .toList();
      assertTrue(names.size() >= 117, "fixture count " + names.size());
      return names.stream();
    }
  }

  @ParameterizedTest(name = "{0}")
  @MethodSource("fixtures")
  void resultEqualsExpectedJson(String name) throws IOException {
    Path dir = fixturesDir().resolve(name);
    Result r = Analyzer.analyze(Main.read(dir.resolve("input.html")));
    Map<String, Object> expected =
        MiniJson.object(Files.readString(dir.resolve("expected.json"), StandardCharsets.UTF_8));
    Map<String, Object> actual = MiniJson.object(ResultJson.format(r, null));
    for (String field : List.of("valid", "error", "blocks", "template_revision", "page_revision")) {
      assertEquals(String.valueOf(expected.get(field)), String.valueOf(actual.get(field)), field);
    }
    if (!r.valid()) {
      Map<String, Object> fixture =
          MiniJson.object(Files.readString(dir.resolve("fixture.json"), StandardCharsets.UTF_8));
      Object rules = fixture.get("rules");
      assertTrue(
          rules instanceof List<?> l && l.contains(r.rule()),
          () -> "rule " + r.rule() + " not in " + rules);
    }
  }
}
