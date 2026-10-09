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
  A("a", Set.GUARDED),
  ADDRESS("address", Set.PCLOSE),
  ANNOTATION_XML("annotation-xml", Set.INTEGRATION),
  APPLET("applet", Set.GUARDED),
  AREA("area", Set.VOID),
  ARTICLE("article", Set.PCLOSE | Set.BLOCK_HTML),
  ASIDE("aside", Set.PCLOSE | Set.BLOCK_HTML),
  B("b", Set.BREAKOUT),
  BASE("base", Set.VOID | Set.FORBIDDEN_CONTENT),
  BASEFONT("basefont", Set.VOID | Set.FORBIDDEN_CONTENT),
  BGSOUND("bgsound", Set.VOID | Set.FORBIDDEN_CONTENT),
  BIG("big", Set.BREAKOUT),
  BLOCKQUOTE("blockquote", Set.PCLOSE | Set.BREAKOUT),
  BODY("body", Set.BREAKOUT | Set.FORBIDDEN_CONTENT),
  BR("br", Set.VOID | Set.BREAKOUT),
  BUTTON("button", Set.GUARDED),
  CAPTION("caption", Set.GUARDED | Set.TABLE_PART),
  CENTER("center", Set.PCLOSE | Set.BREAKOUT),
  CODE("code", Set.BREAKOUT),
  COL("col", Set.VOID),
  COLGROUP("colgroup", Set.GUARDED | Set.TABLE_PART),
  DD("dd", Set.PCLOSE | Set.BREAKOUT | Set.LIST_ITEM),
  DESC("desc", Set.INTEGRATION),
  DETAILS("details", Set.PCLOSE),
  DIALOG("dialog", Set.PCLOSE),
  DIR("dir", Set.PCLOSE),
  DIV("div", Set.PCLOSE | Set.BREAKOUT | Set.BLOCK_HTML),
  DL("dl", Set.PCLOSE | Set.BREAKOUT | Set.LIST),
  DT("dt", Set.PCLOSE | Set.BREAKOUT | Set.LIST_ITEM),
  EM("em", Set.BREAKOUT),
  EMBED("embed", Set.VOID | Set.BREAKOUT | Set.FORBIDDEN_CONTENT),
  FIELDSET("fieldset", Set.PCLOSE),
  FIGCAPTION("figcaption", Set.PCLOSE),
  FIGURE("figure", Set.PCLOSE),
  FONT("font", Set.BREAKOUT),
  FOOTER("footer", Set.PCLOSE | Set.BLOCK_HTML),
  FOREIGNOBJECT("foreignobject", Set.INTEGRATION),
  FORM("form", Set.PCLOSE),
  FRAME("frame", Set.VOID | Set.FORBIDDEN_CONTENT),
  FRAMESET("frameset", Set.FORBIDDEN_CONTENT),
  H1("h1", Set.PCLOSE | Set.BREAKOUT | Set.BLOCK_HTML | Set.HEADING),
  H2("h2", Set.PCLOSE | Set.BREAKOUT | Set.BLOCK_HTML | Set.HEADING),
  H3("h3", Set.PCLOSE | Set.BREAKOUT | Set.BLOCK_HTML | Set.HEADING),
  H4("h4", Set.PCLOSE | Set.BREAKOUT | Set.BLOCK_HTML | Set.HEADING),
  H5("h5", Set.PCLOSE | Set.BREAKOUT | Set.BLOCK_HTML | Set.HEADING),
  H6("h6", Set.PCLOSE | Set.BREAKOUT | Set.BLOCK_HTML | Set.HEADING),
  HEAD("head", Set.BREAKOUT | Set.FORBIDDEN_CONTENT),
  HEADER("header", Set.PCLOSE | Set.BLOCK_HTML),
  HGROUP("hgroup", Set.PCLOSE),
  HR("hr", Set.VOID | Set.PCLOSE | Set.BREAKOUT),
  HTML("html", Set.FORBIDDEN_CONTENT),
  I("i", Set.BREAKOUT),
  IFRAME("iframe", Set.FORBIDDEN_CONTENT, TextMode.RAWTEXT),
  IMAGE("image", Set.VOID),
  IMG("img", Set.VOID | Set.BREAKOUT),
  INPUT("input", Set.VOID),
  KEYGEN("keygen", Set.VOID),
  LI("li", Set.PCLOSE | Set.BREAKOUT | Set.LIST_ITEM),
  LINK("link", Set.VOID | Set.FORBIDDEN_CONTENT),
  LISTING("listing", Set.PCLOSE | Set.BREAKOUT),
  MAIN("main", Set.PCLOSE | Set.BLOCK_HTML),
  MARQUEE("marquee", Set.GUARDED),
  MATH("math", Set.GUARDED | Set.FORBIDDEN_CONTENT),
  MENU("menu", Set.PCLOSE | Set.BREAKOUT | Set.LIST),
  META("meta", Set.VOID | Set.BREAKOUT | Set.FORBIDDEN_CONTENT),
  MI("mi", Set.INTEGRATION),
  MN("mn", Set.INTEGRATION),
  MO("mo", Set.INTEGRATION),
  MS("ms", Set.INTEGRATION),
  MTEXT("mtext", Set.INTEGRATION),
  NAV("nav", Set.PCLOSE | Set.BLOCK_HTML),
  NOBR("nobr", Set.GUARDED | Set.BREAKOUT),
  NOEMBED("noembed", Set.FORBIDDEN_CONTENT, TextMode.RAWTEXT),
  NOFRAMES("noframes", Set.FORBIDDEN_CONTENT, TextMode.RAWTEXT),
  NOSCRIPT("noscript", Set.FORBIDDEN_CONTENT, TextMode.RAWTEXT),
  OBJECT("object", Set.GUARDED | Set.FORBIDDEN_CONTENT),
  OL("ol", Set.PCLOSE | Set.BREAKOUT | Set.LIST),
  OPTGROUP("optgroup", Set.FORBIDDEN_CONTENT),
  OPTION("option", Set.FORBIDDEN_CONTENT),
  P("p", Set.PCLOSE | Set.BREAKOUT | Set.BLOCK_HTML),
  PARAM("param", Set.VOID),
  PLAINTEXT("plaintext", Set.PCLOSE | Set.FORBIDDEN_CONTENT, TextMode.PLAINTEXT),
  PRE("pre", Set.PCLOSE | Set.BREAKOUT),
  RB("rb", Set.RUBY_PART),
  RP("rp", Set.RUBY_PART),
  RT("rt", Set.RUBY_PART),
  RTC("rtc", Set.RUBY_PART),
  RUBY("ruby", Set.BREAKOUT),
  S("s", Set.BREAKOUT),
  SCRIPT("script", Set.FORBIDDEN_CONTENT, TextMode.SCRIPT_DATA),
  SEARCH("search", Set.PCLOSE),
  SECTION("section", Set.PCLOSE | Set.BLOCK_HTML),
  SELECT("select", Set.GUARDED | Set.FORBIDDEN_CONTENT),
  SMALL("small", Set.BREAKOUT),
  SOURCE("source", Set.VOID),
  SPAN("span", Set.BREAKOUT | Set.BLOCK_HTML),
  STRIKE("strike", Set.BREAKOUT),
  STRONG("strong", Set.BREAKOUT),
  STYLE("style", Set.FORBIDDEN_CONTENT, TextMode.RAWTEXT),
  SUB("sub", Set.BREAKOUT),
  SUMMARY("summary", Set.PCLOSE),
  SUP("sup", Set.BREAKOUT),
  SVG("svg", Set.GUARDED | Set.FORBIDDEN_CONTENT),
  TABLE("table", Set.PCLOSE | Set.GUARDED | Set.BREAKOUT | Set.TABLE),
  TBODY("tbody", Set.GUARDED | Set.TABLE_PART),
  TD("td", Set.GUARDED | Set.TABLE_PART),
  TEMPLATE("template", Set.GUARDED | Set.FORBIDDEN_CONTENT),
  TEXTAREA("textarea", Set.FORBIDDEN_CONTENT, TextMode.RCDATA),
  TFOOT("tfoot", Set.GUARDED | Set.TABLE_PART),
  TH("th", Set.GUARDED | Set.TABLE_PART),
  THEAD("thead", Set.GUARDED | Set.TABLE_PART),
  TITLE("title", Set.INTEGRATION | Set.BLOCK_HTML | Set.FORBIDDEN_CONTENT, TextMode.RCDATA),
  TR("tr", Set.GUARDED | Set.TABLE_PART),
  TRACK("track", Set.VOID),
  TT("tt", Set.BREAKOUT),
  U("u", Set.BREAKOUT),
  UL("ul", Set.PCLOSE | Set.BREAKOUT | Set.LIST),
  VAR("var", Set.BREAKOUT),
  WBR("wbr", Set.VOID),
  XMP("xmp", Set.PCLOSE | Set.FORBIDDEN_CONTENT, TextMode.RAWTEXT);

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
