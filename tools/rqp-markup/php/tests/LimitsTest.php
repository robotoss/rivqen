<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup\Tests;

use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\Attributes\Group;
use PHPUnit\Framework\TestCase;
use Rivqen\Tools\RqpMarkup\ErrorCode;
use Rivqen\Tools\RqpMarkup\Parser;
use Rivqen\Tools\RqpMarkup\Result;

/**
 * Resource limits on adversarial inputs of the maximum size (5 MiB).
 *
 * The bounds are far above the measured values (README.md, "Limits") and
 * far below what a quadratic algorithm needs on these inputs, so they detect
 * a complexity defect without timing noise.
 *
 * Inputs deeper than the token stack limit (M-26, 256 elements) must stop
 * at the 257th push. The valid inputs give the maximum stack work within
 * that limit: stacks of 256 elements that are opened and closed again and
 * again until the input has 5 MiB.
 */
#[Group('limits')]
final class LimitsTest extends TestCase
{
    private const MAX_SECONDS = 30;
    private const MAX_LIMIT_SECONDS = 2;
    private const MAX_PEAK_BYTES = 256 * 1024 * 1024;
    private const DEPTH = Parser::MAX_OPEN_ELEMENTS;

    /**
     * Two-byte units with the same PHP string hash (DJBX33A, no seed):
     * 33 * ord('a') + ord('~') === 33 * ord('b') + ord(']'). All names
     * made of 17 of these units have the same hash.
     */
    private const COLLIDING = ['a~', 'b]'];
    private const UNITS = 17;

    private static function fill(string $head, string $unit, string $tail = ''): string
    {
        $n = intdiv(Parser::MAX_INPUT_BYTES - strlen($head) - strlen($tail), strlen($unit));
        return $head . str_repeat($unit, $n) . $tail;
    }

    /**
     * Name number $i (0 <= $i < 2^17) made of 17 units.
     *
     * @param array{string, string} $units
     */
    private static function tagName(int $i, array $units): string
    {
        $name = '';
        for ($bit = self::UNITS - 1; $bit >= 0; $bit--) {
            $name .= $units[($i >> $bit) & 1];
        }
        return $name;
    }

    /**
     * Groups of $depth nested elements with names from $units, each group
     * closed by matching end tags in reverse order, repeated while the
     * result has at most $max bytes. Each group uses new names.
     *
     * @param array{string, string} $units
     */
    private static function nameStacks(array $units, int $depth, int $max): string
    {
        $parts = [];
        $size = 0;
        $i = 0;
        while (true) {
            $open = [];
            $close = [];
            for ($k = 0; $k < $depth; $k++) {
                $name = self::tagName($i++ % (1 << self::UNITS), $units);
                $open[] = '<' . $name . '>';
                $close[] = '</' . $name . '>';
            }
            $group = implode('', $open) . implode('', array_reverse($close));
            if ($size + strlen($group) > $max) {
                return implode('', $parts);
            }
            $parts[] = $group;
            $size += strlen($group);
        }
    }

    /**
     * 256 nested elements with names from $units, then end tags of other
     * names from $units that are not open (M-05 step 3 ignores them).
     *
     * @param array{string, string} $units
     */
    private static function strayEndTags(array $units): string
    {
        $parts = [];
        for ($i = 0; $i < self::DEPTH; $i++) {
            $parts[] = '<' . self::tagName($i, $units) . '>';
        }
        $size = strlen(implode('', $parts));
        for ($i = self::DEPTH; ; $i++) {
            $tag = '</' . self::tagName(self::DEPTH + $i % ((1 << self::UNITS) - self::DEPTH), $units) . '>';
            if ($size + strlen($tag) > Parser::MAX_INPUT_BYTES) {
                return implode('', $parts);
            }
            $parts[] = $tag;
            $size += strlen($tag);
        }
    }

    /**
     * Five html blocks of almost 1 MiB (M-26) whose content repeats $group.
     */
    private static function blocks(string $group): string
    {
        $maxContent = intdiv(Parser::MAX_INPUT_BYTES, 5) - 64;
        $content = str_repeat($group, intdiv($maxContent, strlen($group)));
        $html = '';
        for ($b = 0; $b < 5; $b++) {
            $html .= '<div data-rq-block="b' . $b . '">' . $content . '</div>';
        }
        return $html;
    }

