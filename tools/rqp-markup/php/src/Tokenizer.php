<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup;

/**
 * A subset of the WHATWG HTML tokenizer (HTML Living Standard 13.2.5) that
 * reports byte spans and builds no tree (markup.md M-03, section 3.1).
 *
 * Design:
 * - Works on bytes. Every character that changes a state is ASCII, so a
 *   non-ASCII byte is always "anything else". UTF-8 validity is checked
 *   before (M-02).
 * - Input stream preprocessing turns CR and CR LF into LF. The tokenizer
 *   treats CR as white space, which gives the same token boundaries:
 *   in every state that reads white space, LF after CR is white space too.
 *   Offsets always refer to the input bytes.
 * - Character tokens are not emitted. Character references are not decoded:
 *   no character reference state consumes `<`, `>`, a quote or white space,
 *   so they never change a token boundary.
 * - A start or end tag that reaches EOF is dropped, as in WHATWG
 *   ("eof-in-tag": emit an end-of-file token only).
 * - Only the first value of each name in WATCHED is kept. WHATWG drops a
 *   later attribute with the same name, so the first one is the one that
 *   counts. Memory per token is O(1).
 *
 * Termination: every loop step consumes at least one byte or moves to a
 * state that consumes a byte in its next step (no cycle without progress).
 */
final class Tokenizer
{
    /** Attribute names whose raw value the rules need (M-13, M-14, M-16). */
    public const WATCHED = ['data-rq-block' => true, 'type' => true];

    private const WS = "\t\n\f\r ";
    private const TAG_NAME_END = "\t\n\f\r />";
    private const ATTR_NAME_END = "\t\n\f\r />=";
    private const UNQUOTED_END = "\t\n\f\r >";
    private const ALPHA = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz';

    // Tag states after the tag name (13.2.5.32 to 13.2.5.40).
    private const BEFORE_ATTR_NAME = 0;
    private const AFTER_ATTR_NAME = 1;
    private const BEFORE_ATTR_VALUE = 2;
    private const AFTER_ATTR_VALUE_QUOTED = 3;
    private const SELF_CLOSING_START_TAG = 4;

    // Comment states (13.2.5.43 to 13.2.5.52).
    private const COMMENT_START = 0;
    private const COMMENT_START_DASH = 1;
    private const COMMENT = 2;
    private const COMMENT_LT = 3;
    private const COMMENT_LT_BANG = 4;
    private const COMMENT_LT_BANG_DASH = 5;
    private const COMMENT_LT_BANG_DASH_DASH = 6;
    private const COMMENT_END_DASH = 7;
    private const COMMENT_END = 8;
    private const COMMENT_END_BANG = 9;

    // Script data states (13.2.5.4, 13.2.5.15 to 13.2.5.31).
    private const SCRIPT = 0;
    private const SCRIPT_LT = 1;
    private const SCRIPT_ESCAPE_START = 2;
    private const SCRIPT_ESCAPE_START_DASH = 3;
    private const SCRIPT_ESCAPED = 4;
    private const SCRIPT_ESCAPED_DASH = 5;
    private const SCRIPT_ESCAPED_DASH_DASH = 6;
    private const SCRIPT_ESCAPED_LT = 7;
    private const SCRIPT_DOUBLE_ESCAPE_START = 8;
    private const SCRIPT_DOUBLE_ESCAPED = 9;
    private const SCRIPT_DOUBLE_ESCAPED_DASH = 10;
    private const SCRIPT_DOUBLE_ESCAPED_DASH_DASH = 11;
    private const SCRIPT_DOUBLE_ESCAPED_LT = 12;
    private const SCRIPT_DOUBLE_ESCAPE_END = 13;

    private readonly int $length;
    private int $pos;
    private ContentState $state = ContentState::Data;
    private string $endTagName = '';

    /**
     * @param int $start First offset that the tokenizer reads (3 after a BOM, M-02).
     */
    public function __construct(private readonly string $input, int $start = 0)
    {
        $this->length = strlen($input);
        $this->pos = max(0, min($start, $this->length));
    }

    /**
     * Change the state after a start tag (M-04). `$endTagName` is the name of
     * that start tag: it defines the "appropriate end tag".
     */
    public function switchTo(ContentState $state, string $endTagName): void
    {
        $this->state = $state;
        $this->endTagName = $endTagName;
    }

    public function state(): ContentState
    {
        return $this->state;
    }

