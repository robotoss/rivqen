// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
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
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Base64;
import java.util.List;
import java.util.Random;
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
