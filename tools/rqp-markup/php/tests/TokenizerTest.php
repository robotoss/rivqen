<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup\Tests;

use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;
use Rivqen\Tools\RqpMarkup\ContentState;
use Rivqen\Tools\RqpMarkup\Names;
use Rivqen\Tools\RqpMarkup\Token;
use Rivqen\Tools\RqpMarkup\Tokenizer;
use Rivqen\Tools\RqpMarkup\TokenType;

/**
 * Tokenizer state tests (WHATWG HTML 13.2.5). Each token is written as a
 * compact string: "S:name@start-end", "E:name@start-end", "C@start-end",
 * "D@start-end", "X@start-end" (CDATA, closed) or "X!@start-end" (not
 * closed). A self-closing start tag gets "/" after the name. EOF is not listed.
 */
final class TokenizerTest extends TestCase
{
    /**
     * Tokenize with the M-04 state changes of HTML context.
     *
     * @return list<Token>
     */
    public static function tokenize(string $html, int $start = 0): array
    {
        $tokenizer = new Tokenizer($html, $start);
        $tokens = [];
        $limit = strlen($html) + 2;
        for ($n = 0; $n < $limit; $n++) {
            $token = $tokenizer->next();
            if ($token->type === TokenType::Eof) {
                return $tokens;
            }
            $tokens[] = $token;
            $state = $token->type === TokenType::StartTag ? (Names::TEXT[$token->name] ?? null) : null;
            if ($state !== null) {
                $tokenizer->switchTo($state, $token->name);
            }
        }
        self::fail('the tokenizer did not reach EOF');
    }

    /**
     * @return list<string>
     */
    private static function compact(string $html, int $start = 0): array
    {
        return array_map(static function (Token $t): string {
            $span = '@' . $t->start . '-' . $t->end;
            return match ($t->type) {
                TokenType::StartTag => 'S:' . $t->name . ($t->selfClosing ? '/' : '') . $span,
                TokenType::EndTag => 'E:' . $t->name . $span,
                TokenType::Comment => 'C' . $span,
                TokenType::Doctype => 'D' . $span,
                TokenType::Cdata => ($t->cdataClosed ? 'X' : 'X!') . $span,
                TokenType::Eof => 'EOF',
            };
        }, self::tokenize($html, $start));
    }

