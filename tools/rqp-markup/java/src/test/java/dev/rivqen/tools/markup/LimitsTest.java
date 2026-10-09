// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import static org.junit.jupiter.api.Assertions.assertEquals;

import java.io.ByteArrayOutputStream;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import org.junit.jupiter.api.Test;

/** M-01 (input size), M-02 (encoding) and M-26 (block limits). */
class LimitsTest {

  @Test
  void inputAtTheSizeLimitIsRead() {
    byte[] in = new byte[Analyzer.MAX_INPUT_BYTES];
    Arrays.fill(in, (byte) 'a');
    assertEquals(null, Analyzer.analyze(in).error());
  }

  @Test
  void inputAboveTheSizeLimit() {
    byte[] in = new byte[Analyzer.MAX_INPUT_BYTES + 1];
    Arrays.fill(in, (byte) 'a');
    Result r = Analyzer.analyze(in);
    assertEquals(ErrorCode.LIMIT, r.error());
    assertEquals("M-01", r.rule());
  }

  @Test
  void sizeIsCheckedBeforeEncoding() {
    byte[] in = new byte[Analyzer.MAX_INPUT_BYTES + 1];
    Arrays.fill(in, (byte) 0xFF);
    assertEquals("M-01", Analyzer.analyze(in).rule());
  }

  @Test
  void invalidUtf8() {
    byte[] in = {'<', 'p', '>', (byte) 0xC3, '(', '<', '/', 'p', '>'};
    Result r = Analyzer.analyze(in);
    assertEquals(ErrorCode.ENCODING, r.error());
    assertEquals("M-02", r.rule());
  }

  @Test
  void byteOrderMarkCountsInOffsetsButNotInTokens() {
    byte[] in = Docs.utf8(Docs.BOM + "<p data-rq-block=a>x</p>");
    Result r = Analyzer.analyze(in);
    assertEquals(3 + 19, r.blocks().get(0).start());
  }

  @Test
  void byteOrderMarkElsewhereIsACharacter() {
    assertEquals(
        java.util.List.of("a:html:" + Docs.BOM),
        Docs.blocks("<p data-rq-block=a>" + Docs.BOM + "</p>" + Docs.BOM));
  }

  @Test
  void byteOrderMarkHidesNoTag() {
    // The BOM bytes are text to the tokenizer: they cannot hide or start a tag.
    assertEquals(java.util.List.of("a:html:x"), Docs.blocks(Docs.BOM + "<p data-rq-block=a>x</p>"));
  }

  @Test
  void twoHundredFiftySixBlocks() {
    Result r = Analyzer.analyze(blocks(Analyzer.MAX_BLOCKS));
    assertEquals(null, r.error());
    assertEquals(Analyzer.MAX_BLOCKS, r.blocks().size());
  }

  @Test
  void twoHundredFiftySevenBlocks() {
    Result r = Analyzer.analyze(blocks(Analyzer.MAX_BLOCKS + 1));
    assertEquals(ErrorCode.LIMIT, r.error());
    assertEquals("M-26", r.rule());
  }

  @Test
  void blockAtTheContentLimit() {
    Result r = Analyzer.analyze(bigBlock("div", Analyzer.MAX_BLOCK_BYTES));
    assertEquals(null, r.error());
    assertEquals(Analyzer.MAX_BLOCK_BYTES, r.blocks().get(0).end() - r.blocks().get(0).start());
  }

  @Test
  void blockAboveTheContentLimit() {
    Result r = Analyzer.analyze(bigBlock("div", Analyzer.MAX_BLOCK_BYTES + 1));
    assertEquals(ErrorCode.LIMIT, r.error());
    assertEquals("M-26", r.rule());
  }

  @Test
  void rawTextBlockAboveTheContentLimit() {
    assertEquals("M-26", Analyzer.analyze(bigBlock("title", Analyzer.MAX_BLOCK_BYTES + 1)).rule());
  }

  private static byte[] blocks(int n) {
    StringBuilder s = new StringBuilder("<body>");
    for (int i = 0; i < n; i++) {
      s.append("<p data-rq-block=b").append(i).append('>').append(i).append("</p>");
    }
    return s.toString().getBytes(StandardCharsets.UTF_8);
  }

  private static byte[] bigBlock(String name, int contentBytes) {
    ByteArrayOutputStream out = new ByteArrayOutputStream();
    out.writeBytes(("<" + name + " data-rq-block=a>").getBytes(StandardCharsets.US_ASCII));
    byte[] content = new byte[contentBytes];
    Arrays.fill(content, (byte) 'x');
    out.writeBytes(content);
    out.writeBytes(("</" + name + ">").getBytes(StandardCharsets.US_ASCII));
    return out.toByteArray();
  }
}
