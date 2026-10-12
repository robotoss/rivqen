<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup;

/**
 * A stack of byte offsets, packed in one binary string: 4 bytes for each
 * entry (unsigned 32-bit, little-endian).
 *
 * Used for the stack of the `noscript` content C (M-11 item 5). That stack
 * is not bounded by the depth counter D (M-26 item 7), only by the size of
 * C: up to about 1.7 million entries for 5 MiB. A PHP list of names costs
 * a zval (16 bytes) and often a new string (32 bytes or more) for each
 * entry; this stack costs 4 bytes for each entry and at most the same again
 * as free capacity.
 *
 * Growth: the capacity doubles when it is full, so the string is copied
 * O(log n) times. Entries are written in place (string offset assignment on
 * a string with one reference), never with a copy of the string.
 */
final class OffsetStack
{
    /** First capacity in bytes (1024 entries). */
    private const MIN_CAPACITY = 4096;
    /** Largest value that fits in 4 bytes. */
    public const MAX_VALUE = 0xFFFF_FFFF;

    /** Entries, then free capacity. */
    private string $bytes = '';
    private int $count = 0;

    /**
     * @param int $value 0 to MAX_VALUE. The parser pushes offsets into an
     *        input of at most 5 MiB plus 11 bytes (M-01), so a value out of
     *        range is a defect of the caller, not an input error.
     *
     * @throws \InvalidArgumentException When $value is out of range.
     */
    public function push(int $value): void
    {
        if ($value < 0 || $value > self::MAX_VALUE) {
            throw new \InvalidArgumentException('value does not fit in 4 bytes');
        }
        $at = $this->count * 4;
        if ($at === strlen($this->bytes)) {
            $this->bytes .= str_repeat("\0", max(self::MIN_CAPACITY, $at));
        }
        $this->bytes[$at] = chr($value & 0xFF);
        $this->bytes[$at + 1] = chr(($value >> 8) & 0xFF);
        $this->bytes[$at + 2] = chr(($value >> 16) & 0xFF);
        $this->bytes[$at + 3] = chr($value >> 24);
        $this->count++;
    }

    /** Remove the top entry and return it, or return null when the stack is empty. */
    public function pop(): ?int
    {
        if ($this->count === 0) {
            return null;
        }
        $this->count--;
        $at = $this->count * 4;
        $b = $this->bytes;
        return ord($b[$at]) | (ord($b[$at + 1]) << 8) | (ord($b[$at + 2]) << 16) | (ord($b[$at + 3]) << 24);
    }

    public function count(): int
    {
        return $this->count;
    }

    /** Bytes that the stack holds (entries and free capacity). */
    public function capacityBytes(): int
    {
        return strlen($this->bytes);
    }
}
