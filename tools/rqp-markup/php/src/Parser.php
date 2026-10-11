<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup;

/**
 * RQP markup validity and block extraction (markup.md section 2, rqp/1).
 *
 * One pass over the tokens of Tokenizer with the token stack of M-05 and
 * the depth counter D of M-26. Work per token is O(1), except end tags,
 * whose cost is paid by the elements they pop (each element is pushed once
 * and popped once). The `noscript` check (M-11) tokenizes each `noscript`
 * content once more. Total time is linear in the input size.
 *
 * Every element on the token stack holds at least 1 unit of D: its push
 * adds 1, and only the exact pop of that element (M-26 item 4) gives units
 * back. So the stack never has more than D <= MAX_DEPTH elements, and the
 * stack memory and the work of one end tag are bounded.
 *
 * Hash flooding: tag names come from the input, and PHP hashes string array
 * keys with DJBX33A, which has no seed. An attacker can make many names with
 * the same hash, and a PHP array with such keys has one long collision chain.
 * So no array of this class has keys that are raw input names:
 * - Fixed name sets (M-06, M-19, M-24) use constant arrays and counters by
 *   slot (TRACKED). A lookup of an input name in a constant array probes one
 *   chain of constant keys.
 * - The name index of M-05 step 3 ($byName) uses nameKey(): the name and a
 *   keyed hash of it with a random key for each parse.
 * - The stack of the `noscript` content (M-11 item 5) is a list: an end tag
 *   compares only with its top.
 * - Block ids ($ids) are at most MAX_BLOCKS (M-26), in [a-z0-9_-] (M-15).
 */
final class Parser
{
    public const MAX_INPUT_BYTES = 5_242_880;
    public const MAX_BLOCKS = 256;
    public const MAX_BLOCK_BYTES = 1_048_576;
    /** M-26: maximum value of the depth counter D (H-25, H-31; `maxOpenElements`). */
    public const MAX_DEPTH = 256;
    public const JSON_TYPE = 'application/json';

    private const BLOCK_ATTR = 'data-rq-block';
    private const ID_CHARS = 'abcdefghijklmnopqrstuvwxyz0123456789_-';
    private const ID_FIRST_CHARS = 'abcdefghijklmnopqrstuvwxyz0123456789';
    private const ID_MAX = 64;
    /** The 128 ASCII bytes (M-13 item 3). */
    private const ASCII_BYTES = "\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0A\x0B\x0C\x0D\x0E\x0F"
        . "\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1A\x1B\x1C\x1D\x1E\x1F"
        . " !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~\x7F";

    // Flags of a token-stack element.
    private const FOREIGN = 1; // pushed in a foreign region
    private const CONTENT = 2; // pushed in the content of a block
    private const DIRTY = 4; // M-26 item 5: its exact pop gives no unit back
    private const MARKED = 8; // M-06 item 4: special-crossed mark
    // M-26 item 3: implied parts of an F element. Each one holds 1 unit of D.
    private const ROW_GROUP = 16;
    private const ROW = 32;
    private const COLUMN_GROUP = 64;
    /**
     * M-26 item 3: implied parts by the name of F and the start tag name.
     * The search for F stops at `template`, so `template` has no entry.
     */
    private const IMPLIED_PARTS = [
        'table' => ['tr' => self::ROW_GROUP, 'td' => self::ROW_GROUP | self::ROW, 'th' => self::ROW_GROUP | self::ROW, 'col' => self::COLUMN_GROUP],
        'tbody' => ['td' => self::ROW, 'th' => self::ROW],
        'thead' => ['td' => self::ROW, 'th' => self::ROW],
        'tfoot' => ['td' => self::ROW, 'th' => self::ROW],
    ];

