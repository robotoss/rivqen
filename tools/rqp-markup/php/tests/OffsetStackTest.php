<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup\Tests;

use PHPUnit\Framework\TestCase;
use Random\Engine\Mt19937;
use Random\Randomizer;
use Rivqen\Tools\RqpMarkup\OffsetStack;

/**
 * The packed stack of the noscript content (M-11 item 5): LIFO order,
 * exact values in the full 32-bit range, and 4 bytes for each entry.
 */
final class OffsetStackTest extends TestCase
{
    public function testEmptyStackPopsNull(): void
    {
        $stack = new OffsetStack();
        self::assertNull($stack->pop());
        self::assertSame(0, $stack->count());
    }

    public function testValuesComeBackInReverseOrderAcrossGrowth(): void
    {
        // More entries than the first capacity (1024), and values with
        // each of the 4 bytes set.
        $values = [0, 1, 0xFF, 0x100, 0xFFFF, 0x1_0000, 0xFF_FFFF, 0x100_0000, 0x7FFF_FFFF, 0x8000_0000, OffsetStack::MAX_VALUE];
        for ($i = 0; count($values) < 5000; $i++) {
            $values[] = ($i * 2_654_435_761) & OffsetStack::MAX_VALUE;
        }
        $stack = new OffsetStack();
        foreach ($values as $value) {
            $stack->push($value);
        }
        self::assertSame(count($values), $stack->count());
        $popped = [];
        while (($value = $stack->pop()) !== null) {
            $popped[] = $value;
        }
        self::assertSame(array_reverse($values), $popped);
        self::assertSame(0, $stack->count());
    }

    public function testPushAfterPopReplacesTheEntry(): void
    {
        $stack = new OffsetStack();
        $stack->push(7);
        $stack->push(0xABCD_EF01);
        self::assertSame(0xABCD_EF01, $stack->pop());
        $stack->push(9);
        self::assertSame(2, $stack->count());
        self::assertSame(9, $stack->pop());
        self::assertSame(7, $stack->pop());
        self::assertNull($stack->pop());
    }

    public function testRandomOperationsMatchAList(): void
    {
        $random = new Randomizer(new Mt19937(20261011));
        $stack = new OffsetStack();
        $model = [];
        for ($n = 0; $n < 20_000; $n++) {
            if ($random->getInt(0, 2) > 0) {
                $value = $random->getInt(0, OffsetStack::MAX_VALUE);
                $stack->push($value);
                $model[] = $value;
            } else {
                self::assertSame(array_pop($model), $stack->pop());
            }
            self::assertSame(count($model), $stack->count());
        }
    }

    public function testCapacityIsAtMostTwiceTheEntries(): void
    {
        $stack = new OffsetStack();
        for ($n = 1; $n <= 100_000; $n++) {
            $stack->push($n);
            if ($n % 997 === 0 || $n === 100_000) {
                self::assertGreaterThanOrEqual(4 * $n, $stack->capacityBytes());
                self::assertLessThanOrEqual(max(4096, 8 * $n), $stack->capacityBytes());
            }
        }
    }

    public function testValueOutOfRangeIsRejected(): void
    {
        $stack = new OffsetStack();
        $stack->push(OffsetStack::MAX_VALUE);
        foreach ([-1, OffsetStack::MAX_VALUE + 1] as $value) {
            try {
                $stack->push($value);
                self::fail('no exception for ' . $value);
            } catch (\InvalidArgumentException) {
                self::assertSame(1, $stack->count());
            }
        }
        self::assertSame(OffsetStack::MAX_VALUE, $stack->pop());
    }
}