    public function next(): Token
    {
        return match ($this->state) {
            ContentState::Data => $this->data(),
            ContentState::Rcdata, ContentState::Rawtext => $this->rawText(),
            ContentState::ScriptData => $this->scriptData(),
            ContentState::Plaintext => $this->eof(),
        };
    }

    /** Lower-case ASCII and replace U+0000 by U+FFFD, as the tokenizer does for names. */
    public static function normalizeName(string $raw): string
    {
        $name = strtolower($raw);
        if (str_contains($name, "\0")) {
            $name = str_replace("\0", "\u{FFFD}", $name);
        }
        return $name;
    }

    // ----------------------------------------------------------------------
    // Data state (13.2.5.1), tag open (13.2.5.6), end tag open (13.2.5.7)
    // ----------------------------------------------------------------------

    private function data(): Token
    {
        $in = $this->input;
        $i = $this->pos;
        while ($i < $this->length) {
            $lt = strpos($in, '<', $i);
            if ($lt === false) {
                break;
            }
            $next = $lt + 1;
            if ($next >= $this->length) {
                // Tag open state, EOF: emit '<' and EOF.
                break;
            }
            $c = $in[$next];
            if ($c === '!') {
                return $this->markupDeclarationOpen($lt, $next + 1);
            }
            if ($c === '/') {
                $q = $next + 1;
                if ($q >= $this->length) {
                    break;
                }
                $d = $in[$q];
                if (self::isAlpha($d)) {
                    return $this->tag($lt, $q, false);
                }
                if ($d === '>') {
                    // `</>`: missing-end-tag-name. No token; back to data.
                    $i = $q + 1;
                    continue;
                }
                return $this->bogusComment($lt, $q);
            }
            if (self::isAlpha($c)) {
                return $this->tag($lt, $next, true);
            }
            if ($c === '?') {
                return $this->processingInstruction($lt, $next + 1);
            }
            // Anything else: '<' is a character. Reconsume in the data state.
            $i = $next;
        }
        return $this->eof();
    }

    private function markupDeclarationOpen(int $lt, int $q): Token
    {
        $in = $this->input;
        if (substr($in, $q, 2) === '--') {
            return $this->comment($lt, $q + 2);
        }
        $word = substr($in, $q, 7);
        if (strcasecmp($word, 'DOCTYPE') === 0) {
            // Every DOCTYPE state ends the token at '>' (also inside quoted
            // identifiers: "abrupt-doctype-...-identifier") or at EOF.
            return $this->emit(new Token(TokenType::Doctype, $lt, $this->endAtGt($q + 7)));
        }
        if ($word === '[CDATA[') {
            // In HTML content: bogus comment to the first '>'. In foreign
            // content: CDATA section to the first ']]>'. M-12 requires that
            // both end at the same byte; cdataClosed says if they do.
            $gt = strpos($in, '>', $q + 7);
            $closed = $gt !== false && $gt - 2 >= $q + 7
                && $in[$gt - 1] === ']' && $in[$gt - 2] === ']';
            $end = $gt === false ? $this->length : $gt + 1;
            return $this->emit(new Token(TokenType::Cdata, $lt, $end, cdataClosed: $closed));
        }
        // incorrectly-opened-comment: bogus comment, nothing consumed.
        return $this->bogusComment($lt, $q);
    }

    /** Bogus comment state (13.2.5.41): ends at the first '>' or at EOF. */
    private function bogusComment(int $lt, int $from): Token
    {
        return $this->emit(new Token(TokenType::Comment, $lt, $this->endAtGt($from)));
    }

    /**
     * `<?`: a bogus comment in older parsers, a processing instruction in the
     * current WHATWG text. Both end at the first '>' (M-03). At EOF the
     * current text emits no token and the older one emits a comment; this
     * tokenizer emits a comment. That is the stricter choice: every rule that
     * accepts a comment token also accepts no token.
     */
    private function processingInstruction(int $lt, int $from): Token
    {
        return $this->emit(new Token(TokenType::Comment, $lt, $this->endAtGt($from)));
    }

    private function endAtGt(int $from): int
    {
        $gt = $from < $this->length ? strpos($this->input, '>', $from) : false;
        return $gt === false ? $this->length : $gt + 1;
    }

    // ----------------------------------------------------------------------
    // Comment states (13.2.5.43 to 13.2.5.52)
    // ----------------------------------------------------------------------

