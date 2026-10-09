// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import static dev.rivqen.tools.markup.Docs.assertInvalid;
import static dev.rivqen.tools.markup.Docs.assertStructure;
import static dev.rivqen.tools.markup.Docs.assertValid;
import static dev.rivqen.tools.markup.Docs.blocks;
import static dev.rivqen.tools.markup.Docs.body;
import static org.junit.jupiter.api.Assertions.assertEquals;

import java.util.List;
import org.junit.jupiter.api.Nested;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.ValueSource;

/** One group of tests for each rule of {@code markup.md} §2, on small documents. */
class RulesTest {

  private static final String P = "<p data-rq-block=\"a\">x</p>";

  @Nested
  class M03Tokenizer {
    @Test
    void greaterThanInQuotedValueDoesNotEndTheTag() {
      assertEquals(
          List.of("a:html:v"),
          blocks(body("<p title='>' data-rq-block=\"a\" x=\"<p data-rq-block=b>\">v</p>")));
    }

    @Test
    void tagLikeTextInCommentsIsNotABlock() {
      assertEquals(List.of("a:html:x"), blocks(body("<!-- <p data-rq-block=\"b\">y</p> -->" + P)));
    }

    @Test
    void shortCommentsEndEarly() {
      assertEquals(
          List.of("a:html:x", "b:html:y"),
          blocks(body("<!--><p data-rq-block=a>x</p><!---><p data-rq-block=b>y</p>")));
    }

    @Test
    void commentEndsAtDashDashBangGreaterThan() {
      assertEquals(List.of("a:html:x"), blocks(body("<!-- c --!>" + P)));
    }

    @Test
    void bogusCommentAndProcessingInstructionEndAtFirstGreaterThan() {
      assertEquals(
          List.of("a:html:x"),
          blocks(
              body(
                  "<!x <p data-rq-block=b>></ <p data-rq-block=c>><?pi <p data-rq-block=d>>" + P)));
    }

    @Test
    void doctypeEndsAtFirstGreaterThan() {
      assertEquals(List.of("a:html:x"), blocks("<!DOCTYPE html \"<p data-rq-block=b>\">" + P));
    }

    @Test
    void endTagAttributesAndSelfClosingFlagHaveNoEffect() {
      assertEquals(List.of("a:html:x"), blocks(body("<p data-rq-block=a>x</p data-rq-block=b/>")));
    }

    @Test
    void firstDuplicateAttributeWins() {
      assertEquals(
          List.of("a:html:x"), blocks(body("<p data-rq-block=a DATA-RQ-BLOCK=\"Bad Id\">x</p>")));
    }

    @Test
    void startTagEndsAfterMissingAttributeValue() {
      assertEquals(List.of("a:html:x"), blocks(body("<p data-rq-block=a b=>x</p>")));
    }

    @Test
    void attributeDirectlyAfterQuotedValue() {
      assertEquals(List.of("a:html:y"), blocks(body("<p data-rq-block=\"a\"class=x>y</p>")));
    }

    @Test
    void emptyEndTagEmitsNothing() {
      assertEquals(List.of("a:html:x</>"), blocks(body("<p data-rq-block=a>x</></p>")));
    }

    @Test
    void lessThanSignWithoutTagIsText() {
      assertEquals(List.of("a:html:1 < 2 <3"), blocks(body("<p data-rq-block=a>1 < 2 <3</p>")));
    }
  }

  @Nested
  class M04RawText {
    @ParameterizedTest
    @ValueSource(
        strings = {"title", "textarea", "iframe", "noembed", "noframes", "style", "xmp", "script"})
    void blockMarkupInRawTextIsText(String name) {
      assertEquals(
          List.of("a:html:x"),
          blocks(body("<" + name + "><p data-rq-block=b>y</p></" + name + ">" + P)));
    }

    @Test
    void plaintextNeverEnds() {
      assertEquals(List.of(), blocks(body("<plaintext></plaintext>" + P)));
    }

    @Test
    void selfClosingFlagStillSwitchesState() {
      assertEquals(List.of(), blocks(body("<title/><p data-rq-block=b>y</p>")));
    }

    @Test
    void endTagNameIsCaseInsensitiveAndNeedsATerminator() {
      assertEquals(
          List.of("t:html:a</titlex></tit>"),
          blocks("<title data-rq-block=t>a</titlex></tit></TITLE >"));
      assertEquals(List.of("t:html:a"), blocks("<title data-rq-block=t>a</title/>"));
    }

