// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.CsvSource;

/** Token boundaries of the WHATWG tokenizer subset (M-03, M-04). */
class TokenizerTest {

  /**
   * Tokenizes {@code s} and returns each token as {@code TYPE[name]@start-end} ({@code name} for
   * tags). Raw text names switch the state as M-04 does in HTML context.
   */
  static List<String> tokens(String s) {
    byte[] in = s.getBytes(StandardCharsets.UTF_8);
    Tokenizer t = new Tokenizer(in, 0, in.length);
    List<String> out = new ArrayList<>();
    while (true) {
      Tokenizer.Type type = t.next();
      String name =
          type == Tokenizer.Type.START_TAG || type == Tokenizer.Type.END_TAG
              ? "[" + t.name() + (t.selfClosing() ? "/" : "") + "]"
              : "";
      out.add(type + name + "@" + t.start() + "-" + t.end());
      if (type == Tokenizer.Type.EOF) {
        return out;
      }
      if (type == Tokenizer.Type.START_TAG) {
        Name k = Name.of(t.name());
        if (k != null && k.text != null) {
          t.enterText(k.text, k.tag);
        }
      }
    }
  }

  private static Tokenizer first(String s) {
    byte[] in = s.getBytes(StandardCharsets.UTF_8);
    Tokenizer t = new Tokenizer(in, 0, in.length);
    t.next();
    return t;
  }

  private static String blockValue(String tag) {
    Tokenizer t = first(tag);
    byte[] in = tag.getBytes(StandardCharsets.UTF_8);
    return t.hasBlockAttribute()
        ? new String(
            in,
            t.blockValueStart(),
            t.blockValueEnd() - t.blockValueStart(),
            StandardCharsets.UTF_8)
        : "<none>";
  }

  private static String typeValue(String tag) {
    Tokenizer t = first(tag);
    byte[] in = tag.getBytes(StandardCharsets.UTF_8);
    return t.hasTypeAttribute()
        ? new String(
            in, t.typeValueStart(), t.typeValueEnd() - t.typeValueStart(), StandardCharsets.UTF_8)
        : "<none>";
  }

  @Test
  void tagsAndText() {
    assertEquals(
        List.of("START_TAG[p]@1-4", "END_TAG[p]@5-9", "START_TAG[br/]@9-14", "EOF@15-15"),
        tokens("a<p>b</p><BR/>c"));
  }

  @Test
  void tagNamesStartWithAnyAsciiLetter() {
    assertEquals(
        List.of(
            "START_TAG[a]@0-3",
            "START_TAG[z]@3-6",
            "START_TAG[z]@6-9",
            "END_TAG[za]@9-14",
            "EOF@14-14"),
        tokens("<A><Z><z></Za>"));
    assertEquals(List.of("START_TAG[a]@0-6", "EOF@6-6"), tokens("<a b=>"));
  }

  @Test
  void lessThanSignVariants() {
    assertEquals(List.of("EOF@7-7"), tokens("< <1 <<"));
    assertEquals(List.of("START_TAG[a]@1-4", "EOF@4-4"), tokens("<<a>"));
    assertEquals(List.of("EOF@2-2"), tokens("</"));
    assertEquals(List.of("START_TAG[a]@3-6", "EOF@6-6"), tokens("</><a>"));
  }

  @Test
  void tagNameIsLowerCasedAndNulBecomesReplacementCharacter() {
    Tokenizer t = first("<dIV\0x>");
    assertEquals("divï¿½x", t.name());
  }

  @Test
  void tagAtEndOfInputIsDropped() {
    assertEquals(List.of("EOF@11-11"), tokens("<div class="));
    assertEquals(List.of("EOF@4-4"), tokens("<div"));
    assertEquals(List.of("EOF@7-7"), tokens("<div a/"));
    assertEquals(List.of("EOF@8-8"), tokens("<div a='"));
    assertEquals(List.of("EOF@7-7"), tokens("<div a="));
    assertEquals(List.of("EOF@6-6"), tokens("<div a"));
    assertEquals(List.of("EOF@9-9"), tokens("<div a=b "));
    assertEquals(List.of("EOF@8-8"), tokens("<div a=b"));
    assertEquals(List.of("EOF@9-9"), tokens("<div a=\"\""));
  }