    /**
     * @return iterable<string, array{string, list<string>}>
     */
    public static function tokenCases(): iterable
    {
        // Data state and tag open state.
        yield 'text only' => ['hello world', []];
        yield 'empty input' => ['', []];
        yield 'less-than before a non-letter is text' => ['a < b <3 <-', []];
        yield 'less-than at EOF' => ['abc<', []];
        yield 'end tag open at EOF' => ['abc</', []];
        yield 'missing end tag name is dropped' => ['</><b>', ['S:b@3-6']];
        yield 'start tag' => ['x<div>y', ['S:div@1-6']];
        yield 'tag name is lower-cased' => ['<DiV></DIV>', ['S:div@0-5', 'E:div@5-11']];
        yield 'tag name ends at white space' => ["<p\tclass=a>", ['S:p@0-11']];
        yield 'CR is white space' => ["<p\r\nid=a\r>", ['S:p@0-10']];
        yield 'less-than inside a tag name is part of the name' => ['<a<b>', ['S:a<b@0-5']];
        // Attributes.
        yield 'greater-than inside double quotes' => ['<a title="x>y">', ['S:a@0-15']];
        yield 'greater-than inside single quotes' => ["<a title='x>y'>", ['S:a@0-15']];
        yield 'unquoted value ends at greater-than' => ['<a href=x>y', ['S:a@0-10']];
        yield 'quote inside an unquoted value' => ['<a b=x"y>', ['S:a@0-9']];
        yield 'missing value before greater-than' => ['<a b=>', ['S:a@0-6']];
        yield 'missing white space between attributes' => ['<a b="1"c="2">', ['S:a@0-14']];
        yield 'equals sign starts an attribute name' => ['<a =b>', ['S:a@0-6']];
        yield 'end tag with attributes' => ['<b></b class="x>y">', ['S:b@0-3', 'E:b@3-19']];
        // Self-closing start tag state.
        yield 'self-closing flag' => ['<br/>', ['S:br/@0-5']];
        yield 'self-closing after an attribute' => ['<div a="1"/>', ['S:div/@0-12']];
        yield 'solidus not followed by greater-than' => ['<br/ >', ['S:br@0-6']];
        yield 'solidus in an unquoted value' => ['<a href=x/>', ['S:a@0-11']];
        yield 'solidus then attribute' => ['<a / b>', ['S:a@0-7']];
        // EOF in a tag: the tag is dropped.
        yield 'EOF in tag name' => ['<div', []];
        yield 'EOF in attribute value' => ['<div class="a>', []];
        yield 'EOF after attribute name' => ['<div class', []];
        yield 'EOF after self-closing solidus' => ['<div/', []];
        yield 'EOF before attribute value' => ['<div a= ', []];
        yield 'EOF after quoted value' => ['<div a="1"', []];
        // Comments.
        yield 'comment' => ['<!-- a -->x', ['C@0-10']];
        yield 'abrupt empty comment' => ['<!-->x', ['C@0-5']];
        yield 'abrupt empty comment with dash' => ['<!--->x', ['C@0-6']];
        yield 'comment end bang' => ['<!-- a --!>x', ['C@0-11']];
        yield 'comment end bang continues' => ['<!-- --!x -->', ['C@0-13']];
        yield 'comment end bang dash' => ['<!-- --!-->', ['C@0-11']];
        yield 'double dash inside a comment' => ['<!-- a -- b -->', ['C@0-15']];
        yield 'dashes before the end' => ['<!-- a --->', ['C@0-11']];
        yield 'single dash after start' => ['<!---a-->', ['C@0-9']];
        yield 'nested comment start' => ['<!-- <!-- -->', ['C@0-13']];
        yield 'less-than bang dash dash greater-than' => ['<!--<!-->', ['C@0-9']];
        yield 'less-than bang dash then other' => ['<!--<!-x-->', ['C@0-11']];
        yield 'less-than less-than in comment' => ['<!--<<!---->', ['C@0-12']];
        yield 'tag inside a comment is text' => ['<!-- <p data-rq-block="a"> -->', ['C@0-30']];
        yield 'comment at EOF' => ['<!-- a', ['C@0-6']];
        yield 'comment start at EOF' => ['<!--', ['C@0-4']];
        yield 'comment start dash at EOF' => ['<!---', ['C@0-5']];
        yield 'comment end dash at EOF' => ['<!-- -', ['C@0-6']];
        yield 'comment end at EOF' => ['<!-- --', ['C@0-7']];
        yield 'comment end bang at EOF' => ['<!-- --!', ['C@0-8']];
        // Bogus comments and processing instructions.
        yield 'bogus comment' => ['<!x>y', ['C@0-4']];
        yield 'empty bogus comment' => ['<!>', ['C@0-3']];
        yield 'one dash is a bogus comment' => ['<!-x>', ['C@0-5']];
        yield 'end tag open with space' => ['</ x>y', ['C@0-5']];
        yield 'end tag open with digit' => ['</3>', ['C@0-4']];
        yield 'bogus comment ignores quotes' => ['<!x "a>b">', ['C@0-7']];
        yield 'bogus comment at EOF' => ['<!x', ['C@0-3']];
        yield 'processing instruction' => ['<?xml version="1.0"?>x', ['C@0-21']];
        yield 'processing instruction ends at first greater-than' => ['<?pi a>b?>', ['C@0-7']];
        yield 'empty processing instruction' => ['<?>', ['C@0-3']];
        yield 'processing instruction at EOF' => ['<?pi', ['C@0-4']];
        // DOCTYPE.
        yield 'doctype' => ['<!DOCTYPE html>x', ['D@0-15']];
        yield 'doctype is case-insensitive' => ['<!doctype html>', ['D@0-15']];
        yield 'doctype ends at greater-than in quotes' => ['<!DOCTYPE html PUBLIC "a>b">', ['D@0-25']];
        yield 'empty doctype' => ['<!DOCTYPE>', ['D@0-10']];
        yield 'doctype at EOF' => ['<!DOCTYPE html', ['D@0-14']];
        // CDATA (M-12).
        yield 'CDATA closed' => ['<![CDATA[x]]>y', ['X@0-13']];
        yield 'CDATA empty' => ['<![CDATA[]]>', ['X@0-12']];
        yield 'CDATA with greater-than inside' => ['<![CDATA[a>b]]>', ['X!@0-11']];
        yield 'CDATA greater-than right after the start' => ['<![CDATA[>', ['X!@0-10']];
        yield 'CDATA brackets overlap the start' => ['<![CDATA[]>', ['X!@0-11']];
        yield 'CDATA at EOF' => ['<![CDATA[x', ['X!@0-10']];
        yield 'CDATA is case-sensitive' => ['<![cdata[x]]>', ['C@0-13']];
        // RCDATA (title, textarea).
        yield 'title content is text' => ['<title>a<b>c</title>', ['S:title@0-7', 'E:title@12-20']];
        yield 'title end tag is case-insensitive' => ['<title>a</TITLE >', ['S:title@0-7', 'E:title@8-17']];
        yield 'title end tag needs a terminator' => ['<title>a</titlex></title>', ['S:title@0-7', 'E:title@17-25']];
        yield 'title end tag with attributes' => ["<title>a</title x='>'>", ['S:title@0-7', 'E:title@8-22']];
        yield 'title end tag with solidus' => ['<title>a</title/>', ['S:title@0-7', 'E:title@8-17']];
        yield 'title end tag at EOF' => ['<title>a</title', ['S:title@0-7']];
        yield 'title end tag in an unclosed tag at EOF' => ['<title>a</title x="', ['S:title@0-7']];
        yield 'title without end tag' => ['<title>a<p>', ['S:title@0-7']];
        yield 'textarea content is text' => ['<textarea></textarea x><p>', ['S:textarea@0-10', 'E:textarea@10-23', 'S:p@23-26']];
        yield 'self-closing title is still RCDATA' => ['<title/><p></title>', ['S:title/@0-8', 'E:title@11-19']];
        yield 'less-than slash letters in RCDATA' => ['<title></a</title>', ['S:title@0-7', 'E:title@10-18']];
        // RAWTEXT.
        yield 'style content is text' => ['<style><!--</style>-->', ['S:style@0-7', 'E:style@11-19']];
        yield 'xmp content is text' => ['<xmp><p></xmp>', ['S:xmp@0-5', 'E:xmp@8-14']];
        yield 'noscript content is text' => ['<noscript><p></noscript>', ['S:noscript@0-10', 'E:noscript@13-24']];
        // Script data.
        yield 'script content is text' => ['<script>a<b>c</script>', ['S:script@0-8', 'E:script@13-22']];
        yield 'script end tag needs exact name' => ['<script></scripts></script>', ['S:script@0-8', 'E:script@18-27']];
        yield 'script escaped end tag ends the script' => ['<script><!-- </script> -->', ['S:script@0-8', 'E:script@13-22']];
        yield 'script double escaped' => [
            '<script><!--<script></script></script>',
            ['S:script@0-8', 'E:script@29-38'],
        ];
        yield 'script double escaped with upper case' => [
            '<script><!--<SCRIPT></SCRIPT></script>',
            ['S:script@0-8', 'E:script@29-38'],
        ];
        yield 'script double escape ends at dash dash greater-than' => [
            '<script><!--<script>--></script>',
            ['S:script@0-8', 'E:script@23-32'],
        ];
        yield 'script double escape needs the name script' => [
            '<script><!--<scripts></script>',
            ['S:script@0-8', 'E:script@21-30'],
        ];
        yield 'script double escape start reconsumes' => [
            '<script><!--<s-></script>',
            ['S:script@0-8', 'E:script@16-25'],
        ];
        yield 'script double escaped dash' => [
            '<script><!--<script>a-b-<</script>x</script>',
            ['S:script@0-8', 'E:script@35-44'],
        ];
        yield 'script double escaped dash less-than' => [
            '<script><!--<script>-</script>x</script>',
            ['S:script@0-8', 'E:script@31-40'],
        ];
        yield 'script double escaped dash dash less-than' => [
            '<script><!--<script>--</script>x</script>',
            ['S:script@0-8', 'E:script@32-41'],
        ];
        yield 'script double escaped dash dash dash' => [
            '<script><!--<script>---x</script></script>',
            ['S:script@0-8', 'E:script@33-42'],
        ];
        yield 'script double escaped less-than other' => [
            '<script><!--<script><a</script></script>',
            ['S:script@0-8', 'E:script@31-40'],
        ];
        yield 'script double escape end with other name' => [
            '<script><!--<script></b></script>x</script>',
            ['S:script@0-8', 'E:script@34-43'],
        ];
        yield 'script double escape end reconsumes' => [
            '<script><!--<script></s-</script>x</script>',
            ['S:script@0-8', 'E:script@34-43'],
        ];
        yield 'script escaped dash dash greater-than leaves the escape' => [
            '<script><!--x--><script></script>',
            ['S:script@0-8', 'E:script@24-33'],
        ];
        yield 'script short escape' => ['<script><!--></script>', ['S:script@0-8', 'E:script@13-22']];
        yield 'script escape start then other' => ['<script><!x</script>', ['S:script@0-8', 'E:script@11-20']];
        yield 'script escape start dash then other' => ['<script><!-x</script>', ['S:script@0-8', 'E:script@12-21']];
        yield 'script escaped dash then other' => ['<script><!--a-b</script>', ['S:script@0-8', 'E:script@15-24']];
        yield 'script escaped dash less-than' => ['<script><!--a-</script>', ['S:script@0-8', 'E:script@14-23']];
        yield 'script escaped dash dash other' => ['<script><!--a--b</script>', ['S:script@0-8', 'E:script@16-25']];
        yield 'script escaped dash dash dash' => ['<script><!--a---</script>', ['S:script@0-8', 'E:script@16-25']];
        yield 'script escaped dash dash less-than' => ['<script><!--a--</script>', ['S:script@0-8', 'E:script@15-24']];
        yield 'script escaped less-than other' => ['<script><!--<1</script>', ['S:script@0-8', 'E:script@14-23']];
        yield 'script escaped end tag with other name' => ['<script><!--</b></script>', ['S:script@0-8', 'E:script@16-25']];
        yield 'script less-than other' => ['<script>a<1</script>', ['S:script@0-8', 'E:script@11-20']];
        yield 'script end tag at EOF' => ['<script>a</script', ['S:script@0-8']];
        yield 'script in escape at EOF' => ['<script><!--a', ['S:script@0-8']];
        yield 'script in double escape at EOF' => ['<script><!--<script>a', ['S:script@0-8']];
        yield 'script double escape start at EOF' => ['<script><!--<scr', ['S:script@0-8']];
        // PLAINTEXT.
        yield 'plaintext never ends' => ['<plaintext></plaintext><p>', ['S:plaintext@0-11']];
    }