    // Slots of the counters for the names that M-19 and M-24 query. Names
    // that are only queried together share a slot.
    private const TEMPLATE = 0;
    private const SELECT = 1;
    private const BUTTON = 2;
    private const A = 3;
    private const NOBR = 4;
    private const RB_RP_RT = 5;
    private const RTC = 6;
    private const HEADING = 7; // h1 to h6
    private const LIST = 8; // ul, ol, menu, dl
    private const TABLE = 9;
    private const RUBY = 10;
    private const TRACKED = [
        'template' => self::TEMPLATE, 'select' => self::SELECT, 'button' => self::BUTTON,
        'a' => self::A, 'nobr' => self::NOBR, 'rb' => self::RB_RP_RT, 'rp' => self::RB_RP_RT,
        'rt' => self::RB_RP_RT, 'rtc' => self::RTC, 'h1' => self::HEADING, 'h2' => self::HEADING,
        'h3' => self::HEADING, 'h4' => self::HEADING, 'h5' => self::HEADING, 'h6' => self::HEADING,
        'ul' => self::LIST, 'ol' => self::LIST, 'menu' => self::LIST, 'dl' => self::LIST,
        'table' => self::TABLE, 'ruby' => self::RUBY,
    ];
    private const SLOTS = 11;

    /** Length of the random xxh3 secret of nameKey() (xxh3 accepts 136 to 256 bytes). */
    private const NAME_KEY_SECRET_BYTES = 192;

    private Tokenizer $tokenizer;

    /** @var list<string> Names of the token stack, bottom first. */
    private array $names = [];
    /** @var array<int, int> Flags (FOREIGN, CONTENT, DIRTY, MARKED, implied parts) of the token stack, by stack index. */
    private array $flags = [];
    /** M-26: the depth counter D. */
    private int $depth = 0;
    /** @var array<string, int> Number of elements on the stack, by nameKey() of the name. */
    private array $byName = [];
    /** @var array<int, int> Number of elements on the stack, by slot of TRACKED. */
    private array $open;
    /** @var array<int, int> Number of elements opened in the content, by slot of TRACKED. */
    private array $openInContent;
    private readonly string $nameKeySecret;
    /** @var list<int> Stack indexes of HTML elements in TABLE_FAMILY or `template` (for F, M-09). */
    private array $tableIndex = [];
    /**
     * @var list<int> Stack indexes of the lists (LIST) and list items
     *      (LIST_ITEM) opened in the content (M-24 item j). The top entry is
     *      the nearest of them to the current node.
     */
    private array $listIndex = [];
    /** Stack index of the `svg`/`math` element of the foreign region, or null. */
    private ?int $region = null;
    /**
     * Number of `select` elements pushed in HTML context. Inside such a
     * `select` the position is always in HTML context: M-08 rejects the
     * `svg` and `math` start tags that would open a foreign region.
     */
    private int $selects = 0;
    /** Stack index of the foreign INTEGRATION/TEXT element whose content must be characters only (M-07), or null. */
    private ?int $charsOnly = null;
    /** Offset after the start tag of a `noscript` in HTML context (M-11), or null. */
    private ?int $noscriptStart = null;

    /** Stack index of the open block element, or null. */
    private ?int $blockIndex = null;
    private string $blockId = '';
    private string $blockFormat = '';
    private int $blockStart = 0;
    /** @var list<Block> */
    private array $blocks = [];
    /** @var array<string, true> */
    private array $ids = [];

    private function __construct(private readonly string $input)
    {
        $this->open = array_fill(0, self::SLOTS, 0);
        $this->openInContent = $this->open;
        // A new random key for each parse: one parse cannot learn the key
        // of another one (the result does not depend on the key).
        $this->nameKeySecret = random_bytes(self::NAME_KEY_SECRET_BYTES);
    }