    @Test
    void scriptDataEscapedStatesHideEndTag() {
      assertEquals(
          List.of("a:html:x"),
          blocks(body("<script><!--<script>a</script><p data-rq-block=b>--></script>" + P)));
    }

    @Test
    void scriptDataEscapedEndTagEndsScript() {
      assertEquals(List.of("a:html:x"), blocks(body("<script><!-- a </script>" + P)));
    }

    @Test
    void startTagInForeignRegionDoesNotSwitchState() {
      assertStructure(body("<svg><title><g></g></title></svg>"), "M-07");
    }
  }

  @Nested
  class M05TokenStack {
    @Test
    void strayEndTagOutsideBlocksIsIgnored() {
      assertValid(body("</span></foo>" + P));
    }

    @Test
    void selfClosingSvgOpensNoRegion() {
      assertEquals(List.of("a:html:x"), blocks(body("<svg/>" + P)));
    }

    @Test
    void selfClosingMathOpensNoRegion() {
      assertEquals(List.of("a:html:x"), blocks(body("<math/>" + P)));
    }

    @Test
    void voidElementIsNotPushed() {
      assertValid(body("<div><br><img></div>" + P));
    }

    @Test
    void selfClosingFlagOnOtherHtmlElementIsIgnored() {
      // The div is pushed, so its end tag crosses the button.
      assertStructure(body("<div/><button></div>"), "M-06");
      assertValid(body("<div/>" + P + "</div>"));
    }

    @Test
    void selfClosingForeignElementIsNotPushed() {
      assertValid(body("<svg><g/><path/></svg>" + P));
    }

    @Test
    void elementAtTheBottomOfTheStackIsPoppedByItsEndTag() {
      // No html or body tags: the first element is at index 0 of the token stack.
      assertValid("<button></button><div data-rq-block=a><button>x</button></div>");
      assertValid("<a><a></a></a><div data-rq-block=k><a>x</a></div>");
    }

    @Test
    void blockAtTheBottomOfTheStack() {
      assertStructure("<div data-rq-block=a><span/></div>", "M-22");
      assertStructure("<div data-rq-block=a><!DOCTYPE x></div>", "M-22");
    }

    @Test
    void regionAtTheBottomOfTheStack() {
      assertStructure("<svg><p></svg>", "M-07");
      assertStructure("<svg></div></svg>", "M-07");
    }

    @Test
    void tableAtTheBottomOfTheStack() {
      assertStructure("<table><div data-rq-block=a>x</div>", "M-19");
    }

    @Test
    void regionEndsWithItsOwnEndTag() {
      assertEquals(List.of("a:html:x"), blocks(body("<svg><svg></svg></svg>" + P)));
    }

    @Test
    void tokenStackIgnoresImpliedEndTagsOutsideBlocks() {
      assertValid(body("<ul><li>a<li>b</ul><p>x<p>y<div data-rq-block=a>z</div>"));
    }
  }

  @Nested
  class M06Crossing {
    @ParameterizedTest
    @ValueSource(
        strings = {
          "a",
          "applet",
          "button",
          "caption",
          "colgroup",
          "marquee",
          "nobr",
          "object",
          "table",
          "tbody",
          "td",
          "template",
          "tfoot",
          "th",
          "thead",
          "tr"
        })
    void endTagMustNotCrossGuardedElement(String name) {
      assertStructure(body("<div><" + name + ">a</div>"), "M-06");
    }

    @Test
    void endTagCanCrossOtherElements() {
      assertValid(body("<div><span><b>a</div><ul><li>b</ul>" + P));
    }

    @Test
    void tableFamilyEndTagCanCrossTableParts() {
      assertValid(body("<table><tbody><tr><td>a</table><table><tr><td>b</tr></table>" + P));
    }

    @Test
    void tableFamilyEndTagCannotCrossOtherGuardedElements() {
      assertStructure(body("<table><tr><td><button>a</table>"), "M-06");
    }

    @Test
    void elementsPushedInForeignRegionAreNotGuarded() {
      assertValid(body("<svg><a><g></svg>" + P));
    }
  }

  @Nested
  class M07Foreign {
    @ParameterizedTest
    @ValueSource(
        strings = {
          "b",
          "big",
          "blockquote",
          "body",
          "br",
          "center",
          "code",
          "dd",
          "div",
          "dl",
          "dt",
          "em",
          "embed",
          "font",
          "h1",
          "h6",
          "head",
          "hr",
          "i",
          "img",
          "li",
          "listing",
          "menu",
          "meta",
          "nobr",
          "ol",
          "p",
          "pre",
          "ruby",
          "s",
          "small",
          "span",
          "strike",
          "strong",
          "sub",
          "sup",
          "table",
          "tt",
          "u",
          "ul",
          "var"
        })
    void breakoutStartTagInRegion(String name) {
      assertStructure(body("<svg><" + name + "></svg>"), "M-07");
      assertStructure(body("<math><mrow><" + name + "></math>"), "M-07");
    }

