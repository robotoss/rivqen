<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup\Tests\Fuzz;

use Random\Randomizer;
use Rivqen\Tools\RqpMarkup\Names;
use Rivqen\Tools\RqpMarkup\Parser;
use Rivqen\Tools\RqpMarkup\Revision;
use Rivqen\Tools\RqpMarkup\Tokenizer;
use Rivqen\Tools\RqpMarkup\TokenType;

/**
 * Input generator and invariants for the property tests and for
 * tests/fuzz.php. A violation is returned as a short text without
 * document content.
 */
final class Harness
{
    /** Fragments that reach every tokenizer state and every rule. */
    public const FRAGMENTS = [
        '<p data-rq-block="a">', '</p>', '<div data-rq-block="b">', '</div>', '<span data-rq-block="c">',
        '<span>', '</span>', '<h1 data-rq-block="h">', '</h1>', '<h2>', '</h2>', '<title data-rq-block="t">',
        '<title>', '</title>', '<script type="application/json" data-rq-block="j">', '<script>', '</script>',
        '{"a":[1,"x"]}', '[1,]', '<!--', '-->', '--!>', '<!-->', '<![CDATA[', ']]>', '<?pi', '<!DOCTYPE html>',
        '<', '>', '/', '/>', '"', "'", '=', 'x', ' ', "\r\n", "\t", '&amp;', '&#112;', '-', '!', '[', ']',
        '<table>', '</table>', '<tr>', '</tr>', '<td>', '</td>', '<caption>', '<col>', '<colgroup>',
        '<noscript>', '</noscript>', '<select>', '</select>', '<option>', '</option>', '<optgroup>',
        '<svg>', '</svg>', '<svg/>', '<math>', '</math>', '<mi>', '</mi>', '<foreignObject>', '<desc>', '<g>', '</g>',
        '<b>', '</b>', '<a>', '</a>', '<button>', '</button>', '<nobr>', '<ul>', '</ul>', '<li>', '</li>',
        '<dl>', '<dt>', '<ruby>', '<rt>', '<rb>', '<template>', '</template>', '<style>', '</style>',
        '<textarea>', '</textarea>', '<plaintext>', '<frameset>', '<br>', '</br>', '<img src=x>', '<span/>',
        '<p data-rq-block="d"/>', '<p data-rq-block="BAD">', '<li data-rq-block="e">', '<!--<script>',
        '<script type="rivqen-manifest">', "\xC3\xA9", "\xF0\x9F\x98\x80", "\0", "\xEF\xBB\xBF", '</ x>', '</>',
        '<P DATA-RQ-BLOCK=f>', '<p data-rq-block=g data-rq-block=h>', '<iframe>', '</iframe>',
    ];

    private function __construct() {}

    public static function document(Randomizer $random, int $maxParts = 40): string
    {
        $parts = [];
        $count = $random->getInt(0, $maxParts);
        for ($i = 0; $i < $count; $i++) {
            $parts[] = $random->getInt(0, 30) === 0
                ? $random->getBytes($random->getInt(1, 3))
                : self::FRAGMENTS[$random->getInt(0, count(self::FRAGMENTS) - 1)];
        }
        return implode('', $parts);
    }

    public static function mutate(Randomizer $random, string $html): string
    {
        $steps = $random->getInt(1, 4);
        for ($i = 0; $i < $steps; $i++) {
            $at = $random->getInt(0, strlen($html));
            $html = match ($random->getInt(0, 3)) {
                0 => substr($html, 0, $at) . self::FRAGMENTS[$random->getInt(0, count(self::FRAGMENTS) - 1)] . substr($html, $at),
                1 => substr($html, 0, $at) . substr($html, $at + $random->getInt(1, 8)),
                2 => substr($html, 0, $at) . $random->getBytes(1) . substr($html, $at + 1),
                default => substr($html, 0, $at),
            };
        }
        return $html;
    }

    /**
     * Check every invariant. Return null, or a short description of the
     * first violation.
     */
    public static function check(string $html): ?string
    {
        try {
            return self::checkParse($html) ?? self::checkTokens($html);
        } catch (\Throwable $e) {
            return 'exception ' . $e::class . ' at ' . basename($e->getFile()) . ':' . $e->getLine();
        }
    }

    /**
     * Check a generated valid document: the invariants and the expected blocks.
     *
     * @param list<array{string, string, string}> $expected
     */
    public static function checkExpected(string $html, array $expected): ?string
    {
        $violation = self::check($html);
        if ($violation !== null) {
            return $violation;
        }
        $result = Parser::parse($html);
        if (!$result->valid) {
            return 'generated valid document is invalid by ' . $result->rule;
        }
        $actual = [];
        foreach ($result->blocks as $block) {
            $actual[] = [$block->id, $block->format, substr($html, $block->start, $block->end - $block->start)];
        }
        return $actual === $expected ? null : 'blocks differ from the generated blocks';
    }

