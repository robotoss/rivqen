<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup\Tests;

use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;
use Rivqen\Tools\RqpMarkup\JsonValidator;

/**
 * RFC 8259 rules of markup.md M-25.
 */
final class JsonValidatorTest extends TestCase
{
    /**
     * @return iterable<string, array{string}>
     */
    public static function validTexts(): iterable
    {
        yield 'zero' => ['0'];
        yield 'negative zero' => ['-0'];
        yield 'integer' => ['123'];
        yield 'fraction' => ['-1.50'];
        yield 'exponent' => ['1e5'];
        yield 'exponent with sign' => ['2.5E-3'];
        yield 'exponent with plus' => ['1e+2'];
        yield 'huge number' => ['1e400'];
        yield 'long integer' => [str_repeat('9', 400)];
        yield 'true' => ['true'];
        yield 'false' => ['false'];
        yield 'null' => ['null'];
        yield 'string' => ['"a"'];
        yield 'empty string' => ['""'];
        yield 'escapes' => ['"\\" \\\\ \\/ \\b \\f \\n \\r \\t \\u00e9 \\u0000"'];
        yield 'surrogate pair' => ['"\\ud83d\\ude00"'];
        yield 'surrogate pair upper case' => ['"\\uD83D\\uDE00"'];
        yield 'non-ASCII bytes' => ["\"\u{00E9}\u{1F600}\u{2028}\""];
        yield 'DEL is not a control character in JSON' => ["\"\x7F\""];
        yield 'empty array' => ['[]'];
        yield 'empty object' => ['{}'];
        yield 'empty array with white space' => ["[ \t\n\r]"];
        yield 'empty object with white space' => ["{ \n}"];
        yield 'array' => ['[1, "a", true, null, [], {}]'];
        yield 'object' => ['{"a": 1, "b": [2, {"c": "d"}]}'];
        yield 'duplicate names' => ['{"a":1,"a":2}'];
        yield 'white space around' => [" \t\r\n{\"a\" : [ 1 , 2 ] }\r\n "];
        yield 'nested 64' => [str_repeat('[', 64) . str_repeat(']', 64)];
        yield 'nested 64 objects and arrays' => [str_repeat('{"a":[', 32) . '1' . str_repeat(']}', 32)];
        yield 'depth goes down and up' => ['[' . implode(',', array_fill(0, 3, '[[]]')) . ',' . str_repeat('[', 63) . str_repeat(']', 63) . ']'];
    }

    #[DataProvider('validTexts')]
    public function testValid(string $json): void
    {
        self::assertTrue(JsonValidator::isValid($json));
    }

    /**
     * @return iterable<string, array{string}>
     */
    public static function invalidTexts(): iterable
    {
        yield 'empty' => [''];
        yield 'white space only' => [" \n"];
        yield 'BOM' => ["\xEF\xBB\xBF{}"];
        yield 'no-break space' => ["\u{00A0}{}"];
        yield 'form feed' => ["\x0C{}"];
        yield 'two values' => ['1 2'];
        yield 'trailing comma in array' => ['[1,]'];
        yield 'trailing comma in object' => ['{"a":1,}'];
        yield 'leading comma' => ['[,1]'];
        yield 'missing comma' => ['[1 2]'];
        yield 'unclosed array' => ['[1'];
        yield 'unclosed object' => ['{"a":1'];
        yield 'unclosed empty array' => ['['];
        yield 'unclosed empty object' => ['{'];
        yield 'mismatched close' => ['[1}'];
        yield 'mismatched object close' => ['{"a":1]'];
        yield 'close without open' => [']'];
        yield 'extra close' => ['[]]'];
        yield 'object key not a string' => ['{a:1}'];
        yield 'object number key' => ['{1:1}'];
        yield 'object missing colon' => ['{"a" 1}'];
        yield 'object missing value' => ['{"a":}'];
        yield 'object key at EOF' => ['{"a"'];
        yield 'object key unclosed' => ['{"a'];
        yield 'object after comma at EOF' => ['{"a":1,'];
        yield 'leading zero' => ['01'];
        yield 'negative leading zero' => ['-01'];
        yield 'minus only' => ['-'];
        yield 'plus sign' => ['+1'];
        yield 'dot without fraction' => ['1.'];
        yield 'dot first' => ['.5'];
        yield 'exponent without digits' => ['1e'];
        yield 'exponent sign without digits' => ['1e+'];
        yield 'hex' => ['0x1'];
        yield 'infinity' => ['Infinity'];
        yield 'NaN' => ['NaN'];
        yield 'true truncated' => ['tru'];
        yield 'True' => ['True'];
        yield 'null truncated' => ['nul'];
        yield 'false prefix' => ['falsey'];
        yield 'single quotes' => ["'a'"];
        yield 'unclosed string' => ['"a'];
        yield 'raw control character' => ["\"a\x01\""];
        yield 'raw tab' => ["\"a\tb\""];
        yield 'raw line feed' => ["\"a\nb\""];
        yield 'invalid escape' => ['"\\x"'];
        yield 'escape at EOF' => ['"\\'];
        yield 'short unicode escape' => ['"\\u12"'];
        yield 'bad hex' => ['"\\u12G4"'];
        yield 'lone high surrogate' => ['"\\ud83d"'];
        yield 'lone high surrogate then text' => ['"\\ud83dabcdef"'];
        yield 'high surrogate then non-surrogate' => ['"\\ud83d\\u0041"'];
        yield 'high surrogate then high surrogate' => ['"\\ud83d\\ud83d"'];
        yield 'high surrogate then bad escape' => ['"\\ud83d\\uZZZZ"'];
        yield 'high surrogate then other escape' => ['"\\ud83d\\n"'];
        yield 'lone low surrogate' => ['"\\ude00"'];
        yield 'low surrogate first' => ['"\\udfff\\ud800"'];
        yield 'depth 65 arrays' => [str_repeat('[', 65) . str_repeat(']', 65)];
        yield 'depth 65 objects' => [str_repeat('{"a":', 65) . '1' . str_repeat('}', 65)];
        yield 'depth 65 empty' => [str_repeat('[', 64) . '{}' . str_repeat(']', 64)];
        yield 'comment' => ['[1/*x*/]'];
        yield 'garbage after value' => ['{}x'];
    }

    #[DataProvider('invalidTexts')]
    public function testInvalid(string $json): void
    {
        self::assertFalse(JsonValidator::isValid($json));
    }

    public function testValidatorAgreesWithJsonDecodeOnSimpleTexts(): void
    {
        // json_decode is an independent RFC 8259 parser for texts within its limits.
        $texts = ['{"a":[1,2,{"b":null}]}', '[1e2,-0.5,"\\u00e9"]', '[1,]', '{"a" 1}', '"\\ud800"', '01', '[]', '{}'];
        foreach ($texts as $text) {
            $decoded = json_decode($text, true, 512);
            self::assertSame(json_last_error() === JSON_ERROR_NONE, JsonValidator::isValid($text), $text);
            unset($decoded);
        }
    }

    public function testLargeInputIsLinear(): void
    {
        $json = '[' . implode(',', array_fill(0, 200_000, '{"a":"b\\u00e9","c":[1.5e3,true,null]}')) . ']';
        $start = hrtime(true);
        self::assertTrue(JsonValidator::isValid($json));
        self::assertLessThan(5_000_000_000, hrtime(true) - $start);
    }
}