    @Test
    void strayEndTagInRegion() {
      assertStructure(body("<svg></g></svg>"), "M-07");
    }

    @Test
    void endTagAfterClosedIntegrationPointIsStray() {
      assertStructure(body("<svg><title>t</title></title></svg>"), "M-07");
    }

    @Test
    void endTagOfHtmlElementOutsideRegion() {
      assertStructure(body("<div><svg></div>"), "M-07");
    }

    @Test
    void endTagCanCrossRegionElements() {
      assertValid(body("<math><mrow><msup></math>" + P));
    }

    @ParameterizedTest
    @ValueSource(strings = {"<b>", "</g>", "<!-- c -->", "<![CDATA[x]]>", "<!DOCTYPE x>", "<g/>"})
    void integrationPointContentIsTextOnly(String token) {
      assertStructure(body("<svg><foreignObject>a" + token + "</foreignObject></svg>"), "M-07");
    }

    @ParameterizedTest
    @ValueSource(
        strings = {
          "annotation-xml",
          "desc",
          "foreignObject",
          "mi",
          "mn",
          "mo",
          "ms",
          "mtext",
          "title"
        })
    void integrationPointsWithTextAreValid(String name) {
      assertValid(body("<math><" + name + ">a &lt; b < c</" + name + "></math>" + P));
    }

    @Test
    void selfClosingIntegrationPointHasNoContent() {
      assertValid(body("<svg><foreignObject/><g></g></svg>" + P));
    }

    @Test
    void textElementInRegionMustNotBeSelfClosing() {
      assertStructure(body("<svg><style/></svg>"), "M-07");
      // title is in TEXT and in INTEGRATION; the TEXT rule applies.
      assertStructure(body("<svg><title/></svg>"), "M-07");
    }

    @Test
    void textElementContentInRegionIsTextOnly() {
      assertStructure(body("<svg><script>if (a<b) x()</script></svg>"), "M-07");
      assertValid(body("<svg><script>if (a < b) x()</script></svg>" + P));
    }

    @Test
    void endOfInputInIntegrationPointIsNoErrorByItself() {
      assertValid(P + "<svg><foreignObject>abc");
    }

    @Test
    void plaintextInRegion() {
      assertStructure(body("<svg><plaintext>a</plaintext></svg>"), "M-07");
    }
  }

  @Nested
  class M08Select {
    @Test
    void optionsGroupsAndCommentsAreAllowed() {
      assertValid(
          body(
              "<select><!-- c --><option>a<option>b<optgroup><option>c</optgroup><hr></select>"
                  + P));
    }

    @Test
    void otherStartTagInSelect() {
      assertStructure(body("<select><b>a</b></select>"), "M-08");
    }

    @Test
    void otherEndTagInSelect() {
      assertStructure(body("<div><select></div></select>"), "M-08");
    }

    @Test
    void doctypeInSelect() {
      assertStructure(body("<select><!DOCTYPE html></select>"), "M-08");
    }

    @Test
    void selectNeedsExplicitEndTag() {
      assertStructure(P + "<select><option>a", "M-08");
    }

    @Test
    void foreignSelectIsNotHtmlSelect() {
      assertValid(body("<svg><select><g></g></select></svg>" + P));
    }
  }

  @Nested
  class M09TableParts {
    @ParameterizedTest
    @ValueSource(
        strings = {
          "<table><tr><caption>",
          "<table><tbody><colgroup>",
          "<table><tbody><tbody>",
          "<table><thead><tfoot>",
          "<table><tr><thead>",
          "<table><tr><col>",
          "<table><colgroup><tr>",
          "<table><tr><tr>",
          "<table><tr><td><tr>",
          "<table><caption><td>",
          "<table><colgroup><td>",
          "<table><tr><td><td>",
          "<table><tr><th><th>",
          "<table><tr><td><col>"
        })
    void tablePartOutsideItsParent(String markup) {
      assertStructure(body(markup), "M-09");
    }

    @ParameterizedTest
    @ValueSource(
        strings = {
          "<table><caption>c</caption><colgroup><col></colgroup><col><thead><tr><th>h</th></tr></thead>"
              + "<tbody><tr><td>d</td></tr></tbody><tfoot><tr><td>f</td></tr></tfoot><tr><td>r</td></tr></table>",
          "<table><td>a</td></table>",
          "<td>a</td><tr></tr><caption></caption>",
          "<table><template><td>a</td></template></table>"
        })
    void tablePartsInTheirParents(String markup) {
      assertValid(body(markup + P));
    }
  }