  @ParameterizedTest
  @CsvSource(
      delimiter = '|',
      value = {
        "<p data-rq-block=\"a b\">|a b",
        "<p data-rq-block='a\"b'>|a\"b",
        "<p data-rq-block=a/b>|a/b",
        "<p data-rq-block=a>|a",
        "<p data-rq-block>|''",
        "<p data-rq-block/>|''",
        "<p data-rq-block =  \"x\">|x",
        "<p DATA-RQ-BLOCK=x data-rq-block=y>|x",
        "<p data-rq-block data-rq-block=y>|''",
        "<p x=\"a>b\" data-rq-block='c>d'>|c>d",
        "<p data-rq-block=\"a\"data-x=y>|a",
        "<p/data-rq-block=a>|a",
        "<p a='1'/data-rq-block=a>|a",
        "<p data-rq-block-x=a>|<none>",
        "<p =data-rq-block=a>|<none>",
        "<p data-rq-block=&amp;>|&amp;",
        "<p data-rq-block=a\tb>|a",
        "<p data-rq-block=\"\">|''"
      })
  void blockAttributeRawValue(String tag, String value) {
    assertEquals(value.equals("''") ? "" : value, blockValue(tag));
  }

  @Test
  void typeAttributeRawValue() {
    assertEquals("application/json", typeValue("<script type=application/json>"));
    assertEquals("a", typeValue("<script TYPE='a' type=b>"));
    assertEquals("", typeValue("<script type>"));
    assertEquals("<none>", typeValue("<script types=a>"));
  }

  @Test
  void selfClosingFlag() {
    assertTrue(first("<br/>").selfClosing());
    assertTrue(first("<a x=1 />").selfClosing());
    assertTrue(first("<a x='1'/>").selfClosing());
    assertFalse(first("<a / >").selfClosing());
    assertFalse(first("<a/x>").selfClosing());
    assertFalse(first("<a x=1/>").selfClosing());
  }

  @Test
  void comments() {
    assertEquals(List.of("COMMENT@0-5", "EOF@5-5"), tokens("<!-->"));
    assertEquals(List.of("COMMENT@0-6", "EOF@6-6"), tokens("<!--->"));
    assertEquals(List.of("COMMENT@0-7", "EOF@7-7"), tokens("<!---->"));
    assertEquals(List.of("COMMENT@0-10", "EOF@10-10"), tokens("<!-- a -->"));
    assertEquals(List.of("COMMENT@0-11", "EOF@11-11"), tokens("<!-- a --!>"));
    assertEquals(List.of("COMMENT@0-15", "EOF@15-15"), tokens("<!-- a --!- -->"));
    assertEquals(List.of("COMMENT@0-17", "EOF@17-17"), tokens("<!-- a > - -- -->"));
    assertEquals(List.of("COMMENT@0-14", "EOF@14-14"), tokens("<!-- a --x -->"));
    assertEquals(List.of("COMMENT@0-14", "EOF@14-14"), tokens("<!-- a --!x-->"));
    assertEquals(List.of("COMMENT@0-9", "EOF@9-9"), tokens("<!--a--->"));
    assertEquals(List.of("COMMENT@0-14", "EOF@14-14"), tokens("<!--<!-- x -->"));
    assertEquals(List.of("COMMENT@0-8", "EOF@8-8"), tokens("<!--x-y>"));
    assertEquals(List.of("COMMENT@0-8", "EOF@8-8"), tokens("<!---x>-"));
    assertEquals(List.of("COMMENT@0-6", "EOF@6-6"), tokens("<!-- a"));
    assertEquals(List.of("COMMENT@0-5", "EOF@5-5"), tokens("<!-- "));
  }

  @ParameterizedTest
  @CsvSource({"<!---!>x-->", "<!--a->b-->", "<!--a--x>y-->", "<!--a--!x->y-->", "<!--!>x-->"})
  void commentsThatDoNotEndEarly(String s) {
    assertEquals(
        List.of("COMMENT@0-" + s.length(), "EOF@" + s.length() + "-" + s.length()), tokens(s));
  }

