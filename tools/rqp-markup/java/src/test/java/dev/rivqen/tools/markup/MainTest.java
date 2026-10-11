// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.BufferedWriter;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.PrintStream;
import java.io.StringWriter;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;
import java.util.List;
import java.util.Map;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

/** Command line ({@code CONTRACT.md} §2) and result object (§3). */
class MainTest {
  private static final String SECRET = "secret-content-7f3a";

  @TempDir Path tmp;

  private final StringWriter written = new StringWriter();
  // Buffered as in Main.main: output that run() does not flush is lost.
  private final BufferedWriter out = new BufferedWriter(written);
  private final ByteArrayOutputStream errBytes = new ByteArrayOutputStream();
  private final PrintStream err = new PrintStream(errBytes, true, StandardCharsets.UTF_8);

  private int run(String... args) {
    return Main.run(args, out, err);
  }

  private String err() {
    return errBytes.toString(StandardCharsets.UTF_8);
  }

  private Path write(String relative, String content) throws IOException {
    Path p = tmp.resolve(relative);
    Files.createDirectories(p.getParent());
    Files.writeString(p, content, StandardCharsets.UTF_8);
    return p;
  }

  @Test
  void singleModePrintsOneResultLine() throws IOException {
    Path f = write("a.html", "<p data-rq-block=price>120</p>");
    assertEquals(0, run(f.toString()));
    String line = written.toString();
    assertTrue(line.endsWith("}\n"));
    assertEquals(1, line.lines().count());
    Map<String, Object> r = MiniJson.object(line);
    assertEquals(
        List.of("valid", "error", "blocks", "template_revision", "page_revision"),
        List.copyOf(r.keySet()));
    assertEquals(true, r.get("valid"));
    assertEquals(MiniJson.NULL, r.get("error"));
    Map<?, ?> b = (Map<?, ?>) ((List<?>) r.get("blocks")).get(0);
    assertEquals("price", b.get("id"));
    assertEquals("html", b.get("format"));
    assertEquals(23L, b.get("start"));
    assertEquals(26L, b.get("end"));
    assertEquals(43, ((String) b.get("sha256")).length());
    assertEquals(46, ((String) r.get("template_revision")).length());
    assertEquals(46, ((String) r.get("page_revision")).length());
  }

  @Test
  void invalidDocumentHasNullRevisionsAndExitCodeZero() throws IOException {
    Path f = write("a.html", "<p data-rq-block=a>" + SECRET);
    assertEquals(0, run(f.toString()));
    assertEquals(
        "{\"valid\":false,\"error\":\"RQP_MARKUP_STRUCTURE\",\"blocks\":[],"
            + "\"template_revision\":null,\"page_revision\":null}\n",
        written.toString());
    assertEquals("", err());
  }

  @Test
  void batchModeReadsSubdirectoriesInByteOrder() throws IOException {
    // "D" < "a" < "b" in byte order, but "D" is last without case. Not "B" or "C": on a file
    // system without case (macOS default) "B" and "b", or "C" and "c", are one directory.
    write("fx/b/input.html", "<p data-rq-block=b>x</p>");
    write("fx/D/input.html", "<p data-rq-block=a>" + SECRET);
    write("fx/a\"\\/input.html", "");
    write("fx/c/other.html", "");
    write("fx/file.txt", "x");
    assertEquals(0, run("--batch", tmp.resolve("fx").toString()));
    List<String> lines = written.toString().lines().toList();
    assertEquals(3, lines.size());
    assertEquals("D", MiniJson.object(lines.get(0)).get("fixture"));
    assertEquals("a\"\\", MiniJson.object(lines.get(1)).get("fixture"));
    assertEquals("b", MiniJson.object(lines.get(2)).get("fixture"));
    assertTrue(lines.get(0).startsWith("{\"fixture\":\"D\",\"valid\":false,"));
    assertEquals(true, MiniJson.object(lines.get(2)).get("valid"));
    assertFalse(written.toString().contains(SECRET));
  }

  @Test
  void usageErrors() {
    assertEquals(2, run());
    assertEquals(2, run("--batch"));
    assertEquals(2, run("--help"));
    assertEquals(2, run("a", "b"));
    assertEquals(2, run("--batch", "a", "b"));
    assertEquals("", written.toString());
    assertTrue(err().startsWith("usage: rqp-markup"));
  }

  @Test
  void missingFileIsAnIoError() {
    assertEquals(2, run(tmp.resolve("missing.html").toString()));
    assertTrue(err().startsWith("rqp-markup: I/O error ("), err());
  }

  @Test
  void missingDirectoryIsAnIoError() {
    assertEquals(2, run("--batch", tmp.resolve("missing").toString()));
    assertTrue(err().startsWith("rqp-markup: I/O error ("), err());
  }