  @Nested
  class M10Frameset {
    @Test
    void framesetStartTag() {
      assertStructure("<html><frameset><frame></frameset></html>", "M-10");
    }

    @Test
    void framesetInForeignRegion() {
      assertStructure(body("<svg><frameset></frameset></svg>"), "M-10");
    }

    @Test
    void framesetEndTagAlone() {
      assertValid(body("</frameset>" + P));
    }
  }

  @Nested
  class M11Noscript {
    @Test
    void typicalFallbacksAreValid() {
      assertEquals(
          List.of("a:html:x"),
          blocks(
              body(
                  "<noscript><iframe src=\"https://example.com/\"></iframe></noscript>"
                      + "<noscript><img src=x><style>a{}</style><title>t</title></noscript>"
                      + P)));
    }

    @Test
    void contentMustEndOutsideAComment() {
      assertStructure(body("<noscript><!-- </noscript>" + P + "<!-- --></noscript>"), "M-11");
    }

    @Test
    void contentMustEndOutsideATag() {
      assertStructure(body("<noscript><a title=\"</noscript>\">x</a>"), "M-11");
    }

    @Test
    void contentMustEndOutsideRawText() {
      assertStructure(body("<noscript><style></noscript>"), "M-11");
    }

    @Test
    void contentMustNotContainBlockMarkup() {
      assertStructure(body("<noscript><p data-rq-block=a>x</p></noscript>"), "M-11");
    }

    @ParameterizedTest
    @ValueSource(
        strings = {"frameset", "math", "noscript", "plaintext", "select", "svg", "template"})
    void contentMustNotContainForbiddenStartTag(String name) {
      assertStructure(body("<noscript><" + name + "></noscript>"), "M-11");
    }

    @Test
    void noscriptWithoutEndTagChecksTheRestOfTheInput() {
      assertValid(body("<noscript><img src=x>"));
      assertStructure(body("<noscript><!-- x"), "M-11");
    }

    @Test
    void noscriptInForeignRegionIsATextElement() {
      // Not RAWTEXT (M-04), but a TEXT name in a foreign region: text only (M-07 rule 4).
      assertValid(body("<svg><noscript>a &lt; b</noscript></svg>" + P));
      assertStructure(body("<svg><noscript><g></g></noscript></svg>"), "M-07");
    }
  }

  @Nested
  class M12Cdata {
    @Test
    void greaterThanInsideCdata() {
      assertStructure(body("<p><![CDATA[ a > b ]]></p>"), "M-12");
    }

    @Test
    void greaterThanAfterSingleBracket() {
      assertStructure(body("<svg><![CDATA[ a ]></svg>"), "M-12");
    }

    @Test
    void bracketsMustStartAfterTheOpener() {
      assertStructure(body("<svg><![CDATA[></svg>"), "M-12");
      assertStructure(body("<svg><![CDATA[]></svg>"), "M-12");
    }

    @Test
    void cdataWithoutGreaterThanInside() {
      assertValid(body("<svg><![CDATA[ a < b ]]><![CDATA[]]></svg>" + P));
    }

    @Test
    void cdataUntilEndOfInput() {
      assertValid(P + "<![CDATA[ a");
    }

    @Test
    void lowerCaseCdataIsABogusComment() {
      assertValid(body("<![cdata[ a > b ]]>" + P));
    }

    @Test
    void cdataInRawTextIsText() {
      assertValid(body("<style><![CDATA[ a > b ]]></style>" + P));
    }
  }

  @Nested
  class M13Reserved {
    @Test
    void manifestScript() {
      assertInvalid(
          body("<script type=\"application/rivqen-manifest+json\">{}</script>"),
          ErrorCode.RESERVED,
          "M-13");
    }

    @Test
    void manifestTypeAnyCase() {
      assertInvalid(
          body("<SCRIPT TYPE=' x/RIVQEN-Manifest '>{}</SCRIPT>"), ErrorCode.RESERVED, "M-13");
    }

    @Test
    void characterReferenceInType() {
      assertInvalid(
          body("<script type=\"text/javascript&#x20;\"></script>"), ErrorCode.RESERVED, "M-13");
    }

    @Test
    void onlyTheFirstTypeAttributeCounts() {
      assertValid(
          body("<script type=text/javascript type=application/rivqen-manifest+json></script>" + P));
    }

