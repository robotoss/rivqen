<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup;

/**
 * RQP markup validity and block extraction (markup.md section 2, rqp/1).
 *
 * One pass over the tokens of Tokenizer with the token stack of M-05.
 * Work per token is O(1), except end tags, whose cost is paid by the
 * elements they pop (each element is pushed once and popped once). The
 * `noscript` check (M-11) tokenizes each `noscript` content once more.
 * Total time is linear in the input size; memory is linear in the input
 * size (stack depth <= number of start tags).
 */
final class Parser
{
    public const MAX_INPUT_BYTES = 5_242_880;
    public const MAX_BLOCKS = 256;
    public const MAX_BLOCK_BYTES = 1_048_576;
    public const JSON_TYPE = 'application/json';

    private const BLOCK_ATTR = 'data-rq-block';
    private const ID_CHARS = 'abcdefghijklmnopqrstuvwxyz0123456789_-';
    private const ID_FIRST_CHARS = 'abcdefghijklmnopqrstuvwxyz0123456789';
    private const ID_MAX = 64;

    private const FOREIGN = 1; // pushed in a foreign region
    private const CONTENT = 2; // pushed in the content of a block

    private Tokenizer $tokenizer;

    /** @var list<string> Names of the token stack, bottom first. */
    private array $names = [];
    /** @var list<int> Flags (FOREIGN, CONTENT) of the token stack. */
    private array $flags = [];
    /** @var array<string, int> Number of elements on the stack, by name. */
    private array $count = [];
    /** @var array<string, int> Number of elements opened in the content, by name. */
    private array $contentCount = [];
    /** @var list<int> Stack indexes of HTML elements in TABLE_FAMILY or `template` (for F, M-09). */
    private array $tableIndex = [];
    /** Stack index of the `svg`/`math` element of the foreign region, or -1. */
    private int $region = -1;
    /** Number of `select` elements pushed in HTML context. */
    private int $selects = 0;
    /** Stack index of the foreign INTEGRATION/TEXT element whose content must be characters only (M-07), or -1. */
    private int $charsOnly = -1;
    /** Offset after the start tag of a `noscript` in HTML context (M-11), or -1. */
    private int $noscriptStart = -1;

    /** Stack index of the open block element, or -1. */
    private int $blockIndex = -1;
    private string $blockId = '';
    private string $blockFormat = '';
    private int $blockStart = 0;
    /** @var list<Block> */
    private array $blocks = [];
    /** @var array<string, true> */
    private array $ids = [];

    private function __construct(private readonly string $input) {}

    /**
     * Check the document and extract its blocks. Never throws for any input
     * string: rule failures become an invalid Result.
     */
    public static function parse(string $input): Result
    {
        try {
            $blocks = (new self($input))->run();
        } catch (MarkupError $e) {
            return Result::invalid($e->errorCode, $e->rule);
        }
        return Result::valid($input, $blocks);
    }

    /**
     * @return list<Block>
     */
    private function run(): array
    {
        $input = $this->input;
        // M-01 first, before any other rule.
        if (strlen($input) > self::MAX_INPUT_BYTES) {
            throw new MarkupError(ErrorCode::Limit, 'M-01');
        }
        // M-02: RFC 3629 UTF-8. PCRE checks the whole subject in UTF mode and
        // rejects overlong forms, surrogates, values above U+10FFFF and
        // truncated sequences. The empty pattern cannot backtrack.
        if (preg_match('//u', $input) !== 1) {
            throw new MarkupError(ErrorCode::Encoding, 'M-02');
        }
        $start = str_starts_with($input, "\xEF\xBB\xBF") ? 3 : 0;
        $this->tokenizer = new Tokenizer($input, $start);

        while (true) {
            $token = $this->tokenizer->next();
            switch ($token->type) {
                case TokenType::StartTag:
                    $this->startTag($token);
                    break;
                case TokenType::EndTag:
                    $this->endTag($token);
                    break;
                case TokenType::Comment:
                case TokenType::Cdata:
                    $this->comment($token);
                    break;
                case TokenType::Doctype:
                    $this->doctype();
                    break;
                case TokenType::Eof:
                    $this->eof();
                    return $this->blocks;
            }
        }
    }

    // ----------------------------------------------------------------------
    // Tokens
    // ----------------------------------------------------------------------

    private function comment(Token $token): void
    {
        if ($token->type === TokenType::Cdata && !$token->cdataClosed) {
            throw new MarkupError(ErrorCode::Structure, 'M-12');
        }
        if ($this->charsOnly >= 0) {
            throw new MarkupError(ErrorCode::Structure, 'M-07');
        }
        // Comments are allowed in `select` (M-08) and in content (M-22).
    }