    private static function checkParse(string $html): ?string
    {
        $result = Parser::parse($html);
        if ($result != Parser::parse($html)) {
            return 'not deterministic';
        }
        if (!$result->valid) {
            $ok = $result->error !== null && $result->rule !== null && $result->blocks === []
                && $result->templateRevision === null && $result->pageRevision === null;
            return $ok ? null : 'invalid result has extra fields';
        }
        if ($result->error !== null || $result->rule !== null || count($result->blocks) > Parser::MAX_BLOCKS) {
            return 'valid result has an error or too many blocks';
        }
        $length = strlen($html);
        $previousEnd = -1;
        $removed = 0;
        $ids = [];
        foreach ($result->blocks as $k => $block) {
            if ($block->start <= $previousEnd || $block->start > $block->end || $block->end > $length) {
                return "block $k: offsets out of order or out of the input";
            }
            if ($html[$block->start - 1] !== '>' || substr($html, $block->end, 2) !== '</') {
                return "block $k: content is not between a start tag and an end tag";
            }
            $content = substr($html, $block->start, $block->end - $block->start);
            if (Revision::b64uSha256($content) !== $block->sha256) {
                return "block $k: sha256 is not the hash of input[start, end)";
            }
            if (!Parser::isValidId($block->id) || isset($ids[$block->id])) {
                return "block $k: invalid or duplicate id";
            }
            if ($block->end - $block->start > Parser::MAX_BLOCK_BYTES || !in_array($block->format, ['html', 'json'], true)) {
                return "block $k: too large or unknown format";
            }
            $ids[$block->id] = true;
            $removed += $block->end - $block->start;
            $previousEnd = $block->end;
        }
        $template = Revision::template($html, $result->blocks);
        if (strlen($template) !== $length - $removed || Revision::templateRevision($template) !== $result->templateRevision) {
            return 'template revision does not match';
        }
        if (Revision::pageRevision((string) $result->templateRevision, $result->blocks) !== $result->pageRevision) {
            return 'page revision does not match';
        }
        return null;
    }

    private static function checkTokens(string $html): ?string
    {
        $tokenizer = new Tokenizer($html);
        $length = strlen($html);
        $previousEnd = 0;
        for ($n = 0; $n <= $length + 1; $n++) {
            $token = $tokenizer->next();
            if ($token->type === TokenType::Eof) {
                return $token->start === $length && $token->end === $length ? null : 'EOF token is not at the end';
            }
            if ($token->start < $previousEnd || $token->end <= $token->start || $token->end > $length) {
                return "token $n: span out of order or out of the input";
            }
            if ($html[$token->start] !== '<') {
                return "token $n: does not start at <";
            }
            $isTag = $token->type === TokenType::StartTag || $token->type === TokenType::EndTag;
            if ($isTag && $html[$token->end - 1] !== '>') {
                return "token $n: tag does not end at >";
            }
            $previousEnd = $token->end;
            $state = $token->type === TokenType::StartTag ? (Names::TEXT[$token->name] ?? null) : null;
            if ($state !== null) {
                $tokenizer->switchTo($state, $token->name);
            }
        }
        return 'more tokens than bytes';
    }

    /**
     * Generate a document that is valid by construction, with the blocks
     * that the parser must find: [html, list of [id, format, content]].
     * The grammar follows markup.md section 2 (allowed contexts, balanced
     * content, no forbidden content, no implied end tags).
     *
     * @return array{string, list<array{string, string, string}>}
     */
    public static function validDocument(Randomizer $random): array
    {
        $blocks = [];
        $html = $random->getInt(0, 3) === 0 ? "\xEF\xBB\xBF" : '';
        $html .= self::pick($random, ['', '<!DOCTYPE html>', "<!doctype html>\r\n"]);
        $count = $random->getInt(0, 6);
        for ($i = 0; $i < $count; $i++) {
            $html .= self::outside($random, $blocks, 0);
        }
        return [$html, $blocks];
    }

    /**
     * Markup outside blocks; may contain blocks.
     *
     * @param list<array{string, string, string}> $blocks
     */
    private static function outside(Randomizer $random, array &$blocks, int $depth): string
    {
        $choice = $random->getInt(0, $depth > 3 ? 4 : 16);
        return match ($choice) {
            0 => self::text($random),
            1 => '<!-- ' . self::text($random) . ' -->',
            2 => '<img src="x.png" alt="a>b">',
            3 => '<svg viewBox="0 0 1 1"><g><rect/><title>t</title><style>a{}</style><![CDATA[x]]></g></svg>',
            4 => self::block($random, $blocks),
            5 => '<div class="w">' . self::outside($random, $blocks, $depth + 1) . '</div>',
            6 => '<section>' . self::outside($random, $blocks, $depth + 1) . self::outside($random, $blocks, $depth + 1) . '</section>',
            7 => '<table><tr><td>' . self::outside($random, $blocks, $depth + 1) . '</td></tr></table>',
            8 => '<select><option>a</option><option>b</select>',
            9 => '<noscript><img src="https://example.com/p.gif"></noscript>',
            10 => '<script>if (a < b) { s = "</p>"; }</script><style>p > b {}</style>',
            11 => '<math><mi>x</mi><mo>+</mo><mn>1</mn></math>',
            12 => '<ul><li>' . self::text($random) . '<li>' . self::text($random) . '</ul>',
            13 => '<template><p>t</p></template>',
            default => self::block($random, $blocks),
        };
    }