    /**
     * Valid inputs with the maximum work within the limits.
     *
     * @return iterable<string, array{\Closure(): string}>
     */
    public static function inputs(): iterable
    {
        $d = self::DEPTH;
        yield 'stacks of 256 b' => [static fn(): string => self::fill('', str_repeat('<b>', $d) . str_repeat('</b>', $d))];
        yield 'stacks of 256 different names' => [static function () use ($d): string {
            $parts = [];
            $size = 0;
            for ($i = 0; ; $i += $d) {
                $open = [];
                $close = [];
                for ($k = 0; $k < $d; $k++) {
                    $open[] = '<q' . ($i + $k) . '>';
                    $close[] = '</q' . ($i + $k) . '>';
                }
                $group = implode('', $open) . implode('', array_reverse($close));
                if ($size + strlen($group) > Parser::MAX_INPUT_BYTES) {
                    return implode('', $parts);
                }
                $parts[] = $group;
                $size += strlen($group);
            }
        }];
        yield 'stray end tags at depth 256' => [static fn(): string => self::fill(str_repeat('<b>', $d), '</i>')];
        yield 'one end tag crosses 255 elements, repeated' => [static fn(): string => self::fill('', '<html>' . str_repeat('<div>', $d - 1) . '</html>')];
        yield 'foreign region of depth 256, repeated' => [static fn(): string => self::fill('<svg>', str_repeat('<g>', $d - 1) . str_repeat('</g>', $d - 1), '</svg>')];
        yield 'tables of depth 255, repeated' => [static fn(): string => self::fill('', str_repeat('<table><tr><td>', 85) . str_repeat('</table>', 85))];
        // M-24 item j keeps one index stack of the lists and list items in
        // the content: the block element and 254 list elements.
        yield 'nested lists of depth 255 in blocks' => [static fn(): string => self::blocks(str_repeat('<ul><li>', 127) . str_repeat('</li></ul>', 127))];
        // Hash flooding: all names have the same PHP string hash.
        yield 'names with one hash in stacks of 256' => [static fn(): string => self::nameStacks(self::COLLIDING, $d, Parser::MAX_INPUT_BYTES)];
        yield 'end tags of names with one hash that are not open' => [static fn(): string => self::strayEndTags(self::COLLIDING)];
        yield 'names with one hash in blocks' => [static fn(): string => self::blocks(self::nameStacks(self::COLLIDING, $d - 1, 64 * 1024))];
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
     * Inputs deeper than the token stack limit (M-26).
     *
     * @return iterable<string, array{\Closure(): string}>
     */
    public static function deepInputs(): iterable
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
        yield 'deep nesting then matching end tags' => [static fn(): string => str_repeat('<b>', 740_000) . str_repeat('</b>', 740_000)];
        yield 'deep foreign region' => [static fn(): string => self::fill('<svg>', '<g>', '</svg>')];
        yield 'deep tables' => [static fn(): string => self::fill('', '<table><tr><td>')];
        yield 'deep nested lists in a block' => [static fn(): string => '<div data-rq-block="a">'
            . str_repeat('<ul><li>', 58_000) . str_repeat('</li></ul>', 58_000) . '</div>'];
        yield 'nested names with one hash' => [static function (): string {
            $parts = [];
            for ($i = 0; $i < 1 << self::UNITS; $i++) {
                $parts[] = '<' . self::tagName($i, self::COLLIDING) . '>';
            }
            return implode('', $parts);
        }];
        yield 'nested names with one hash, closed in reverse order' => [static fn(): string => self::nameStacks(self::COLLIDING, 71_000, Parser::MAX_INPUT_BYTES)];
        yield 'nested names with one hash in a block' => [static fn(): string => '<div data-rq-block="a">'
            . self::nameStacks(self::COLLIDING, 14_000, Parser::MAX_BLOCK_BYTES - 64) . '</div>'];
    }

    /**
     * @param \Closure(): string $make
     */
    #[DataProvider('inputs')]
    public function testLinearTimeAndBoundedMemory(\Closure $make): void
    {
        $input = $make();
        self::assertGreaterThan(1_000_000, strlen($input));
        [$result, $seconds, $peak] = self::measure($input);
        self::assertTrue($result->valid, (string) $result->rule);
        self::assertLessThan(self::MAX_SECONDS, $seconds);
        self::assertLessThan(self::MAX_PEAK_BYTES, $peak);
    }

    /**
     * @param \Closure(): string $make
     */
    #[DataProvider('deepInputs')]
    public function testTooDeepInputStopsAtTheDepthLimit(\Closure $make): void
    {
        $input = $make();
        self::assertGreaterThan(1_000_000, strlen($input));
        [$result, $seconds, $peak] = self::measure($input);
        self::assertSame(ErrorCode::Limit, $result->error);
        self::assertSame('M-26', $result->rule);
        self::assertLessThan(self::MAX_LIMIT_SECONDS, $seconds);
        self::assertLessThan(self::MAX_PEAK_BYTES, $peak);
    }

    /**
     * @return array{Result, float, int}
     */
    private static function measure(string $input): array
    {
        self::assertLessThanOrEqual(Parser::MAX_INPUT_BYTES, strlen($input));
        gc_collect_cycles();
        memory_reset_peak_usage();
        $base = memory_get_usage();
        $start = hrtime(true);
        $result = Parser::parse($input);
        $seconds = (hrtime(true) - $start) / 1e9;
        return [$result, $seconds, memory_get_peak_usage() - $base];
    }
}
