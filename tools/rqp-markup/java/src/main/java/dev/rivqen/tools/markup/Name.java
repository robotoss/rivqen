// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import java.util.HashMap;
import java.util.Map;
import org.jspecify.annotations.Nullable;

/**
 * The tag names that the rules of {@code markup.md} §2.1 name, with their name sets. A tag name
 * that is not here belongs to no set.
 */
enum Name {
  A("a", Set.GUARDED | Set.FORMATTING),
  ADDRESS("address", Set.PCLOSE | Set.SPECIAL),
  ANNOTATION_XML("annotation-xml", Set.INTEGRATION),
  APPLET("applet", Set.GUARDED | Set.SPECIAL),
  AREA("area", Set.VOID | Set.SPECIAL),
  ARTICLE("article", Set.PCLOSE | Set.BLOCK_HTML | Set.SPECIAL),
  ASIDE("aside", Set.PCLOSE | Set.BLOCK_HTML | Set.SPECIAL),
  B("b", Set.BREAKOUT | Set.FORMATTING),
  BASE("base", Set.VOID | Set.SPECIAL),
  BASEFONT("basefont", Set.VOID | Set.SPECIAL),
  BGSOUND("bgsound", Set.VOID | Set.SPECIAL),
  BIG("big", Set.BREAKOUT | Set.FORMATTING),
  BLOCKQUOTE("blockquote", Set.PCLOSE | Set.BREAKOUT | Set.SPECIAL),
  BODY("body", Set.BREAKOUT | Set.FORBIDDEN_CONTENT | Set.SPECIAL),
  BR("br", Set.VOID | Set.BREAKOUT | Set.SPECIAL),
  BUTTON("button", Set.GUARDED | Set.SPECIAL),
  CAPTION("caption", Set.GUARDED | Set.TABLE_PART | Set.SPECIAL),
  CENTER("center", Set.PCLOSE | Set.BREAKOUT | Set.SPECIAL),
  CODE("code", Set.BREAKOUT | Set.FORMATTING),
  COL("col", Set.VOID | Set.SPECIAL),
  COLGROUP("colgroup", Set.GUARDED | Set.TABLE_PART | Set.SPECIAL),
  DD("dd", Set.PCLOSE | Set.BREAKOUT | Set.LIST_ITEM | Set.SPECIAL),
  DESC("desc", Set.INTEGRATION),
  DETAILS("details", Set.PCLOSE | Set.SPECIAL),
  DIALOG("dialog", Set.PCLOSE),
  DIR("dir", Set.PCLOSE | Set.SPECIAL),
  DIV("div", Set.PCLOSE | Set.BREAKOUT | Set.BLOCK_HTML | Set.SPECIAL),
  DL("dl", Set.PCLOSE | Set.BREAKOUT | Set.LIST | Set.SPECIAL),
  DT("dt", Set.PCLOSE | Set.BREAKOUT | Set.LIST_ITEM | Set.SPECIAL),
  EM("em", Set.BREAKOUT | Set.FORMATTING),
  EMBED("embed", Set.VOID | Set.BREAKOUT | Set.SPECIAL),
  FIELDSET("fieldset", Set.PCLOSE | Set.SPECIAL),
  FIGCAPTION("figcaption", Set.PCLOSE | Set.SPECIAL),
  FIGURE("figure", Set.PCLOSE | Set.SPECIAL),
  FONT("font", Set.BREAKOUT | Set.FORMATTING),
  FOOTER("footer", Set.PCLOSE | Set.BLOCK_HTML | Set.SPECIAL),
  FOREIGNOBJECT("foreignobject", Set.INTEGRATION),
  FORM("form", Set.PCLOSE | Set.SPECIAL),
  FRAME("frame", Set.VOID | Set.FORBIDDEN_CONTENT | Set.SPECIAL),
  FRAMESET("frameset", Set.FORBIDDEN_CONTENT | Set.SPECIAL),
  H1("h1", Set.PCLOSE | Set.BREAKOUT | Set.BLOCK_HTML | Set.HEADING | Set.SPECIAL),
  H2("h2", Set.PCLOSE | Set.BREAKOUT | Set.BLOCK_HTML | Set.HEADING | Set.SPECIAL),
  H3("h3", Set.PCLOSE | Set.BREAKOUT | Set.BLOCK_HTML | Set.HEADING | Set.SPECIAL),
  H4("h4", Set.PCLOSE | Set.BREAKOUT | Set.BLOCK_HTML | Set.HEADING | Set.SPECIAL),
  H5("h5", Set.PCLOSE | Set.BREAKOUT | Set.BLOCK_HTML | Set.HEADING | Set.SPECIAL),
  H6("h6", Set.PCLOSE | Set.BREAKOUT | Set.BLOCK_HTML | Set.HEADING | Set.SPECIAL),
  HEAD("head", Set.BREAKOUT | Set.FORBIDDEN_CONTENT | Set.SPECIAL),
  HEADER("header", Set.PCLOSE | Set.BLOCK_HTML | Set.SPECIAL),
  HGROUP("hgroup", Set.PCLOSE | Set.SPECIAL),
  HR("hr", Set.VOID | Set.PCLOSE | Set.BREAKOUT | Set.SPECIAL),
  HTML("html", Set.FORBIDDEN_CONTENT | Set.SPECIAL),
  I("i", Set.BREAKOUT | Set.FORMATTING),
  IFRAME("iframe", Set.SPECIAL, TextMode.RAWTEXT),
  IMAGE("image", Set.VOID),
  IMG("img", Set.VOID | Set.BREAKOUT | Set.SPECIAL),
  INPUT("input", Set.VOID | Set.SPECIAL),
  KEYGEN("keygen", Set.VOID | Set.SPECIAL),
  LI("li", Set.PCLOSE | Set.BREAKOUT | Set.LIST_ITEM | Set.SPECIAL),
  LINK("link", Set.VOID | Set.SPECIAL),
  LISTING("listing", Set.PCLOSE | Set.BREAKOUT | Set.SPECIAL),
  MAIN("main", Set.PCLOSE | Set.BLOCK_HTML | Set.SPECIAL),
  MARQUEE("marquee", Set.GUARDED | Set.SPECIAL),
  MATH("math", Set.GUARDED),
  MENU("menu", Set.PCLOSE | Set.BREAKOUT | Set.LIST | Set.SPECIAL),
  META("meta", Set.VOID | Set.BREAKOUT | Set.SPECIAL),
  MI("mi", Set.INTEGRATION),
  MN("mn", Set.INTEGRATION),
  MO("mo", Set.INTEGRATION),
  MS("ms", Set.INTEGRATION),
  MTEXT("mtext", Set.INTEGRATION),
  NAV("nav", Set.PCLOSE | Set.BLOCK_HTML | Set.SPECIAL),
  NOBR("nobr", Set.GUARDED | Set.BREAKOUT | Set.FORMATTING),
  NOEMBED("noembed", Set.FORBIDDEN_CONTENT | Set.SPECIAL, TextMode.RAWTEXT),
  NOFRAMES("noframes", Set.FORBIDDEN_CONTENT | Set.SPECIAL, TextMode.RAWTEXT),
  NOSCRIPT("noscript", Set.FORBIDDEN_CONTENT | Set.SPECIAL, TextMode.RAWTEXT),
  OBJECT("object", Set.GUARDED | Set.SPECIAL),
  OL("ol", Set.PCLOSE | Set.BREAKOUT | Set.LIST | Set.SPECIAL),
  OPTGROUP("optgroup", Set.FORBIDDEN_CONTENT),
  OPTION("option", Set.FORBIDDEN_CONTENT),
  P("p", Set.PCLOSE | Set.BREAKOUT | Set.BLOCK_HTML | Set.SPECIAL),
  PARAM("param", Set.VOID | Set.SPECIAL),
  PLAINTEXT("plaintext", Set.PCLOSE | Set.FORBIDDEN_CONTENT | Set.SPECIAL, TextMode.PLAINTEXT),
  PRE("pre", Set.PCLOSE | Set.BREAKOUT | Set.SPECIAL),
  RB("rb", Set.RUBY_PART),
  RP("rp", Set.RUBY_PART),
  RT("rt", Set.RUBY_PART),
  RTC("rtc", Set.RUBY_PART),
  RUBY("ruby", Set.BREAKOUT),
  S("s", Set.BREAKOUT | Set.FORMATTING),
  SCRIPT("script", Set.SPECIAL, TextMode.SCRIPT_DATA),
  SEARCH("search", Set.PCLOSE | Set.SPECIAL),
  SECTION("section", Set.PCLOSE | Set.BLOCK_HTML | Set.SPECIAL),
  SELECT("select", Set.GUARDED | Set.FORBIDDEN_CONTENT | Set.SPECIAL),
  SMALL("small", Set.BREAKOUT | Set.FORMATTING),
  SOURCE("source", Set.VOID | Set.SPECIAL),
  SPAN("span", Set.BREAKOUT | Set.BLOCK_HTML),
  STRIKE("strike", Set.BREAKOUT | Set.FORMATTING),
  STRONG("strong", Set.BREAKOUT | Set.FORMATTING),
  STYLE("style", Set.SPECIAL, TextMode.RAWTEXT),
  SUB("sub", Set.BREAKOUT),
  SUMMARY("summary", Set.PCLOSE | Set.SPECIAL),
  SUP("sup", Set.BREAKOUT),
  SVG("svg", Set.GUARDED),
  TABLE("table", Set.PCLOSE | Set.GUARDED | Set.BREAKOUT | Set.TABLE | Set.SPECIAL),
  TBODY("tbody", Set.GUARDED | Set.TABLE_PART | Set.SPECIAL),
  TD("td", Set.GUARDED | Set.TABLE_PART | Set.SPECIAL),
  TEMPLATE("template", Set.GUARDED | Set.FORBIDDEN_CONTENT | Set.SPECIAL),
  TEXTAREA("textarea", Set.FORBIDDEN_CONTENT | Set.SPECIAL, TextMode.RCDATA),
  TFOOT("tfoot", Set.GUARDED | Set.TABLE_PART | Set.SPECIAL),
  TH("th", Set.GUARDED | Set.TABLE_PART | Set.SPECIAL),
  THEAD("thead", Set.GUARDED | Set.TABLE_PART | Set.SPECIAL),
  TITLE(
      "title",
      Set.INTEGRATION | Set.BLOCK_HTML | Set.FORBIDDEN_CONTENT | Set.SPECIAL,
      TextMode.RCDATA),
  TR("tr", Set.GUARDED | Set.TABLE_PART | Set.SPECIAL),
  TRACK("track", Set.VOID | Set.SPECIAL),
  TT("tt", Set.BREAKOUT | Set.FORMATTING),
  U("u", Set.BREAKOUT | Set.FORMATTING),
  UL("ul", Set.PCLOSE | Set.BREAKOUT | Set.LIST | Set.SPECIAL),
  VAR("var", Set.BREAKOUT),
  WBR("wbr", Set.VOID | Set.SPECIAL),
  XMP("xmp", Set.PCLOSE | Set.FORBIDDEN_CONTENT | Set.SPECIAL, TextMode.RAWTEXT);