    @Test
    void otherElementsAndPartialNamesAreNotReserved() {
      assertValid(
          body(
              "<div type=\"application/rivqen-manifest+json\"></div>"
                  + "<script type=\"rivqen-manifes\"></script><script></script>"
                  + P));
    }

    @Test
    void scriptInForeignRegion() {
      assertInvalid(
          body("<svg><script type=\"rivqen-manifest\"></script></svg>"),
          ErrorCode.RESERVED,
          "M-13");
    }

    @Test
    void scriptMarkupInRawText() {
      assertValid(body("<textarea><script type=\"rivqen-manifest\"></textarea>" + P));
    }
  }

  @Nested
  class M14BlockStartTag {
    @Test
    void attributeNameIsCaseInsensitive() {
      assertEquals(
          List.of("a:html:x", "b:html:y"),
          blocks(body("<DIV DATA-RQ-BLOCK=a>x</DIV><P Data-Rq-Block=b>y</p>")));
    }

    @Test
    void similarAttributeNamesAreNotMarkers() {
      assertEquals(
          List.of(),
          blocks(body("<p data-rq-block-x=a data-rq-blocks=b data-rq=c =data-rq-block=d>x</p>")));
    }

    @Test
    void markerInAttributeValueIsNotABlock() {
      assertEquals(List.of(), blocks(body("<p title=\"<p data-rq-block=a>\">x</p>")));
    }
  }

  @Nested
  class M15Id {
    @ParameterizedTest
    @ValueSource(
        strings = {
          "a",
          "0",
          "a_b-c",
          "z9",
          "0123456789012345678901234567890123456789012345678901234567890123"
        })
    void validIds(String id) {
      assertEquals(List.of(id + ":html:x"), blocks(body("<p data-rq-block=\"" + id + "\">x</p>")));
    }

    @ParameterizedTest
    @ValueSource(
        strings = {
          "",
          "-a",
          "_a",
          "A",
          "aB",
          "a.b",
          "a b",
          "a/",
          "a:b",
          "é",
          "&#97;",
          "01234567890123456789012345678901234567890123456789012345678901234"
        })
    void invalidIds(String id) {
      assertInvalid(body("<p data-rq-block=\"" + id + "\">x</p>"), ErrorCode.INVALID_ID, "M-15");
    }

    @Test
    void attributeWithoutValueHasEmptyId() {
      assertInvalid(body("<p data-rq-block>x</p>"), ErrorCode.INVALID_ID, "M-15");
      assertInvalid(body("<p data-rq-block >x</p>"), ErrorCode.INVALID_ID, "M-15");
      assertInvalid(body("<p data-rq-block=>x</p>"), ErrorCode.INVALID_ID, "M-15");
    }
  }

  @Nested
  class M16BlockElement {
    @ParameterizedTest
    @ValueSource(
        strings = {
          "article", "aside", "div", "footer", "h1", "h2", "h3", "h4", "h5", "h6", "header", "main",
          "nav", "p", "section", "span"
        })
    void htmlBlockElements(String name) {
      assertEquals(
          List.of("a:html:x"), blocks(body("<" + name + " data-rq-block=a>x</" + name + ">")));
    }

    @Test
    void titleBlockKeepsRawContent() {
      assertEquals(
          List.of("t:html:a &amp; <b>"), blocks("<title data-rq-block=t>a &amp; <b></title>"));
    }

    @Test
    void jsonBlock() {
      assertEquals(
          List.of("j:json:{\"a\":[1,\"</p>\"]}"),
          blocks(
              body(
                  "<script type=\"application/json\" data-rq-block=j>{\"a\":[1,\"</p>\"]}</script>")));
    }

    @ParameterizedTest
    @ValueSource(
        strings = {
          "<li data-rq-block=a>x</li>",
          "<img data-rq-block=a>",
          "<style data-rq-block=a>x</style>",
          "<textarea data-rq-block=a>x</textarea>",
          "<template data-rq-block=a>x</template>",
          "<script data-rq-block=a>1</script>",
          "<script type=\"application/ld+json\" data-rq-block=a>1</script>",
          "<script type=\"Application/JSON\" data-rq-block=a>1</script>",
          "<script type=\"application/json \" data-rq-block=a>1</script>",
          "<script type=text/plain type=application/json data-rq-block=a>1</script>",
          "<svg><title data-rq-block=a>x</title></svg>",
          "<math><mrow data-rq-block=a>x</mrow></math>",
          "<foo data-rq-block=a>x</foo>"
        })
    void otherElementsCannotBeBlocks(String markup) {
      assertInvalid(body(markup), ErrorCode.FORBIDDEN_ELEMENT, "M-16");
    }
  }

