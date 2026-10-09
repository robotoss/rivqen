<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup\Tests;

use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;
use Random\Engine\Mt19937;
use Random\Randomizer;
use Rivqen\Tools\RqpMarkup\Token;
use Rivqen\Tools\RqpMarkup\TokenType;
use Rivqen\Tools\RqpMarkup\Tests\Fuzz\ReferenceTokenizer;

/**
 * Differential test: src/Tokenizer.php against the literal reference
 * tokenizer (tests/Fuzz/ReferenceTokenizer.php) on random inputs that are
 * dense in state-changing characters.
 */
final class ReferenceTokenizerTest extends TestCase
{
    private const PIECES = [
        '<', '>', '/', '!', '-', '?', '[', ']', '"', "'", '=', ' ', "\t", "\n", "\r", "\r\n", "\f", "\0",
        'a', 'b', 'x', 'S', '_', '1', "\xC3\xA9", 'script', 'SCRIPT', 'title', 'style', 'textarea',
        'plaintext', 'noscript', 'p', 'data-rq-block', 'DATA-RQ-BLOCK', 'type', 'DOCTYPE', 'doctype',
        'PUBLIC', 'SYSTEM', '[CDATA[', ']]>', '<!--', '-->', '--!>', '<!-', '</', '<!', '<?', 'xml', '&amp;',
        '<script>', '</script>', '<!--<script>', '<title>', '</title', '<p data-rq-block="a">', '/>',
    ];

    /**
     * @return list<array{string, int, int, string, bool, array<string, string>}>
     */
    private static function production(string $html): array
    {
        $out = [];
        foreach (TokenizerTest::tokenize($html) as $t) {
            $out[] = [self::type($t), $t->start, $t->end, $t->name, $t->selfClosing, $t->attributes];
        }
        return $out;
    }

    private static function type(Token $t): string
    {
        return match ($t->type) {
            TokenType::StartTag => 'start',
            TokenType::EndTag => 'end',
            TokenType::Comment => 'comment',
            TokenType::Cdata => 'cdata',
            TokenType::Doctype => 'doctype',
            TokenType::Eof => 'eof',
        };
    }

    private static function assertSameTokens(string $html, string $message): void
    {
        $expected = ReferenceTokenizer::tokenize($html);
        $actual = self::production($html);
        // Known difference (README.md): `<?` that reaches EOF in the
        // processing instruction states gives no token in the current WHATWG
        // text and a comment to EOF in src/Tokenizer.php.
        $last = $actual[count($actual) - 1] ?? null;
        if ($last !== null && count($actual) === count($expected) + 1 && $last[0] === 'comment'
            && $last[2] === strlen($html) && substr($html, $last[1], 2) === '<?') {
            array_pop($actual);
        }
        self::assertSame($expected, $actual, $message . ' ' . bin2hex($html));
    }

    /**
     * @return iterable<string, array{string}>
     */
    public static function examples(): iterable
    {
        yield 'comment start dash then bang' => ['<!---!>x-->'];
        yield 'comment end bang dash' => ['<!-- --!-x-->'];
        yield 'comment less-than bang dash dash' => ['<!--<!--x-->'];
        yield 'doctype with identifiers' => ['<!DOCTYPE html PUBLIC "a>b" \'c\'>x'];
        yield 'doctype system' => ["<!doctype html system 'x' >"];
        yield 'doctype bogus' => ['<!DOCTYPE html FOO "x">y'];
        yield 'CR LF in a tag' => ["<p\r\ndata-rq-block=a\r\n>x</p\r>"];
        yield 'CR in an end tag name of RCDATA' => ["<title>x</title\r>"];
        yield 'processing instruction xml' => ['<?xml version="1.0"?><p>'];
        yield 'processing instruction at EOF' => ['<?target data'];
        yield 'processing instruction bad target' => ['<?1x>'];
        yield 'script double escape' => ['<script><!--<script></script>--></script>'];
    }

    #[DataProvider('examples')]
    public function testAgreesOnExamples(string $html): void
    {
        self::assertSameTokens($html, 'example');
    }

    public function testAgreesOnRandomInputs(): void
    {
        $random = new Randomizer(new Mt19937(20261010));
        for ($n = 0; $n < 100000; $n++) {
            $parts = [];
            $count = $random->getInt(1, 24);
            for ($i = 0; $i < $count; $i++) {
                $parts[] = self::PIECES[$random->getInt(0, count(self::PIECES) - 1)];
            }
            self::assertSameTokens(implode('', $parts), "input $n");
        }
    }
}
