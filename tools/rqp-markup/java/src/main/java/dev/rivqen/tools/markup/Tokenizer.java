// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import org.jspecify.annotations.Nullable;

/**
 * A subset of the WHATWG HTML tokenizer (HTML §13.2.5) over UTF-8 bytes, with exact byte offsets.
 *
 * <p>It implements the states that {@code markup.md} §3.1 lists: data, tags and attributes,
 * comments, bogus comments and processing instructions, DOCTYPE, RCDATA, RAWTEXT, script data (with
 * the escaped and double-escaped states) and PLAINTEXT. A CDATA section is read as a bogus comment;
 * {@link #cdataClosed()} tells whether both readings end at the same byte (M-12).
 *
 * <p>Character tokens are not reported: no rule depends on them. Character references are not
 * decoded: the rules use raw values (DL-010). Preprocessing (CR LF to LF) does not change token
 * boundaries; CR counts as white space. All structural characters are ASCII, so the tokenizer can
 * work on bytes of valid UTF-8 directly. Every method advances monotonically: one call to {@link
 * #next()} reads each byte a bounded number of times.
 *
 * <p>The caller switches to raw text with {@link #enterText} after a start tag (M-04). The
 * tokenizer has no tree builder.
 */
final class Tokenizer {

  /** Token types. Character tokens are not reported. */
  enum Type {
    START_TAG,
    END_TAG,
    /** A comment, a bogus comment, a processing instruction or a CDATA section. */
    COMMENT,
    DOCTYPE,
    EOF
  }

  private static final byte[] DATA_RQ_BLOCK = ascii("data-rq-block");
  private static final byte[] TYPE = ascii("type");
  private static final byte[] DOCTYPE = ascii("doctype");
  private static final byte[] CDATA_OPEN = ascii("[CDATA[");
  private static final byte[] SCRIPT = ascii("script");
  private static final byte[] REPLACEMENT = {(byte) 0xEF, (byte) 0xBF, (byte) 0xBD};

  // Attribute states.
  private static final int BEFORE_NAME = 0;
  private static final int NAME = 1;
  private static final int AFTER_NAME = 2;
  private static final int BEFORE_VALUE = 3;
  private static final int VALUE_DOUBLE = 4;
  private static final int VALUE_SINGLE = 5;
  private static final int VALUE_UNQUOTED = 6;
  private static final int AFTER_QUOTED_VALUE = 7;
  private static final int SELF_CLOSING = 8;

  // Attributes that the rules read.
  private static final int ATTR_OTHER = 0;
  private static final int ATTR_BLOCK = 1;
  private static final int ATTR_TYPE = 2;

  // Comment states.
  private static final int COMMENT_START = 0;
  private static final int COMMENT_START_DASH = 1;
  private static final int COMMENT_BODY = 2;
  private static final int COMMENT_END_DASH = 3;
  private static final int COMMENT_END = 4;
  private static final int COMMENT_END_BANG = 5;

  // Script data states.
  private static final int SD = 0;
  private static final int SD_LT = 1;
  private static final int SD_ESCAPE_START = 2;
  private static final int SD_ESCAPE_START_DASH = 3;
  private static final int SD_ESCAPED = 4;
  private static final int SD_ESCAPED_DASH = 5;
  private static final int SD_ESCAPED_DASH_DASH = 6;
  private static final int SD_ESCAPED_LT = 7;
  private static final int SD_DOUBLE = 8;
  private static final int SD_DOUBLE_DASH = 9;
  private static final int SD_DOUBLE_DASH_DASH = 10;
  private static final int SD_DOUBLE_LT = 11;

  private final byte[] in;
  private final int limit;
  private int pos;

  private @Nullable TextMode textMode;
  private byte[] textName = new byte[0];

  private Type type = Type.EOF;
  private int start;
  private int end;
  private String name = "";
  private boolean selfClosing;
  private boolean cdata;
  private boolean cdataClosed;
  private int blockValueStart = -1;
  private int blockValueEnd = -1;
  private int typeValueStart = -1;
  private int typeValueEnd = -1;

  private byte[] nameBuf = new byte[32];
  private int nameLen;

  /**
   * Creates a tokenizer for {@code in[from, limit)} in the data state.
   *
   * @param in the input bytes (valid UTF-8)
   * @param from the first byte to read (after a byte order mark)
   * @param limit the end of the input
   */
  Tokenizer(byte[] in, int from, int limit) {
    this.in = in;
    this.pos = from;
    this.limit = limit;
  }