    /**
     * @param list<string> $expected
     */
    #[DataProvider('tokenCases')]
    public function testTokenBoundaries(string $html, array $expected): void
    {
        self::assertSame($expected, self::compact($html));
    }

    public function testStartOffsetSkipsTheByteOrderMark(): void
    {
        self::assertSame(['S:p@3-6'], self::compact("\xEF\xBB\xBF<p>", 3));
    }

    public function testStartOffsetIsClampedToTheInput(): void
    {
        self::assertSame([], self::compact('<p>', 10));
        self::assertSame(['S:p@0-3'], self::compact('<p>', -5));
    }

    public function testAttributeRawValuesInAllQuotingForms(): void
    {
        $tokens = self::tokenize('<p data-rq-block="a&amp;b" type=\'x>y\'><p type=c&#100; data-rq-block>');
        self::assertSame(['data-rq-block' => 'a&amp;b', 'type' => 'x>y'], $tokens[0]->attributes);
        self::assertSame(['type' => 'c&#100;', 'data-rq-block' => ''], $tokens[1]->attributes);
    }

    public function testDuplicateAttributeKeepsTheFirstValue(): void
    {
        $tokens = self::tokenize('<p data-rq-block="a" DATA-RQ-BLOCK="b" type=x Type="y">');
        self::assertSame(['data-rq-block' => 'a', 'type' => 'x'], $tokens[0]->attributes);
    }