  @Nested
  class M17Nesting {
    @Test
    void blockInBlockContent() {
      assertInvalid(
          body("<div data-rq-block=a><p data-rq-block=b>x</p></div>"), ErrorCode.NESTED, "M-17");
    }

    @Test
    void nestingIsCheckedBeforeTheInnerId() {
      assertInvalid(
          body("<div data-rq-block=a><p data-rq-block=\"B B\">x</p></div>"),
          ErrorCode.NESTED,
          "M-17");
    }

    @Test
    void blocksAfterEachOtherAreNotNested() {
      assertEquals(
          List.of("a:html:x", "b:html:y"),
          blocks(body("<div data-rq-block=a>x</div><div data-rq-block=b>y</div>")));
    }
  }

  @Nested
  class M18SelfClosing {
    @Test
    void selfClosingBlockStartTag() {
      assertStructure(body("<div data-rq-block=\"a\"/>x</div>"), "M-18");
    }

    @Test
    void slashInUnquotedValueIsNotSelfClosing() {
      assertInvalid(body("<div data-rq-block=a/>x</div>"), ErrorCode.INVALID_ID, "M-15");
    }
  }

  @Nested
  class M19Context {
    @Test
    void blockInsideTemplate() {
      assertStructure(body("<template><p data-rq-block=a>x</p></template>"), "M-19");
    }

    @Test
    void blockInsideSelect() {
      assertStructure(body("<select><option><p data-rq-block=a>x</p></select>"), "M-08");
    }

    @ParameterizedTest
    @ValueSource(
        strings = {
          "<table>",
          "<table><tbody>",
          "<table><thead>",
          "<table><tfoot>",
          "<table><tr>",
          "<table><colgroup>"
        })
    void blockDirectlyInTablePart(String markup) {
      assertStructure(body(markup + P), "M-19");
    }

    @ParameterizedTest
    @ValueSource(strings = {"<table><tr><td>", "<table><tr><th>", "<table><caption>"})
    void blockInCellOrCaption(String markup) {
      assertEquals(List.of("a:html:x"), blocks(body(markup + P + "</table>")));
    }

    @Test
    void blockAfterTemplateAndTable() {
      assertValid(body("<template><p>x</p></template><table><tr><td>a</td></tr></table>" + P));
    }

    @Test
    void blockInDivInsideTable() {
      // F is the table: the div does not stop the search.
      assertStructure(body("<table><div>" + P), "M-19");
    }
  }

  @Nested
  class M20EndTag {
    @Test
    void endOfInputInHtmlBlock() {
      assertStructure("<div data-rq-block=a>abc", "M-20");
    }

    @Test
    void endOfInputInsideTheEndTag() {
      assertStructure("<div data-rq-block=a>abc</div", "M-20");
      assertStructure("<div data-rq-block=a>abc</div class=\">", "M-20");
    }

    @Test
    void endOfInputInTitleBlock() {
      assertStructure("<title data-rq-block=t>abc</title", "M-20");
    }

    @Test
    void endOfInputInJsonBlock() {
      assertStructure("<script type=application/json data-rq-block=j>[1]", "M-20");
    }

    @Test
    void endTagWithAttributesEndsRawTextBlock() {
      assertEquals(List.of("t:html:a"), blocks("<title data-rq-block=t>a</title class=\"x\">"));
    }

    @Test
    void blockEndsAtItsOwnEndTagWhenItIsTheCurrentNode() {
      assertEquals(
          List.of("a:html:<div>x</div>y"),
          blocks(body("<div data-rq-block=a><div>x</div>y</div>")));
    }
  }

  @Nested
  class M21Duplicate {
    @Test
    void sameIdTwice() {
      assertInvalid(body(P + P), ErrorCode.DUPLICATE, "M-21");
    }

    @Test
    void sameIdOnDifferentFormats() {
      assertInvalid(
          "<title data-rq-block=a>t</title><script type=application/json data-rq-block=a>1</script>",
          ErrorCode.DUPLICATE,
          "M-21");
    }
  }

  @Nested
  class M22ContentTokens {
    @Test
    void doctypeInContent() {
      assertStructure(body("<div data-rq-block=a><!DOCTYPE html></div>"), "M-22");
    }