  /**
   * Switches to a raw text state after the start tag that was just returned (M-04).
   *
   * @param mode the state
   * @param endName the name of the appropriate end tag (lower-case ASCII)
   */
  void enterText(TextMode mode, String endName) {
    textMode = mode;
    textName = ascii(endName);
  }

  /** Reads the next token that is not a character token. */
  Type next() {
    TextMode mode = textMode;
    if (mode != null) {
      textMode = null;
      return switch (mode) {
        case RCDATA, RAWTEXT -> rawText();
        case SCRIPT_DATA -> scriptData();
        case PLAINTEXT -> eof();
      };
    }
    int i = pos;
    while (true) {
      int lt = indexOf((byte) '<', i);
      if (lt < 0) {
        return eof();
      }
      int p = lt + 1;
      if (p >= limit) {
        return eof();
      }
      byte c = in[p];
      if (isAlpha(c)) {
        return tag(lt, Type.START_TAG, p);
      }
      if (c == '!') {
        return markupDeclaration(lt);
      }
      if (c == '?') {
        return bogusComment(lt, p);
      }
      if (c == '/') {
        int q = p + 1;
        if (q >= limit) {
          return eof();
        }
        byte d = in[q];
        if (isAlpha(d)) {
          return tag(lt, Type.END_TAG, q);
        }
        if (d != '>') {
          return bogusComment(lt, q);
        }
        // "</>" emits nothing.
        i = q + 1;
      } else {
        // The '<' is a character; reconsume c in the data state.
        i = p;
      }
    }
  }

  /** Offset of the first byte of the token (its {@code <}). */
  int start() {
    return start;
  }

  /** Offset after the last byte of the token (after its {@code >}). */
  int end() {
    return end;
  }

  /** Tag name: ASCII lower case, U+0000 replaced by U+FFFD, bytes as ISO-8859-1 characters. */
  String name() {
    return name;
  }

  boolean selfClosing() {
    return selfClosing;
  }

  /** True for a {@code <![CDATA[} token. */
  boolean cdata() {
    return cdata;
  }

  /** For a CDATA token: true when its first {@code >} ends a {@code ]]>} (M-12). */
  boolean cdataClosed() {
    return cdataClosed;
  }

  /** True when the start tag has a {@code data-rq-block} attribute (the first one counts). */
  boolean hasBlockAttribute() {
    return blockValueStart >= 0;
  }

  int blockValueStart() {
    return blockValueStart;
  }

  int blockValueEnd() {
    return blockValueEnd;
  }

  /** True when the tag has a {@code type} attribute (the first one counts). */
  boolean hasTypeAttribute() {
    return typeValueStart >= 0;
  }

  int typeValueStart() {
    return typeValueStart;
  }

  int typeValueEnd() {
    return typeValueEnd;
  }

  // ---------------------------------------------------------------------------------------------
  // Tags and attributes

  private Type tag(int lt, Type kind, int nameStart) {
    beginTag(lt, kind);
    nameLen = 0;
    for (int i = nameStart; i < limit; i++) {
      byte b = in[i];
      if (isSpace(b) || b == '/' || b == '>') {
        name = new String(nameBuf, 0, nameLen, StandardCharsets.ISO_8859_1);
        return attributes(i);
      }
      appendName(b);
    }
    return eof();
  }

  private void beginTag(int lt, Type kind) {
    type = kind;
    start = lt;
    selfClosing = false;
    cdata = false;
    cdataClosed = false;
    blockValueStart = -1;
    blockValueEnd = -1;
    typeValueStart = -1;
    typeValueEnd = -1;
  }

  private void appendName(byte b) {
    if (b == 0) {
      for (byte r : REPLACEMENT) {
        appendNameByte(r);
      }
    } else {
      appendNameByte(toLower(b));
    }
  }

  private void appendNameByte(byte b) {
    if (nameLen == nameBuf.length) {
      nameBuf = Arrays.copyOf(nameBuf, nameBuf.length * 2);
    }
    nameBuf[nameLen++] = b;
  }

