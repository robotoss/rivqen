<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup\Tests;

use PHPUnit\Framework\TestCase;
use Random\Engine\Mt19937;
use Random\Randomizer;
use Rivqen\Tools\RqpMarkup\Names;
use Rivqen\Tools\RqpMarkup\Parser;
use Rivqen\Tools\RqpMarkup\Revision;
use Rivqen\Tools\RqpMarkup\Tokenizer;
use Rivqen\Tools\RqpMarkup\TokenType;

/**
 * Property tests on random documents with fixed seeds. The documents are
 * built from fragments that reach every tokenizer state and every rule,
 * plus random bytes.
 */
final class PropertyTest extends TestCase
{
    private const SEED = 20261009;
    private const DOCUMENTS = 4000;

    private const FRAGMENTS = [
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

    public function testRandomDocumentsKeepTheInvariants(): void
    {
        $random = new Randomizer(new Mt19937(self::SEED));
        $valid = 0;
        for ($n = 0; $n < self::DOCUMENTS; $n++) {
            $html = self::document($random);
            $this->checkParse($html);
            $valid += Parser::parse($html)->valid ? 1 : 0;
            $this->checkTokens($html);
        }
        // The generator must reach valid documents with blocks, not only errors.
        self::assertGreaterThan(self::DOCUMENTS / 20, $valid);
    }

    public function testRandomBytesNeverEscape(): void
    {
        $random = new Randomizer(new Mt19937(self::SEED + 1));
        for ($n = 0; $n < 500; $n++) {
            $bytes = $random->getBytes($random->getInt(1, 300));
            $this->checkParse($bytes);
            $this->checkTokens($bytes);
        }
    }

    public function testMutatedFixturesKeepTheInvariants(): void
    {
        $random = new Randomizer(new Mt19937(self::SEED + 2));
        $inputs = [];
        foreach (glob(FixturesTest::fixturesDir() . '/FX-RQ-MARKUP-*/input.html') ?: [] as $path) {
            $data = (string) file_get_contents($path);
            if (strlen($data) < 20_000) {
                $inputs[] = $data;
            }
        }
        self::assertNotSame([], $inputs);
        for ($n = 0; $n < 1500; $n++) {
            $html = $inputs[$random->getInt(0, count($inputs) - 1)];
            $html = self::mutate($random, $html);
            $this->checkParse($html);
            $this->checkTokens($html);
        }
    }

    private function checkParse(string $html): void
    {
        $result = Parser::parse($html);
        self::assertEquals($result, Parser::parse($html), 'deterministic');
        if (!$result->valid) {
            self::assertNotNull($result->error);
            self::assertNotNull($result->rule);
            self::assertSame([], $result->blocks);
            self::assertNull($result->templateRevision);
            return;
        }
        self::assertNull($result->error);
        $length = strlen($html);
        $previousEnd = -1;
        $ids = [];
        foreach ($result->blocks as $block) {
            self::assertGreaterThan($previousEnd, $block->start, 'document order, no overlap');
            self::assertLessThanOrEqual($block->end, $block->start);
            self::assertLessThanOrEqual($length, $block->end);
            self::assertSame('>', $html[$block->start - 1], 'content starts after the start tag');
            self::assertSame('</', substr($html, $block->end, 2), 'content ends at an end tag');
            $content = substr($html, $block->start, $block->end - $block->start);
            self::assertSame(Revision::b64uSha256($content), $block->sha256);
            self::assertTrue(Parser::isValidId($block->id));
            self::assertArrayNotHasKey($block->id, $ids);
            $ids[$block->id] = true;
            self::assertContains($block->format, ['html', 'json']);
            $previousEnd = $block->end;
        }
        self::assertLessThanOrEqual(Parser::MAX_BLOCKS, count($result->blocks));
        $template = Revision::template($html, $result->blocks);
        self::assertSame(Revision::templateRevision($template), $result->templateRevision);
        self::assertSame($length - strlen($template), array_sum(array_map(static fn($b) => $b->end - $b->start, $result->blocks)));
    }

    private function checkTokens(string $html): void
    {
        $tokenizer = new Tokenizer($html);
        $length = strlen($html);
        $previousEnd = 0;
        for ($n = 0; $n <= $length + 1; $n++) {
            $token = $tokenizer->next();
            if ($token->type === TokenType::Eof) {
                self::assertSame($length, $token->start);
                return;
            }
            self::assertGreaterThanOrEqual($previousEnd, $token->start);
            self::assertGreaterThan($token->start, $token->end);
            self::assertLessThanOrEqual($length, $token->end);
            self::assertSame('<', $html[$token->start]);
            if ($token->type === TokenType::StartTag || $token->type === TokenType::EndTag) {
                self::assertSame('>', $html[$token->end - 1], 'a tag ends at >');
            }
            $previousEnd = $token->end;
            $state = $token->type === TokenType::StartTag ? (Names::TEXT[$token->name] ?? null) : null;
            if ($state !== null) {
                $tokenizer->switchTo($state, $token->name);
            }
        }
        self::fail('more tokens than bytes');
    }

    private static function document(Randomizer $random): string
    {
        $parts = [];
        $count = $random->getInt(0, 40);
        for ($i = 0; $i < $count; $i++) {
            $parts[] = $random->getInt(0, 30) === 0
                ? $random->getBytes($random->getInt(1, 3))
                : self::FRAGMENTS[$random->getInt(0, count(self::FRAGMENTS) - 1)];
        }
        return implode('', $parts);
    }

    private static function mutate(Randomizer $random, string $html): string
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
}