  @Test
  void endOfInputInsideCommentStates() {
    assertEquals(List.of("COMMENT@0-3", "EOF@3-3"), tokens("<!-"));
    assertEquals(List.of("COMMENT@0-4", "EOF@4-4"), tokens("<!--"));
    assertEquals(List.of("COMMENT@0-5", "EOF@5-5"), tokens("<!---"));
    assertEquals(List.of("COMMENT@0-7", "EOF@7-7"), tokens("<!--a--"));
    assertEquals(List.of("COMMENT@0-8", "EOF@8-8"), tokens("<!--a--!"));
  }

  @Test
  void bogusCommentsAndProcessingInstructions() {
    assertEquals(List.of("COMMENT@0-3", "EOF@3-3"), tokens("<!>"));
    assertEquals(List.of("COMMENT@0-6", "EOF@6-6"), tokens("<!-x->"));
    assertEquals(List.of("COMMENT@0-5", "EOF@5-5"), tokens("<!--x".substring(0, 3) + "a>"));
    assertEquals(List.of("COMMENT@0-6", "EOF@6-6"), tokens("<?a b>"));
    assertEquals(List.of("COMMENT@0-4", "EOF@4-4"), tokens("</ >"));
    assertEquals(List.of("COMMENT@0-4", "EOF@4-4"), tokens("</1>"));
    assertEquals(List.of("COMMENT@0-2", "EOF@2-2"), tokens("<!"));
    assertEquals(List.of("COMMENT@0-3", "EOF@3-3"), tokens("<?x"));
  }

  @Test
  void doctype() {
    assertEquals(List.of("DOCTYPE@0-15", "EOF@15-15"), tokens("<!DOCTYPE html>"));
    assertEquals(List.of("DOCTYPE@0-10", "EOF@10-10"), tokens("<!doctype>"));
    assertEquals(List.of("DOCTYPE@0-15", "EOF@19-19"), tokens("<!DocType a \"b>\" c>"));
    assertEquals(List.of("DOCTYPE@0-9", "EOF@9-9"), tokens("<!DOCTYPE"));
    assertEquals(List.of("COMMENT@0-6", "EOF@6-6"), tokens("<!DOCT"));
  }

  @Test
  void cdata() {
    Tokenizer t = first("<![CDATA[a]]>");
    assertTrue(t.cdata());
    assertTrue(t.cdataClosed());
    assertEquals(13, t.end());
    t = first("<![CDATA[a>]]>");
    assertTrue(t.cdata());
    assertFalse(t.cdataClosed());
    assertEquals(11, t.end());
    t = first("<![CDATA[a");
    assertTrue(t.cdata());
    assertTrue(t.cdataClosed());
    assertTrue(first("<![CDATA[").cdata());
    assertFalse(first("<![CDATA").cdata());
    assertFalse(first("<!x>").cdata());
  }

  @Test
  void rcdataAndRawtextEndAtTheAppropriateEndTag() {
    assertEquals(
        List.of("START_TAG[title]@0-7", "END_TAG[title]@26-36", "EOF@36-36"),
        tokens("<title><b></titl></title1></title  >"));
    assertEquals(
        List.of("START_TAG[style]@0-7", "END_TAG[style/]@11-20", "EOF@20-20"),
        tokens("<style>a<b></STYLE/>"));
    assertEquals(List.of("START_TAG[textarea]@0-10", "EOF@20-20"), tokens("<textarea></textarea"));
    assertEquals(
        List.of("START_TAG[xmp]@0-5", "END_TAG[xmp]@8-14", "EOF@14-14"), tokens("<xmp></<</xmp>"));
    assertEquals(List.of("START_TAG[iframe]@0-8", "EOF@9-9"), tokens("<iframe><"));
  }

  @Test
  void plaintextRunsToTheEnd() {
    assertEquals(
        List.of("START_TAG[plaintext]@0-11", "EOF@26-26"), tokens("<plaintext></plaintext><a>"));
  }

  @Test
  void scriptData() {
    assertEquals(
        List.of("START_TAG[script]@0-8", "END_TAG[script]@13-22", "EOF@22-22"),
        tokens("<script>a<b>c</script>"));
    assertEquals(
        List.of("START_TAG[script]@0-8", "END_TAG[script]@25-34", "EOF@34-34"),
        tokens("<script></scrip</scriptx></script>"));
    assertEquals(
        List.of("START_TAG[script]@0-8", "END_TAG[script]@11-20", "EOF@20-20"),
        tokens("<script><!-</script>"));
  }

