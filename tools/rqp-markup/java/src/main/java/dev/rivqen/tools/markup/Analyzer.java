// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayList;
import java.util.Base64;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import org.jspecify.annotations.Nullable;

/**
 * Decides whether a document is valid for RQP (rqp/1) and finds its blocks ({@code
 * docs/engineering/protocol/markup.md} §2, rules M-01 to M-27).
 *
 * <p>One pass over the tokens of {@link Tokenizer}: the token stack (M-05) is the only structure.
 * There is no tree builder (DL-008). The work is linear in the input size; the memory is bounded by
 * the input size, and the input size is checked first (M-01).
 */
public final class Analyzer {
  /** M-01: maximum input size in bytes. */
  public static final int MAX_INPUT_BYTES = 5 * 1024 * 1024;

  /** M-26: maximum number of blocks. */
  public static final int MAX_BLOCKS = 256;

  /** M-26: maximum content size of one block in bytes. */
  public static final int MAX_BLOCK_BYTES = 1024 * 1024;

  /** M-15: maximum id length. */
  static final int MAX_ID_LENGTH = 64;

  static final String FORMAT_HTML = "html";
  static final String FORMAT_JSON = "json";

  private static final byte[] JSON_TYPE = Tokenizer.ascii("application/json");
  private static final byte[] MANIFEST = Tokenizer.ascii("rivqen-manifest");
  private static final byte[] NOSCRIPT_END = Tokenizer.ascii("</noscript>");

  private final byte[] in;
  private final Tokenizer tok;
  private final TokenStack stack = new TokenStack();
  private final List<Result.Block> blocks = new ArrayList<>();
  private final Set<String> ids = new HashSet<>();

  /** The block whose content is being read, or null. */
  private @Nullable Pending pending;

  /** Stack index of the element of the open html block (not {@code title}), or -1. */
  private int blockIndex = -1;

  /** Stack index of a foreign TEXT or INTEGRATION element whose content is text only, or -1. */
  private int textOnly = -1;

  /** Offset of the content of an open {@code noscript} element in HTML context, or -1. */
  private int noscriptContent = -1;

  private record Pending(String id, String format, int start) {}

  /** A rule failed. Internal control flow only; never escapes {@link #analyze}. */
  private static final class Reject extends RuntimeException {
    private static final long serialVersionUID = 1L;

    @SuppressWarnings("serial") // never serialized
    private final ErrorCode code;

    private final String rule;

    Reject(ErrorCode code, String rule) {
      super(rule, null, false, false);
      this.code = code;
      this.rule = rule;
    }
  }

  private Analyzer(byte[] in) {
    this.in = in;
    // M-02: the tokenizer does not see a byte order mark at offset 0. Its bytes (EF BB BF) are not
    // ASCII, so in the data state they are characters that can neither start nor end a token:
    // starting at offset 0 gives the same tokens as starting after the mark. Offsets always count
    // the mark.
    this.tok = new Tokenizer(in, 0, in.length);
  }

  /**
   * Analyzes one document.
   *
   * @param input the document bytes; only the length is read when it is above {@link
   *     #MAX_INPUT_BYTES}
   * @return the result; never null; no exception for any input
   */
  public static Result analyze(byte[] input) {
    if (input.length > MAX_INPUT_BYTES) {
      return Result.invalid(ErrorCode.LIMIT, "M-01");
    }
    if (!Utf8.isValid(input)) {
      return Result.invalid(ErrorCode.ENCODING, "M-02");
    }
    Analyzer a = new Analyzer(input);
    try {
      a.run();
    } catch (Reject r) {
      return Result.invalid(r.code, r.rule);
    }
    return a.result();
  }

  private void run() {
    while (true) {
      switch (tok.next()) {
        case START_TAG -> startTag();
        case END_TAG -> endTag();
        case COMMENT -> comment();
        case DOCTYPE -> doctype();
        case EOF -> {
          eof();
          return;
        }
      }
    }
  }

  // ---------------------------------------------------------------------------------------------
  // Tokens