    public function testDuplicateAttributeWithoutValueKeepsTheFirstValue(): void
    {
        $tokens = self::tokenize('<p data-rq-block="a" data-rq-block>');
        self::assertSame(['data-rq-block' => 'a'], $tokens[0]->attributes);
    }

    public function testOnlyWatchedAttributesAreKept(): void
    {
        $tokens = self::tokenize('<p data-rq-blocks="a" data-rq-bloc="b" typ="c" types="d" class="e">');
        self::assertSame([], $tokens[0]->attributes);
    }

    public function testAttributeNameEndsAtSolidusAndEqualsSign(): void
    {
        $tokens = self::tokenize('<p type/data-rq-block=x>');
        self::assertSame(['type' => '', 'data-rq-block' => 'x'], $tokens[0]->attributes);
        self::assertFalse($tokens[0]->selfClosing);
    }

    public function testAttributeValueAfterWhiteSpaceAroundEqualsSign(): void
    {
        $tokens = self::tokenize("<p data-rq-block \n= \t'v'>");
        self::assertSame(['data-rq-block' => 'v'], $tokens[0]->attributes);
    }

    public function testUnquotedValueEndsAtWhiteSpace(): void
    {
        $tokens = self::tokenize("<p type=a\ndata-rq-block=b>");
        self::assertSame(['type' => 'a', 'data-rq-block' => 'b'], $tokens[0]->attributes);
    }