  @Test
  void unreadableInputInBatchIsAnIoError() throws IOException {
    Files.createDirectories(tmp.resolve("fx/a/input.html"));
    assertEquals(2, run("--batch", tmp.resolve("fx").toString()));
    assertFalse(err().isEmpty());
  }

  @Test
  void invalidPathIsAnIoError() {
    assertEquals(2, run("a\0b"));
  }

  @Test
  void readStopsAfterTheSizeLimit() throws IOException {
    byte[] big = new byte[Analyzer.MAX_INPUT_BYTES + 100];
    Arrays.fill(big, (byte) 'a');
    Path f = tmp.resolve("big.html");
    Files.write(f, big);
    assertEquals(Analyzer.MAX_INPUT_BYTES + 1, Main.read(f).length);
    assertEquals(0, run(f.toString()));
    assertTrue(written.toString().contains("RQP_MARKUP_LIMIT"));
  }

  @Test
  void messagesNeverContainDocumentContent() throws IOException {
    Path f = write("a.html", "<p data-rq-block=a>" + SECRET + "</p>");
    assertEquals(0, run(f.toString()));
    assertFalse(err().contains(SECRET));
  }

  // Internal errors (security review F-4): fail closed, exit code 3, no document content.

  /** An analysis with a defect: it throws for documents that contain "defect". */
  private static Result defective(byte[] input) {
    return Analyzer.failClosed(
        () -> {
          if (new String(input, StandardCharsets.UTF_8).contains("defect")) {
            throw new IllegalStateException("defect at " + SECRET);
          }
          return Analyzer.analyze(input);
        });
  }

  @Test
  void unexpectedExceptionInTheAnalysisGivesAnInvalidResult() {
    Result r =
        Analyzer.failClosed(
            () -> {
              throw new ArrayIndexOutOfBoundsException(SECRET);
            });
    assertEquals(ErrorCode.STRUCTURE, r.error());
    assertEquals("internal-error:java.lang.ArrayIndexOutOfBoundsException", r.rule());
    assertTrue(r.internalError());
    assertTrue(r.blocks().isEmpty());
    assertEquals(null, r.templateRevision());
    assertEquals(null, r.pageRevision());
  }

  @Test
  void resultsWithoutDefectPassTheBoundary() {
    Result valid = Analyzer.analyze(Docs.utf8("<p data-rq-block=a>x</p>"));
    assertEquals(valid, Analyzer.failClosed(() -> valid));
    assertFalse(valid.internalError());
    assertFalse(Analyzer.analyze(Docs.utf8("<p data-rq-block=a>")).internalError());
  }

  @Test
  void defectInSingleModePrintsAFailClosedResultAndExitsWithThree() throws IOException {
    Path f = write("a.html", "<p data-rq-block=a>defect " + SECRET + "</p>");
    assertEquals(3, Main.run(new String[] {f.toString()}, out, err, MainTest::defective));
    assertEquals(
        "{\"valid\":false,\"error\":\"RQP_MARKUP_STRUCTURE\",\"blocks\":[],"
            + "\"template_revision\":null,\"page_revision\":null}\n",
        written.toString());
    assertTrue(err().startsWith("rqp-markup: internal error"), err());
    assertFalse(err().contains(SECRET));
    assertFalse(err().contains("defect at"));
  }

  @Test
  void defectInBatchModeKeepsTheOtherResults() throws IOException {
    write("fx/a/input.html", "<p data-rq-block=a>x</p>");
    write("fx/b/input.html", "defect " + SECRET);
    write("fx/c/input.html", "<p data-rq-block=c>y</p>");
    String[] args = {"--batch", tmp.resolve("fx").toString()};
    assertEquals(3, Main.run(args, out, err, MainTest::defective));
    List<String> lines = written.toString().lines().toList();
    assertEquals(3, lines.size());
    assertEquals(true, MiniJson.object(lines.get(0)).get("valid"));
    assertEquals("RQP_MARKUP_STRUCTURE", MiniJson.object(lines.get(1)).get("error"));
    assertEquals(true, MiniJson.object(lines.get(2)).get("valid"));
    assertEquals("rqp-markup: internal error in 1 document(s)\n", err());
    assertFalse(written.toString().contains(SECRET));
  }

  @Test
  void loneSurrogatesAtTheEndsAreEscaped() {
    StringBuilder s = new StringBuilder();
    ResultJson.string(s, "\uD800");
    ResultJson.string(s, "\uDC00x");
    assertEquals("\"\\ud800\"\"\\udc00x\"", s.toString());
  }

  @Test
  void jsonStringEscaping() {
    StringBuilder s = new StringBuilder();
    ResultJson.string(s, " a\"b\\c\n\r\t\u0001\u001f\u007fé😀\uD800x\uDC00");
    assertEquals("\" a\\\"b\\\\c\\n\\r\\t\\u0001\\u001f\u007fé😀\\ud800x\\udc00\"", s.toString());
  }
}
