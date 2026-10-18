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
import java.util.function.Supplier;
import org.jspecify.annotations.Nullable;

/**
 * Decides whether a document is valid for RQP (rqp/1) and finds its blocks ({@code
 * docs/engineering/protocol/markup.md} §2, rules M-01 to M-27).
 *
 * <p>One pass over the tokens of {@link Tokenizer}: the token stack (M-05) with its depth counter D
 * (M-26) and special-crossed marks (M-06 item 4) is the only structure. There is no tree builder
 * (DL-008). The work is linear in the input size; the memory is bounded by the input size, and the
 * input size is checked first (M-01). D is at most {@link #MAX_OPEN_ELEMENTS}, and the token stack
 * never has more elements than D, so it has a fixed capacity (M-26).
 */
public final class Analyzer {
  /** M-01: maximum input size in bytes. */
  public static final int MAX_INPUT_BYTES = 5 * 1024 * 1024;

  /** M-26: maximum number of blocks. */
  public static final int MAX_BLOCKS = 256;

  /** M-26: maximum content size of one block in bytes. */
  public static final int MAX_BLOCK_BYTES = 1024 * 1024;

  /**
   * M-26: maximum value of the depth counter D at any point of the document, in HTML context and in
   * a foreign region, inside and outside blocks ({@code maxOpenElements}; H-25, amended by H-31).
   * The token stack (M-05) never has more elements than D.
   */
  public static final int MAX_OPEN_ELEMENTS = 256;

  /** Prefix of {@link Result#rule()} for an internal error (a defect) at the boundary (F-4). */
  static final String INTERNAL_ERROR = "internal-error:";

  /** M-15: maximum id length. */
  static final int MAX_ID_LENGTH = 64;

  static final String FORMAT_HTML = "html";
  static final String FORMAT_JSON = "json";

  private static final byte[] JSON_TYPE = Tokenizer.ascii("application/json");
  private static final byte[] MANIFEST = Tokenizer.ascii("rivqen-manifest");
  private static final byte[] NOSCRIPT_END = Tokenizer.ascii("</noscript>");

  private final byte[] in;

  /** The bytes that {@link #tok} reads: the document, or the content C of a noscript (M-11). */
  private byte[] buf;

  /** The active tokenizer: the main one, or the one of the pass on C (M-11 item 8). */
  private Tokenizer tok;

  private final TokenStack stack = new TokenStack();
  private final List<Result.Block> blocks = new ArrayList<>();
  private final Set<String> ids = new HashSet<>();

  /** Tag names of this document (F-2): the stacks keep these strings. */
  private final NameTable names = new NameTable();

  /** M-26: the depth counter D. */
  private int depth;

  /** The block whose content is being read, or null. */
  private @Nullable Pending pending;

  /** Stack index of the element of the open html block (not {@code title}), or -1. */
  private int blockIndex = -1;

  /** Stack index of a foreign TEXT or INTEGRATION element whose content is text only, or -1. */
  private int textOnly = -1;

  /** Offset of the content of an open {@code noscript} element in HTML context, or -1. */
  private int noscriptContent = -1;

  /**
   * M-11 item 8: stack size after the {@code noscript} push while the pass on C runs, else -1. The
   * elements above this size are the stack of C (item 5).
   */
  private int noscriptBase = -1;

