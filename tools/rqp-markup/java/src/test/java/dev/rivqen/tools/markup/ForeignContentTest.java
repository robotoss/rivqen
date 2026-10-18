// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import static dev.rivqen.tools.markup.Docs.assertInvalid;
import static dev.rivqen.tools.markup.Docs.assertStructure;
import static dev.rivqen.tools.markup.Docs.assertValid;
import static dev.rivqen.tools.markup.Docs.blocks;
import static dev.rivqen.tools.markup.Docs.body;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;
import org.junit.jupiter.api.Nested;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.ValueSource;

/**
 * Tests for the WP-17 S3 rules: foreign regions in the content of an html block (M-22 item 5, M-23;
 * H-73), the end tag of the region root (M-06 item 4, M-23; H-75), and the content C of a {@code
 * noscript} element in the main pass (M-11 items 7 to 9; H-71, H-74). Each test checks the rule
 * that fails, not only the error code: the rules of the old checks gave the same code for most
 * inputs.
 */
class ForeignContentTest {
  private static final String P = "<p data-rq-block=\"a\">x</p>";

  /** An html block with this content. */
  private static String block(String content) {
    return body("<div data-rq-block=\"a\">" + content + "</div>");
  }

  @Nested
  class M22Item5 {
    @Test
    void svgIconInSpanBlockIsValid() {
      String content =
          "<svg viewBox=\"0 0 16 16\"><title>Close</title><path d=\"M0\"/></svg> Close";
      assertEquals(
          List.of("i:html:" + content),
          blocks(body("<span data-rq-block=\"i\">" + content + "</span>")));
    }

    @Test
    void mathInBlockIsValid() {
      String content = "<math><mi>x</mi><mo>+</mo><mn>1</mn></math>";
      assertEquals(
          List.of("f:html:" + content), blocks(body("<p data-rq-block=\"f\">" + content + "</p>")));
    }

    @Test
    void selfClosingSvgAndMathOpenNoRegion() {
      assertValid(block("<svg/><math/>x"));
    }

    @Test
    void selfClosingOtherNonVoidElementIsStillInvalid() {
      assertStructure(block("<span/>"), "M-22");
    }

    @ParameterizedTest
    @ValueSource(
        strings = {
          "a",
          "animate",
          "animatemotion",
          "animatetransform",
          "set",
          "discard",
          "mpath",
          "image",
          "feimage",
          "textpath",
          "cursor",
          "view",
          "unknown-name"
        })
    void svgElementsOutsideTheOldAllowedSetAreValid(String name) {
      // H-78: FOREIGN_ALLOWED is a patch rule, not a markup rule.
      assertValid(block("<svg><" + name + "></" + name + "></svg>"));
    }

    @ParameterizedTest
    @ValueSource(strings = {"foreignobject", "script", "style"})
    void textOnlyElementsAreValidWithText(String name) {
      String content = "<svg><" + name + ">t</" + name + "></svg>";
      assertEquals(List.of("a:html:" + content), blocks(block(content)));
    }

    @Test
    void foreignScriptKeepsItsM07TextRule() {
      assertStructure(block("<svg><script>a<b</script></svg>"), "M-07");
    }

    @ParameterizedTest
    @ValueSource(strings = {"maction", "annotation-xml", "mglyph", "a"})
    void mathElementsOutsideTheOldAllowedSetAreValid(String name) {
      assertValid(block("<math><" + name + "></" + name + "></math>"));
    }

    @Test
    void selfClosingForeignElementIsValid() {
      assertValid(block("<svg><animate attributeName=x /></svg>"));
    }

    @Test
    void allowedNamesInARegion() {
      assertValid(
          block(
              "<svg><defs><lineargradient><stop/></lineargradient></defs><g><rect/><circle/>"
                  + "<text><tspan>t</tspan></text><use/></g></svg>"
                  + "<math><mrow><msup><mi>x</mi><mn>2</mn></msup></mrow><semantics><mi>y</mi>"
                  + "<annotation>t</annotation></semantics></math>"));
    }

    @Test
    void integrationPointKeepsItsTextOnlyRule() {
      // M-07 item 3 applies to the content too: `g` is not an HTML element in the `desc`.
      assertStructure(block("<svg><desc><g></g></desc></svg>"), "M-07");
    }