  @Test
  void scriptDataEscapedState() {
    // <!-- enters the escaped state; </script> still ends the script.
    assertEquals(
        List.of("START_TAG[script]@0-8", "END_TAG[script]@14-23", "EOF@23-23"),
        tokens("<script><!-- a</script>"));
    // --> leaves the escaped state.
    assertEquals(
        List.of("START_TAG[script]@0-8", "END_TAG[script]@16-25", "EOF@25-25"),
        tokens("<script><!---->x</script>"));
  }

  @ParameterizedTest
  @CsvSource({
    "<script><!--a-><script></script>x</script>",
    "<script><!--a>b<script></script>c</script>",
    "<script><!--a--b<script></script>c</script>",
    "<script><!--a-b<script></script>c</script>"
  })
  void scriptDataEscapedStateEndsOnlyAtDashDashGreaterThan(String s) {
    // Still escaped: <script> starts the double-escaped state, so the first </script> is text.
    int end = s.lastIndexOf("</script>");
    assertEquals(
        List.of(
            "START_TAG[script]@0-8",
            "END_TAG[script]@" + end + "-" + s.length(),
            "EOF@" + s.length() + "-" + s.length()),
        tokens(s));
  }

  @Test
  void scriptDataDoubleEscapedState() {
    // <!--<script> enters the double-escaped state: </script> does not end the script.
    String s = "<script><!--<script></script>--></script>";
    assertEquals(List.of("START_TAG[script]@0-8", "END_TAG[script]@32-41", "EOF@41-41"), tokens(s));
    // </script> in the double-escaped state goes back to the escaped state.
    String s2 = "<script><!--<script></script></script>";
    assertEquals(
        List.of("START_TAG[script]@0-8", "END_TAG[script]@29-38", "EOF@38-38"), tokens(s2));
    // <scriptx> does not start the double-escaped state.
    String s3 = "<script><!--<scriptx></script>";
    assertEquals(
        List.of("START_TAG[script]@0-8", "END_TAG[script]@21-30", "EOF@30-30"), tokens(s3));
    // --> in the double-escaped state leaves both escaped states.
    String s4 = "<script><!--<script>--></script>";
    assertEquals(
        List.of("START_TAG[script]@0-8", "END_TAG[script]@23-32", "EOF@32-32"), tokens(s4));
  }

  @Test
  void scriptDataEscapedDashStates() {
    String s = "<script><!--a-b--c---><x></script>";
    assertEquals(List.of("START_TAG[script]@0-8", "END_TAG[script]@25-34", "EOF@34-34"), tokens(s));
    String s2 = "<script><!--a-<b--<c</script>";
    assertEquals(
        List.of("START_TAG[script]@0-8", "END_TAG[script]@20-29", "EOF@29-29"), tokens(s2));
  }

  @Test
  void scriptDataDoubleEscapedDashStates() {
    String s = "<script><!--<script>-a--b---<c-<d--<e</x</scrip></script>--></script>";
    assertEquals(List.of("START_TAG[script]@0-8", "END_TAG[script]@60-69", "EOF@69-69"), tokens(s));
  }

  @Test
  void scriptDataEndOfInput() {
    assertEquals(List.of("START_TAG[script]@0-8", "EOF@20-20"), tokens("<script><!--<script>"));
    assertEquals(List.of("START_TAG[script]@0-8", "EOF@14-14"), tokens("<script><!--<s"));
    assertEquals(List.of("START_TAG[script]@0-8", "EOF@22-22"), tokens("<script><!--<script></"));
    assertEquals(List.of("START_TAG[script]@0-8", "EOF@15-15"), tokens("<script></scrip"));
  }

  @Test
  void endTagOfRawTextCanHaveAttributes() {
    assertEquals(
        List.of("START_TAG[title]@0-7", "END_TAG[title]@7-21", "EOF@21-21"),
        tokens("<title></title a=\">\">"));
  }

  @Test
  void startOffsetSkipsByteOrderMark() {
    byte[] in = Docs.utf8(Docs.BOM + "<a>");
    Tokenizer t = new Tokenizer(in, 3, in.length);
    assertEquals(Tokenizer.Type.START_TAG, t.next());
    assertEquals(3, t.start());
  }
}