  /** M-11 item 6: the number of {@code table} elements of C that are open. */
  private int noscriptTables;

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
    this.buf = in;
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
   * @return the result; never null; no exception for any input. An internal error (a defect) gives
   *     an invalid result whose rule starts with {@link #INTERNAL_ERROR} (fail closed, F-4).
   */
  public static Result analyze(byte[] input) {
    return failClosed(() -> analyzeOrThrow(input));
  }

  /**
   * The boundary of {@link #analyze} (F-4): an unexpected {@link RuntimeException} is a defect. It
   * gives an invalid result ({@code RQP_MARKUP_STRUCTURE}), so a caller never uses blocks of a
   * document that was not checked to the end. The result keeps only the exception class name: a
   * message can contain document content. {@link Main} exits with code 3 for such a result.
   */
  static Result failClosed(Supplier<Result> work) {
    try {
      return work.get();
    } catch (RuntimeException e) {
      return Result.invalid(ErrorCode.STRUCTURE, INTERNAL_ERROR + e.getClass().getName());
    }
  }

  private static Result analyzeOrThrow(byte[] input) {
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
    String name = names.intern(raw, k);
    boolean html = stack.regionRoot() < 0;
    boolean inContent = blockIndex >= 0;
    boolean selfClosing = tok.selfClosing();
    if (textOnly >= 0) {
      throw structure("M-07");
    }
    if (k == Name.SCRIPT && reservedType(buf, tok)) {
      throw new Reject(ErrorCode.RESERVED, "M-13");
    }
    if (noscriptBase >= 0) {
      noscriptStartTag(k, html);
    }
    if (k == Name.FRAMESET) {
      throw structure("M-10");
    }
    if (html && k == Name.TEMPLATE && tok.hasForAttribute()) {
      throw structure("M-10"); // item 3 (H-43)
    }
    if (stack.open(Name.SELECT) > 0 && k != Name.OPTION && k != Name.OPTGROUP && k != Name.HR) {
      throw structure("M-08");
    }
    Pending block = null;
    if (tok.hasBlockAttribute()) {
      block = blockStart(k, html, inContent);
    } else if (inContent) {
      if (html) {
        contentStartTag(k, selfClosing);
      } else if (!Name.foreignAllowed(raw)) {
        throw structure("M-22"); // item 5 (H-73)
      }
    }
    if (!html) {
      foreignStartTag(name, k, selfClosing);
      return;
    }
    tablePart(k);
    nestedForm(k);
    impliedParts(k);
    boolean region = k == Name.SVG || k == Name.MATH;
    if (!(region && selfClosing) && !Name.is(k, Name.Set.VOID)) {
      int index = push(name, k, false, inContent);
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
    int index = push(name, k, true, false);
    if (text || Name.is(k, Name.Set.INTEGRATION)) {
      textOnly = index;
    }
  }

  /**
   * M-05 push with M-26 item 2: each push adds 1 to D. The caller checks the other rules of the
   * start tag first (markup.md §2.10: "on each push to the token stack").
   */
  private int push(String name, @Nullable Name k, boolean inForeign, boolean inContent) {
    addDepth(1);
    return stack.push(name, k, inForeign, inContent);
  }

  /** M-26: adds {@code units} to D; D larger than {@link #MAX_OPEN_ELEMENTS} is an error. */
  private void addDepth(int units) {
    depth += units;
    if (depth > MAX_OPEN_ELEMENTS) {
      throw new Reject(ErrorCode.LIMIT, "M-26");
    }
  }

  /**
   * M-26 item 3: the parts that the tree builder inserts implicitly for a start tag in HTML context
   * (a row group, a row, a column group) count once for each F of M-09. {@code col} is not pushed,
   * but its column group counts.
   */
  private void impliedParts(@Nullable Name k) {
    int f = stack.tableFamilyIndex();
    if (f < 0 || k == null) {
      return;
    }
    Name fk = stack.known(f);
    int parts =
        switch (k) {
          case TR -> fk == Name.TABLE ? TokenStack.ROW_GROUP : 0;
          case TD, TH ->
              fk == Name.TABLE
                  ? TokenStack.ROW_GROUP | TokenStack.ROW
                  : (fk == Name.TBODY || fk == Name.THEAD || fk == Name.TFOOT ? TokenStack.ROW : 0);
          case COL -> fk == Name.TABLE ? TokenStack.COLUMN_GROUP : 0;
          default -> 0;
        };
    addDepth(stack.addParts(f, parts));
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

  /**
   * M-11 items 2, 3, 6 and 7 for a start tag in C, in HTML context and in a foreign region of C
   * (item 9). The other items are the rules of the main pass, which run on this start tag too.
   */
  private void noscriptStartTag(@Nullable Name k, boolean html) {
    if (tok.hasBlockAttribute()) {
      throw structure("M-11"); // item 2
    }
    if (k == Name.FRAMESET
        || k == Name.MATH
        || k == Name.NOSCRIPT
        || k == Name.PLAINTEXT
        || k == Name.SELECT
        || k == Name.TEMPLATE) {
      throw structure("M-11"); // item 3
    }
    if (k == Name.HTML || k == Name.BODY || k == Name.HEAD) {
      throw structure("M-11"); // item 7
    }
    // Item 6 (H-38): with scripting disabled, a table part or col without a table of C closes a
    // cell or caption around the noscript element.
    if ((Name.is(k, Name.Set.TABLE_PART) || k == Name.COL) && noscriptTables == 0) {
      throw structure("M-11");
    }
    if (html && k == Name.TABLE) {
      noscriptTables++; // a table is never void, so the main pass pushes it
    }
  }

  /**
   * M-11 items 5 and 7 for an end tag in C (not the end tag {@code noscript} that ends C): it has
   * the name of the last open element of C and closes it, or it is the end tag of the region root
   * of item 9 (H-75), which closes the region.
   */
  private void noscriptEndTag(String name, @Nullable Name k) {
    if (k == Name.HTML || k == Name.BODY || k == Name.HEAD) {
      throw structure("M-11"); // item 7
    }
    int top = stack.size() - 1;
    boolean closesTop = top >= noscriptBase && stack.name(top).equals(name);
    boolean closesRegion =
        stack.regionRoot() >= noscriptBase && stack.nearest(name) == stack.regionRoot();
    if (!closesTop && !closesRegion) {
      throw structure("M-11");
    }
    if (closesTop && k == Name.TABLE && !stack.foreign(top)) {
      noscriptTables--;
    }
  }

  /** M-22 and M-24: a start tag in HTML context in the content of an html block. */
  private void contentStartTag(@Nullable Name k, boolean selfClosing) {
    if (Name.is(k, Name.Set.FORBIDDEN_CONTENT)) {
      throw structure("M-22");
    }
    if (selfClosing && !Name.is(k, Name.Set.VOID) && k != Name.SVG && k != Name.MATH) {
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

  /**
   * M-09 for a start tag in HTML context: table parts only directly in their table-family parent,
   * with condition 1 (H-35: no {@code col} where the search for F stops at a {@code template}) and
   * condition 2 (H-37: in table context, F is the current node). The checks come before the implied
   * parts and the push (M-26), so the tree builder never pops a foster-parented element without its
   * end tag ("clear the stack back to a table context").
   */
  private void tablePart(@Nullable Name k) {
    if (k == null || (!k.inAny(Name.Set.TABLE_FAMILY) && k != Name.COL)) {
      return;
    }
    if (k == Name.COL && stack.templateTableContext()) {
      throw structure("M-09"); // condition 1
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
          default -> true; // table
        };
    if (!allowed) {
      throw structure("M-09");
    }
    if (tableContext() && stack.tableFamilyIndex() != stack.size() - 1) {
      throw structure("M-09"); // condition 2
    }
  }

  /**
   * M-10 item 2 (H-36): a {@code form} start tag in HTML context while a {@code form} pushed in
   * HTML context is on the token stack. The count of such forms goes down on every pop, also on a
   * crossing ({@link TokenStack#open(Name)}).
   */
  private void nestedForm(@Nullable Name k) {
    if (k == Name.FORM && stack.open(Name.FORM) > 0) {
      throw structure("M-10");
    }
  }

  private void endTag() {
    String raw = tok.name();
    Name k = Name.of(raw);
    String name = names.intern(raw, k);
    if (textOnly >= 0) {
      // M-07 rules 3 and 4: only the end tag of the element ends its text content.
      if (stack.size() - 1 == textOnly && stack.name(textOnly).equals(name)) {
        popTo(textOnly, k);
        textOnly = -1;
        return;
      }
      throw structure("M-07");
    }
    if (noscriptBase >= 0) {
      noscriptEndTag(name, k);
    }
    if (pending != null && blockIndex < 0) {
      // The appropriate end tag of a title or json block (M-20).
      finishBlock(tok.start());
    }
    if (noscriptContent >= 0) {
      int from = noscriptContent;
      noscriptContent = -1;
      noscript(from, tok.start());
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
      popTo(index, k);
      return;
    }
    if (index < 0) {
      // M-06 item 3 (H-30): the tree builder can close an implied row group or row, and the cell.
      if (k == Name.TBODY
          || k == Name.THEAD
          || k == Name.TFOOT
          || k == Name.TR
          || k == Name.TABLE) {
        throw structure("M-06");
      }
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
    popTo(index, k);
  }

  /**
   * M-05 step 3 for an end tag with the known name {@code k} that matches the element at {@code
   * index}: M-06 items 2 and 4, then the pops, the change of D (M-26 items 4 and 5) and of the
   * special-crossed mark (M-06 item 4). The loop visits each popped element once, so the checks add
   * no extra pass.
   */
  private void popTo(int index, @Nullable Name k) {
    int top = stack.size() - 1;
    boolean formatting = Name.is(k, Name.Set.FORMATTING);
    boolean mark = false;
    // H-75: the end tag of the region root pops the whole region in the tree builder too, so the
    // foreign elements that it crosses set no mark.
    boolean rootEnd = index == stack.regionRoot();
    for (int i = top; i >= index; i--) {
      // A crossed element sets the mark when it is special (pushed in HTML context) or foreign.
      boolean special =
          i > index && (stack.foreign(i) ? !rootEnd : Name.is(stack.known(i), Name.Set.SPECIAL));
      boolean marked = stack.marked(i);
      if (formatting && (special || marked)) {
        throw structure("M-06"); // item 2 (special or foreign), item 4 (marked)
      }
      mark |= special || marked;
    }
    // M-26 item 4: an exact pop of a clean element gives its units back. The matched element has
    // the name of the end tag.
    boolean exact = index == top && !stack.dirty(index) && k != Name.HTML && k != Name.BODY;
    if (exact) {
      depth -= 1 + stack.partCount(index);
    }
    stack.popTo(index);
    int current = stack.size() - 1;
    if (current >= 0) {
      if (!exact) {
        stack.setDirty(current); // M-26 item 5
      }
      if (mark) {
        stack.mark(current); // M-06 item 4
      }
    }
  }

  /** M-22 item 4, M-23 and the block end tag of M-20. */
  private void contentEndTag(String name, @Nullable Name k) {
    int top = stack.size() - 1;
    int root = stack.regionRoot();
    if (root >= 0) {
      // M-23 (H-75): in a foreign region of the content, an end tag has the name of the current
      // node, or it is the end tag of the region root, which pops the region. M-22 item 4 is for
      // HTML context only.
      int index = stack.nearest(name);
      if (!stack.name(top).equals(name) && index != root) {
        throw structure("M-23");
      }
      popTo(index, k);
      return;
    }
    if (Name.is(k, Name.Set.VOID)) {
      throw structure("M-22");
    }
    if (!stack.name(top).equals(name)) {
      throw structure("M-23");
    }
    if (top == blockIndex) {
      finishBlock(tok.start());
      blockIndex = -1;
    }
    popTo(top, k);
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
      int from = noscriptContent;
      noscriptContent = -1;
      noscript(from, in.length);
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
   * M-11: the content C of a {@code noscript} element. The tokens of C and a following {@code
   * </noscript>} alone, from the data state, go through the rules of the main pass, in place, on
   * the token stack of the main pass (item 8, H-71; DL-044 c). The caller has pushed the {@code
   * noscript} element. {@link #noscriptStartTag} and {@link #noscriptEndTag} add the items of M-11
   * that are not rules of the main pass. When the {@code noscript} element has no end tag, C is the
   * rest of the input (E3).
   *
   * <p>The elements of C are on the token stack above the {@code noscript} element, so the stack is
   * bounded by D (M-26, item 7). After the end tag {@code noscript}, D and the dirty mark of the
   * {@code noscript} element go back to the values that they had before C; C is balanced (item 5),
   * so the stack has the same elements again. The marks need no restore: C sets none (H-75).
   *
   * @param from offset of the first byte of C
   * @param to offset of the end of C
   */
  private void noscript(int from, int to) {
    int length = to - from;
    byte[] c = new byte[length + NOSCRIPT_END.length];
    System.arraycopy(in, from, c, 0, length);
    System.arraycopy(NOSCRIPT_END, 0, c, length, NOSCRIPT_END.length);
    Tokenizer outer = tok;
    int element = stack.size() - 1;
    int savedDepth = depth;
    boolean savedDirty = stack.dirty(element);
    buf = c;
    tok = new Tokenizer(c, 0, c.length);
    noscriptBase = stack.size();
    noscriptTables = 0;
    boolean done = false;
    while (!done) {
      switch (tok.next()) {
        case START_TAG -> startTag();
        case END_TAG -> {
          if (Name.of(tok.name()) == Name.NOSCRIPT) {
            // Item 1, then item 5: no element of C is open at the end of C.
            if (tok.start() != length || stack.size() != noscriptBase) {
              throw structure("M-11");
            }
            done = true;
          } else {
            endTag();
          }
        }
        case COMMENT -> comment();
        case DOCTYPE -> doctype();
        case EOF -> throw structure("M-11");
      }
    }
    tok = outer;
    buf = in;
    noscriptBase = -1;
    depth = savedDepth;
    stack.restoreDirty(element, savedDirty);
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
   * {@code rivqen-manifest} (ASCII case-insensitive), '&' or a byte of 0x80 or more (H-28).
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
      // A Java byte of 0x80 or more is negative: a byte of a non-ASCII character (item 3).
      if (in[i] == '&' || in[i] < 0) {
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