    @Test
    void blockStartTagInARegionIsNested() {
      assertInvalid(block("<svg><g data-rq-block=\"b\"></g></svg>"), ErrorCode.NESTED, "M-17");
    }

    @Test
    void htmlItemsDoNotApplyInARegion() {
      // Item 2 (a name in FORBIDDEN_CONTENT) and item 3 (self-closing) are for HTML context.
      // `title` is in FORBIDDEN_CONTENT.
      assertValid(block("<svg><title>t</title><g/></svg>"));
    }

    @Test
    void breakoutNameInARegionIsInvalid() {
      assertStructure(block("<svg><div></div></svg>"), "M-07");
    }
  }

  @Nested
  class M23Region {
    @Test
    void regionRootEndTagClosesTheRegion() {
      assertValid(block("<svg><path d=\"M0\"></svg>"));
    }

    @Test
    void innerEndTagMustMatchTheCurrentNode() {
      assertStructure(block("<svg><g><path d=\"M0\"></g></svg>"), "M-23");
    }

    @Test
    void strayEndTagInTheRegion() {
      assertStructure(block("<svg></g></svg>"), "M-23");
    }

    @Test
    void endTagOfTheBlockElementInsideTheRegion() {
      assertStructure(block("<svg>"), "M-23");
    }

    @Test
    void endTagOfNestedSvgMustMatchTheCurrentNode() {
      assertValid(block("<svg><svg></svg></svg>"));
      assertStructure(block("<svg><svg><path></svg></svg>"), "M-23");
    }

    @Test
    void regionRootEndTagCrossesNestedSvg() {
      // The nearest `svg` of the second end tag is the inner one, so it is not the region root.
      assertStructure(block("<svg><svg><g></svg></svg>"), "M-23");
    }

    @Test
    void endOfInputInAContentRegionIsInvalidByM20() {
      assertStructure("<div data-rq-block=\"a\"><svg><path/>", "M-20");
      assertStructure("<div data-rq-block=\"a\"><svg><title>t", "M-20");
    }

    @Test
    void regionInContentCountsInD() {
      // div + svg + 254 `g` = 256; one more is 257.
      String start = "<div data-rq-block=\"a\"><svg>";
      assertValid(start + "<g>".repeat(254) + "</g>".repeat(254) + "</svg></div>");
      assertInvalid(
          start + "<g>".repeat(255) + "</g>".repeat(255) + "</svg></div>", ErrorCode.LIMIT, "M-26");
    }

    @Test
    void htmlContextRulesResumeAfterTheRegion() {
      assertStructure(block("<svg><path/></svg><span>x"), "M-23");
      assertStructure(block("<svg><path/></svg><br></br>"), "M-22");
    }

    @Test
    void m24KStillAppliesToTheSvgStartTagInTableContext() {
      assertStructure(block("<table><svg></svg></table>"), "M-24");
      assertValid(block("<table><tr><td><svg><g></g></svg></td></tr></table>"));
    }
  }

  @Nested
  class M06Item4RegionRoot {
    @Test
    void iconInsideAnchorIsValid() {
      assertValid(body("<a href=\"#\"><svg><path d=\"M0\"></svg></a>" + P));
    }

    @Test
    void passedMarkStillReachesTheFormattingElement() {
      assertStructure(body("<a href=\"#\"><svg><g><path d=\"M0\"></g></svg></a>"), "M-06");
    }
  }

  @Nested
  class M11Items7To9 {
    @ParameterizedTest
    @ValueSource(
        strings = {
          "<html>",
          "<body>",
          "<head>",
          "</html>",
          "</body>",
          "</head>",
          "<svg><body></svg>",
          "<svg></head></svg>"
        })
    void htmlBodyAndHeadTagsAreInvalidInContent(String markup) {
      assertStructure(body("<noscript>" + markup + "</noscript>" + P), "M-11");
    }

    @Test
    void svgRegionInContentIsValid() {
      assertValid(
          body(
              "<noscript><img src=x><svg width=1 height=1><path d=M0></path></svg></noscript>"
                  + P));
      assertValid(body("<noscript><svg/><svg><g/></svg></noscript>" + P));
    }

    @Test
    void regionRootEndTagClosesTheRegionInContent() {
      assertValid(
          body("<noscript><a href=\"/\"><svg><path d=\"M0\"></svg>Home</a></noscript>" + P));
    }