    @ParameterizedTest
    @ValueSource(
        strings = {
          "base",
          "basefont",
          "bgsound",
          "body",
          "embed",
          "frame",
          "head",
          "html",
          "iframe",
          "link",
          "math",
          "meta",
          "noembed",
          "noframes",
          "noscript",
          "object",
          "optgroup",
          "option",
          "plaintext",
          "script",
          "select",
          "style",
          "svg",
          "template",
          "textarea",
          "title",
          "xmp"
        })
    void forbiddenContentElement(String name) {
      assertStructure(body("<div data-rq-block=a><" + name + "></div>"), "M-22");
    }

    @ParameterizedTest
    @ValueSource(strings = {"<span/>", "<foo/>", "<div/>"})
    void selfClosingNonVoidElement(String tag) {
      assertStructure(body("<div data-rq-block=a>" + tag + "</div>"), "M-22");
    }

    @Test
    void selfClosingVoidElement() {
      assertValid(body("<div data-rq-block=a><br/><img src=x /></div>"));
    }

    @ParameterizedTest
    @ValueSource(strings = {"</br>", "</img>", "</col>", "</input>"})
    void endTagOfVoidElement(String tag) {
      assertStructure(body("<div data-rq-block=a>" + tag + "</div>"), "M-22");
    }

    @Test
    void commentsCharacterReferencesAndOtherElements() {
      assertValid(
          body("<div data-rq-block=a><!-- c --><?pi?>&amp;<custom-el x=y>z</custom-el></div>"));
    }
  }

  @Nested
  class M23Balance {
    @Test
    void elementOpenAtBlockEnd() {
      assertStructure(body("<div data-rq-block=a><span>x</div>"), "M-23");
    }

    @Test
    void strayEndTagInContent() {
      assertStructure(body("<div data-rq-block=a>x</span></div>"), "M-23");
    }

    @Test
    void endTagOfElementOpenedBeforeBlock() {
      assertStructure(body("<span><div data-rq-block=a>x</span></div>"), "M-23");
    }

    @Test
    void optionalEndTagsMustBeExplicitInContent() {
      assertStructure(body("<div data-rq-block=a><ul><li>a<li>b</ul></div>"), "M-23");
      assertValid(body("<div data-rq-block=a><ul><li>a</li><li>b</li></ul></div>"));
    }
  }

  @Nested
  class M24ImpliedEnd {
    @ParameterizedTest
    @ValueSource(strings = {"p", "span"})
    void pcloseStartTagInParagraphOrSpanBlock(String block) {
      for (String inner : List.of("div", "ul", "p", "table", "hr", "h2", "address", "pre")) {
        assertStructure(
            body("<" + block + " data-rq-block=a><" + inner + ">x</" + inner + "></" + block + ">"),
            "M-24");
      }
    }

    @Test
    void pcloseStartTagInDivBlock() {
      assertValid(body("<div data-rq-block=a><p>x</p><div>y</div><hr></div>"));
    }

    @Test
    void headingInHeadingBlock() {
      assertStructure(body("<h1 data-rq-block=a><h2>x</h2></h1>"), "M-24");
    }

    @Test
    void headingInHeadingOpenedInContent() {
      assertStructure(body("<div data-rq-block=a><h2>x<h3>y</h3></h2></div>"), "M-24");
    }

    @ParameterizedTest
    @ValueSource(strings = {"h1", "h2", "h3", "h4", "h5", "h6"})
    void headingWhileAnyHeadingLevelIsOpenInContent(String open) {
      assertStructure(
          body("<div data-rq-block=a><" + open + ">x<h1>y</h1></" + open + "></div>"), "M-24");
    }

    @Test
    void headingsAfterEachOther() {
      assertValid(body("<h1><div data-rq-block=a><h2>x</h2><h3>y</h3></div></h1>"));
    }

    @ParameterizedTest
    @ValueSource(strings = {"li", "dd", "dt"})
    void listItemWithoutListInContent(String item) {
      assertStructure(
          body("<ul><li><div data-rq-block=a><" + item + ">x</" + item + "></div></li></ul>"),
          "M-24");
    }

    @ParameterizedTest
    @ValueSource(strings = {"ul", "ol", "menu", "dl"})
    void listItemInListOpenedInContent(String list) {
      assertValid(
          body(
              "<div data-rq-block=a><"
                  + list
                  + "><li>x</li><dd>y</dd><dt>z</dt></"
                  + list
                  + "></div>"));
    }

    @Test
    void listItemAfterListClosed() {
      assertStructure(body("<div data-rq-block=a><ul></ul><li>x</li></div>"), "M-24");
    }