    private function comment(int $lt, int $i): Token
    {
        $in = $this->input;
        $len = $this->length;
        $st = self::COMMENT_START;
        while ($i < $len) {
            $c = $in[$i];
            switch ($st) {
                case self::COMMENT_START:
                    if ($c === '-') {
                        $st = self::COMMENT_START_DASH;
                        $i++;
                    } elseif ($c === '>') {
                        // `<!-->`: abrupt-closing-of-empty-comment.
                        return $this->emit(new Token(TokenType::Comment, $lt, $i + 1));
                    } else {
                        $st = self::COMMENT;
                    }
                    break;
                case self::COMMENT_START_DASH:
                    if ($c === '-') {
                        $st = self::COMMENT_END;
                        $i++;
                    } elseif ($c === '>') {
                        // `<!--->`: abrupt-closing-of-empty-comment.
                        return $this->emit(new Token(TokenType::Comment, $lt, $i + 1));
                    } else {
                        $st = self::COMMENT;
                    }
                    break;
                case self::COMMENT:
                    if ($c === '<') {
                        $st = self::COMMENT_LT;
                        $i++;
                    } elseif ($c === '-') {
                        $st = self::COMMENT_END_DASH;
                        $i++;
                    } else {
                        // Append the run of characters that do not change the state.
                        $i += strcspn($in, '<-', $i);
                    }
                    break;
                case self::COMMENT_LT:
                    if ($c === '!') {
                        $st = self::COMMENT_LT_BANG;
                        $i++;
                    } elseif ($c === '<') {
                        $i++;
                    } else {
                        $st = self::COMMENT;
                    }
                    break;
                case self::COMMENT_LT_BANG:
                    if ($c === '-') {
                        $st = self::COMMENT_LT_BANG_DASH;
                        $i++;
                    } else {
                        $st = self::COMMENT;
                    }
                    break;
                case self::COMMENT_LT_BANG_DASH:
                    if ($c === '-') {
                        $st = self::COMMENT_LT_BANG_DASH_DASH;
                        $i++;
                    } else {
                        $st = self::COMMENT_END_DASH;
                    }
                    break;
                case self::COMMENT_LT_BANG_DASH_DASH:
                    // '>' and anything else (nested-comment): reconsume in comment end.
                    $st = self::COMMENT_END;
                    break;
                case self::COMMENT_END_DASH:
                    if ($c === '-') {
                        $st = self::COMMENT_END;
                        $i++;
                    } else {
                        $st = self::COMMENT;
                    }
                    break;
                case self::COMMENT_END:
                    if ($c === '>') {
                        return $this->emit(new Token(TokenType::Comment, $lt, $i + 1));
                    }
                    if ($c === '!') {
                        $st = self::COMMENT_END_BANG;
                        $i++;
                    } elseif ($c === '-') {
                        $i++;
                    } else {
                        $st = self::COMMENT;
                    }
                    break;
                default: // COMMENT_END_BANG
                    if ($c === '-') {
                        $st = self::COMMENT_END_DASH;
                        $i++;
                    } elseif ($c === '>') {
                        // `--!>`: incorrectly-closed-comment.
                        return $this->emit(new Token(TokenType::Comment, $lt, $i + 1));
                    } else {
                        $st = self::COMMENT;
                    }
                    break;
            }
        }
        // EOF in a comment state: emit the comment, then EOF.
        return $this->emit(new Token(TokenType::Comment, $lt, $len));
    }

    // ----------------------------------------------------------------------
    // Tag name (13.2.5.8) and attribute states (13.2.5.32 to 13.2.5.40)
    // ----------------------------------------------------------------------

    private function tag(int $lt, int $nameStart, bool $isStart): Token
    {
        $n = strcspn($this->input, self::TAG_NAME_END, $nameStart);
        $name = self::normalizeName(substr($this->input, $nameStart, $n));
        return $this->tagRest($lt, $nameStart + $n, $name, $isStart);
    }