  private void startTag() {
    String raw = tok.name();
    Name k = Name.of(raw);
    String name = k == null ? raw : k.tag;
    boolean html = stack.regionRoot() < 0;
    boolean inContent = blockIndex >= 0;
    boolean selfClosing = tok.selfClosing();
    if (textOnly >= 0) {
      throw structure("M-07");
    }
    if (k == Name.FRAMESET) {
      throw structure("M-10");
    }
    if (k == Name.SCRIPT && reservedType(in, tok)) {
      throw new Reject(ErrorCode.RESERVED, "M-13");
    }
    if (stack.open(Name.SELECT) > 0 && k != Name.OPTION && k != Name.OPTGROUP && k != Name.HR) {
      throw structure("M-08");
    }
    Pending block = null;
    if (tok.hasBlockAttribute()) {
      block = blockStart(k, html, inContent);
    } else if (inContent) {
      contentStartTag(k, selfClosing);
    }
    if (!html) {
      foreignStartTag(name, k, selfClosing);
      return;
    }
    tablePart(k);
    boolean region = k == Name.SVG || k == Name.MATH;
    if (!(region && selfClosing) && !Name.is(k, Name.Set.VOID)) {
      int index = stack.push(name, k, false, inContent);
      if (region) {
        stack.startRegion(index);
      }
      if (block != null && k != Name.TITLE && k != Name.SCRIPT) {
        blockIndex = index;
      }
    }
    if (block != null) {
      pending = block;
    }
    if (k != null && k.text != null) {
      tok.enterText(k.text, k.tag);
      if (k == Name.NOSCRIPT) {
        noscriptContent = tok.end();
      }
    }
  }

  /** M-07: a start tag in a foreign region. */
  private void foreignStartTag(String name, @Nullable Name k, boolean selfClosing) {
    if (Name.is(k, Name.Set.BREAKOUT)) {
      throw structure("M-07");
    }
    boolean text = k != null && k.text != null;
    if (text && (selfClosing || k == Name.PLAINTEXT)) {
      throw structure("M-07");
    }
    if (selfClosing) {
      return;
    }
    int index = stack.push(name, k, true, false);
    if (text || Name.is(k, Name.Set.INTEGRATION)) {
      textOnly = index;
    }
  }

  /** M-14 to M-19, M-21 and the block count of M-26, in the order of markup.md §2.10. */
  private Pending blockStart(@Nullable Name k, boolean html, boolean inContent) {
    if (inContent) {
      throw new Reject(ErrorCode.NESTED, "M-17");
    }
    int from = tok.blockValueStart();
    int to = tok.blockValueEnd();
    if (!validId(from, to)) {
      throw new Reject(ErrorCode.INVALID_ID, "M-15");
    }
    String format;
    if (html && Name.is(k, Name.Set.BLOCK_HTML)) {
      format = FORMAT_HTML;
    } else if (html && k == Name.SCRIPT && typeEquals(JSON_TYPE)) {
      format = FORMAT_JSON;
    } else {
      throw new Reject(ErrorCode.FORBIDDEN_ELEMENT, "M-16");
    }
    if (tok.selfClosing()) {
      throw structure("M-18");
    }
    if (stack.open(Name.TEMPLATE) > 0 || stack.open(Name.SELECT) > 0) {
      throw structure("M-19");
    }
    if (tableContext()) {
      throw structure("M-19");
    }
    String id = new String(in, from, to - from, StandardCharsets.US_ASCII);
    if (!ids.add(id)) {
      throw new Reject(ErrorCode.DUPLICATE, "M-21");
    }
    if (blocks.size() >= MAX_BLOCKS) {
      throw new Reject(ErrorCode.LIMIT, "M-26");
    }
    return new Pending(id, format, tok.end());
  }