    private function doctype(): void
    {
        if ($this->charsOnly >= 0) {
            throw new MarkupError(ErrorCode::Structure, 'M-07');
        }
        if ($this->selects > 0 && $this->region < 0) {
            throw new MarkupError(ErrorCode::Structure, 'M-08');
        }
        if ($this->blockIndex >= 0) {
            throw new MarkupError(ErrorCode::Structure, 'M-22');
        }
    }

    private function startTag(Token $token): void
    {
        if ($this->charsOnly >= 0) {
            throw new MarkupError(ErrorCode::Structure, 'M-07');
        }
        $name = $token->name;
        $foreign = $this->region >= 0;

        if ($name === 'frameset') {
            throw new MarkupError(ErrorCode::Structure, 'M-10');
        }
        if ($name === 'script') {
            $type = $token->attribute('type');
            if ($type !== null && (stripos($type, 'rivqen-manifest') !== false || str_contains($type, '&'))) {
                throw new MarkupError(ErrorCode::Reserved, 'M-13');
            }
        }
        if (!$foreign) {
            if ($this->selects > 0 && !isset(Names::SELECT_START[$name])) {
                throw new MarkupError(ErrorCode::Structure, 'M-08');
            }
            $allowed = Names::TABLE_START_ALLOWED_F[$name] ?? null;
            if ($allowed !== null) {
                $f = $this->tableContext();
                if ($f !== null && !isset($allowed[$f])) {
                    throw new MarkupError(ErrorCode::Structure, 'M-09');
                }
            }
        }

        if ($token->attribute(self::BLOCK_ATTR) !== null) {
            $this->blockStartTag($token, $foreign);
            return;
        }
        if ($this->blockIndex >= 0) {
            $this->contentStartTag($token);
        }

        if ($foreign) {
            $this->foreignStartTag($token);
            return;
        }
        if (($name === 'svg' || $name === 'math') && $token->selfClosing) {
            return;
        }
        if (isset(Names::VOID[$name])) {
            return;
        }
        $index = $this->push($name, false);
        if ($name === 'svg' || $name === 'math') {
            $this->region = $index;
        }
        $this->switchState($name, $token->end);
    }

    private function foreignStartTag(Token $token): void
    {
        $name = $token->name;
        if (isset(Names::BREAKOUT[$name])) {
            throw new MarkupError(ErrorCode::Structure, 'M-07');
        }
        $text = isset(Names::TEXT[$name]);
        if ($text && ($token->selfClosing || $name === 'plaintext')) {
            throw new MarkupError(ErrorCode::Structure, 'M-07');
        }
        if ($token->selfClosing) {
            return;
        }
        $index = $this->push($name, true);
        if ($text || isset(Names::INTEGRATION[$name])) {
            $this->charsOnly = $index;
        }
    }

    /** M-04: state change after a start tag in HTML context. */
    private function switchState(string $name, int $contentStart): void
    {
        $state = Names::TEXT[$name] ?? null;
        if ($state === null) {
            return;
        }
        $this->tokenizer->switchTo($state, $name);
        if ($name === 'noscript') {
            $this->noscriptStart = $contentStart;
        }
    }

    private function blockStartTag(Token $token, bool $foreign): void
    {
        // Recommended order (markup.md 2.10): M-17, M-15, M-16, M-18, M-19, M-21, M-26.
        if ($this->blockIndex >= 0) {
            throw new MarkupError(ErrorCode::Nested, 'M-17');
        }
        $id = (string) $token->attribute(self::BLOCK_ATTR);
        if (!self::isValidId($id)) {
            throw new MarkupError(ErrorCode::InvalidId, 'M-15');
        }
        $name = $token->name;
        if ($foreign) {
            throw new MarkupError(ErrorCode::ForbiddenElement, 'M-16');
        }
        if (isset(Names::BLOCK_HTML[$name])) {
            $format = 'html';
        } elseif ($name === 'script' && $token->attribute('type') === self::JSON_TYPE) {
            $format = 'json';
        } else {
            throw new MarkupError(ErrorCode::ForbiddenElement, 'M-16');
        }
        if ($token->selfClosing) {
            throw new MarkupError(ErrorCode::Structure, 'M-18');
        }
        if (isset($this->count['template']) || isset($this->count['select'])) {
            throw new MarkupError(ErrorCode::Structure, 'M-19');
        }
        $f = $this->tableContext();
        if ($f !== null && !isset(Names::BLOCK_ALLOWED_F[$f])) {
            throw new MarkupError(ErrorCode::Structure, 'M-19');
        }
        if (isset($this->ids[$id])) {
            throw new MarkupError(ErrorCode::Duplicate, 'M-21');
        }
        if (count($this->blocks) >= self::MAX_BLOCKS) {
            throw new MarkupError(ErrorCode::Limit, 'M-26');
        }
        $this->ids[$id] = true;
        $this->blockId = $id;
        $this->blockFormat = $format;
        $this->blockStart = $token->end;
        $this->blockIndex = $this->push($name, false);
        $this->switchState($name, $token->end);
    }