    /**
     * Continue a tag at offset $i: the byte that ended the tag name
     * (white space, '/', '>') or EOF.
     */
    private function tagRest(int $lt, int $i, string $name, bool $isStart): Token
    {
        $in = $this->input;
        $len = $this->length;
        /** @var array<string, string> $attrs */
        $attrs = [];
        $current = null; // watched name of the attribute being read, if it counts

        if ($i >= $len) {
            return $this->eof();
        }
        $c = $in[$i];
        if ($c === '>') {
            return $this->emitTag($lt, $i + 1, $name, $isStart, false, $attrs);
        }
        $st = $c === '/' ? self::SELF_CLOSING_START_TAG : self::BEFORE_ATTR_NAME;
        $i++;

        while ($i < $len) {
            switch ($st) {
                case self::BEFORE_ATTR_NAME:
                case self::AFTER_ATTR_NAME:
                    $i += strspn($in, self::WS, $i);
                    if ($i >= $len) {
                        break 2;
                    }
                    $c = $in[$i];
                    if ($st === self::BEFORE_ATTR_NAME && ($c === '/' || $c === '>')) {
                        $st = self::AFTER_ATTR_NAME; // reconsume
                        break;
                    }
                    if ($st === self::AFTER_ATTR_NAME) {
                        if ($c === '/') {
                            $st = self::SELF_CLOSING_START_TAG;
                            $i++;
                            break;
                        }
                        if ($c === '=') {
                            $st = self::BEFORE_ATTR_VALUE;
                            $i++;
                            break;
                        }
                        if ($c === '>') {
                            return $this->emitTag($lt, $i + 1, $name, $isStart, false, $attrs);
                        }
                    }
                    // Start a new attribute; attribute name state. The first
                    // character is always part of the name (also '=' in the
                    // before attribute name state).
                    $nameStart = $i;
                    $i++;
                    $i += strcspn($in, self::ATTR_NAME_END, $i);
                    $current = $this->watchedName($nameStart, $i, $attrs);
                    if ($current !== null) {
                        $attrs[$current] = '';
                    }
                    if ($i >= $len) {
                        break 2;
                    }
                    if ($in[$i] === '=') {
                        $st = self::BEFORE_ATTR_VALUE;
                        $i++;
                    } else {
                        $st = self::AFTER_ATTR_NAME; // reconsume
                    }
                    break;
                case self::BEFORE_ATTR_VALUE:
                    $i += strspn($in, self::WS, $i);
                    if ($i >= $len) {
                        break 2;
                    }
                    $c = $in[$i];
                    if ($c === '"' || $c === "'") {
                        $close = strpos($in, $c, $i + 1);
                        if ($close === false) {
                            break 2;
                        }
                        if ($current !== null) {
                            $attrs[$current] = substr($in, $i + 1, $close - $i - 1);
                        }
                        $i = $close + 1;
                        $st = self::AFTER_ATTR_VALUE_QUOTED;
                        break;
                    }
                    if ($c === '>') {
                        // missing-attribute-value: the value is empty.
                        return $this->emitTag($lt, $i + 1, $name, $isStart, false, $attrs);
                    }
                    // Attribute value (unquoted) state.
                    $n = strcspn($in, self::UNQUOTED_END, $i);
                    if ($current !== null) {
                        $attrs[$current] = substr($in, $i, $n);
                    }
                    $i += $n;
                    if ($i >= $len) {
                        break 2;
                    }
                    if ($in[$i] === '>') {
                        return $this->emitTag($lt, $i + 1, $name, $isStart, false, $attrs);
                    }
                    $st = self::BEFORE_ATTR_NAME;
                    $i++;
                    break;
                case self::AFTER_ATTR_VALUE_QUOTED:
                    $c = $in[$i];
                    if ($c === '>') {
                        return $this->emitTag($lt, $i + 1, $name, $isStart, false, $attrs);
                    }
                    if ($c === '/') {
                        $st = self::SELF_CLOSING_START_TAG;
                        $i++;
                    } else {
                        // White space: consume. Anything else
                        // (missing-whitespace-between-attributes): reconsume.
                        $st = self::BEFORE_ATTR_NAME;
                    }
                    break;
                default: // SELF_CLOSING_START_TAG
                    if ($in[$i] === '>') {
                        return $this->emitTag($lt, $i + 1, $name, $isStart, true, $attrs);
                    }
                    // unexpected-solidus-in-tag: reconsume in before attribute name.
                    $st = self::BEFORE_ATTR_NAME;
                    break;
            }
        }
        // eof-in-tag: the tag is dropped.
        return $this->eof();
    }

    /**
     * Return the lower-case name of the attribute [$from, $to) if it is a
     * watched name that the tag does not have yet; else null.
     *
     * @param array<string, string> $attrs
     */
    private function watchedName(int $from, int $to, array $attrs): ?string
    {
        $n = $to - $from;
        // Only the lengths of the watched names can match ('type', 'data-rq-block').
        if ($n !== 4 && $n !== 13) {
            return null;
        }
        $name = strtolower(substr($this->input, $from, $n));
        if (!isset(self::WATCHED[$name]) || isset($attrs[$name])) {
            return null;
        }
        return $name;
    }

