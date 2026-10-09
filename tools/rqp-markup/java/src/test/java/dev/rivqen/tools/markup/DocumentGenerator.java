// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import java.util.List;
import java.util.Random;

/**
 * Generates random documents that are valid for RQP by construction: balanced explicit tags, blocks
 * only where M-16 to M-24 allow them, valid JSON in json blocks. It also produces the shapes that
 * M-24 j and k and the edge-case decisions (H-21) keep valid: text and comments directly in a
 * table, lists inside list items, CDATA sections in SVG text elements.
 */
final class DocumentGenerator {
  private static final List<String> TEXT =
      List.of(
          "x",
          "a &amp; b",
          "é",
          "1 < 2",
          "\r\n",
          "  ",
          "&#x1F600;",
          new String(Character.toChars(0x1F600)));
  private static final List<String> PHRASING = List.of("b", "i", "em", "code", "small", "span");
  private static final List<String> BLOCK_NAMES =
      List.of(
          "div", "section", "article", "aside", "header", "footer", "main", "nav", "p", "span",
          "h2");

  private final Random r;
  private final StringBuilder s = new StringBuilder();
  private int blocks;

  DocumentGenerator(Random r) {
    this.r = r;
  }

  String document() {
    s.append(r.nextBoolean() ? "<!DOCTYPE html>" : "");
    s.append("<html><head>");
    if (r.nextInt(3) == 0) {
      s.append("<title data-rq-block=").append(id()).append('>');
      text();
      s.append("</title>");
    } else {
      s.append("<title>t</title>");
    }
    s.append("</head><body>");
    int n = 1 + r.nextInt(5);
    for (int i = 0; i < n; i++) {
      outside(0);
    }
    s.append("</body></html>");
    return s.toString();
  }

  private String id() {
    return "b" + blocks++;
  }

  private void text() {
    s.append(TEXT.get(r.nextInt(TEXT.size())));
  }

  /** Markup outside blocks: containers, blocks, json blocks, text, comments. */
  private void outside(int depth) {
    int k = r.nextInt(depth > 3 ? 4 : 9);
    switch (k) {
      case 0 -> text();
      case 1 -> s.append("<!-- c -->");
      case 2 -> block();
      case 3 -> json();
      case 4 -> {
        s.append("<div>");
        int n = 1 + r.nextInt(3);
        for (int i = 0; i < n; i++) {
          outside(depth + 1);
        }
        s.append("</div>");
      }
      case 5 -> {
        s.append("<ul><li>");
        outside(depth + 1);
        s.append("</li></ul>");
      }
      case 6 -> {
        s.append("<table><tbody><tr><td>");
        outside(depth + 1);
        s.append("</td></tr></tbody></table>");
      }
      case 7 -> {
        s.append("<svg viewBox=\"0 0 1 1\"><title>i</title>");
        if (r.nextBoolean()) {
          s.append("<style><![CDATA[.a{fill:red}]]></style><desc>a < b<![CDATA[c]]></desc>");
        }
        s.append("<path d=\"M0 0\"/></svg>");
        block();
      }
      default -> {
        s.append("<noscript><img src=x></noscript><p>");
        text();
        s.append("</p>");
      }
    }
  }

  private void block() {
    String name = BLOCK_NAMES.get(r.nextInt(BLOCK_NAMES.size()));
    s.append('<').append(name).append(" class=k data-rq-block=").append(id()).append('>');
    boolean phrasingOnly = name.equals("p") || name.equals("span") || name.equals("h2");
    int n = r.nextInt(4);
    for (int i = 0; i < n; i++) {
      content(phrasingOnly, 0);
    }
    s.append("</").append(name).append('>');
  }

  /** Content of an html block (M-22 to M-24). */
  private void content(boolean phrasingOnly, int depth) {
    int k = r.nextInt(depth > 2 ? 3 : (phrasingOnly ? 4 : 8));
    switch (k) {
      case 0 -> text();
      case 1 -> s.append(r.nextBoolean() ? "<br>" : "<img src=a.png alt=\"\">");
      case 2 -> s.append("<!-- n -->");
      case 3 -> {
        String p = PHRASING.get(r.nextInt(PHRASING.size()));
        s.append('<').append(p).append('>');
        content(true, depth + 1);
        s.append("</").append(p).append('>');
      }
      case 4 -> {
        s.append("<div>");
        content(false, depth + 1);
        s.append("</div>");
      }
      case 5 -> {
        if (r.nextBoolean()) {
          s.append("<ul><li>");
          content(false, depth + 1);
          s.append("</li><li>x</li></ul>");
        } else {
          s.append("<dl><dt>t</dt><dd>");
          content(false, depth + 1);
          s.append("</dd></dl>");
        }
      }
      case 6 -> {
        boolean body = r.nextBoolean();
        s.append(body ? "<table><tbody><tr><td>" : "<table>t<!-- c --><tr> <td>");
        content(false, depth + 1);
        s.append("</td><th>h</th></tr>");
        s.append(body ? "</tbody></table>" : "</table>");
      }
      default -> {
        s.append("<p>");
        content(true, depth + 1);
        s.append("</p>");
      }
    }
  }

  private void json() {
    s.append("<script type=\"application/json\" data-rq-block=").append(id()).append('>');
    jsonValue(0);
    s.append("</script>");
  }

  private void jsonValue(int depth) {
    switch (r.nextInt(depth > 3 ? 4 : 6)) {
      case 0 -> s.append("-12.5e3");
      case 1 -> s.append("\"a\\u00e9\\n\"");
      case 2 -> s.append(r.nextBoolean() ? "true" : "null");
      case 3 -> s.append("0");
      case 4 -> {
        s.append("[ ");
        jsonValue(depth + 1);
        s.append(", ");
        jsonValue(depth + 1);
        s.append(']');
      }
      default -> {
        s.append("{\"k\": ");
        jsonValue(depth + 1);
        s.append('}');
      }
    }
  }
}