  /** M-22 and M-24: a start tag in the content of an html block. */
  private void contentStartTag(@Nullable Name k, boolean selfClosing) {
    if (Name.is(k, Name.Set.FORBIDDEN_CONTENT)) {
      throw structure("M-22");
    }
    if (selfClosing && !Name.is(k, Name.Set.VOID)) {
      throw structure("M-22");
    }
    // M-24 k: in table context only table parts and col; also for names that no set knows.
    if (tableContext() && !Name.is(k, Name.Set.TABLE_PART) && k != Name.COL) {
      throw structure("M-24");
    }
    if (k == null) {
      return;
    }
    Name block = stack.known(blockIndex);
    boolean invalid =
        switch (k) {
          case H1, H2, H3, H4, H5, H6 -> Name.is(block, Name.Set.HEADING) || headingInContent();
          case LI, DD, DT -> listItemImplied();
          case BUTTON, A, NOBR -> stack.open(k) > 0;
          case CAPTION, COLGROUP, TBODY, TD, TFOOT, TH, THEAD, TR, COL ->
              stack.openInContent(Name.TABLE) == 0;
          case RB, RTC -> stack.openInContent(Name.RUBY) == 0 || rubyPartOpen(true);
          case RP, RT -> stack.openInContent(Name.RUBY) == 0 || rubyPartOpen(false);
          default -> false;
        };
    // M-24 a; headings and list items are also in PCLOSE.
    if (invalid || (k.in(Name.Set.PCLOSE) && (block == Name.P || block == Name.SPAN))) {
      throw structure("M-24");
    }
  }

  private boolean headingInContent() {
    return stack.openInContent(Name.H1)
            + stack.openInContent(Name.H2)
            + stack.openInContent(Name.H3)
            + stack.openInContent(Name.H4)
            + stack.openInContent(Name.H5)
            + stack.openInContent(Name.H6)
        > 0;
  }

  /**
   * M-24 c and j for an {@code li}, {@code dd} or {@code dt} start tag in the content: invalid when
   * no list opened in the content is open (c), or when an {@code li}, {@code dd} or {@code dt} is
   * above the nearest list (j). Elements opened in the content are above every element opened
   * before the block, so the nearest list is the nearest list opened in the content when there is
   * one, and an item above it was opened in the content.
   */
  private boolean listItemImplied() {
    int list = stack.nearestList();
    return list < 0 || !stack.inContent(list) || stack.nearestListItem() > list;
  }

  /**
   * True when F of M-09 is {@code table}, {@code tbody}, {@code thead}, {@code tfoot}, {@code tr}
   * or {@code colgroup}: the table context of M-19 rule 2 and M-24 k, where the WHATWG parser moves
   * an element out of the table (foster parenting).
   */
  private boolean tableContext() {
    Name f = stack.tableFamily();
    return f != null && f != Name.TD && f != Name.TH && f != Name.CAPTION;
  }

  private boolean rubyPartOpen(boolean withRtc) {
    int n = stack.open(Name.RB) + stack.open(Name.RP) + stack.open(Name.RT);
    return n + (withRtc ? stack.open(Name.RTC) : 0) > 0;
  }

  /** M-09: table parts only directly in their table-family parent. */
  private void tablePart(@Nullable Name k) {
    if (k == null) {
      return;
    }
    Name f = stack.tableFamily();
    if (f == null) {
      return;
    }
    boolean allowed =
        switch (k) {
          case CAPTION, COLGROUP, TBODY, THEAD, TFOOT -> f == Name.TABLE;
          case COL -> f == Name.TABLE || f == Name.COLGROUP;
          case TR -> f == Name.TABLE || f == Name.TBODY || f == Name.THEAD || f == Name.TFOOT;
          case TD, TH ->
              f == Name.TABLE
                  || f == Name.TBODY
                  || f == Name.THEAD
                  || f == Name.TFOOT
                  || f == Name.TR;
          default -> true;
        };
    if (!allowed) {
      throw structure("M-09");
    }
  }

