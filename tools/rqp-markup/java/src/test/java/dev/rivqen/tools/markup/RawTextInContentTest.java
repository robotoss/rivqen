// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import static dev.rivqen.tools.markup.Docs.assertInvalid;
import static dev.rivqen.tools.markup.Docs.assertStructure;
import static dev.rivqen.tools.markup.Docs.blocks;
import static dev.rivqen.tools.markup.Docs.body;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;

import java.nio.charset.StandardCharsets;
import java.util.List;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.ValueSource;

/** M-04 in the content of an html block: raw text, and the block end tag after it (H-78). */
class RawTextInContentTest {
  private static String block(String content) {
    return body("<div data-rq-block=w>" + content + "</div>");
  }

  private static void assertContent(String content) {
    assertEquals(List.of("w:html:" + content), blocks(block(content)));
  }

  @Test
  void blockEndTagInScriptDataIsText() {
    assertContent("x<script>document.write(\"</div>\")</script>y");
  }

  @Test
  void blockStartTagAndCommentInRawTextAreText() {
    assertContent("<style><p data-rq-block=\"x\">a<!-- b</style>");
    assertContent("<iframe></div><span></iframe>");
  }

  @Test
  void escapedStateKeepsTheScriptOpenUntilTheDoubleEscapedEnds() {
    assertContent("<script>a<!--<script>b</script>c--></script>");
  }

  @ParameterizedTest
  @ValueSource(strings = {"</SCRIPT >", "</script\t>", "</script/>", "</script foo>", "</ScRiPt>"})
  void appropriateEndTagForms(String end) {
    assertContent("<script>a" + end);
  }

  @Test
  void longerNameIsNotAnEndTag() {
    // `</scripty>` is text, so the script ends at the second end tag.
    assertContent("<script>a</scripty>b</script>");
  }

  @Test
  void otherRawTextEndTagIsText() {
    assertContent("<style>a</script>b</style>");
  }

  @Test
  void commentLikeTextInStyleDoesNotChangeTheState() {
    assertContent("<style><!--<style></style>");
  }

  @Test
  void rawTextWithoutAnEndLeavesTheBlockOpen() {
    assertStructure(block("<script>a"), "M-20");
    assertStructure(block("<style>a"), "M-20");
    assertStructure(block("<iframe>a"), "M-20");
  }

  @Test
  void doubleEscapedScriptDataWithoutAnEndLeavesTheBlockOpen() {
    assertStructure(block("<script><!--<script></script>"), "M-20");
  }

  @Test
  void scriptInContentIsCheckedByM13() {
    assertInvalid(
        block("<script type=\"application/rivqen-manifest+json\">{}</script>"),
        ErrorCode.RESERVED,
        "M-13");
    assertInvalid(block("<script type=\"a&amp;b\"></script>"), ErrorCode.RESERVED, "M-13");
  }

  @Test
  void rawTextElementIsPoppedBeforeTheBlockEnd() {
    assertContent("<p>a</p><script></script>");
  }

  @Test
  void manyRawTextElementsStayLinear() {
    StringBuilder sb = new StringBuilder("<div data-rq-block=w>");
    for (int i = 0; i < 20_000; i++) {
      sb.append("<script>a<!--<script></script>--></script>");
    }
    sb.append("</div>");
    long t = System.nanoTime();
    Result r = Analyzer.analyze(sb.toString().getBytes(StandardCharsets.UTF_8));
    assertNull(r.error());
    assertEquals(1, r.blocks().size());
    assertEquals(true, System.nanoTime() - t < 5_000_000_000L);
  }
}