    /**
     * @param array<string, string> $attrs
     */
    private function emitTag(int $lt, int $end, string $name, bool $isStart, bool $selfClosing, array $attrs): Token
    {
        if ($isStart) {
            return $this->emit(new Token(TokenType::StartTag, $lt, $end, $name, $selfClosing, $attrs));
        }
        // An end tag can have attributes and a self-closing flag; they have no effect (M-03).
        return $this->emit(new Token(TokenType::EndTag, $lt, $end, $name));
    }

    // ----------------------------------------------------------------------
    // RCDATA and RAWTEXT (13.2.5.2, 13.2.5.3, 13.2.5.9 to 13.2.5.14)
    // ----------------------------------------------------------------------

    private function rawText(): Token
    {
        $in = $this->input;
        $i = $this->pos;
        while ($i < $this->length) {
            $lt = strpos($in, '</', $i);
            if ($lt === false) {
                break;
            }
            $q = $lt + 2;
            $after = $this->appropriateEndTag($q);
            if ($after >= 0) {
                $this->state = ContentState::Data;
                return $this->tagRest($lt, $after, $this->endTagName, false);
            }
            // Not an appropriate end tag: '</' and the letters are characters.
            // Reconsume the next byte in RCDATA/RAWTEXT.
            $i = $q + strspn($in, self::ALPHA, $q);
        }
        return $this->eof();
    }

    /**
     * End tag name states of RCDATA, RAWTEXT and script data: at $q (after
     * `</`), read ASCII letters. Return the offset of the byte after them if
     * they form the appropriate end tag name and are followed by white space,
     * '/' or '>'. Else return -1.
     */
    private function appropriateEndTag(int $q): int
    {
        $n = strspn($this->input, self::ALPHA, $q);
        $after = $q + $n;
        if ($n === 0 || $n !== strlen($this->endTagName) || $after >= $this->length) {
            return -1;
        }
        if (strpos(self::TAG_NAME_END, $this->input[$after]) === false) {
            return -1;
        }
        if (strtolower(substr($this->input, $q, $n)) !== $this->endTagName) {
            return -1;
        }
        return $after;
    }

    // ----------------------------------------------------------------------
    // Script data with the escaped and double-escaped states
    // (13.2.5.4, 13.2.5.15 to 13.2.5.31)
    // ----------------------------------------------------------------------