  private void endTag() {
    String raw = tok.name();
    Name k = Name.of(raw);
    String name = k == null ? raw : k.tag;
    if (textOnly >= 0) {
      // M-07 rules 3 and 4: only the end tag of the element ends its text content.
      if (stack.size() - 1 == textOnly && stack.name(textOnly).equals(name)) {
        stack.pop();
        textOnly = -1;
        return;
      }
      throw structure("M-07");
    }
    if (pending != null && blockIndex < 0) {
      // The appropriate end tag of a title or json block (M-20).
      finishBlock(tok.start());
    }
    if (noscriptContent >= 0) {
      noscript(noscriptContent, tok.start());
      noscriptContent = -1;
    }
    if (stack.open(Name.SELECT) > 0 && k != Name.OPTION && k != Name.OPTGROUP && k != Name.SELECT) {
      throw structure("M-08");
    }
    if (blockIndex >= 0) {
      contentEndTag(name, k);
      return;
    }
    int index = stack.nearest(name);
    int root = stack.regionRoot();
    if (root >= 0) {
      // M-07 rule 2: no stray end tag; the region ends only with its own end tag.
      if (index < root) {
        throw structure("M-07");
      }
      stack.popTo(index);
      return;
    }
    if (index < 0) {
      return;
    }
    for (int i = stack.size() - 1; i > index; i--) {
      Name crossed = stack.known(i);
      boolean tableException =
          Name.isAny(k, Name.Set.TABLE_FAMILY) && Name.is(crossed, Name.Set.TABLE_PART);
      // HTML context: no foreign region is open, so every crossed element was pushed in HTML
      // context (M-06 guards only those).
      if (Name.is(crossed, Name.Set.GUARDED) && !tableException) {
        throw structure("M-06");
      }
    }
    stack.popTo(index);
  }

  /** M-22 rule 4, M-23 and the block end tag of M-20. */
  private void contentEndTag(String name, @Nullable Name k) {
    if (Name.is(k, Name.Set.VOID)) {
      throw structure("M-22");
    }
    int top = stack.size() - 1;
    if (!stack.name(top).equals(name)) {
      throw structure("M-23");
    }
    if (top == blockIndex) {
      finishBlock(tok.start());
      blockIndex = -1;
    }
    stack.pop();
  }

  private void comment() {
    if (tok.cdata()) {
      if (!tok.cdataClosed()) {
        throw structure("M-12");
      }
      // M-07 rules 3 and 4 (E5, H-21): a CDATA section that satisfies M-12 is allowed in text-only
      // content. A CDATA section and the bogus comment of this tokenizer end at the same byte.
      return;
    }
    if (textOnly >= 0) {
      throw structure("M-07");
    }
  }

  private void doctype() {
    if (textOnly >= 0) {
      throw structure("M-07");
    }
    if (stack.open(Name.SELECT) > 0) {
      throw structure("M-08");
    }
    if (blockIndex >= 0) {
      throw structure("M-22");
    }
  }

  private void eof() {
    if (noscriptContent >= 0) {
      noscript(noscriptContent, in.length);
    }
    if (pending != null) {
      throw structure("M-20");
    }
    if (stack.open(Name.SELECT) > 0) {
      throw structure("M-08");
    }
  }

  // ---------------------------------------------------------------------------------------------
  // Blocks and rules on values

  private void finishBlock(int end) {
    Pending p = pending;
    if (p == null) {
      throw new IllegalStateException("no open block");
    }
    pending = null;
    if (end - p.start() > MAX_BLOCK_BYTES) {
      throw new Reject(ErrorCode.LIMIT, "M-26");
    }
    if (p.format().equals(FORMAT_JSON) && !JsonText.isValid(in, p.start(), end)) {
      throw new Reject(ErrorCode.BAD_JSON, "M-25");
    }
    MessageDigest d = sha256();
    d.update(in, p.start(), end - p.start());
    blocks.add(new Result.Block(p.id(), p.format(), p.start(), end, base64Url(d.digest())));
  }

  /**
   * M-11: tokenize the content of a {@code noscript} element and a following {@code </noscript>}
   * alone, from the data state, in HTML context. M-13 applies to the start tags of this pass too
   * (E8). When the {@code noscript} element has no end tag, the content is the rest of the input
   * (E3).
   */
  private static void noscript(byte[] in, int from, int to) {
    int length = to - from;
    byte[] c = new byte[length + NOSCRIPT_END.length];
    System.arraycopy(in, from, c, 0, length);
    System.arraycopy(NOSCRIPT_END, 0, c, length, NOSCRIPT_END.length);
    Tokenizer sub = new Tokenizer(c, 0, c.length);
    while (true) {
      switch (sub.next()) {
        case START_TAG -> {
          Name k = Name.of(sub.name());
          // M-13 applies also to C (E8, H-22): a reader with scripting disabled sees this script.
          if (k == Name.SCRIPT && reservedType(c, sub)) {
            throw new Reject(ErrorCode.RESERVED, "M-13");
          }
          if (sub.hasBlockAttribute()) {
            throw structure("M-11");
          }
          if (k == Name.FRAMESET
              || k == Name.MATH
              || k == Name.NOSCRIPT
              || k == Name.PLAINTEXT
              || k == Name.SELECT
              || k == Name.SVG
              || k == Name.TEMPLATE) {
            throw structure("M-11");
          }
          if (k != null && k.text != null) {
            sub.enterText(k.text, k.tag);
          }
        }
        case END_TAG -> {
          if (sub.name().equals(Name.NOSCRIPT.tag)) {
            if (sub.start() == length) {
              return;
            }
            throw structure("M-11");
          }
        }
        case EOF -> throw structure("M-11");
        case COMMENT, DOCTYPE -> {}
      }
    }
  }