  /** Bit flags for the name sets of {@code markup.md} §2.1 and for groups that M-24 uses. */
  static final class Set {
    static final int VOID = 1;
    static final int PCLOSE = 1 << 1;
    static final int GUARDED = 1 << 2;
    static final int TABLE_PART = 1 << 3;

    /** {@code table} itself; {@code TABLE_FAMILY} is {@code TABLE | TABLE_PART}. */
    static final int TABLE = 1 << 4;

    static final int BREAKOUT = 1 << 5;
    static final int INTEGRATION = 1 << 6;
    static final int BLOCK_HTML = 1 << 7;
    static final int FORBIDDEN_CONTENT = 1 << 8;

    /** {@code h1}–{@code h6} (M-24 b). */
    static final int HEADING = 1 << 9;

    /** {@code ul}, {@code ol}, {@code menu}, {@code dl} (M-24 c). */
    static final int LIST = 1 << 10;

    /** {@code li}, {@code dd}, {@code dt} (M-24 c). */
    static final int LIST_ITEM = 1 << 11;

    /** {@code rb}, {@code rp}, {@code rt}, {@code rtc} (M-24 h, i). */
    static final int RUBY_PART = 1 << 12;

    /**
     * The HTML part of the WHATWG "special" category (M-06 items 2 and 4). It applies only to
     * elements pushed in HTML context; the caller checks the context.
     */
    static final int SPECIAL = 1 << 13;