  /**
   * Reads the attributes of a tag from {@code from}, in the "before attribute name" state, to the
   * {@code >} that ends the tag. A tag that the input ends inside is dropped (EOF).
   */
  private Type attributes(int from) {
    int state = BEFORE_NAME;
    int nameStart = from;
    int attr = ATTR_OTHER;
    int valueStart = from;
    int i = from;
    while (i < limit) {
      byte b = in[i];
      switch (state) {
        case BEFORE_NAME -> {
          if (isSpace(b)) {
            i++;
          } else if (b == '/' || b == '>') {
            state = AFTER_NAME;
          } else {
            // Also '=': it starts the name of a new attribute.
            nameStart = i;
            state = NAME;
            i++;
          }
        }
        case NAME -> {
          if (isSpace(b) || b == '/' || b == '>') {
            attr = attributeName(nameStart, i);
            state = AFTER_NAME;
          } else if (b == '=') {
            attr = attributeName(nameStart, i);
            state = BEFORE_VALUE;
            i++;
          } else {
            i++;
          }
        }
        case AFTER_NAME -> {
          if (isSpace(b)) {
            i++;
          } else if (b == '/') {
            state = SELF_CLOSING;
            i++;
          } else if (b == '=') {
            state = BEFORE_VALUE;
            i++;
          } else if (b == '>') {
            return emitTag(i + 1);
          } else {
            nameStart = i;
            state = NAME;
            i++;
          }
        }
        case BEFORE_VALUE -> {
          if (isSpace(b)) {
            i++;
          } else if (b == '"') {
            state = VALUE_DOUBLE;
            valueStart = ++i;
          } else if (b == '\'') {
            state = VALUE_SINGLE;
            valueStart = ++i;
          } else if (b == '>') {
            return emitTag(i + 1);
          } else {
            state = VALUE_UNQUOTED;
            valueStart = i;
          }
        }
        case VALUE_DOUBLE, VALUE_SINGLE -> {
          if (b == (state == VALUE_DOUBLE ? (byte) '"' : (byte) '\'')) {
            attributeValue(attr, valueStart, i);
            state = AFTER_QUOTED_VALUE;
          }
          i++;
        }
        case VALUE_UNQUOTED -> {
          if (isSpace(b)) {
            attributeValue(attr, valueStart, i);
            state = BEFORE_NAME;
            i++;
          } else if (b == '>') {
            attributeValue(attr, valueStart, i);
            return emitTag(i + 1);
          } else {
            i++;
          }
        }
        case AFTER_QUOTED_VALUE -> {
          if (isSpace(b)) {
            state = BEFORE_NAME;
            i++;
          } else if (b == '/') {
            state = SELF_CLOSING;
            i++;
          } else if (b == '>') {
            return emitTag(i + 1);
          } else {
            state = BEFORE_NAME;
          }
        }
        default -> {
          // SELF_CLOSING
          if (b == '>') {
            selfClosing = true;
            return emitTag(i + 1);
          }
          state = BEFORE_NAME;
        }
      }
    }
    return eof();
  }

  /**
   * Classifies a finished attribute name. A name that is already on the tag is dropped (WHATWG
   * "attribute name state"), so only the first {@code data-rq-block} and {@code type} count. An
   * attribute without a value has the empty value.
   */
  private int attributeName(int from, int to) {
    if (blockValueStart < 0 && equalsIgnoreCase(from, to, DATA_RQ_BLOCK)) {
      blockValueStart = to;
      blockValueEnd = to;
      return ATTR_BLOCK;
    }
    if (typeValueStart < 0 && equalsIgnoreCase(from, to, TYPE)) {
      typeValueStart = to;
      typeValueEnd = to;
      return ATTR_TYPE;
    }
    return ATTR_OTHER;
  }

  private void attributeValue(int attr, int from, int to) {
    if (attr == ATTR_BLOCK) {
      blockValueStart = from;
      blockValueEnd = to;
    } else if (attr == ATTR_TYPE) {
      typeValueStart = from;
      typeValueEnd = to;
    }
  }

  private Type emitTag(int after) {
    end = after;
    pos = after;
    return type;
  }

  // ---------------------------------------------------------------------------------------------
  // Comments, bogus comments, DOCTYPE, CDATA

  private Type markupDeclaration(int lt) {
    int p = lt + 2;
    if (p + 1 < limit && in[p] == '-' && in[p + 1] == '-') {
      return comment(lt, p + 2);
    }
    if (equalsIgnoreCase(p, Math.min(limit, p + DOCTYPE.length), DOCTYPE)) {
      return untilGreaterThan(lt, p + DOCTYPE.length, Type.DOCTYPE);
    }
    if (regionEquals(p, CDATA_OPEN)) {
      int contentStart = p + CDATA_OPEN.length;
      int gt = indexOf((byte) '>', contentStart);
      untilGreaterThan(lt, contentStart, Type.COMMENT);
      cdata = true;
      // The "]]" cannot overlap the opener: the byte before contentStart is '['.
      cdataClosed = gt < 0 || (in[gt - 1] == ']' && in[gt - 2] == ']');
      return Type.COMMENT;
    }
    return bogusComment(lt, p);
  }