    @Test
    void innerEndTagInContentRegionMustMatchTheTop() {
      assertStructure(body("<noscript><svg><g><path d=M0></g></svg></noscript>" + P), "M-11");
      assertStructure(body("<noscript><svg><svg><g></svg></svg></noscript>" + P), "M-11");
      assertStructure(body("<noscript><svg></g></svg></noscript>" + P), "M-11");
    }

    @Test
    void nestedSvgInContentIsValid() {
      assertValid(body("<noscript><svg><svg></svg></svg></noscript>" + P));
    }

    @Test
    void regionOpenAtTheEndOfContent() {
      assertStructure(body("<noscript><svg><g></noscript>" + P), "M-11");
      assertStructure(P + "<noscript><svg><g>", "M-11");
    }

    @Test
    void startTagInContentRegionNeverChangesTheTokenizerState() {
      // If `title` were RCDATA, the comment would be text and the content would be valid.
      assertStructure(body("<noscript><svg><title><!--x--></title></svg></noscript>" + P), "M-07");
    }

    @Test
    void mathAndTemplateInContentRegionAreInvalid() {
      assertStructure(body("<noscript><svg><math></math></svg></noscript>" + P), "M-11");
      assertStructure(body("<noscript><svg><template></template></svg></noscript>" + P), "M-11");
    }

    @Test
    void blockMarkerInContentRegionIsInvalid() {
      assertStructure(body("<noscript><svg><g data-rq-block=a></g></svg></noscript>" + P), "M-11");
    }

    @Test
    void tablePartInContentRegionNeedsATable() {
      assertStructure(body("<noscript><svg><tr></tr></svg></noscript>" + P), "M-11");
      assertStructure(body("<noscript><table></table><tr></tr></noscript>" + P), "M-11");
      assertValid(body("<noscript><table><tr><td>x</td></tr></table></noscript>" + P));
    }

    @Test
    void reservedScriptInContentRegion() {
      assertInvalid(
          body(
              "<noscript><svg><script type=\"application/rivqen-manifest+json\"></script></svg>"
                  + "</noscript>"),
          ErrorCode.RESERVED,
          "M-13");
    }

    @Test
    void breakoutTagInContentRegion() {
      assertStructure(body("<noscript><svg><p>x</p></svg></noscript>" + P), "M-07");
    }

    @Test
    void cdataRuleAppliesInContent() {
      assertStructure(body("<noscript><![CDATA[a>b]]></noscript>" + P), "M-12");
    }

    @Test
    void nestedFormInContentInsideAForm() {
      assertStructure(body("<form><noscript><form>x</form></noscript></form>" + P), "M-10");
    }

    @Test
    void tableInContentInsideATable() {
      assertStructure(body("<table><noscript><table></table></noscript></table>" + P), "M-09");
    }

    @Test
    void tableConditionTwoInContent() {
      assertStructure(
          body(
              "<object><a href=\"1\"><noscript><table><marquee><table></table></marquee></table>"
                  + "</noscript>x</a></object>"
                  + P),
          "M-09");
    }

    @Test
    void depthLimitInContentGivesLimit() {
      assertInvalid(
          body("<noscript>" + "<b>".repeat(255) + "</b>".repeat(255) + "</noscript>"),
          ErrorCode.LIMIT,
          "M-26");
    }

    @Test
    void mainPassContinuesWithItsOwnStateAfterTheEndTag() {
      // C leaves the `noscript` element dirty and adds to D; both go back.
      String c = "<noscript><span><svg><path d=M0></svg></span></noscript>";
      assertValid(body(c.repeat(300) + P));
      // A mark of the main pass is not changed by C: the `a` still has no mark.
      assertValid(body("<a href=x><noscript><b></b></noscript><i>y</i></a>" + P));
    }

    @Test
    void contentCrossingDoesNotLeaveMarks() {
      assertValid(body("<b><noscript><a href=x><svg><g></svg></a></noscript>x</b>" + P));
    }

    @Test
    void resultOfAnInvalidDocumentHasNoBlocks() {
      Result r = Docs.analyze(body("<noscript><svg><g></noscript>" + P));
      assertFalse(r.error() == null);
      assertTrue(r.blocks().isEmpty());
    }
  }
}