    private function scriptData(): Token
    {
        $in = $this->input;
        $len = $this->length;
        $i = $this->pos;
        $st = self::SCRIPT;
        $lt = $i;
        while ($i < $len) {
            $c = $in[$i];
            switch ($st) {
                case self::SCRIPT:
                    $found = strpos($in, '<', $i);
                    if ($found === false) {
                        break 2;
                    }
                    $lt = $found;
                    $i = $found + 1;
                    $st = self::SCRIPT_LT;
                    break;
                case self::SCRIPT_LT:
                    if ($c === '/') {
                        $after = $this->appropriateEndTag($i + 1);
                        if ($after >= 0) {
                            $this->state = ContentState::Data;
                            return $this->tagRest($lt, $after, $this->endTagName, false);
                        }
                        $i = $i + 1 + strspn($in, self::ALPHA, $i + 1);
                        $st = self::SCRIPT;
                    } elseif ($c === '!') {
                        $st = self::SCRIPT_ESCAPE_START;
                        $i++;
                    } else {
                        $st = self::SCRIPT;
                    }
                    break;
                case self::SCRIPT_ESCAPE_START:
                    if ($c === '-') {
                        $st = self::SCRIPT_ESCAPE_START_DASH;
                        $i++;
                    } else {
                        $st = self::SCRIPT;
                    }
                    break;
                case self::SCRIPT_ESCAPE_START_DASH:
                    if ($c === '-') {
                        $st = self::SCRIPT_ESCAPED_DASH_DASH;
                        $i++;
                    } else {
                        $st = self::SCRIPT;
                    }
                    break;
                case self::SCRIPT_ESCAPED:
                    if ($c === '-') {
                        $st = self::SCRIPT_ESCAPED_DASH;
                        $i++;
                    } elseif ($c === '<') {
                        $lt = $i;
                        $st = self::SCRIPT_ESCAPED_LT;
                        $i++;
                    } else {
                        $i += strcspn($in, '-<', $i);
                    }
                    break;
                case self::SCRIPT_ESCAPED_DASH:
                case self::SCRIPT_ESCAPED_DASH_DASH:
                    if ($c === '-') {
                        $st = self::SCRIPT_ESCAPED_DASH_DASH;
                    } elseif ($c === '<') {
                        $lt = $i;
                        $st = self::SCRIPT_ESCAPED_LT;
                    } elseif ($c === '>' && $st === self::SCRIPT_ESCAPED_DASH_DASH) {
                        $st = self::SCRIPT;
                    } else {
                        $st = self::SCRIPT_ESCAPED;
                    }
                    $i++;
                    break;
                case self::SCRIPT_ESCAPED_LT:
                    if ($c === '/') {
                        $after = $this->appropriateEndTag($i + 1);
                        if ($after >= 0) {
                            $this->state = ContentState::Data;
                            return $this->tagRest($lt, $after, $this->endTagName, false);
                        }
                        $i = $i + 1 + strspn($in, self::ALPHA, $i + 1);
                        $st = self::SCRIPT_ESCAPED;
                    } elseif (self::isAlpha($c)) {
                        $st = self::SCRIPT_DOUBLE_ESCAPE_START;
                    } else {
                        $st = self::SCRIPT_ESCAPED;
                    }
                    break;
                case self::SCRIPT_DOUBLE_ESCAPE_START:
                case self::SCRIPT_DOUBLE_ESCAPE_END:
                    $n = strspn($in, self::ALPHA, $i);
                    $j = $i + $n;
                    if ($j >= $len) {
                        break 2;
                    }
                    $start = $st === self::SCRIPT_DOUBLE_ESCAPE_START;
                    if (strpos(self::TAG_NAME_END, $in[$j]) !== false) {
                        $isScript = $n === 6 && strtolower(substr($in, $i, 6)) === 'script';
                        if ($start) {
                            $st = $isScript ? self::SCRIPT_DOUBLE_ESCAPED : self::SCRIPT_ESCAPED;
                        } else {
                            $st = $isScript ? self::SCRIPT_ESCAPED : self::SCRIPT_DOUBLE_ESCAPED;
                        }
                        $i = $j + 1;
                    } else {
                        // Anything else: reconsume.
                        $st = $start ? self::SCRIPT_ESCAPED : self::SCRIPT_DOUBLE_ESCAPED;
                        $i = $j;
                    }
                    break;
                case self::SCRIPT_DOUBLE_ESCAPED:
                    if ($c === '-') {
                        $st = self::SCRIPT_DOUBLE_ESCAPED_DASH;
                        $i++;
                    } elseif ($c === '<') {
                        $st = self::SCRIPT_DOUBLE_ESCAPED_LT;
                        $i++;
                    } else {
                        $i += strcspn($in, '-<', $i);
                    }
                    break;
                case self::SCRIPT_DOUBLE_ESCAPED_DASH:
                case self::SCRIPT_DOUBLE_ESCAPED_DASH_DASH:
                    if ($c === '-') {
                        $st = self::SCRIPT_DOUBLE_ESCAPED_DASH_DASH;
                    } elseif ($c === '<') {
                        $st = self::SCRIPT_DOUBLE_ESCAPED_LT;
                    } elseif ($c === '>' && $st === self::SCRIPT_DOUBLE_ESCAPED_DASH_DASH) {
                        $st = self::SCRIPT;
                    } else {
                        $st = self::SCRIPT_DOUBLE_ESCAPED;
                    }
                    $i++;
                    break;
                default: // SCRIPT_DOUBLE_ESCAPED_LT
                    if ($c === '/') {
                        $st = self::SCRIPT_DOUBLE_ESCAPE_END;
                        $i++;
                    } else {
                        $st = self::SCRIPT_DOUBLE_ESCAPED;
                    }
                    break;
            }
        }
        return $this->eof();
    }

    // ----------------------------------------------------------------------

    private function emit(Token $token): Token
    {
        $this->pos = $token->end;
        return $token;
    }

    private function eof(): Token
    {
        $this->pos = $this->length;
        return new Token(TokenType::Eof, $this->length, $this->length);
    }

    private static function isAlpha(string $c): bool
    {
        $o = ord($c) | 0x20;
        return $o >= 0x61 && $o <= 0x7a;
    }
}