    public function testEndTagsHaveNoAttributesAndNoFlag(): void
    {
        $tokens = self::tokenize('<p></p data-rq-block="a"/>');
        self::assertSame(TokenType::EndTag, $tokens[1]->type);
        self::assertSame([], $tokens[1]->attributes);
        self::assertFalse($tokens[1]->selfClosing);
        self::assertNull($tokens[1]->attribute('data-rq-block'));
    }

    public function testNullInTagNameBecomesReplacementCharacter(): void
    {
        $tokens = self::tokenize("<a\0b></a\u{FFFD}b>");
        self::assertSame("a\u{FFFD}b", $tokens[0]->name);
        self::assertSame("a\u{FFFD}b", $tokens[1]->name);
    }

    public function testNormalizeNameLowersAsciiOnly(): void
    {
        self::assertSame("div\u{00C4}", Tokenizer::normalizeName("DIV\u{00C4}"));
    }

    public function testStateCanBeReadAfterSwitch(): void
    {
        $tokenizer = new Tokenizer('<title>x</title>');
        self::assertSame(ContentState::Data, $tokenizer->state());
        $tokenizer->next();
        $tokenizer->switchTo(ContentState::Rcdata, 'title');
        self::assertSame(ContentState::Rcdata, $tokenizer->state());
        $end = $tokenizer->next();
        self::assertSame(TokenType::EndTag, $end->type);
        self::assertSame(ContentState::Data, $tokenizer->state());
    }

    public function testEofTokenRepeatsAtTheEnd(): void
    {
        $tokenizer = new Tokenizer('<p>');
        $tokenizer->next();
        $eof = $tokenizer->next();
        self::assertSame(TokenType::Eof, $eof->type);
        self::assertSame([3, 3], [$eof->start, $eof->end]);
        self::assertSame(TokenType::Eof, $tokenizer->next()->type);
    }

    public function testRawTextEndTagNameComparesTheWholeName(): void
    {
        // `</tit>` has fewer letters than the end tag name; `</titlf>` the same number.
        self::assertSame(['S:title@0-7', 'E:title@21-29'], self::compact('<title></tit></titlf></title>'));
    }
}