    /**
     * Check the document and extract its blocks. Never throws for any input
     * string: rule failures become an invalid Result.
     *
     * @throws \Random\RandomException Only when the system has no source of
     *         random bytes (not caused by the input; the command line exits
     *         with code 70).
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
                    $this->comment();
                    break;
                case TokenType::Cdata:
                    $this->cdata($token);
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

    private function comment(): void
    {
        if ($this->charsOnly !== null) {
            throw new MarkupError(ErrorCode::Structure, 'M-07');
        }
        // Comments are allowed in `select` (M-08) and in content (M-22).
    }

    /**
     * `<![CDATA[`: a CDATA section in foreign content, a bogus comment in
     * HTML content (M-04). M-12 makes both end at the same byte. Then it is
     * allowed wherever a comment is allowed, and also in the content of a
     * foreign INTEGRATION or TEXT element (M-07 items 3 and 4, edge case E5).
     */
    private function cdata(Token $token): void
    {
        if (!$token->cdataSameEnd) {
            throw new MarkupError(ErrorCode::Structure, 'M-12');
        }
    }

    private function doctype(): void
    {
        if ($this->charsOnly !== null) {
            throw new MarkupError(ErrorCode::Structure, 'M-07');
        }
        if ($this->selects > 0) {
            throw new MarkupError(ErrorCode::Structure, 'M-08');
        }
        if ($this->blockIndex !== null) {
            throw new MarkupError(ErrorCode::Structure, 'M-22');
        }
    }

