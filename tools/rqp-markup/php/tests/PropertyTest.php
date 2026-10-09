<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup\Tests;

use PHPUnit\Framework\TestCase;
use Random\Engine\Mt19937;
use Random\Randomizer;
use Rivqen\Tools\RqpMarkup\Parser;
use Rivqen\Tools\RqpMarkup\Tests\Fuzz\Harness;

/**
 * Property tests with fixed seeds. Invariants (Harness::check): no error
 * escapes; the result is deterministic; for a valid result the blocks are
 * in document order, inside the input, between a start tag and an end tag,
 * with unique valid ids, sha256 = hash of input[start, end), and the
 * revisions match CONTRACT.md section 4; tokens are in order, inside the
 * input, and fewer than the bytes.
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
