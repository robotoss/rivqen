<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup\Tests\Fuzz;

use Rivqen\Tools\RqpMarkup\ContentState;
use Rivqen\Tools\RqpMarkup\Names;
use Rivqen\Tools\RqpMarkup\Tokenizer;

/**
 * Test oracle: a literal transcription of the WHATWG tokenizer states
 * (HTML 13.2.5), one character at a time, with no fast paths.
 *
 * It differs from src/Tokenizer.php on purpose:
 * - it runs the input stream preprocessing (CR LF and CR become LF) and
 *   maps each character back to its input offset, so it does not use the
 *   "CR is white space" equivalence;
 * - it implements every DOCTYPE state and the processing instruction states
 *   of the current WHATWG text, so it does not use "ends at the first >";
 * - it consumes one character per step with explicit reconsume.
 *
 * `<![CDATA[` is read as in HTML content (bogus comment). Character
 * references are not decoded (no rule needs them). M-04 state changes are
 * applied for HTML context.
 *
 * Tokens are arrays: [type, start, end, name, selfClosing, attributes].
 */
final class ReferenceTokenizer
{
    /** @var list<string> */
    private array $c = [];
    /** @var list<int> */
    private array $o = [];
    private int $n = 0;
    private int $p = 0;
    private string $state = 'data';
    private string $lastStartTag = '';
    private string $tmp = '';
    private int $lt = 0;
    private bool $isEnd = false;
    private string $name = '';
    private bool $selfClosing = false;
    /** @var list<array{string, int, int, bool}> name, value start, value end, kept */
    private array $attrs = [];
    private string $tokenType = '';
    /** @var list<array{string, int, int, string, bool, array<string, string>}> */
    private array $out = [];

    private function __construct(private readonly string $input)
    {
        $len = strlen($input);
        for ($i = 0; $i < $len; $i++) {
            $ch = $input[$i];
            if ($ch === "\r") {
                $this->c[] = "\n";
                $this->o[] = $i;
                if ($i + 1 < $len && $input[$i + 1] === "\n") {
                    $i++;
                }
                continue;
            }
            $this->c[] = $ch;
            $this->o[] = $i;
        }
        $this->n = count($this->c);
    }

    /**
     * @return list<array{string, int, int, string, bool, array<string, string>}>
     */
    public static function tokenize(string $input): array
    {
        $t = new self($input);
        $t->run();
        return $t->out;
    }

    private function run(): void
    {
        $guard = 4 * ($this->n + 4);
        while ($guard-- > 0) {
            $ch = $this->p < $this->n ? $this->c[$this->p++] : null;
            if (!$this->step($ch)) {
                return;
            }
        }
        throw new \LogicException('reference tokenizer did not stop');
    }

    /** Reconsume the current character in $state. */
    private function re(?string $ch, string $state): bool
    {
        if ($ch !== null) {
            $this->p--;
        }
        $this->state = $state;
        return true;
    }

    private function go(string $state): bool
    {
        $this->state = $state;
        return true;
    }

    private static function ws(?string $ch): bool
    {
        return $ch === "\t" || $ch === "\n" || $ch === "\f" || $ch === ' ';
    }

    private static function alpha(?string $ch): bool
    {
        return $ch !== null && (($ch >= 'a' && $ch <= 'z') || ($ch >= 'A' && $ch <= 'Z'));
    }

    private static function upper(string $ch): bool
    {
        return $ch >= 'A' && $ch <= 'Z';
    }

    /** Input offset after the current (consumed) character. */
    private function after(): int
    {
        return $this->o[$this->p - 1] + 1;
    }

    /** Input offset of the next character, or the input length at EOF. */
    private function nextOffset(): int
    {
        return $this->p < $this->n ? $this->o[$this->p] : strlen($this->input);
    }

    private function newTag(bool $isEnd): void
    {
        $this->isEnd = $isEnd;
        $this->name = '';
        $this->selfClosing = false;
        $this->attrs = [];
    }