    /**
     * @param list<array{string, string, string}> $blocks
     */
    private static function block(Randomizer $random, array &$blocks): string
    {
        $id = 'b' . count($blocks) . self::pick($random, ['', '-x', '_y']);
        $kind = $random->getInt(0, 9);
        if ($kind === 0) {
            $content = self::json($random, 0);
            if ($random->getInt(0, 1) === 0) {
                $content = " \r\n" . $content . "\t";
            }
            $blocks[] = [$id, 'json', $content];
            return '<script type="application/json" data-rq-block="' . $id . '">' . $content . '</script>';
        }
        if ($kind === 1) {
            $content = self::pick($random, ['', 'Shop', 'a <b> &amp; c', "\u{1F600} \r\n x"]);
            $blocks[] = [$id, 'html', $content];
            return '<title data-rq-block="' . $id . '">' . $content . '</TITLE>';
        }
        $name = self::pick($random, ['div', 'p', 'span', 'section', 'h2', 'article', 'main']);
        $phrasing = $name === 'p' || $name === 'span' || $name === 'h2';
        $content = '';
        $parts = $random->getInt(0, 4);
        for ($i = 0; $i < $parts; $i++) {
            $content .= self::content($random, $phrasing, 0);
        }
        $blocks[] = [$id, 'html', $content];
        $quote = self::pick($random, ['"', "'", '']);
        return '<' . $name . ' class="c" data-rq-block=' . $quote . $id . $quote . ' title="x>y">' . $content . '</' . $name . '>';
    }

    /** Content of an html block (balanced, allowed tokens only). */
    private static function content(Randomizer $random, bool $phrasing, int $depth): string
    {
        $max = $phrasing ? 7 : 11;
        $choice = $random->getInt(0, $depth > 2 ? 3 : $max);
        return match ($choice) {
            0 => self::text($random),
            1 => '<br><img src="x.gif" alt="">',
            2 => '<!-- c <p> -->',
            3 => '&amp; &lt;p&gt; ' . "\0",
            4 => '<b>' . self::content($random, $phrasing, $depth + 1) . '</b>',
            5 => '<em class=x>' . self::content($random, $phrasing, $depth + 1) . '</em>',
            6 => '<a href="https://example.com/?a=1&amp;b=2">' . self::text($random) . '</a>',
            7 => '<ruby>x<rt>y</rt></ruby>',
            8 => '<div>' . self::content($random, false, $depth + 1) . '</div>',
            9 => '<ul><li>' . self::content($random, false, $depth + 1) . '</li></ul>',
            10 => '<table><tbody><tr><td>' . self::text($random) . '</td></tr></tbody></table>',
            default => '<p>' . self::content($random, true, $depth + 1) . '</p>',
        };
    }

    private static function json(Randomizer $random, int $depth): string
    {
        return match ($random->getInt(0, $depth > 3 ? 2 : 4)) {
            0 => self::pick($random, ['0', '-1.5e3', '1e400', 'true', 'null']),
            1 => '"' . self::pick($random, ['', 'a', '\\u00e9', '\\ud83d\\ude00', '</p>', '\\"']) . '"',
            2 => '[]',
            3 => '[' . self::json($random, $depth + 1) . ', ' . self::json($random, $depth + 1) . ']',
            default => '{"k": ' . self::json($random, $depth + 1) . ', "k": 1}',
        };
    }

    private static function text(Randomizer $random): string
    {
        return self::pick($random, ['', 'x', 'a > b', "line\r\nline", "\u{00E9}\u{4E2D}", "\u{1F600}", 'x = 1;']);
    }

    /**
     * @param non-empty-list<string> $options
     */
    private static function pick(Randomizer $random, array $options): string
    {
        return $options[$random->getInt(0, count($options) - 1)];
    }

    /**
     * Inputs of the fixtures (smaller than $maxBytes) as a corpus.
     *
     * @return list<string>
     */
    public static function corpus(string $fixturesDir, int $maxBytes = 20_000): array
    {
        $inputs = [];
        foreach (glob($fixturesDir . '/FX-RQ-MARKUP-*/input.html') ?: [] as $path) {
            $data = file_get_contents($path);
            if (is_string($data) && strlen($data) < $maxBytes) {
                $inputs[] = $data;
            }
        }
        return $inputs;
    }
}