  private Type bogusComment(int lt, int from) {
    return untilGreaterThan(lt, from, Type.COMMENT);
  }

  /** A token that ends at the first {@code >} at or after {@code from}, or at EOF. */
  private Type untilGreaterThan(int lt, int from, Type kind) {
    beginTag(lt, kind);
    int gt = indexOf((byte) '>', from);
    return emitTag(gt < 0 ? limit : gt + 1);
  }

  /**
   * The WHATWG comment states from "comment start". The comment less-than sign states only add
   * parse errors; they do not move the end of the comment, so they are folded into the comment
   * state.
   */
  private Type comment(int lt, int from) {
    beginTag(lt, Type.COMMENT);
    int state = COMMENT_START;
    int i = from;
    while (i < limit) {
      byte b = in[i];
      switch (state) {
        case COMMENT_START -> {
          if (b == '>') {
            return emitTag(i + 1);
          }
          if (b == '-') {
            state = COMMENT_START_DASH;
            i++;
          } else {
            state = COMMENT_BODY;
          }
        }
        case COMMENT_START_DASH -> {
          if (b == '>') {
            return emitTag(i + 1);
          }
          if (b == '-') {
            state = COMMENT_END;
            i++;
          } else {
            state = COMMENT_BODY;
          }
        }
        case COMMENT_BODY -> {
          if (b == '-') {
            state = COMMENT_END_DASH;
          }
          i++;
        }
        case COMMENT_END_DASH -> {
          if (b == '-') {
            state = COMMENT_END;
            i++;
          } else {
            state = COMMENT_BODY;
          }
        }
        case COMMENT_END -> {
          if (b == '>') {
            return emitTag(i + 1);
          }
          if (b == '!') {
            state = COMMENT_END_BANG;
            i++;
          } else if (b == '-') {
            i++;
          } else {
            state = COMMENT_BODY;
          }
        }
        default -> {
          // COMMENT_END_BANG
          if (b == '>') {
            return emitTag(i + 1);
          }
          if (b == '-') {
            state = COMMENT_END_DASH;
            i++;
          } else {
            state = COMMENT_BODY;
          }
        }
      }
    }
    return emitTag(limit);
  }

  // ---------------------------------------------------------------------------------------------
  // Raw text

  /** RCDATA and RAWTEXT: up to the appropriate end tag. */
  private Type rawText() {
    int i = pos;
    while (true) {
      int lt = indexOf((byte) '<', i);
      if (lt < 0) {
        return eof();
      }
      int j = lt + 1;
      if (j < limit && in[j] == '/') {
        j++;
        int k = letterRunEnd(j);
        if (appropriateEndTag(j, k)) {
          return textEndTag(lt, k);
        }
        // Reconsume the byte after the letters in the text state.
        i = k;
      } else {
        i = j;
      }
    }
  }