    private function emitTag(): bool
    {
        $attrs = [];
        if (!$this->isEnd) {
            foreach ($this->attrs as [$name, $from, $to, $kept]) {
                if ($kept && isset(Tokenizer::WATCHED[$name])) {
                    $attrs[$name] = substr($this->input, $from, $to - $from);
                }
            }
        }
        $type = $this->isEnd ? 'end' : 'start';
        $this->out[] = [$type, $this->o[$this->lt], $this->after(), $this->name, !$this->isEnd && $this->selfClosing, $attrs];
        $this->state = 'data';
        if (!$this->isEnd) {
            $text = Names::TEXT[$this->name] ?? null;
            if ($text !== null) {
                $this->state = match ($text) {
                    ContentState::Rcdata => 'rcdata',
                    ContentState::Rawtext => 'rawtext',
                    ContentState::ScriptData => 'script',
                    default => 'plaintext',
                };
                $this->lastStartTag = $this->name;
            }
        }
        return true;
    }

    /** Emit the current comment-like token; at EOF it ends at the input length. */
    private function emitOther(?string $ch): bool
    {
        $end = $ch === null ? strlen($this->input) : $this->after();
        $this->out[] = [$this->tokenType, $this->o[$this->lt], $end, '', false, []];
        $this->state = 'data';
        return $ch !== null;
    }

    private function startAttr(string $first): void
    {
        $this->attrs[] = [$first, 0, 0, true];
    }

    private function appendAttrName(string $ch): void
    {
        $k = count($this->attrs) - 1;
        $this->attrs[$k][0] .= self::upper($ch) ? strtolower($ch) : ($ch === "\0" ? "\u{FFFD}" : $ch);
    }

    /** Leaving the attribute name state: drop a duplicate name. */
    private function leaveAttrName(): void
    {
        $k = count($this->attrs) - 1;
        for ($j = 0; $j < $k; $j++) {
            if ($this->attrs[$j][0] === $this->attrs[$k][0]) {
                $this->attrs[$k][3] = false;
                return;
            }
        }
    }

    private function setValue(int $from, int $to): void
    {
        $k = count($this->attrs) - 1;
        [$name, , , $kept] = $this->attrs[$k];
        $this->attrs[$k] = [$name, $from, $to, $kept];
    }

    private function appropriate(): bool
    {
        return $this->name === $this->lastStartTag;
    }

    /** Matches the next characters (not consumed) with $word. */
    private function lookahead(string $word, bool $caseInsensitive): bool
    {
        $len = strlen($word);
        if ($this->p + $len > $this->n) {
            return false;
        }
        $text = implode('', array_slice($this->c, $this->p, $len));
        return $caseInsensitive ? strcasecmp($text, $word) === 0 : $text === $word;
    }