    @ParameterizedTest
    @ValueSource(strings = {"button", "a", "nobr"})
    void formattingOrButtonWhileOpen(String name) {
      assertStructure(
          body(
              "<"
                  + name
                  + "><div data-rq-block=b><"
                  + name
                  + ">x</"
                  + name
                  + "></div></"
                  + name
                  + ">"),
          "M-24");
      assertStructure(
          body(
              "<div data-rq-block=b><"
                  + name
                  + "><"
                  + name
                  + ">x</"
                  + name
                  + "></"
                  + name
                  + "></div>"),
          "M-24");
      assertValid(
          body(
              "<div data-rq-block=b><"
                  + name
                  + ">x</"
                  + name
                  + "><"
                  + name
                  + ">y</"
                  + name
                  + "></div>"));
    }

    @ParameterizedTest
    @ValueSource(
        strings = {"caption", "colgroup", "tbody", "td", "tfoot", "th", "thead", "tr", "col"})
    void tablePartWithoutTableInContent(String name) {
      assertStructure(body("<div data-rq-block=a><" + name + "></div>"), "M-24");
    }

    @Test
    void tableInContent() {
      assertValid(
          body(
              "<div data-rq-block=a><table><colgroup><col></colgroup><tr><td>x</td></tr></table></div>"));
    }

    @Test
    void rubyPartWithoutRubyInContent() {
      assertStructure(body("<ruby>a<p data-rq-block=a><rt>b</rt></p></ruby>"), "M-24");
      assertStructure(body("<ruby>a<p data-rq-block=a><rb>b</rb></p></ruby>"), "M-24");
      assertStructure(body("<ruby>a<p data-rq-block=a><rtc>b</rtc></p></ruby>"), "M-24");
      assertStructure(body("<ruby>a<p data-rq-block=a><rp>b</rp></p></ruby>"), "M-24");
    }

    @ParameterizedTest
    @ValueSource(strings = {"rb", "rp", "rt"})
    void rbOrRtcWhileRubyPartOpen(String open) {
      assertStructure(
          body("<div data-rq-block=a><ruby><" + open + ">x<rb>y</rb></" + open + "></ruby></div>"),
          "M-24");
      assertStructure(
          body(
              "<div data-rq-block=a><ruby><" + open + ">x<rtc>y</rtc></" + open + "></ruby></div>"),
          "M-24");
    }

    @Test
    void rbWhileRtcOpen() {
      assertStructure(
          body("<div data-rq-block=a><ruby><rtc>x<rb>y</rb></rtc></ruby></div>"), "M-24");
    }

    @ParameterizedTest
    @ValueSource(strings = {"rb", "rp", "rt"})
    void rpOrRtWhileRubyPartOpen(String open) {
      assertStructure(
          body("<div data-rq-block=a><ruby><" + open + ">x<rt>y</rt></" + open + "></ruby></div>"),
          "M-24");
      assertStructure(
          body("<div data-rq-block=a><ruby><" + open + ">x<rp>y</rp></" + open + "></ruby></div>"),
          "M-24");
    }

    @Test
    void rtInsideRtcIsAllowed() {
      assertValid(body("<div data-rq-block=a><ruby>a<rtc><rt>b</rt><rp>c</rp></rtc></ruby></div>"));
    }

    @Test
    void rubyPartsAfterEachOther() {
      assertValid(
          body(
              "<div data-rq-block=a><ruby><rb>a</rb><rp>(</rp><rt>b</rt><rp>)</rp><rtc>c</rtc></ruby></div>"));
    }
  }

  @Nested
  class M25Json {
    @Test
    void invalidJson() {
      assertInvalid(
          body("<script type=application/json data-rq-block=j>[1,]</script>"),
          ErrorCode.BAD_JSON,
          "M-25");
    }

    @Test
    void emptyJson() {
      assertInvalid(
          body("<script type=application/json data-rq-block=j></script>"),
          ErrorCode.BAD_JSON,
          "M-25");
    }
  }

  @Nested
  class M27Result {
    @Test
    void offsetsAreUtf8BytesAfterByteOrderMark() {
      byte[] in = Docs.utf8(Docs.BOM + "<p>😀</p><p data-rq-block=a>é\r\n</p>");
      Result r = Analyzer.analyze(in);
      assertEquals(1, r.blocks().size());
      Result.Block b = r.blocks().get(0);
      assertEquals(3 + 3 + 4 + 4 + 19, b.start());
      assertEquals(b.start() + 4, b.end());
    }

    @Test
    void emptyDocumentIsValidWithoutBlocks() {
      Result r = Docs.analyze("");
      assertEquals(null, r.error());
      assertEquals(List.of(), r.blocks());
      assertEquals("t1.", r.templateRevision().substring(0, 3));
      assertEquals("r1.", r.pageRevision().substring(0, 3));
    }
  }
}