  /** Script data with the escaped and double-escaped states (HTML §13.2.5.4 to §13.2.5.31). */
  private Type scriptData() {
    int state = SD;
    int i = pos;
    while (i < limit) {
      byte b = in[i];
      switch (state) {
        case SD -> {
          if (b == '<') {
            state = SD_LT;
          }
          i++;
        }
        case SD_LT -> {
          if (b == '/') {
            int k = letterRunEnd(i + 1);
            if (appropriateEndTag(i + 1, k)) {
              return textEndTag(i - 1, k);
            }
            state = SD;
            i = k;
          } else if (b == '!') {
            state = SD_ESCAPE_START;
            i++;
          } else {
            state = SD;
          }
        }
        case SD_ESCAPE_START -> {
          if (b == '-') {
            state = SD_ESCAPE_START_DASH;
            i++;
          } else {
            state = SD;
          }
        }
        case SD_ESCAPE_START_DASH -> {
          if (b == '-') {
            state = SD_ESCAPED_DASH_DASH;
            i++;
          } else {
            state = SD;
          }
        }
        case SD_ESCAPED, SD_ESCAPED_DASH, SD_ESCAPED_DASH_DASH -> {
          if (b == '-') {
            state = state == SD_ESCAPED ? SD_ESCAPED_DASH : SD_ESCAPED_DASH_DASH;
          } else if (b == '<') {
            state = SD_ESCAPED_LT;
          } else if (b == '>' && state == SD_ESCAPED_DASH_DASH) {
            state = SD;
          } else {
            state = SD_ESCAPED;
          }
          i++;
        }
        case SD_ESCAPED_LT -> {
          if (b == '/') {
            int k = letterRunEnd(i + 1);
            if (appropriateEndTag(i + 1, k)) {
              return textEndTag(i - 1, k);
            }
            state = SD_ESCAPED;
            i = k;
          } else if (isAlpha(b)) {
            // Script data double escape start state.
            int k = letterRunEnd(i);
            if (k < limit && isEndNameTerminator(in[k])) {
              state = equalsIgnoreCase(i, k, SCRIPT) ? SD_DOUBLE : SD_ESCAPED;
              i = k + 1;
            } else {
              state = SD_ESCAPED;
              i = k;
            }
          } else {
            state = SD_ESCAPED;
          }
        }
        case SD_DOUBLE, SD_DOUBLE_DASH, SD_DOUBLE_DASH_DASH -> {
          if (b == '-') {
            state = state == SD_DOUBLE ? SD_DOUBLE_DASH : SD_DOUBLE_DASH_DASH;
          } else if (b == '<') {
            state = SD_DOUBLE_LT;
          } else if (b == '>' && state == SD_DOUBLE_DASH_DASH) {
            state = SD;
          } else {
            state = SD_DOUBLE;
          }
          i++;
        }
        default -> {
          // SD_DOUBLE_LT
          if (b == '/') {
            // Script data double escape end state.
            int j = i + 1;
            int k = letterRunEnd(j);
            if (k < limit && isEndNameTerminator(in[k])) {
              state = equalsIgnoreCase(j, k, SCRIPT) ? SD_ESCAPED : SD_DOUBLE;
              i = k + 1;
            } else {
              state = SD_DOUBLE;
              i = k;
            }
          } else {
            state = SD_DOUBLE;
          }
        }
      }
    }
    return eof();
  }

  /**
   * True when {@code in[from, to)} is the name of the raw text element and a terminator follows.
   */
  private boolean appropriateEndTag(int from, int to) {
    return to < limit && isEndNameTerminator(in[to]) && equalsIgnoreCase(from, to, textName);
  }

  private Type textEndTag(int lt, int nameEnd) {
    beginTag(lt, Type.END_TAG);
    name = new String(textName, StandardCharsets.ISO_8859_1);
    return attributes(nameEnd);
  }

  private Type eof() {
    beginTag(limit, Type.EOF);
    textMode = null;
    return emitTag(limit);
  }

  // ---------------------------------------------------------------------------------------------
  // Byte helpers

  private int indexOf(byte b, int from) {
    for (int i = from; i < limit; i++) {
      if (in[i] == b) {
        return i;
      }
    }
    return -1;
  }

  private int letterRunEnd(int from) {
    int i = from;
    while (i < limit && isAlpha(in[i])) {
      i++;
    }
    return i;
  }

  private boolean equalsIgnoreCase(int from, int to, byte[] lower) {
    if (to - from != lower.length) {
      return false;
    }
    for (int i = 0; i < lower.length; i++) {
      if (toLower(in[from + i]) != lower[i]) {
        return false;
      }
    }
    return true;
  }

  private boolean regionEquals(int from, byte[] exact) {
    if (limit - from < exact.length) {
      return false;
    }
    for (int i = 0; i < exact.length; i++) {
      if (in[from + i] != exact[i]) {
        return false;
      }
    }
    return true;
  }

  /** Tokenizer white space after preprocessing: TAB, LF, FF, SPACE, and CR (it becomes LF). */
  static boolean isSpace(byte b) {
    return b == ' ' || b == '\n' || b == '\t' || b == '\f' || b == '\r';
  }

  private static boolean isEndNameTerminator(byte b) {
    return isSpace(b) || b == '/' || b == '>';
  }

  static boolean isAlpha(byte b) {
    return (b >= 'a' && b <= 'z') || (b >= 'A' && b <= 'Z');
  }

  static byte toLower(byte b) {
    return b >= 'A' && b <= 'Z' ? (byte) (b + ('a' - 'A')) : b;
  }

  static byte[] ascii(String s) {
    return s.getBytes(StandardCharsets.US_ASCII);
  }
}