    /**
     * One step: $ch is the consumed character, or null for EOF. Return false
     * after the end-of-file token.
     */
    private function step(?string $ch): bool
    {
        switch ($this->state) {
            case 'data':
                if ($ch === '<') {
                    $this->lt = $this->p - 1;
                    return $this->go('tagOpen');
                }
                return $ch !== null;
            case 'rcdata':
            case 'rawtext':
            case 'script':
                if ($ch === '<') {
                    $this->lt = $this->p - 1;
                    return $this->go($this->state . 'Lt');
                }
                return $ch !== null;
            case 'plaintext':
                return $ch !== null;
            case 'tagOpen':
                if ($ch === '!') {
                    return $this->go('markupDecl');
                }
                if ($ch === '/') {
                    return $this->go('endTagOpen');
                }
                if (self::alpha($ch)) {
                    $this->newTag(false);
                    return $this->re($ch, 'tagName');
                }
                if ($ch === '?') {
                    $this->tmp = '';
                    return $this->go('piOpen');
                }
                return $ch === null ? false : $this->re($ch, 'data');
            case 'endTagOpen':
                if (self::alpha($ch)) {
                    $this->newTag(true);
                    return $this->re($ch, 'tagName');
                }
                if ($ch === '>') {
                    return $this->go('data');
                }
                if ($ch === null) {
                    return false;
                }
                $this->tokenType = 'comment';
                return $this->re($ch, 'bogusComment');
            case 'tagName':
                if (self::ws($ch)) {
                    return $this->go('beforeAttrName');
                }
                if ($ch === '/') {
                    return $this->go('selfClosingStartTag');
                }
                if ($ch === '>') {
                    return $this->emitTag();
                }
                if ($ch === null) {
                    return false;
                }
                $this->name .= self::upper($ch) ? strtolower($ch) : ($ch === "\0" ? "\u{FFFD}" : $ch);
                return true;

            // RCDATA, RAWTEXT, script data: less-than sign, end tag open, end tag name.
            case 'rcdataLt':
            case 'rawtextLt':
                $base = substr($this->state, 0, -2);
                if ($ch === '/') {
                    $this->tmp = '';
                    return $this->go($base . 'EndTagOpen');
                }
                return $this->re($ch, $base);
            case 'scriptLt':
                if ($ch === '/') {
                    $this->tmp = '';
                    return $this->go('scriptEndTagOpen');
                }
                if ($ch === '!') {
                    return $this->go('scriptEscapeStart');
                }
                return $this->re($ch, 'script');
            case 'rcdataEndTagOpen':
            case 'rawtextEndTagOpen':
            case 'scriptEndTagOpen':
            case 'scriptEscapedEndTagOpen':
                $base = substr($this->state, 0, -strlen('EndTagOpen'));
                if (self::alpha($ch)) {
                    $this->newTag(true);
                    return $this->re($ch, $base . 'EndTagName');
                }
                return $this->re($ch, $base);
            case 'rcdataEndTagName':
            case 'rawtextEndTagName':
            case 'scriptEndTagName':
            case 'scriptEscapedEndTagName':
                $base = substr($this->state, 0, -strlen('EndTagName'));
                if (self::ws($ch) && $this->appropriate()) {
                    return $this->go('beforeAttrName');
                }
                if ($ch === '/' && $this->appropriate()) {
                    return $this->go('selfClosingStartTag');
                }
                if ($ch === '>' && $this->appropriate()) {
                    return $this->emitTag();
                }
                if (self::alpha($ch)) {
                    $this->name .= strtolower((string) $ch);
                    $this->tmp .= $ch;
                    return true;
                }
                return $this->re($ch, $base);

            // Script data escape states.
            case 'scriptEscapeStart':
                return $ch === '-' ? $this->go('scriptEscapeStartDash') : $this->re($ch, 'script');
            case 'scriptEscapeStartDash':
                return $ch === '-' ? $this->go('scriptEscapedDashDash') : $this->re($ch, 'script');
            case 'scriptEscaped':
            case 'scriptEscapedDash':
            case 'scriptEscapedDashDash':
                if ($ch === '-') {
                    return $this->go($this->state === 'scriptEscaped' ? 'scriptEscapedDash' : 'scriptEscapedDashDash');
                }
                if ($ch === '<') {
                    $this->lt = $this->p - 1;
                    return $this->go('scriptEscapedLt');
                }
                if ($ch === '>' && $this->state === 'scriptEscapedDashDash') {
                    return $this->go('script');
                }
                if ($ch === null) {
                    return false;
                }
                return $this->go('scriptEscaped');
            case 'scriptEscapedLt':
                if ($ch === '/') {
                    $this->tmp = '';
                    return $this->go('scriptEscapedEndTagOpen');
                }
                if (self::alpha($ch)) {
                    $this->tmp = '';
                    return $this->re($ch, 'scriptDoubleEscapeStart');
                }
                return $this->re($ch, 'scriptEscaped');
            case 'scriptDoubleEscapeStart':
            case 'scriptDoubleEscapeEnd':
                $start = $this->state === 'scriptDoubleEscapeStart';
                if (self::ws($ch) || $ch === '/' || $ch === '>') {
                    $isScript = $this->tmp === 'script';
                    return $this->go($start === $isScript ? 'scriptDoubleEscaped' : 'scriptEscaped');
                }
                if (self::alpha($ch)) {
                    $this->tmp .= strtolower((string) $ch);
                    return true;
                }
                return $this->re($ch, $start ? 'scriptEscaped' : 'scriptDoubleEscaped');
            case 'scriptDoubleEscaped':
            case 'scriptDoubleEscapedDash':
            case 'scriptDoubleEscapedDashDash':
                if ($ch === '-') {
                    return $this->go($this->state === 'scriptDoubleEscaped' ? 'scriptDoubleEscapedDash' : 'scriptDoubleEscapedDashDash');
                }
                if ($ch === '<') {
                    return $this->go('scriptDoubleEscapedLt');
                }
                if ($ch === '>' && $this->state === 'scriptDoubleEscapedDashDash') {
                    return $this->go('script');
                }
                if ($ch === null) {
                    return false;
                }
                return $this->go('scriptDoubleEscaped');
            case 'scriptDoubleEscapedLt':
                if ($ch === '/') {
                    $this->tmp = '';
                    return $this->go('scriptDoubleEscapeEnd');
                }
                return $this->re($ch, 'scriptDoubleEscaped');

            // Attributes.
            case 'beforeAttrName':
                if (self::ws($ch)) {
                    return true;
                }
                if ($ch === '/' || $ch === '>' || $ch === null) {
                    return $this->re($ch, 'afterAttrName');
                }
                if ($ch === '=') {
                    $this->startAttr('=');
                    return $this->go('attrName');
                }
                $this->startAttr('');
                return $this->re($ch, 'attrName');
            case 'attrName':
                if (self::ws($ch) || $ch === '/' || $ch === '>' || $ch === null) {
                    $this->leaveAttrName();
                    return $this->re($ch, 'afterAttrName');
                }
                if ($ch === '=') {
                    $this->leaveAttrName();
                    return $this->go('beforeAttrValue');
                }
                $this->appendAttrName($ch);
                return true;
            case 'afterAttrName':
                if (self::ws($ch)) {
                    return true;
                }
                if ($ch === '/') {
                    return $this->go('selfClosingStartTag');
                }
                if ($ch === '=') {
                    return $this->go('beforeAttrValue');
                }
                if ($ch === '>') {
                    return $this->emitTag();
                }
                if ($ch === null) {
                    return false;
                }
                $this->startAttr('');
                return $this->re($ch, 'attrName');
            case 'beforeAttrValue':
                if (self::ws($ch)) {
                    return true;
                }
                if ($ch === '"' || $ch === "'") {
                    $from = $this->nextOffset();
                    $this->setValue($from, $from);
                    return $this->go($ch === '"' ? 'attrValueDq' : 'attrValueSq');
                }
                if ($ch === '>') {
                    return $this->emitTag();
                }
                $this->re($ch, 'attrValueUq');
                $from = $this->nextOffset();
                $this->setValue($from, $from);
                return true;
            case 'attrValueDq':
            case 'attrValueSq':
                if ($ch === null) {
                    return false;
                }
                if ($ch === ($this->state === 'attrValueDq' ? '"' : "'")) {
                    $k = count($this->attrs) - 1;
                    $this->setValue($this->attrs[$k][1], $this->o[$this->p - 1]);
                    return $this->go('afterAttrValueQuoted');
                }
                return true;
            case 'attrValueUq':
                if ($ch === null) {
                    return false;
                }
                if (self::ws($ch) || $ch === '>') {
                    $k = count($this->attrs) - 1;
                    $this->setValue($this->attrs[$k][1], $this->o[$this->p - 1]);
                    return $ch === '>' ? $this->emitTag() : $this->go('beforeAttrName');
                }
                return true;
            case 'afterAttrValueQuoted':
                if (self::ws($ch)) {
                    return $this->go('beforeAttrName');
                }
                if ($ch === '/') {
                    return $this->go('selfClosingStartTag');
                }
                if ($ch === '>') {
                    return $this->emitTag();
                }
                return $ch === null ? false : $this->re($ch, 'beforeAttrName');
            case 'selfClosingStartTag':
                if ($ch === '>') {
                    $this->selfClosing = true;
                    return $this->emitTag();
                }
                return $ch === null ? false : $this->re($ch, 'beforeAttrName');

            // Markup declarations, comments, bogus comments.
            case 'markupDecl':
                $this->re($ch, 'markupDecl');
                if ($this->lookahead('--', false)) {
                    $this->p += 2;
                    $this->tokenType = 'comment';
                    return $this->go('commentStart');
                }
                if ($this->lookahead('DOCTYPE', true)) {
                    $this->p += 7;
                    $this->tokenType = 'doctype';
                    return $this->go('doctype');
                }
                if ($this->lookahead('[CDATA[', false)) {
                    $this->p += 7;
                    $this->tokenType = 'cdata';
                    return $this->go('bogusComment');
                }
                $this->tokenType = 'comment';
                return $this->go('bogusComment');
            case 'bogusComment':
                if ($ch === '>' || $ch === null) {
                    return $this->emitOther($ch);
                }
                return true;
            case 'commentStart':
                if ($ch === '-') {
                    return $this->go('commentStartDash');
                }
                if ($ch === '>') {
                    return $this->emitOther($ch);
                }
                return $this->re($ch, 'comment');
            case 'commentStartDash':
                if ($ch === '-') {
                    return $this->go('commentEnd');
                }
                if ($ch === '>' || $ch === null) {
                    return $this->emitOther($ch);
                }
                return $this->re($ch, 'comment');
            case 'comment':
                if ($ch === '<') {
                    return $this->go('commentLt');
                }
                if ($ch === '-') {
                    return $this->go('commentEndDash');
                }
                return $ch === null ? $this->emitOther($ch) : true;
            case 'commentLt':
                if ($ch === '!') {
                    return $this->go('commentLtBang');
                }
                if ($ch === '<') {
                    return true;
                }
                return $this->re($ch, 'comment');
            case 'commentLtBang':
                return $ch === '-' ? $this->go('commentLtBangDash') : $this->re($ch, 'comment');
            case 'commentLtBangDash':
                return $ch === '-' ? $this->go('commentLtBangDashDash') : $this->re($ch, 'commentEndDash');
            case 'commentLtBangDashDash':
                return $this->re($ch, 'commentEnd');
            case 'commentEndDash':
                if ($ch === '-') {
                    return $this->go('commentEnd');
                }
                return $ch === null ? $this->emitOther($ch) : $this->re($ch, 'comment');
            case 'commentEnd':
                if ($ch === '>' || $ch === null) {
                    return $this->emitOther($ch);
                }
                if ($ch === '!') {
                    return $this->go('commentEndBang');
                }
                if ($ch === '-') {
                    return true;
                }
                return $this->re($ch, 'comment');
            case 'commentEndBang':
                if ($ch === '-') {
                    return $this->go('commentEndDash');
                }
                if ($ch === '>' || $ch === null) {
                    return $this->emitOther($ch);
                }
                return $this->re($ch, 'comment');

            // Processing instructions (current WHATWG text).
            case 'piOpen':
                if (self::alpha($ch) || $ch === '_') {
                    return $this->re($ch, 'piTarget');
                }
                if ($ch === null) {
                    return false;
                }
                $this->tokenType = 'comment';
                return $this->re($ch, 'bogusComment');
            case 'piTarget':
                if (self::ws($ch) || $ch === '?' || $ch === '>') {
                    $this->tokenType = 'comment';
                    $low = strtolower($this->tmp);
                    return $this->re($ch, $low === 'xml' || $low === 'xml-stylesheet' ? 'bogusComment' : 'afterPiTarget');
                }
                if ($ch !== null && (self::alpha($ch) || ($ch >= '0' && $ch <= '9') || $ch === '-' || $ch === '_')) {
                    $this->tmp .= $ch;
                    return true;
                }
                if ($ch === null) {
                    return false;
                }
                $this->tokenType = 'comment';
                return $this->re($ch, 'bogusComment');
            case 'afterPiTarget':
                return self::ws($ch) ? true : $this->re($ch, 'piData');
            case 'piData':
                if ($ch === '?') {
                    return $this->go('piQuestionable');
                }
                if ($ch === '>') {
                    return $this->emitOther($ch);
                }
                return $ch !== null;
            case 'piQuestionable':
                if ($ch === '>') {
                    return $this->emitOther($ch);
                }
                return $ch === null ? false : $this->re($ch, 'piData');

            // DOCTYPE states.
            case 'doctype':
                if (self::ws($ch)) {
                    return $this->go('beforeDoctypeName');
                }
                return $ch === null ? $this->emitOther($ch) : $this->re($ch, 'beforeDoctypeName');
            case 'beforeDoctypeName':
                if (self::ws($ch)) {
                    return true;
                }
                if ($ch === '>' || $ch === null) {
                    return $this->emitOther($ch);
                }
                return $this->go('doctypeName');
            case 'doctypeName':
                if (self::ws($ch)) {
                    return $this->go('afterDoctypeName');
                }
                return $ch === '>' || $ch === null ? $this->emitOther($ch) : true;
            case 'afterDoctypeName':
                if (self::ws($ch)) {
                    return true;
                }
                if ($ch === '>' || $ch === null) {
                    return $this->emitOther($ch);
                }
                $this->re($ch, 'afterDoctypeName');
                if ($this->lookahead('PUBLIC', true)) {
                    $this->p += 6;
                    return $this->go('afterDoctypeKeyword');
                }
                if ($this->lookahead('SYSTEM', true)) {
                    $this->p += 6;
                    return $this->go('afterDoctypeKeyword');
                }
                return $this->go('bogusDoctype');
            case 'afterDoctypeKeyword':
                // After the PUBLIC or SYSTEM keyword: the two states have the same transitions.
                if (self::ws($ch)) {
                    return $this->go('beforeDoctypeId');
                }
                if ($ch === '"' || $ch === "'") {
                    return $this->go($ch === '"' ? 'doctypeIdDq' : 'doctypeIdSq');
                }
                if ($ch === '>' || $ch === null) {
                    return $this->emitOther($ch);
                }
                return $this->re($ch, 'bogusDoctype');
            case 'beforeDoctypeId':
            case 'betweenDoctypeIds':
                if (self::ws($ch)) {
                    return true;
                }
                if ($ch === '"' || $ch === "'") {
                    return $this->go($ch === '"' ? 'doctypeIdDq' : 'doctypeIdSq');
                }
                if ($ch === '>' || $ch === null) {
                    return $this->emitOther($ch);
                }
                return $this->re($ch, 'bogusDoctype');
            case 'doctypeIdDq':
            case 'doctypeIdSq':
                if ($ch === ($this->state === 'doctypeIdDq' ? '"' : "'")) {
                    return $this->go('afterDoctypeId');
                }
                return $ch === '>' || $ch === null ? $this->emitOther($ch) : true;
            case 'afterDoctypeId':
                if (self::ws($ch)) {
                    return $this->go('betweenDoctypeIds');
                }
                if ($ch === '"' || $ch === "'") {
                    return $this->go($ch === '"' ? 'doctypeIdDq' : 'doctypeIdSq');
                }
                if ($ch === '>' || $ch === null) {
                    return $this->emitOther($ch);
                }
                return $this->re($ch, 'bogusDoctype');
            case 'bogusDoctype':
                return $ch === '>' || $ch === null ? $this->emitOther($ch) : true;
        }
        throw new \LogicException('unknown state ' . $this->state);
    }
}