    /** M-22 and M-24 for a start tag in the content of an html block. */
    private function contentStartTag(Token $token): void
    {
        $name = $token->name;
        if (isset(Names::FORBIDDEN_CONTENT[$name])) {
            throw new MarkupError(ErrorCode::Structure, 'M-22');
        }
        $void = isset(Names::VOID[$name]);
        if ($token->selfClosing && !$void) {
            throw new MarkupError(ErrorCode::Structure, 'M-22');
        }
        $blockName = $this->names[$this->blockIndex];
        $invalid = match (true) {
            isset(Names::PCLOSE[$name]) && ($blockName === 'p' || $blockName === 'span') => true,
            isset(Names::HEADING[$name]) => isset(Names::HEADING[$blockName]) || $this->contentHeadings() > 0,
            $name === 'li', $name === 'dd', $name === 'dt' => $this->openedInContent('ul') + $this->openedInContent('ol')
                + $this->openedInContent('menu') + $this->openedInContent('dl') === 0,
            $name === 'button', $name === 'a', $name === 'nobr' => $this->open($name) > 0,
            isset(Names::TABLE_PART[$name]), $name === 'col' => $this->openedInContent('table') === 0,
            $name === 'rb', $name === 'rtc' => $this->openedInContent('ruby') === 0
                || $this->open('rb') + $this->open('rp') + $this->open('rt') + $this->open('rtc') > 0,
            $name === 'rp', $name === 'rt' => $this->openedInContent('ruby') === 0
                || $this->open('rb') + $this->open('rp') + $this->open('rt') > 0,
            default => false,
        };
        if ($invalid) {
            throw new MarkupError(ErrorCode::Structure, 'M-24');
        }
    }

    private function endTag(Token $token): void
    {
        $name = $token->name;
        if ($this->charsOnly >= 0) {
            // M-07 items 3 and 4: only the end tag of that element may follow.
            if ($name !== $this->names[$this->charsOnly]) {
                throw new MarkupError(ErrorCode::Structure, 'M-07');
            }
            $this->charsOnly = -1;
            $this->popTo(count($this->names) - 1);
            return;
        }
        if ($this->noscriptStart >= 0) {
            // The end tag that ends the RAWTEXT of `noscript` (M-11).
            $this->checkNoscript($token->start);
        }
        if ($this->blockIndex >= 0) {
            $this->contentEndTag($token);
            return;
        }
        $foreign = $this->region >= 0;
        if ($this->selects > 0 && !$foreign && !isset(Names::SELECT_END[$name])) {
            throw new MarkupError(ErrorCode::Structure, 'M-08');
        }
        // M-05 step 3.
        if (!isset($this->count[$name])) {
            if ($foreign) {
                throw new MarkupError(ErrorCode::Structure, 'M-07');
            }
            return; // no element with this name: ignore the end tag
        }
        $match = count($this->names) - 1;
        while ($this->names[$match] !== $name) {
            $match--;
        }
        if ($foreign && $match < $this->region) {
            throw new MarkupError(ErrorCode::Structure, 'M-07');
        }
        // M-06: crossed elements, from the top down to the match.
        $tablePartsOk = isset(Names::TABLE_FAMILY[$name]);
        for ($i = count($this->names) - 1; $i > $match; $i--) {
            $crossed = $this->names[$i];
            if (($this->flags[$i] & self::FOREIGN) === 0 && isset(Names::GUARDED[$crossed])
                && !($tablePartsOk && isset(Names::TABLE_PART[$crossed]))) {
                throw new MarkupError(ErrorCode::Structure, 'M-06');
            }
        }
        $this->popTo($match);
    }

    /** M-22 item 4, M-23 and the block end (M-20, M-26, M-25). */
    private function contentEndTag(Token $token): void
    {
        $name = $token->name;
        if (isset(Names::VOID[$name])) {
            throw new MarkupError(ErrorCode::Structure, 'M-22');
        }
        $top = count($this->names) - 1;
        if ($this->names[$top] !== $name) {
            throw new MarkupError(ErrorCode::Structure, 'M-23');
        }
        if ($top === $this->blockIndex) {
            $this->endBlock($token->start);
        }
        $this->popTo($top);
    }

    private function endBlock(int $end): void
    {
        $start = $this->blockStart;
        if ($end - $start > self::MAX_BLOCK_BYTES) {
            throw new MarkupError(ErrorCode::Limit, 'M-26');
        }
        $content = substr($this->input, $start, $end - $start);
        if ($this->blockFormat === 'json' && !JsonValidator::isValid($content)) {
            throw new MarkupError(ErrorCode::BadJson, 'M-25');
        }
        $this->blocks[] = new Block($this->blockId, $this->blockFormat, $start, $end, Revision::b64uSha256($content));
        $this->blockIndex = -1;
    }

