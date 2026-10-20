<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup\Tests;

use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;
use Rivqen\Tools\RqpMarkup\Parser;

/**
 * Every golden fixture of fixtures/rqp/markup/ against its expected.json
 * (the same comparison as tools/rqp-markup/diff.mjs, in process).
 */
final class FixturesTest extends TestCase
{
    public static function fixturesDir(): string
    {
        return dirname(__DIR__, 4) . '/fixtures/rqp/markup';
    }

    /**
     * @return iterable<string, array{string}>
     */
    public static function fixtures(): iterable
    {
        $dirs = glob(self::fixturesDir() . '/FX-RQ-MARKUP-*', GLOB_ONLYDIR);
        self::assertIsArray($dirs);
        foreach ($dirs as $dir) {
            yield basename($dir) => [$dir];
        }
    }

    public function testFixtureSetIsComplete(): void
    {
        self::assertGreaterThanOrEqual(117, iterator_count(self::fixtures()));
    }

    #[DataProvider('fixtures')]
    public function testFixture(string $dir): void
    {
        $input = file_get_contents($dir . '/input.html');
        $expected = json_decode((string) file_get_contents($dir . '/expected.json'), true, 16, JSON_THROW_ON_ERROR);
        self::assertIsString($input);
        self::assertSame($expected, Parser::parse($input)->toArray());
    }
}
