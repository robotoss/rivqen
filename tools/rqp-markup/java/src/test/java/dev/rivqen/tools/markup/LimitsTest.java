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

/** M-01 (input size), M-02 (encoding) and M-26 (blocks, block size, depth counter D). */
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

  // M-26: depth counter D (H-25, H-31). A step that makes D larger than 256 fails.

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
    // D is 256: any push would fail.
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
    // M-26 item 7: the tags of the noscript content C are not part of the main pass.
    String c = open("div", 300) + close("div", 300);
    Docs.assertValid(open("div", DEPTH - 1) + "<noscript>" + c + "</noscript>");
  }

  @Test
  void noscriptElementCounts() {
    assertDepthLimit(open("div", DEPTH) + "<noscript><img src=x></noscript>");
  }

  @Test
  void depthRecoversAfterMatchingEndTags() {
    Docs.assertValid((open("b", DEPTH) + close("b", DEPTH)).repeat(3));
    Docs.assertValid(open("b", DEPTH) + "</b>" + "<i>" + "</i></b>");
  }

  // M-26 items 4 to 6 (H-31): only an exact pop of a clean element gives units back.

  @Test
  void crossingKeepsTheUnitsOfThePoppedElements() {
    // The tree builder ignores </div> over a span only when a special element is above; the
    // token stack cannot know, so a crossing never gives units back.
    String crossed = "<div>" + open("span", DEPTH - 1) + "</div>";
    // D stays 256: tokens that do not push are still allowed, a push is not.
    Docs.assertValid(crossed + "</x></div>x<!--c--><br>");
    assertDepthLimit(crossed + "<b>");
  }

  @Test
  void eachSpanOverDivCrossingKeepsTwoUnits() {
    // markup.md M-26 example; 300 of them is the review repro (WHATWG stack of 603 in parse5).
    String unit = "<span><div></span>";
    Docs.assertValid(unit.repeat(DEPTH / 2));
    assertDepthLimit(unit.repeat(DEPTH / 2) + "<b>");
    assertDepthLimit(Docs.body(unit.repeat(300) + "<div data-rq-block=a>x</div>"));
  }

  @Test
  void crossingMakesTheNewCurrentNodeDirty() {
    // The first </span> crosses the div and makes the outer span dirty: the second </span> is an
    // exact pop that gives nothing back. Each unit keeps 3.
    String unit = "<span><span><div></span></span>";
    Docs.assertValid(unit.repeat(85) + "<b></b>");
    assertDepthLimit(unit.repeat(85) + "<b><b>");
    // Without the dirty mark the second unit would give 1 back.
    Docs.assertValid("<q><span><div></span></q>".repeat(85) + "<b></b>");
    assertDepthLimit("<q><span><div></span></q>".repeat(85) + "<b><b>");
  }

  @Test
  void dirtyMarkPassesDownOnEachPop() {
    // The pop of a dirty element makes the element below it dirty too.
    String unit = "<q><q><span><div></span></q></q>";
    Docs.assertValid(unit.repeat(64));
    assertDepthLimit(unit.repeat(64) + "<b>");
  }

  @ParameterizedTest
  @ValueSource(strings = {"html", "body"})
  void popOfHtmlOrBodyGivesNothingBack(String name) {
    // The tree builder never pops html for </html> and body for </body>.
    String unit = "<" + name + "></" + name + ">";
    Docs.assertValid(unit.repeat(DEPTH));
    assertDepthLimit(unit.repeat(DEPTH) + "<b>");
    assertDepthLimit("<div>" + unit + "</div>" + open("b", DEPTH - 1));
    Docs.assertValid("<div></div>".repeat(DEPTH) + "<div>" + unit + open("b", DEPTH - 2));
  }

  @Test
  void popOfForeignHtmlElementGivesNothingBack() {
    // M-26 items 4 and 5 name html and body without a context (reading of this candidate).
    String region = "<svg>" + "<html></html>".repeat(DEPTH - 1);
    Docs.assertValid(region + "x");
    assertDepthLimit(region + "<g>");
    Docs.assertValid("<svg>" + "<g></g>".repeat(DEPTH) + "<g>");
  }

  @Test
  void strayEndTagDoesNotChangeTheDepth() {
    Docs.assertValid(open("b", DEPTH) + "</x></i>" + close("b", DEPTH) + open("b", DEPTH));
  }

  @Test
  void depthRecoversAfterForeignRegionEnds() {
    String region = "<svg>" + open("g", DEPTH - 1) + close("g", DEPTH - 1) + "</svg>";
    Docs.assertValid(region + open("b", DEPTH));
  }

  @Test
  void regionEndThatCrossesKeepsTheUnits() {
    String region = "<svg>" + open("g", DEPTH - 1) + "</svg>";
    Docs.assertValid(region + "x");
    assertDepthLimit(region + "<b>");
  }

  // M-26 item 3: implied parts, once for each F of M-09.

  @Test
  void cellInTableCountsTheImpliedRowGroupAndRow() {
    // table, row group, row, td: 4 (markup.md M-26 example).
    String table = "<table><tr><td>a</td><td>b</td></tr></table>";
    Docs.assertValid(open("div", DEPTH - 4) + table + table);
    assertDepthLimit(open("div", DEPTH - 3) + table);
    String cell = "<table><td>a</td></table>";
    Docs.assertValid(open("div", DEPTH - 4) + cell);
    assertDepthLimit(open("div", DEPTH - 3) + cell);
  }

  @Test
  void impliedPartsCountOncePerTable() {
    String cells = "<td>a</td>".repeat(300);
    Docs.assertValid(open("div", DEPTH - 4) + "<table>" + cells + "</table>");
    String rows = "<tr><td>a</td></tr>".repeat(300);
    Docs.assertValid(open("div", DEPTH - 4) + "<table>" + rows + "</table>");
    Docs.assertValid(open("div", DEPTH - 2) + "<table>" + "<col>".repeat(300) + "</table>");
  }

  @Test
  void impliedPartsComeBackWithTheirTable() {
    Docs.assertValid("<table><tr><td>a</td></tr></table><table><col></table>".repeat(300));
  }

  @Test
  void rowInTableCountsTheImpliedRowGroup() {
    Docs.assertValid(open("div", DEPTH - 3) + "<table><tr></tr></table>");
    assertDepthLimit(open("div", DEPTH - 2) + "<table><tr></tr></table>");
  }

  @ParameterizedTest
  @ValueSource(strings = {"tbody", "thead", "tfoot"})
  void cellInRowGroupCountsTheImpliedRow(String group) {
    String table = "<table><" + group + "><td>a</td></" + group + "></table>";
    Docs.assertValid(open("div", DEPTH - 4) + table);
    assertDepthLimit(open("div", DEPTH - 3) + table);
  }

  @Test
  void cellInRowAndRowInRowGroupImplyNothing() {
    Docs.assertValid(open("div", DEPTH - 4) + "<table><tbody><tr><td>a</td></tr></tbody></table>");
  }

  @Test
  void columnInTableCountsTheImpliedColumnGroup() {
    // col is not pushed, but its column group counts.
    Docs.assertValid(open("div", DEPTH - 2) + "<table><col></table>");
    assertDepthLimit(open("div", DEPTH - 1) + "<table><col></table>");
    Docs.assertValid(open("div", DEPTH - 2) + "<table><colgroup><col></colgroup></table>");
  }

  @Test
  void templateHidesTheTable() {
    // F of M-09 stops at a template: the cell has no F, so nothing is implied.
    Docs.assertValid(open("div", DEPTH - 3) + "<table><template><td>a</td></template></table>");
  }

  @Test
  void impliedPartAboveTheLimit() {
    // The table makes D 255; the td adds the row group (256) and the row (257) before its push.
    assertDepthLimit(open("div", DEPTH - 2) + "<table><td>");
    assertDepthLimit(open("div", DEPTH - 1) + "<table><col>");
  }

  @Test
  void tablesInCellsAddFourForEachLevel() {
    // Fixture LIMIT-DEPTH-TABLE: 64 levels of <table><td> and a block make D 257.
    String block = "<div data-rq-block=a>x</div>";
    Docs.assertValid("<table><td>".repeat(63) + block);
    assertDepthLimit("<table><td>".repeat(64) + block);
  }

  @Test
  void crossedTableKeepsItsImpliedParts() {
    // </table> crosses the cell (allowed, M-06 item 1): no units come back.
    String table = "<table><td>a</table>";
    Docs.assertValid(table.repeat(64));
    assertDepthLimit(table.repeat(64) + "<b>");
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
    // Placement in this candidate: the D check is the last check before the push.
    Docs.assertInvalid(
        open("div", DEPTH) + "<div data-rq-block=A>x</div>", ErrorCode.INVALID_ID, "M-15");
    Docs.assertStructure(open("div", DEPTH) + "<frameset>", "M-10");
    // table, implied row group, tr: 3.
    Docs.assertStructure("<table><tr>" + open("div", DEPTH - 3) + "<tr>", "M-09");
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