    private function startTag(Token $token): void
    {
        if ($this->charsOnly !== null) {
            throw new MarkupError(ErrorCode::Structure, 'M-07');
        }
        $name = $token->name;
        $foreign = $this->region !== null;

        if ($name === 'frameset') {
            throw new MarkupError(ErrorCode::Structure, 'M-10');
        }
        self::checkReserved($token);
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
        if ($this->blockIndex !== null) {
            $this->contentStartTag($token, $this->names[$this->blockIndex]);
        }

        if ($foreign) {
            $this->foreignStartTag($token);
            return;
        }
        $this->impliedParts($name);
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

    /**
     * M-13: a `script` start tag whose first `type` attribute has a raw value
     * that contains `rivqen-manifest` (ASCII case-insensitive), `&` or a
     * byte >= 0x80 (H-28). No PCRE: a PCRE error must not let a value pass.
     */
    private static function checkReserved(Token $startTag): void
    {
        if ($startTag->name !== 'script') {
            return;
        }
        $type = $startTag->attribute('type');
        if ($type !== null && (stripos($type, 'rivqen-manifest') !== false || str_contains($type, '&')
            || strspn($type, self::ASCII_BYTES) !== strlen($type))) {
            throw new MarkupError(ErrorCode::Reserved, 'M-13');
        }
    }

    /**
     * M-26 item 3: for a start tag in HTML context, add 1 to D for each
     * implied part that F (M-09) does not have yet, and give it to F.
     */
    private function impliedParts(string $name): void
    {
        if ($this->tableIndex === []) {
            return;
        }
        $f = $this->tableIndex[count($this->tableIndex) - 1];
        $parts = self::IMPLIED_PARTS[$this->names[$f]][$name] ?? 0;
        $new = $parts & ~$this->flags[$f];
        if ($new === 0) {
            return;
        }
        $this->flags[$f] |= $new;
        $this->raise(self::partCount($new));
    }

    /** Number of implied-part flags in $flags. */
    private static function partCount(int $flags): int
    {
        return (($flags & self::ROW_GROUP) === 0 ? 0 : 1)
            + (($flags & self::ROW) === 0 ? 0 : 1)
            + (($flags & self::COLUMN_GROUP) === 0 ? 0 : 1);
    }

    /** M-26: add $units to D. D larger than MAX_DEPTH is an error. */
    private function raise(int $units): void
    {
        $this->depth += $units;
        if ($this->depth > self::MAX_DEPTH) {
            throw new MarkupError(ErrorCode::Limit, 'M-26');
        }
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
        if ($this->blockIndex !== null) {
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
        if ($this->open[self::TEMPLATE] > 0 || $this->open[self::SELECT] > 0) {
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
    private function contentStartTag(Token $token, string $blockName): void
    {
        $name = $token->name;
        if (isset(Names::FORBIDDEN_CONTENT[$name])) {
            throw new MarkupError(ErrorCode::Structure, 'M-22');
        }
        $void = isset(Names::VOID[$name]);
        if ($token->selfClosing && !$void) {
            throw new MarkupError(ErrorCode::Structure, 'M-22');
        }
        // Item k: in table context the WHATWG parser foster-parents every
        // start tag except the table parts (a `table` start tag closes the
        // open table). Text and comments stay allowed.
        $f = $this->tableContext();
        if ($f !== null && isset(Names::TABLE_CONTEXT[$f]) && !isset(Names::TABLE_PART[$name]) && $name !== 'col') {
            throw new MarkupError(ErrorCode::Structure, 'M-24');
        }
        $invalid = match (true) {
            isset(Names::PCLOSE[$name]) && ($blockName === 'p' || $blockName === 'span') => true,
            isset(Names::HEADING[$name]) => isset(Names::HEADING[$blockName])
                || $this->openInContent[self::HEADING] > 0,
            isset(Names::LIST_ITEM[$name]) => $this->openInContent[self::LIST] === 0 // item c
                || $this->listItemAboveList(), // item j
            $name === 'button' => $this->open[self::BUTTON] > 0,
            $name === 'a' => $this->open[self::A] > 0,
            $name === 'nobr' => $this->open[self::NOBR] > 0,
            isset(Names::TABLE_PART[$name]), $name === 'col' => $this->openInContent[self::TABLE] === 0,
            $name === 'rb', $name === 'rtc' => $this->openInContent[self::RUBY] === 0
                || $this->open[self::RB_RP_RT] > 0 || $this->open[self::RTC] > 0,
            $name === 'rp', $name === 'rt' => $this->openInContent[self::RUBY] === 0 || $this->open[self::RB_RP_RT] > 0,
            default => false,
        };
        if ($invalid) {
            throw new MarkupError(ErrorCode::Structure, 'M-24');
        }
    }

    private function endTag(Token $token): void
    {
        $name = $token->name;
        if ($this->charsOnly !== null) {
            // M-07 items 3 and 4: only the end tag of that element may follow.
            if ($name !== $this->names[$this->charsOnly]) {
                throw new MarkupError(ErrorCode::Structure, 'M-07');
            }
            $this->charsOnly = null;
            $this->endTagPop(count($this->names) - 1, $name);
            return;
        }
        if ($this->noscriptStart !== null) {
            // The end tag that ends the RAWTEXT of `noscript` (M-11).
            $this->checkNoscript($this->noscriptStart, $token->start);
        }
        if ($this->blockIndex !== null) {
            $this->contentEndTag($token);
            return;
        }
        $foreign = $this->region !== null;
        if ($this->selects > 0 && !isset(Names::SELECT_END[$name])) {
            throw new MarkupError(ErrorCode::Structure, 'M-08');
        }
        // M-05 step 3.
        if (!isset($this->byName[$this->nameKey($name)])) {
            if ($foreign) {
                throw new MarkupError(ErrorCode::Structure, 'M-07');
            }
            // M-06 item 3: the tree builder can close an implied `tbody` or
            // `tr` and the open cell for these end tags.
            if (isset(Names::STRAY_TABLE_END[$name])) {
                throw new MarkupError(ErrorCode::Structure, 'M-06');
            }
            return; // no element with this name: ignore the end tag
        }
        $match = count($this->names) - 1;
        while ($this->names[$match] !== $name) {
            $match--;
        }
        if ($this->region !== null && $match < $this->region) {
            throw new MarkupError(ErrorCode::Structure, 'M-07');
        }
        $this->endTagPop($match, $name);
    }

    /**
     * M-05 step 3 for an end tag that matches the element at $match: check
     * M-06 items 1, 2 and 4, pop, then update D (M-26 items 4 and 5) and the
     * marks of the new current node (M-06 item 4).
     *
     * Cost: O(number of popped elements).
     */
    private function endTagPop(int $match, string $name): void
    {
        $top = count($this->names) - 1;
        $tablePartsOk = isset(Names::TABLE_FAMILY[$name]);
        $crossesSpecial = false;
        $marked = ($this->flags[$match] & self::MARKED) !== 0;
        // Crossed elements, from the top down to the match.
        for ($i = $top; $i > $match; $i--) {
            $crossed = $this->names[$i];
            $flags = $this->flags[$i];
            $foreign = ($flags & self::FOREIGN) !== 0;
            if (!$foreign && isset(Names::GUARDED[$crossed]) && !($tablePartsOk && isset(Names::TABLE_PART[$crossed]))) {
                throw new MarkupError(ErrorCode::Structure, 'M-06'); // item 1
            }
            if ($foreign || isset(Names::SPECIAL[$crossed])) {
                $crossesSpecial = true;
            }
            if (($flags & self::MARKED) !== 0) {
                $marked = true;
            }
        }
        // Items 2 and 4: then the adoption agency of the tree builder has no
        // furthest block and pops the same elements.
        if (($crossesSpecial || $marked) && isset(Names::FORMATTING[$name])) {
            throw new MarkupError(ErrorCode::Structure, 'M-06');
        }
        $flags = $this->flags[$match];
        $clean = $match === $top && ($flags & self::DIRTY) === 0 && $name !== 'html' && $name !== 'body';
        $this->popTo($match);
        if ($clean) {
            // M-26 item 4: give back the push unit and the implied parts.
            $this->depth -= 1 + self::partCount($flags);
        }
        if ($match === 0) {
            return; // no new current node
        }
        $current = $match - 1;
        if (!$clean) {
            $this->flags[$current] |= self::DIRTY; // M-26 item 5
        }
        if ($crossesSpecial || $marked) {
            $this->flags[$current] |= self::MARKED; // M-06 item 4
        }
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
        $this->endTagPop($top, $name);
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
        $this->blockIndex = null;
    }

    private function eof(): void
    {
        if ($this->noscriptStart !== null) {
            $this->checkNoscript($this->noscriptStart, strlen($this->input));
        }
        if ($this->blockIndex !== null) {
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
     *
     * Item 5 (H-27): C is balanced. $open is the stack of C: a list of names
     * (no name keys, see the class comment). Each start tag adds at most one
     * entry and has at least 3 bytes, so the list is bounded by the size of
     * C. C is not counted in D (M-26 item 7).
     */
    private function checkNoscript(int $start, int $contentEnd): void
    {
        $this->noscriptStart = null;
        $length = $contentEnd - $start;
        $sub = new Tokenizer(substr($this->input, $start, $length) . '</noscript>');
        /** @var list<string> $open */
        $open = [];
        while (true) {
            $token = $sub->next();
            if ($token->type === TokenType::Eof) {
                throw new MarkupError(ErrorCode::Structure, 'M-11');
            }
            if ($token->type === TokenType::EndTag) {
                if ($token->name === 'noscript') {
                    // Item 1, and item 5: no element of C is open at the end of C.
                    if ($token->start !== $length || $open !== []) {
                        throw new MarkupError(ErrorCode::Structure, 'M-11');
                    }
                    return;
                }
                // Item 5: an end tag closes the last open element of C.
                if (array_pop($open) !== $token->name) {
                    throw new MarkupError(ErrorCode::Structure, 'M-11');
                }
                continue;
            }
            if ($token->type !== TokenType::StartTag) {
                continue;
            }
            // M-13 applies also in C (edge case E8, human decision H-22): a
            // reader with scripting disabled sees this `script` as an element.
            self::checkReserved($token);
            $name = $token->name;
            if ($token->attribute(self::BLOCK_ATTR) !== null || isset(Names::NOSCRIPT_FORBIDDEN[$name])) {
                throw new MarkupError(ErrorCode::Structure, 'M-11');
            }
            // M-05 item 1 pushes every name that is not in VOID. Item 3 has
            // already rejected `svg` and `math`.
            if (!isset(Names::VOID[$name])) {
                $open[] = $name;
            }
            $state = Names::TEXT[$name] ?? null;
            if ($state !== null) {
                $sub->switchTo($state, $name);
            }
        }
    }

    // ----------------------------------------------------------------------
    // Token stack
    // ----------------------------------------------------------------------

    /**
     * M-05 push. M-26 item 2: each push adds 1 to D. D is checked before the
     * element goes on the stack, so the stack never has more than MAX_DEPTH
     * elements.
     */
    private function push(string $name, bool $foreign): int
    {
        $this->raise(1);
        $index = count($this->names);
        $flags = $foreign ? self::FOREIGN : 0;
        $slot = self::TRACKED[$name] ?? null;
        if ($this->blockIndex !== null) {
            $flags |= self::CONTENT;
            if ($slot !== null) {
                $this->openInContent[$slot]++;
            }
            if (isset(Names::LIST[$name]) || isset(Names::LIST_ITEM[$name])) {
                $this->listIndex[] = $index;
            }
        }
        $this->names[] = $name;
        $this->flags[] = $flags;
        if ($slot !== null) {
            $this->open[$slot]++;
        }
        $key = $this->nameKey($name);
        $this->byName[$key] = ($this->byName[$key] ?? 0) + 1;
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

    /**
     * Pop all elements above $index, then the element at $index.
     *
     * Cost: O(number of popped elements). Do not use array_splice() here: it
     * rebuilds the whole array, so a document with many matching end tags
     * (`<b>` x n, then `</b>` x n) would take O(n^2) time.
     */
    private function popTo(int $index): void
    {
        for ($i = count($this->names) - 1; $i >= $index; $i--) {
            $name = $this->names[$i];
            $flags = $this->flags[$i];
            $key = $this->nameKey($name);
            if (--$this->byName[$key] === 0) {
                unset($this->byName[$key]);
            }
            $slot = self::TRACKED[$name] ?? null;
            if ($slot !== null) {
                $this->open[$slot]--;
                if (($flags & self::CONTENT) !== 0) {
                    $this->openInContent[$slot]--;
                }
            }
            if ($this->tableIndex !== [] && $this->tableIndex[count($this->tableIndex) - 1] === $i) {
                array_pop($this->tableIndex);
            }
            if ($this->listIndex !== [] && $this->listIndex[count($this->listIndex) - 1] === $i) {
                array_pop($this->listIndex);
            }
            if ($name === 'select' && ($flags & self::FOREIGN) === 0) {
                $this->selects--;
            }
            if ($i === $this->region) {
                $this->region = null;
            }
            array_pop($this->names);
            array_pop($this->flags);
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

    /**
     * M-24 item j: a list item opened in the content is above the nearest
     * list opened in the content. That is: the top entry of $listIndex is a
     * list item. Call only when a list opened in the content is open (item c),
     * so $listIndex is not empty.
     */
    private function listItemAboveList(): bool
    {
        return isset(Names::LIST_ITEM[$this->names[$this->listIndex[count($this->listIndex) - 1]]]);
    }

    /**
     * Key of $name in $byName: the name, then a keyed 64-bit xxh3 hash of
     * the name (random secret of this parse).
     *
     * Exact: the hash has a fixed length, so two keys are equal only when
     * the names are equal. Not predictable: DJBX33A(name . h) is
     * DJBX33A(name) * 33^8 + (a sum over the 8 bytes of h), modulo 2^64. So
     * names with the same DJBX33A hash get buckets that the attacker cannot
     * predict: h depends on the secret.
     */
    private function nameKey(string $name): string
    {
        return $name . hash('xxh3', $name, true, ['secret' => $this->nameKeySecret]);
    }

    /** M-15: `^[a-z0-9][a-z0-9_-]{0,63}$` on the raw value, without a regex. */
    public static function isValidId(string $id): bool
    {
        $n = strlen($id);
        // The first-character check also rejects the empty value.
        return $n <= self::ID_MAX
            && strspn($id, self::ID_FIRST_CHARS, 0, 1) === 1
            && strspn($id, self::ID_CHARS) === $n;
    }
}
