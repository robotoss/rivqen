<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup\Tests;

use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\Attributes\Group;
use PHPUnit\Framework\TestCase;
use Rivqen\Tools\RqpMarkup\Parser;

/**
 * Resource limits on adversarial inputs of the maximum size (5 MiB).
 *
 * The bounds are far above the measured values (README.md, "Limits") and
 * far below what a quadratic algorithm needs on these inputs (hours), so
 * they detect a complexity defect without timing noise.
 */
#[Group('limits')]
final class LimitsTest extends TestCase
{
    private const MAX_SECONDS = 30;
    private const MAX_PEAK_BYTES = 256 * 1024 * 1024;

    private static function fill(string $head, string $unit, string $tail = ''): string
    {
        $n = intdiv(Parser::MAX_INPUT_BYTES - strlen($head) - strlen($tail), strlen($unit));
        return $head . str_repeat($unit, $n) . $tail;
    }

    /**
     * @return iterable<string, array{\Closure(): string}>
     */
    public static function inputs(): iterable
    {
        yield 'deep nesting' => [static fn(): string => self::fill('', '<b>')];
        yield 'deep nesting with unique names' => [static function (): string {
            $parts = [];
            $size = 0;
            for ($i = 0; $size < Parser::MAX_INPUT_BYTES - 20; $i++) {
                $part = '<q' . $i . '>';
                $parts[] = $part;
                $size += strlen($part);
            }
            return implode('', $parts);
        }];
        yield 'deep nesting then stray end tags' => [static fn(): string => str_repeat('<b>', 800_000) . str_repeat('</i>', 700_000)];
        yield 'one end tag crosses everything' => [static fn(): string => self::fill('<html>', '<div>', '</html>')];
        yield 'deep foreign region' => [static fn(): string => self::fill('<svg>', '<g>', '</svg>')];
        yield 'deep tables' => [static fn(): string => self::fill('', '<table><tr><td>')];
        yield 'many attributes' => [static fn(): string => self::fill('<p', ' a', '>')];
        yield 'many watched attributes' => [static fn(): string => self::fill('<p', ' type=1', '>')];
        yield 'comment state churn' => [static fn(): string => self::fill('<!--', '<!-')];
        yield 'script escape churn' => [static fn(): string => self::fill('<script><!--', '<script></script>')];
        yield 'RCDATA near end tags' => [static fn(): string => self::fill('<title>', '</titl')];
        yield 'noscript content' => [static fn(): string => self::fill('<noscript>', '<img src=x>', '</noscript>')];
        yield 'less-than flood' => [static fn(): string => self::fill('', '<')];
        yield 'CDATA flood' => [static fn(): string => self::fill('<svg>', '<![CDATA[]]>', '</svg>')];
        yield 'large json block' => [static fn(): string => '<script type="application/json" data-rq-block="j">['
            . str_repeat('1,', 500_000) . '1]</script>'];
    }

    /**
     * @param \Closure(): string $make
     */
    #[DataProvider('inputs')]
    public function testLinearTimeAndBoundedMemory(\Closure $make): void
    {
        $input = $make();
        self::assertLessThanOrEqual(Parser::MAX_INPUT_BYTES, strlen($input));
        gc_collect_cycles();
        memory_reset_peak_usage();
        $base = memory_get_usage();
        $start = hrtime(true);
        $result = Parser::parse($input);
        $seconds = (hrtime(true) - $start) / 1e9;
        $peak = memory_get_peak_usage() - $base;
        self::assertTrue($result->valid, (string) $result->rule);
        self::assertLessThan(self::MAX_SECONDS, $seconds);
        self::assertLessThan(self::MAX_PEAK_BYTES, $peak);
    }
}