    /** The WHATWG "formatting" category: end tags that run the adoption agency (M-06 item 2). */
    static final int FORMATTING = 1 << 14;

    static final int TABLE_FAMILY = TABLE | TABLE_PART;

    private Set() {}
  }

  private static final Map<String, Name> BY_TAG = index();

  /** The tag name in lower case, as the tokenizer produces it. */
  final String tag;

  private final int sets;

  /** The tokenizer state after a start tag in HTML context (M-04), or null for no change. */
  final @Nullable TextMode text;

  Name(String tag, int sets) {
    this(tag, sets, null);
  }

  Name(String tag, int sets, @Nullable TextMode text) {
    this.tag = tag;
    this.sets = sets;
    this.text = text;
  }

  /** True when this name is in every set of {@code mask}. */
  boolean in(int mask) {
    return (sets & mask) == mask;
  }

  /** True when this name is in at least one set of {@code mask}. */
  boolean inAny(int mask) {
    return (sets & mask) != 0;
  }

  /** The known name for a lower-case tag name, or null. */
  static @Nullable Name of(String tag) {
    return BY_TAG.get(tag);
  }

  /** True when {@code name} is non-null and in every set of {@code mask}. */
  static boolean is(@Nullable Name name, int mask) {
    return name != null && name.in(mask);
  }

  /** True when {@code name} is non-null and in at least one set of {@code mask}. */
  static boolean isAny(@Nullable Name name, int mask) {
    return name != null && name.inAny(mask);
  }

  private static Map<String, Name> index() {
    Map<String, Name> map = new HashMap<>();
    for (Name n : values()) {
      map.put(n.tag, n);
    }
    return Map.copyOf(map);
  }
}
