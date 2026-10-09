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
            if ($t->type === TokenType::Cdata) {
                // M-12: the first '>' after `<![CDATA[` ends a `]]>` that starts after it.
                $text = substr($html, $t->start, $t->end - $t->start);
                self::assertSame(strlen($text) >= 12 && str_ends_with($text, ']]>'), $t->cdataClosed, bin2hex($html));
            } else {
                self::assertFalse($t->cdataClosed);
            }
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

    /**
     * Every string of up to 6 characters over the comment alphabet, after `<!--`.
     */
    public function testAgreesOnAllShortCommentBodies(): void
    {
        $alphabet = ['-', '!', '<', '>', 'x'];
        $bodies = [''];
        for ($length = 1; $length <= 6; $length++) {
            $next = [];
            foreach ($bodies as $body) {
                if (strlen($body) === $length - 1) {
                    foreach ($alphabet as $ch) {
                        $next[] = $body . $ch;
                    }
                }
            }
            foreach ($next as $body) {
                self::assertSameTokens('<!--' . $body, 'comment');
                self::assertSameTokens('<!--' . $body . '-->', 'comment');
            }
            $bodies = array_merge($bodies, $next);
        }
        self::assertCount(1 + 5 + 25 + 125 + 625 + 3125 + 15625, $bodies);
    }

    /**
     * @return iterable<string, array{string, list<string>}>
     */
    public static function denseContexts(): iterable
    {
        yield 'start tag attributes' => ['<p', [' ', "\t", '/', '=', '"', "'", '>', 'x', 'type', 'data-rq-block', '<', "\r\n"]];
        yield 'end tag' => ['</p', [' ', '/', '=', '"', "'", '>', 'x', "\n"]];
        yield 'script data' => ['<script>', ['<', '!', '-', '>', '/', 'script', 'SCRIPT', 'scrip', 'x', ' ', "\t"]];
        yield 'RCDATA' => ['<title>', ['<', '/', 'title', 'TITLE', 'titl', '>', ' ', 'x', "\r"]];
        yield 'markup declaration' => ['<!', ['D', 'DOCTYPE', '[CDATA[', '[', ']', '-', '>', 'x', ' ', '"', "'", 'PUBLIC', 'system']];
        yield 'processing instruction' => ['<?', ['x', 'xml', 'xml-stylesheet', '?', '>', '-', '_', '1', ' ', '<']];
    }

    /**
     * @param list<string> $alphabet
     */
    #[DataProvider('denseContexts')]
    public function testAgreesOnDenseInputs(string $prefix, array $alphabet): void
    {
        $random = new Randomizer(new Mt19937(crc32($prefix)));
        for ($n = 0; $n < 20000; $n++) {
            $html = $prefix;
            $count = $random->getInt(0, 12);
            for ($i = 0; $i < $count; $i++) {
                $html .= $alphabet[$random->getInt(0, count($alphabet) - 1)];
            }
            self::assertSameTokens($html, "input $n");
        }
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
