<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup\Tests;

use PHPUnit\Framework\TestCase;
use Random\Engine\Mt19937;
use Random\Randomizer;
use Rivqen\Tools\RqpMarkup\ErrorCode;
use Rivqen\Tools\RqpMarkup\Parser;
use Rivqen\Tools\RqpMarkup\Tests\Fuzz\Harness;

/**
 * Property tests with fixed seeds. Invariants (Harness::check): no error
 * escapes; the result is deterministic; for a valid result the blocks are
 * in document order, inside the input, between a start tag and an end tag,
 * with unique valid ids, sha256 = hash of input[start, end), and the
 * revisions match CONTRACT.md section 4; tokens are in order, inside the
 * input, and fewer than the bytes. Two model tests check the depth counter
 * D (M-26) and the balance of the noscript content (M-11 item 5) against a
 * model that the test computes from the generated parts.
 */
final class PropertyTest extends TestCase
{
    private const SEED = 20261009;

    public function testRandomDocumentsKeepTheInvariants(): void
    {
        $random = new Randomizer(new Mt19937(self::SEED));
        for ($n = 0; $n < 4000; $n++) {
            self::assertNull(Harness::check(Harness::document($random)), "document $n");
        }
    }

    public function testGeneratedValidDocumentsGiveTheGeneratedBlocks(): void
    {
        // Oracle: the generator knows the blocks of each document it builds.
        $random = new Randomizer(new Mt19937(self::SEED + 3));
        $blocks = 0;
        for ($n = 0; $n < 2000; $n++) {
            [$html, $expected] = Harness::validDocument($random);
            $result = Parser::parse($html);
            self::assertTrue($result->valid, "document $n: " . ($result->rule ?? ''));
            $actual = [];
            foreach ($result->blocks as $block) {
                $actual[] = [$block->id, $block->format, substr($html, $block->start, $block->end - $block->start)];
            }
            self::assertSame($expected, $actual, "document $n");
            self::assertNull(Harness::check($html), "document $n");
            $blocks += count($expected);
        }
        self::assertGreaterThan(1500, $blocks);
    }

    public function testMutatedValidDocumentsKeepTheInvariants(): void
    {
        $random = new Randomizer(new Mt19937(self::SEED + 4));
        for ($n = 0; $n < 2000; $n++) {
            [$html] = Harness::validDocument($random);
            self::assertNull(Harness::check(Harness::mutate($random, $html)), "document $n");
        }
    }

    public function testRandomBytesKeepTheInvariants(): void
    {
        $random = new Randomizer(new Mt19937(self::SEED + 1));
        for ($n = 0; $n < 500; $n++) {
            self::assertNull(Harness::check($random->getBytes($random->getInt(1, 300))), "input $n");
        }
    }

    /**
     * Shapes that leave the token stack empty, with their effect on the
     * depth counter D of M-26 (worked out by hand from the rule text):
     * [html, units that stay, largest D inside the shape].
     */
    private const DEPTH_SHAPES = [
        ['<div></div>', 0, 1],
        ['<br><img src=x><svg/>', 0, 0],
        ['<span><div></span>', 2, 2], // crossing (item 5)
        ['<span><span><div></span></span>', 3, 3], // exact pop of a dirty element
        ['<body></body>', 1, 1],
        ['<html></html>', 1, 1],
        ['<table><td>a</td></table>', 0, 4], // implied row group and row, given back with F
        ['<table><tr><td>a</table>', 4, 4], // crossing keeps the implied row group
        ['<table><col><col></table>', 0, 2], // implied column group, once
        ['<table><tr></tr><tr></tr></table>', 0, 3], // implied row group, once
        ['<table><tbody><td></td><th></th></tbody></table>', 0, 4], // implied row, once
        ['<b><i>a</b>', 2, 2],
        ['<ul><li>a</ul>', 2, 2],
        ['<noscript><b><i>x</i></b></noscript>', 0, 1], // item 7
        ['<svg><g><rect></g></svg>', 3, 3],
        ['<a href="#"><b><span><i>x</span></b></a>', 4, 4],
        ['<p><span><div></span></p>', 3, 3],
        ['<select><option>a<option>b</select>', 3, 3],
    ];

    public function testDepthCounterMatchesTheShapeModel(): void
    {
        $random = new Randomizer(new Mt19937(self::SEED + 5));
        $limits = 0;
        for ($n = 0; $n < 400; $n++) {
            $html = '';
            $d = 0;
            $max = 0;
            $count = $random->getInt(0, 330);
            for ($k = 0; $k < $count; $k++) {
                [$shape, $units, $peak] = self::DEPTH_SHAPES[$random->getInt(0, count(self::DEPTH_SHAPES) - 1)];
                $html .= $shape;
                $max = max($max, $d + $peak);
                $d += $units;
            }
            $html .= '<p data-rq-block="a">x</p>';
            $max = max($max, $d + 1);
            $result = Parser::parse($html);
            if ($max <= Parser::MAX_DEPTH) {
                self::assertTrue($result->valid, "document $n: " . ($result->rule ?? ''));
            } else {
                self::assertSame(['M-26', ErrorCode::Limit], [$result->rule, $result->error], "document $n");
                $limits++;
            }
        }
        // Both results are reached often.
        self::assertGreaterThan(100, $limits);
        self::assertLessThan(300, $limits);
    }

    public function testNoscriptContentIsValidOnlyWhenBalanced(): void
    {
        // [token, name that it opens, name that it closes]
        $tokens = [
            ['<b>', 'b', null], ['<b/>', 'b', null], ['</b>', null, 'b'], ['<i>', 'i', null], ['</i>', null, 'i'],
            ['<img src=x>', null, null], ['<br>', null, null], ['x', null, null], ['<!--c-->', null, null],
            ['</p>', null, 'p'], ['<style>s</b></style>', null, null], ['<table>', 'table', null], ['</table>', null, 'table'],
        ];
        $random = new Randomizer(new Mt19937(self::SEED + 6));
        $valid = 0;
        for ($n = 0; $n < 3000; $n++) {
            $c = '';
            $open = [];
            $balanced = true;
            $count = $random->getInt(0, 8);
            for ($k = 0; $k < $count; $k++) {
                [$token, $opens, $closes] = $tokens[$random->getInt(0, count($tokens) - 1)];
                $c .= $token;
                if ($opens !== null) {
                    $open[] = $opens;
                } elseif ($closes !== null && array_pop($open) !== $closes) {
                    $balanced = false;
                }
            }
            $balanced = $balanced && $open === [];
            $block = '<p data-rq-block="a">x</p>';
            // E3: without an end tag, C is the rest of the input.
            $html = $random->getInt(0, 1) === 0 ? '<noscript>' . $c . '</noscript>' . $block : $block . '<noscript>' . $c;
            $result = Parser::parse($html);
            if ($balanced) {
                self::assertTrue($result->valid, "document $n: " . ($result->rule ?? ''));
                $valid++;
            } else {
                self::assertSame(['M-11', ErrorCode::Structure], [$result->rule, $result->error], "document $n");
            }
        }
        self::assertGreaterThan(300, $valid);
    }

    public function testMutatedFixturesKeepTheInvariants(): void
    {
        $random = new Randomizer(new Mt19937(self::SEED + 2));
        $corpus = Harness::corpus(FixturesTest::fixturesDir());
        self::assertGreaterThan(100, count($corpus));
        for ($n = 0; $n < 1500; $n++) {
            $html = Harness::mutate($random, $corpus[$random->getInt(0, count($corpus) - 1)]);
            self::assertNull(Harness::check($html), "input $n");
        }
    }
}
