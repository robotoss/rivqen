// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import static org.junit.jupiter.api.Assertions.assertEquals;

import java.io.ByteArrayOutputStream;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.List;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.ValueSource;

/** M-01 (input size), M-02 (encoding) and M-26 (blocks, block size, token stack depth). */
class LimitsTest {
  private static final int DEPTH = Analyzer.MAX_OPEN_ELEMENTS;

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

  // M-26: token stack depth (H-25). A push that makes the stack larger than 256 elements fails.

  private static String open(String tag, int n) {
    return ("<" + tag + ">").repeat(n);
  }

  private static String close(String tag, int n) {
    return ("</" + tag + ">").repeat(n);
  }

  private static void assertDepthLimit(String html) {
    Docs.assertInvalid(html, ErrorCode.LIMIT, "M-26");
  }

  @Test
  void blockElementAtTheDepthLimit() {
    String html = open("div", DEPTH - 1) + "<div data-rq-block=a>x</div>" + close("div", DEPTH - 1);
    assertEquals(List.of("a:html:x"), Docs.blocks(html));
  }

  @Test
  void blockElementAboveTheDepthLimit() {
    assertDepthLimit(open("div", DEPTH) + "<div data-rq-block=a>x</div>" + close("div", DEPTH));
  }

  @Test
  void depthLimitOutsideBlocks() {
    Docs.assertValid(open("b", DEPTH) + close("b", DEPTH) + "<p data-rq-block=a>x</p>");
    assertDepthLimit(open("b", DEPTH + 1) + close("b", DEPTH + 1) + "<p data-rq-block=a>x</p>");
  }

  @Test
  void depthLimitInBlockContent() {
    int n = DEPTH - 1; // the block element is the first element
    assertEquals(
        List.of("a:html:" + open("b", n) + close("b", n)),
        Docs.blocks("<div data-rq-block=a>" + open("b", n) + close("b", n) + "</div>"));
    assertDepthLimit("<div data-rq-block=a>" + open("b", n + 1) + close("b", n + 1) + "</div>");
  }

  @Test
  void depthLimitInBlockContentOverElementsOpenedBefore() {
    String content = open("b", 99) + close("b", 99);
    Docs.assertValid(open("div", 156) + "<div data-rq-block=a>" + content + "</div>");
    assertDepthLimit(open("div", 157) + "<div data-rq-block=a>" + content + "</div>");
  }

  @Test
  void depthLimitInForeignRegion() {
    // The input can end in a foreign region (E2): only the depth decides.
    Docs.assertValid("<svg>" + open("g", DEPTH - 1));
    assertDepthLimit("<svg>" + open("g", DEPTH));
  }

  @Test
  void htmlAndForeignElementsCountTogether() {
    Docs.assertValid(open("div", 200) + "<math>" + open("mrow", 55));
    assertDepthLimit(open("div", 200) + "<math>" + open("mrow", 56));
  }

  @Test
  void foreignRootCounts() {
    Docs.assertValid(open("div", DEPTH - 1) + "<svg></svg>");
    assertDepthLimit(open("div", DEPTH) + "<svg></svg>");
  }

  @ParameterizedTest
  @ValueSource(
      strings = {
        "<br>",
        "<img src=x>",
        "<input/>",
        "<hr>",
        "<wbr>",
        "<svg/>",
        "<math/>",
        "<svg />"
      })
  void elementsThatAreNotPushedDoNotCount(String tag) {
    // The stack is full: any push would fail.
    Docs.assertValid(open("div", DEPTH) + tag.repeat(3) + close("div", DEPTH));
  }

  @Test
  void voidElementsInBlockContentDoNotCount() {
    String html = open("div", DEPTH - 1) + "<div data-rq-block=a><br><img src=x></div>";
    assertEquals(List.of("a:html:<br><img src=x>"), Docs.blocks(html));
  }

  @Test
  void selfClosingForeignElementsDoNotCount() {
    Docs.assertValid("<svg>" + open("g", DEPTH - 1) + "<rect/><path d=\"x\"/><g/>" + "</svg>");
  }

  @Test
  void selfClosingFlagOnOtherHtmlElementCounts() {
    // M-05 item 1: the flag is ignored, so the element is pushed.
    Docs.assertValid(open("div", DEPTH - 1) + "<span/>");
    assertDepthLimit(open("div", DEPTH) + "<span/>");
  }

  @ParameterizedTest
  @ValueSource(strings = {"<title>t</title>", "<script>s</script>", "<textarea></textarea>"})
  void textElementsCount(String element) {
    Docs.assertValid(open("div", DEPTH - 1) + element);
    assertDepthLimit(open("div", DEPTH) + element);
  }

  @Test
  void rawTextContentIsNotCounted() {
    Docs.assertValid(open("div", DEPTH - 1) + "<textarea>" + open("b", 300) + "</textarea>");
  }

  @Test
  void noscriptContentIsNotCounted() {
    // M-26 limits the token stack of M-05. The M-11 pass over the noscript content has none.
    Docs.assertValid(open("div", DEPTH - 1) + "<noscript>" + open("div", 300) + "</noscript>");
  }

  @Test
  void depthRecoversAfterMatchingEndTags() {
    Docs.assertValid((open("b", DEPTH) + close("b", DEPTH)).repeat(3));
    Docs.assertValid(open("b", DEPTH) + "</b>" + "<i>" + "</i></b>");
  }

  @Test
  void depthRecoversAfterOneEndTagPopsManyElements() {
    Docs.assertValid("<div>" + open("span", DEPTH - 1) + "</div>" + open("div", DEPTH));
    assertDepthLimit("<div>" + open("span", DEPTH - 1) + "</div>" + open("div", DEPTH + 1));
  }

  @Test
  void depthRecoversAfterForeignRegionEnds() {
    Docs.assertValid("<svg>" + open("g", DEPTH - 1) + "</svg>" + open("b", DEPTH));
  }

  @Test
  void depthRecoversAfterBlockEnds() {
    String block = "<div data-rq-block=b%d>" + open("b", DEPTH - 1) + close("b", DEPTH - 1);
    Docs.assertValid(String.format(block, 1) + "</div>" + String.format(block, 2) + "</div>");
  }

  @Test
  void strayEndTagOnAFullStack() {
    Docs.assertValid(open("div", DEPTH) + "</x>" + close("div", DEPTH));
  }

  @Test
  void otherRulesOfTheStartTagAreCheckedBeforeTheDepth() {
    // Placement in this candidate: the depth check is the last check before the push.
    Docs.assertInvalid(
        open("div", DEPTH) + "<div data-rq-block=A>x</div>", ErrorCode.INVALID_ID, "M-15");
    Docs.assertStructure(open("div", DEPTH) + "<frameset>", "M-10");
    Docs.assertStructure("<table><tr>" + open("div", DEPTH - 2) + "<tr>", "M-09");
    Docs.assertStructure("<svg>" + open("g", DEPTH - 1) + "<p>", "M-07");
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