    private function eof(): void
    {
        if ($this->noscriptStart >= 0) {
            $this->checkNoscript(strlen($this->input));
        }
        if ($this->blockIndex >= 0) {
            throw new MarkupError(ErrorCode::Structure, 'M-20');
        }
        if ($this->selects > 0) {
            throw new MarkupError(ErrorCode::Structure, 'M-08');
        }
    }

    // ----------------------------------------------------------------------
    // M-11
    // ----------------------------------------------------------------------

    /**
     * Tokenize the `noscript` content C followed by `</noscript>` alone, from
     * the data state, in HTML context (M-11).
     */
    private function checkNoscript(int $contentEnd): void
    {
        $start = $this->noscriptStart;
        $this->noscriptStart = -1;
        $length = $contentEnd - $start;
        $sub = new Tokenizer(substr($this->input, $start, $length) . '</noscript>');
        while (true) {
            $token = $sub->next();
            if ($token->type === TokenType::Eof) {
                throw new MarkupError(ErrorCode::Structure, 'M-11');
            }
            if ($token->type === TokenType::EndTag && $token->name === 'noscript') {
                if ($token->start !== $length) {
                    throw new MarkupError(ErrorCode::Structure, 'M-11');
                }
                return;
            }
            if ($token->type !== TokenType::StartTag) {
                continue;
            }
            if ($token->attribute(self::BLOCK_ATTR) !== null || isset(Names::NOSCRIPT_FORBIDDEN[$token->name])) {
                throw new MarkupError(ErrorCode::Structure, 'M-11');
            }
            $state = Names::TEXT[$token->name] ?? null;
            if ($state !== null) {
                $sub->switchTo($state, $token->name);
            }
        }
    }

    // ----------------------------------------------------------------------
    // Token stack
    // ----------------------------------------------------------------------

    private function push(string $name, bool $foreign): int
    {
        $index = count($this->names);
        $flags = $foreign ? self::FOREIGN : 0;
        if ($this->blockIndex >= 0) {
            $flags |= self::CONTENT;
            $this->contentCount[$name] = ($this->contentCount[$name] ?? 0) + 1;
        }
        $this->names[] = $name;
        $this->flags[] = $flags;
        $this->count[$name] = ($this->count[$name] ?? 0) + 1;
        if (!$foreign) {
            if (isset(Names::TABLE_FAMILY[$name]) || $name === 'template') {
                $this->tableIndex[] = $index;
            }
            if ($name === 'select') {
                $this->selects++;
            }
        }
        return $index;
    }

    /** Pop all elements above $index, then the element at $index. */
    private function popTo(int $index): void
    {
        for ($i = count($this->names) - 1; $i >= $index; $i--) {
            $name = array_pop($this->names);
            $flags = array_pop($this->flags);
            if ($name === null || $flags === null) {
                throw new \LogicException('token stack underflow');
            }
            if (--$this->count[$name] === 0) {
                unset($this->count[$name]);
            }
            if (($flags & self::CONTENT) !== 0 && --$this->contentCount[$name] === 0) {
                unset($this->contentCount[$name]);
            }
            if (($flags & self::FOREIGN) === 0) {
                if ($this->tableIndex !== [] && $this->tableIndex[count($this->tableIndex) - 1] === $i) {
                    array_pop($this->tableIndex);
                }
                if ($name === 'select') {
                    $this->selects--;
                }
            }
            if ($i === $this->region) {
                $this->region = -1;
            }
        }
    }

    /** F of M-09 and M-19: the nearest TABLE_FAMILY element; the search stops at `template`. */
    private function tableContext(): ?string
    {
        if ($this->tableIndex === []) {
            return null;
        }
        $name = $this->names[$this->tableIndex[count($this->tableIndex) - 1]];
        return $name === 'template' ? null : $name;
    }

    private function open(string $name): int
    {
        return $this->count[$name] ?? 0;
    }

    private function openedInContent(string $name): int
    {
        return $this->contentCount[$name] ?? 0;
    }

    private function contentHeadings(): int
    {
        return $this->openedInContent('h1') + $this->openedInContent('h2') + $this->openedInContent('h3')
            + $this->openedInContent('h4') + $this->openedInContent('h5') + $this->openedInContent('h6');
    }

    /** M-15: `^[a-z0-9][a-z0-9_-]{0,63}$` on the raw value, without a regex. */
    public static function isValidId(string $id): bool
    {
        $n = strlen($id);
        return $n >= 1 && $n <= self::ID_MAX
            && strspn($id, self::ID_FIRST_CHARS, 0, 1) === 1
            && strspn($id, self::ID_CHARS) === $n;
    }
}