  private void noscript(int from, int to) {
    noscript(in, from, to);
  }

  /** M-15: {@code ^[a-z0-9][a-z0-9_-]{0,63}$} on the raw value. */
  private boolean validId(int from, int to) {
    int n = to - from;
    if (n < 1 || n > MAX_ID_LENGTH) {
      return false;
    }
    for (int i = from; i < to; i++) {
      byte b = in[i];
      boolean alnum = (b >= 'a' && b <= 'z') || (b >= '0' && b <= '9');
      if (!alnum && (i == from || (b != '_' && b != '-'))) {
        return false;
      }
    }
    return true;
  }

  /** M-16: the first {@code type} raw value is exactly {@code value}. */
  private boolean typeEquals(byte[] value) {
    if (!tok.hasTypeAttribute()) {
      return false;
    }
    int from = tok.typeValueStart();
    if (tok.typeValueEnd() - from != value.length) {
      return false;
    }
    for (int i = 0; i < value.length; i++) {
      if (in[from + i] != value[i]) {
        return false;
      }
    }
    return true;
  }

  /**
   * M-13: the first {@code type} raw value of the start tag that {@code t} just returned contains
   * {@code rivqen-manifest} (any case) or '&'.
   *
   * @param in the bytes that {@code t} reads
   * @param t the tokenizer
   */
  private static boolean reservedType(byte[] in, Tokenizer t) {
    if (!t.hasTypeAttribute()) {
      return false;
    }
    int from = t.typeValueStart();
    int to = t.typeValueEnd();
    for (int i = from; i < to; i++) {
      if (in[i] == '&') {
        return true;
      }
    }
    for (int i = from; i + MANIFEST.length <= to; i++) {
      int j = 0;
      while (j < MANIFEST.length && Tokenizer.toLower(in[i + j]) == MANIFEST[j]) {
        j++;
      }
      if (j == MANIFEST.length) {
        return true;
      }
    }
    return false;
  }

  // ---------------------------------------------------------------------------------------------
  // Result (M-27, CONTRACT.md §4)

  private Result result() {
    MessageDigest t = sha256();
    t.update(Tokenizer.ascii("rqp-t1\n"));
    int at = 0;
    for (Result.Block b : blocks) {
      t.update(in, at, b.start() - at);
      at = b.end();
    }
    t.update(in, at, in.length - at);
    String templateRevision = "t1." + base64Url(t.digest());
    StringBuilder p = new StringBuilder("rqp-r1\n").append(templateRevision).append('\n');
    for (Result.Block b : blocks) {
      p.append(b.id()).append('\t').append(b.format()).append('\t').append(b.sha256());
      p.append('\n');
    }
    MessageDigest r = sha256();
    r.update(p.toString().getBytes(StandardCharsets.UTF_8));
    String pageRevision = "r1." + base64Url(r.digest());
    return new Result(null, null, blocks, templateRevision, pageRevision);
  }

  private static Reject structure(String rule) {
    return new Reject(ErrorCode.STRUCTURE, rule);
  }

  private static MessageDigest sha256() {
    try {
      return MessageDigest.getInstance("SHA-256");
    } catch (NoSuchAlgorithmException e) {
      // Every Java platform must support SHA-256 (MessageDigest specification).
      throw new IllegalStateException("SHA-256 is not available", e);
    }
  }

  static String base64Url(byte[] digest) {
    return Base64.getUrlEncoder().withoutPadding().encodeToString(digest);
  }
}
