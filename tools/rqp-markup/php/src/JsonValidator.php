<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup;

/**
 * RFC 8259 JSON text validator for markup.md M-25.
 *
 * - Iterative: no recursion; the open arrays and objects are a string of at
 *   most MAX_DEPTH bytes.
 * - Builds no value, so memory does not grow with the input.
 * - The input is valid UTF-8 already (M-02), so only ASCII bytes are checked
 *   in strings; other bytes are accepted as they are.
 */
final class JsonValidator
{
    public const MAX_DEPTH = 64;

    private const WS = " \t\n\r";
    private const DIGITS = '0123456789';
    private const HEX = '0123456789abcdefABCDEF';
    /** Bytes that end a run of plain string characters: '"', '\' and U+0000 to U+001F. */
    private const STRING_STOP = "\"\\\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f"
        . "\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f";

    private function __construct() {}

    public static function isValid(string $s): bool
    {
        $len = strlen($s);
        $i = strspn($s, self::WS);
        $open = ''; // '[' or '{' for each open container
        $expectValue = true;

        while (true) {
            if ($expectValue) {
                if ($i >= $len) {
                    return false;
                }
                $c = $s[$i];
                if ($c === '[' || $c === '{') {
                    if (strlen($open) >= self::MAX_DEPTH) {
                        return false;
                    }
                    $open .= $c;
                    $i++;
                    $i += strspn($s, self::WS, $i);
                    $close = $c === '[' ? ']' : '}';
                    if ($i < $len && $s[$i] === $close) {
                        $open = substr($open, 0, -1);
                        $i++;
                        $expectValue = false;
                    } elseif ($c === '{') {
                        $i = self::memberName($s, $i);
                        if ($i < 0) {
                            return false;
                        }
                    }
                    continue;
                }
                $i = match (true) {
                    $c === '"' => self::string($s, $i),
                    $c === '-' || str_contains(self::DIGITS, $c) => self::number($s, $i),
                    $c === 't' => self::literal($s, $i, 'true'),
                    $c === 'f' => self::literal($s, $i, 'false'),
                    $c === 'n' => self::literal($s, $i, 'null'),
                    default => -1,
                };
                if ($i < 0) {
                    return false;
                }
                $expectValue = false;
                continue;
            }

            // After a value.
            $i += strspn($s, self::WS, $i);
            if ($open === '') {
                return $i === $len;
            }
            if ($i >= $len) {
                return false;
            }
            $c = $s[$i];
            $inObject = $open[strlen($open) - 1] === '{';
            if ($c === ',') {
                $i++;
                $i += strspn($s, self::WS, $i);
                if ($inObject) {
                    $i = self::memberName($s, $i);
                    if ($i < 0) {
                        return false;
                    }
                }
                $expectValue = true;
                continue;
            }
            if ($c === ($inObject ? '}' : ']')) {
                $open = substr($open, 0, -1);
                $i++;
                continue;
            }
            return false;
        }
    }

    /**
     * A member name, white space, ':' and white space. Return the offset of
     * the value, or -1.
     */
    private static function memberName(string $s, int $i): int
    {
        if ($i >= strlen($s) || $s[$i] !== '"') {
            return -1;
        }
        $i = self::string($s, $i);
        if ($i < 0) {
            return -1;
        }
        $i += strspn($s, self::WS, $i);
        if ($i >= strlen($s) || $s[$i] !== ':') {
            return -1;
        }
        $i++;
        return $i + strspn($s, self::WS, $i);
    }

    /** A string that starts at $i (the '"'). Return the offset after it, or -1. */
    private static function string(string $s, int $i): int
    {
        $len = strlen($s);
        $i++;
        while (true) {
            $i += strcspn($s, self::STRING_STOP, $i);
            if ($i >= $len) {
                return -1;
            }
            $c = $s[$i];
            if ($c === '"') {
                return $i + 1;
            }
            if ($c !== '\\') {
                return -1; // unescaped control character
            }
            $e = $s[$i + 1] ?? '';
            if ($e === 'u') {
                $unit = self::hex4($s, $i + 2);
                if ($unit < 0) {
                    return -1;
                }
                $i += 6;
                if ($unit >= 0xDC00 && $unit <= 0xDFFF) {
                    return -1; // lone low surrogate
                }
                if ($unit >= 0xD800 && $unit <= 0xDBFF) {
                    // A high surrogate must be followed by a low surrogate escape.
                    if (substr($s, $i, 2) !== '\\u') {
                        return -1;
                    }
                    $low = self::hex4($s, $i + 2);
                    if ($low < 0xDC00 || $low > 0xDFFF) {
                        return -1;
                    }
                    $i += 6;
                }
                continue;
            }
            if ($e === '' || !str_contains('"\\/bfnrt', $e)) {
                return -1;
            }
            $i += 2;
        }
    }

    /** Four hex digits at $i as an integer, or -1. */
    private static function hex4(string $s, int $i): int
    {
        if ($i + 4 > strlen($s) || strspn($s, self::HEX, $i, 4) !== 4) {
            return -1;
        }
        return (int) hexdec(substr($s, $i, 4));
    }

    /** A number that starts at $i. Return the offset after it, or -1. */
    private static function number(string $s, int $i): int
    {
        if ($s[$i] === '-') {
            $i++;
        }
        $int = strspn($s, self::DIGITS, $i);
        if ($int === 0 || ($int > 1 && $s[$i] === '0')) {
            return -1;
        }
        $i += $int;
        if (($s[$i] ?? '') === '.') {
            $frac = strspn($s, self::DIGITS, $i + 1);
            if ($frac === 0) {
                return -1;
            }
            $i += 1 + $frac;
        }
        $e = $s[$i] ?? '';
        if ($e === 'e' || $e === 'E') {
            $i++;
            $sign = $s[$i] ?? '';
            if ($sign === '+' || $sign === '-') {
                $i++;
            }
            $exp = strspn($s, self::DIGITS, $i);
            if ($exp === 0) {
                return -1;
            }
            $i += $exp;
        }
        return $i;
    }

    private static function literal(string $s, int $i, string $word): int
    {
        return substr($s, $i, strlen($word)) === $word ? $i + strlen($word) : -1;
    }
}
