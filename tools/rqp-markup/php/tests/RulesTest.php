<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup\Tests;

use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;
use Rivqen\Tools\RqpMarkup\ErrorCode;
use Rivqen\Tools\RqpMarkup\Parser;
use Rivqen\Tools\RqpMarkup\Result;

/**
 * One or more tests for each rule M-01 to M-27 of markup.md section 2 and
 * for each error code. Invalid cases check the error code and the rule ID.
 */
final class RulesTest extends TestCase
{
    /**
     * @return iterable<string, array{string, string, ErrorCode}>
     */
    public static function invalidCases(): iterable
    {
        $S = ErrorCode::Structure;
        $P = '<p data-rq-block="a">';

        // M-02
        yield 'M-02 invalid sequence' => ["a\xC3\x28b", 'M-02', ErrorCode::Encoding];
        yield 'M-02 overlong two bytes' => ["\xC0\xAF", 'M-02', ErrorCode::Encoding];
        yield 'M-02 overlong three bytes' => ["\xE0\x80\xAF", 'M-02', ErrorCode::Encoding];
        yield 'M-02 overlong four bytes' => ["\xF0\x80\x80\xAF", 'M-02', ErrorCode::Encoding];
        yield 'M-02 surrogate' => ["\xED\xA0\x80", 'M-02', ErrorCode::Encoding];
        yield 'M-02 above U+10FFFF' => ["\xF4\x90\x80\x80", 'M-02', ErrorCode::Encoding];
        yield 'M-02 byte F5' => ["\xF5\x80\x80\x80", 'M-02', ErrorCode::Encoding];
        yield 'M-02 byte FF' => ["a\xFF", 'M-02', ErrorCode::Encoding];
        yield 'M-02 lone continuation byte' => ["\x80", 'M-02', ErrorCode::Encoding];
        yield 'M-02 truncated at EOF' => ["a\xE2\x82", 'M-02', ErrorCode::Encoding];
        yield 'M-02 truncated before ASCII' => ["\xE2\x82a", 'M-02', ErrorCode::Encoding];
        yield 'M-02 invalid byte in block content' => [$P . "\xC3</p>", 'M-02', ErrorCode::Encoding];

        // M-04: in a foreign region a start tag does not change the state, so
        // the `a` start tag in svg noscript is a tag (and M-07 rejects it).
        yield 'M-04 no state change in a foreign region' => ['<svg><noscript><a>x</a></noscript></svg>', 'M-07', $S];

        // M-06
        yield 'M-06 crosses button' => ['<div><button>a</div>', 'M-06', $S];
        yield 'M-06 crosses a in td' => ['<table><tr><td><a href="#">a</td></tr></table>', 'M-06', $S];
        yield 'M-06 table end crosses a' => ['<table><tr><td><a>x</table>', 'M-06', $S];
        yield 'M-06 crosses template' => ['<div><template></div>', 'M-06', $S];
        yield 'M-06 crosses select' => ['<optgroup><select></optgroup>', 'M-06', $S];
        yield 'M-06 crosses svg' => ['<div><svg></svg><object></div>', 'M-06', $S];
        yield 'M-06 td end crosses table' => ['<table><tr><td><table></td>', 'M-06', $S];

        // M-07
        yield 'M-07.1 breakout in svg' => ['<svg><p>', 'M-07', $S];
        yield 'M-07.1 breakout in math' => ['<math><mi>x</mi><div></div></math>', 'M-07', $S];
        yield 'M-07.1 breakout when self-closing' => ['<svg><br/></svg>', 'M-07', $S];
        yield 'M-07.2 stray end tag' => ['<svg></g></svg>', 'M-07', $S];
        yield 'M-07.2 end tag of an element outside the region' => ['<div><svg></div>', 'M-07', $S];
        yield 'M-07.2 stray end tag after a closed integration point' => ['<svg><desc>a</desc></desc></svg>', 'M-07', $S];
        yield 'M-07.2 stray end tag after a closed text element' => ['<svg><style>a</style></style></svg>', 'M-07', $S];
        yield 'M-07.3 tag in foreignObject' => ['<svg><foreignObject><b>x</b></foreignObject></svg>', 'M-07', $S];
        yield 'M-07.3 comment in desc' => ['<svg><desc><!-- c --></desc></svg>', 'M-07', $S];
        yield 'M-07.3 CDATA in mi' => ['<math><mi><![CDATA[x]]></mi></math>', 'M-07', $S];
        yield 'M-07.3 doctype in mtext' => ['<math><mtext><!DOCTYPE html></mtext></math>', 'M-07', $S];
        yield 'M-07.3 other end tag in annotation-xml' => ['<math><annotation-xml></math>', 'M-07', $S];
        yield 'M-07.4 tag in svg style' => ['<svg><style><g></g></style></svg>', 'M-07', $S];
        yield 'M-07.4 self-closing svg script' => ['<svg><script/></svg>', 'M-07', $S];
        yield 'M-07.4 plaintext in svg' => ['<svg><plaintext>', 'M-07', $S];
        yield 'M-07.4 comment in svg script' => ['<svg><script><!-- x --></script></svg>', 'M-07', $S];

        // M-08
        yield 'M-08 b in select' => ['<select><b>x</b></select>', 'M-08', $S];
        yield 'M-08 div in select' => ['<select><option>a</option><div></div></select>', 'M-08', $S];
        yield 'M-08 doctype in select' => ['<select><!DOCTYPE html></select>', 'M-08', $S];
        yield 'M-08 other end tag in select' => ['<select></b></select>', 'M-08', $S];
        yield 'M-08 select without end tag' => ['<select><option>a', 'M-08', $S];
        yield 'M-08 select closed only by a crossing end tag' => ['<div><select></div>', 'M-08', $S];
        yield 'M-08 block in select' => ['<select><p data-rq-block="a">x</p></select>', 'M-08', $S];

        // M-09
        yield 'M-09 td while td is open' => ['<table><tr><td>a<td>b</td></tr></table>', 'M-09', $S];
        yield 'M-09 col in td' => ['<table><tr><td><col></td></tr></table>', 'M-09', $S];
        yield 'M-09 tr in tr' => ['<table><tr><tr>', 'M-09', $S];
        yield 'M-09 caption in tbody' => ['<table><tbody><caption>', 'M-09', $S];
        yield 'M-09 tbody in tbody' => ['<table><tbody><tbody>', 'M-09', $S];
        yield 'M-09 colgroup in colgroup' => ['<table><colgroup><colgroup>', 'M-09', $S];
        yield 'M-09 thead in tr' => ['<table><tr><thead>', 'M-09', $S];
        yield 'M-09 th in caption' => ['<table><caption><th>', 'M-09', $S];
        yield 'M-09 col in tbody' => ['<table><tbody><col>', 'M-09', $S];

        // M-10
        yield 'M-10 frameset' => ['<html><frameset></frameset></html>', 'M-10', $S];
        yield 'M-10 frameset in svg' => ['<svg><frameset/></svg>', 'M-10', $S];

        // M-11
        yield 'M-11 block in noscript' => ['<noscript><p data-rq-block="a">x</p></noscript>', 'M-11', $S];
        yield 'M-11 content ends in a comment' => ['<noscript><!--</noscript>-->', 'M-11', $S];
        yield 'M-11 content ends in a tag' => ['<noscript><a title="</noscript>">', 'M-11', $S];
        yield 'M-11 content ends in RAWTEXT' => ['<noscript><style></noscript></style>', 'M-11', $S];
        yield 'M-11 content ends in script data' => ['<noscript><script></noscript></script>', 'M-11', $S];
        yield 'M-11 content ends in PLAINTEXT' => ['<noscript><plaintext></noscript>', 'M-11', $S];
        yield 'M-11 svg in noscript' => ['<noscript><svg></svg></noscript>', 'M-11', $S];
        yield 'M-11 math in noscript' => ['<noscript><math></math></noscript>', 'M-11', $S];
        yield 'M-11 noscript in noscript' => ['<noscript><noscript></noscript>', 'M-11', $S];
        yield 'M-11 select in noscript' => ['<noscript><select></select></noscript>', 'M-11', $S];
        yield 'M-11 template in noscript' => ['<noscript><template></template></noscript>', 'M-11', $S];
        yield 'M-11 frameset in noscript' => ['<noscript><frameset></noscript>', 'M-11', $S];
        yield 'M-11 bad content at EOF' => ['<noscript><!--', 'M-11', $S];
        yield 'M-11 bad content at EOF in a block' => ['<div data-rq-block="a">x</div><noscript><svg>', 'M-11', $S];

        // M-12
        yield 'M-12 greater-than inside CDATA in HTML' => ['<p><![CDATA[a>b]]></p>', 'M-12', $S];
        yield 'M-12 greater-than inside CDATA in svg' => ['<svg><![CDATA[a>b]]></svg>', 'M-12', $S];
        yield 'M-12 CDATA without greater-than' => ['<![CDATA[x', 'M-12', $S];
        yield 'M-12 CDATA with one bracket' => ['<![CDATA[x]>', 'M-12', $S];
        yield 'M-12 CDATA in block content' => [$P . '<![CDATA[>]]></p>', 'M-12', $S];

        // M-13
        yield 'M-13 manifest script' => ['<script type="application/rivqen-manifest+json">{}</script>', 'M-13', ErrorCode::Reserved];
        yield 'M-13 case-insensitive' => ['<script type="Application/RIVQEN-Manifest+JSON ">{}</script>', 'M-13', ErrorCode::Reserved];
        yield 'M-13 character reference' => ['<script type="application/json&#59;x">{}</script>', 'M-13', ErrorCode::Reserved];
        yield 'M-13 unquoted' => ['<script type=rivqen-manifest></script>', 'M-13', ErrorCode::Reserved];
        yield 'M-13 in svg' => ['<svg><script type="rivqen-manifest"></script></svg>', 'M-13', ErrorCode::Reserved];
        yield 'M-13 first type counts' => ['<script type="rivqen-manifest" type="text/javascript"></script>', 'M-13', ErrorCode::Reserved];
        yield 'M-13 on a json block' => ['<script type="rivqen-manifest" data-rq-block="a">{}</script>', 'M-13', ErrorCode::Reserved];

        // M-15
        yield 'M-15 empty' => ['<p data-rq-block="">x</p>', 'M-15', ErrorCode::InvalidId];
        yield 'M-15 no value' => ['<p data-rq-block>x</p>', 'M-15', ErrorCode::InvalidId];
        yield 'M-15 65 characters' => ['<p data-rq-block="' . str_repeat('a', 65) . '">x</p>', 'M-15', ErrorCode::InvalidId];
        yield 'M-15 upper case' => ['<p data-rq-block="Price">x</p>', 'M-15', ErrorCode::InvalidId];
        yield 'M-15 leading hyphen' => ['<p data-rq-block="-a">x</p>', 'M-15', ErrorCode::InvalidId];
        yield 'M-15 leading underscore' => ['<p data-rq-block="_a">x</p>', 'M-15', ErrorCode::InvalidId];
        yield 'M-15 dot' => ['<p data-rq-block="a.b">x</p>', 'M-15', ErrorCode::InvalidId];
        yield 'M-15 white space' => ['<p data-rq-block=" a">x</p>', 'M-15', ErrorCode::InvalidId];
        yield 'M-15 trailing white space' => ['<p data-rq-block="a ">x</p>', 'M-15', ErrorCode::InvalidId];
        yield 'M-15 non-ASCII' => ["<p data-rq-block=\"pr\u{00E9}\">x</p>", 'M-15', ErrorCode::InvalidId];
        yield 'M-15 character reference' => ['<p data-rq-block="&#112;rice">x</p>', 'M-15', ErrorCode::InvalidId];
        yield 'M-15 checked before M-16 in svg' => ['<svg><title data-rq-block="A"></title></svg>', 'M-15', ErrorCode::InvalidId];

        // M-16
        yield 'M-16 li' => ['<ul><li data-rq-block="a">x</li></ul>', 'M-16', ErrorCode::ForbiddenElement];
        yield 'M-16 img' => ['<img data-rq-block="a">', 'M-16', ErrorCode::ForbiddenElement];
        yield 'M-16 style' => ['<style data-rq-block="a"></style>', 'M-16', ErrorCode::ForbiddenElement];
        yield 'M-16 textarea' => ['<textarea data-rq-block="a"></textarea>', 'M-16', ErrorCode::ForbiddenElement];
        yield 'M-16 template' => ['<template data-rq-block="a"></template>', 'M-16', ErrorCode::ForbiddenElement];
        yield 'M-16 script without type' => ['<script data-rq-block="a">1</script>', 'M-16', ErrorCode::ForbiddenElement];
        yield 'M-16 script ld+json' => ['<script type="application/ld+json" data-rq-block="a">{}</script>', 'M-16', ErrorCode::ForbiddenElement];
        yield 'M-16 script type upper case' => ['<script type="Application/JSON" data-rq-block="a">{}</script>', 'M-16', ErrorCode::ForbiddenElement];
        yield 'M-16 script type with parameter' => ['<script type="application/json;charset=utf-8" data-rq-block="a">{}</script>', 'M-16', ErrorCode::ForbiddenElement];
        yield 'M-16 script type with white space' => ['<script type=" application/json" data-rq-block="a">{}</script>', 'M-16', ErrorCode::ForbiddenElement];
        yield 'M-16 json type not first' => ['<script type="text/plain" type="application/json" data-rq-block="a">{}</script>', 'M-16', ErrorCode::ForbiddenElement];
        yield 'M-16 svg title' => ['<svg><title data-rq-block="a">x</title></svg>', 'M-16', ErrorCode::ForbiddenElement];
        yield 'M-16 div in svg' => ['<svg><g data-rq-block="a"></g></svg>', 'M-16', ErrorCode::ForbiddenElement];

        // M-17
        yield 'M-17 nested' => ['<div data-rq-block="a"><p data-rq-block="b">x</p></div>', 'M-17', ErrorCode::Nested];
        yield 'M-17 nested with a bad id' => ['<div data-rq-block="a"><p data-rq-block="B">x</p></div>', 'M-17', ErrorCode::Nested];
        yield 'M-17 nested forbidden element' => ['<div data-rq-block="a"><li data-rq-block="b"></li></div>', 'M-17', ErrorCode::Nested];

        // M-18
        yield 'M-18 self-closing block' => ['<div data-rq-block="a"/>x</div>', 'M-18', $S];
        yield 'M-18 self-closing json block' => ['<script type="application/json" data-rq-block="a"/>{}</script>', 'M-18', $S];

        // M-19
        yield 'M-19 in template' => ['<template><p data-rq-block="a">x</p></template>', 'M-19', $S];
        yield 'M-19 in table' => ['<table><div data-rq-block="a">x</div></table>', 'M-19', $S];
        yield 'M-19 in tbody' => ['<table><tbody><p data-rq-block="a">x</p></tbody></table>', 'M-19', $S];
        yield 'M-19 in tr' => ['<table><tr><p data-rq-block="a">x</p></tr></table>', 'M-19', $S];
        yield 'M-19 in colgroup' => ['<table><colgroup><p data-rq-block="a">x</p></colgroup></table>', 'M-19', $S];
        yield 'M-19 in thead' => ['<table><thead><p data-rq-block="a">x</p></thead></table>', 'M-19', $S];
        yield 'M-19 in tfoot' => ['<table><tfoot><p data-rq-block="a">x</p></tfoot></table>', 'M-19', $S];
        yield 'M-19 json in table' => ['<table><script type="application/json" data-rq-block="a">1</script></table>', 'M-19', $S];

        // M-20
        yield 'M-20 EOF in html block' => [$P . 'x', 'M-20', $S];
        yield 'M-20 EOF in title block' => ['<title data-rq-block="t">x', 'M-20', $S];
        yield 'M-20 EOF in json block' => ['<script type="application/json" data-rq-block="a">{}', 'M-20', $S];
        yield 'M-20 EOF in the block end tag' => [$P . 'x</p', 'M-20', $S];
        yield 'M-20 EOF in a title end tag' => ['<title data-rq-block="t">x</title class="', 'M-20', $S];

        // M-21
        yield 'M-21 duplicate' => ['<p data-rq-block="a">1</p><div data-rq-block="a">2</div>', 'M-21', ErrorCode::Duplicate];

        // M-22
        yield 'M-22.1 doctype in content' => [$P . '<!DOCTYPE html></p>', 'M-22', $S];
        yield 'M-22.2 script in content' => [$P . '<script></script></p>', 'M-22', $S];
        yield 'M-22.2 option in content' => [$P . '<option></option></p>', 'M-22', $S];
        yield 'M-22.2 svg in content' => [$P . '<svg></svg></p>', 'M-22', $S];
        yield 'M-22.2 self-closing svg in content' => [$P . '<svg/></p>', 'M-22', $S];
        yield 'M-22.2 meta in content' => [$P . '<meta charset="utf-8"></p>', 'M-22', $S];
        yield 'M-22.2 title in content' => ['<div data-rq-block="a"><title>t</title></div>', 'M-22', $S];
        yield 'M-22.3 self-closing span' => [$P . '<span/></p>', 'M-22', $S];
        yield 'M-22.4 end tag br' => [$P . 'a</br>b</p>', 'M-22', $S];
        yield 'M-22.4 end tag img' => [$P . '</img></p>', 'M-22', $S];

        // M-23
        yield 'M-23 element open at block end' => [$P . '<b>x</p>', 'M-23', $S];
        yield 'M-23 stray end tag' => [$P . 'x</b></p>', 'M-23', $S];
        yield 'M-23 end tag of an element outside the block' => ['<div><p data-rq-block="a">x</div></p>', 'M-23', $S];
        yield 'M-23 crossing inside content' => ['<div data-rq-block="a"><b><i>x</b></i></div>', 'M-23', $S];

        // M-24
        yield 'M-24.a ul in p block' => [$P . '<ul></ul></p>', 'M-24', $S];
        yield 'M-24.a div in span block' => ['<span data-rq-block="a"><div></div></span>', 'M-24', $S];
        yield 'M-24.a p in p block' => [$P . '<p></p></p>', 'M-24', $S];
        yield 'M-24.a table in p block' => [$P . '<table></table></p>', 'M-24', $S];
        yield 'M-24.b heading in heading block' => ['<h1 data-rq-block="a"><h2>x</h2></h1>', 'M-24', $S];
        yield 'M-24.b heading in a heading of the content' => ['<div data-rq-block="a"><h3><h4>x</h4></h3></div>', 'M-24', $S];
        yield 'M-24.b heading in h6 block' => ['<h6 data-rq-block="a"><h1>x</h1></h6>', 'M-24', $S];
        yield 'M-24.c li without list' => ['<ul><li><div data-rq-block="a"><li>x</li></div></li></ul>', 'M-24', $S];
        yield 'M-24.c dd without list' => ['<div data-rq-block="a"><dd>x</dd></div>', 'M-24', $S];
        yield 'M-24.c dt without list' => ['<div data-rq-block="a"><dt>x</dt></div>', 'M-24', $S];
        yield 'M-24.d button in button' => ['<button><div data-rq-block="a"><button>x</button></div></button>', 'M-24', $S];
        yield 'M-24.e a in a' => ['<div data-rq-block="a"><a href="#"><a>x</a></a></div>', 'M-24', $S];
        yield 'M-24.f nobr in nobr' => ['<nobr><div data-rq-block="a"><nobr>x</nobr></div></nobr>', 'M-24', $S];
        yield 'M-24.g td without table' => ['<div data-rq-block="a"><td>x</td></div>', 'M-24', $S];
        yield 'M-24.g col without table' => ['<div data-rq-block="a"><col></div>', 'M-24', $S];
        yield 'M-24.g caption without table' => ['<div data-rq-block="a"><caption>x</caption></div>', 'M-24', $S];
        yield 'M-24.h rb without ruby' => ['<div data-rq-block="a"><rb>x</rb></div>', 'M-24', $S];
        yield 'M-24.h rtc without ruby in the content' => ['<ruby><div data-rq-block="a"><rtc>x</rtc></div></ruby>', 'M-24', $S];
        yield 'M-24.h rb while rt is open' => ['<div data-rq-block="a"><ruby><rt><rb>x</rb></rt></ruby></div>', 'M-24', $S];
        yield 'M-24.h rtc while rtc is open' => ['<div data-rq-block="a"><ruby><rtc><rtc></rtc></rtc></ruby></div>', 'M-24', $S];
        yield 'M-24.h rb while rp is open' => ['<div data-rq-block="a"><ruby><rp><rb></rb></rp></ruby></div>', 'M-24', $S];
        yield 'M-24.h rb while rb is open' => ['<div data-rq-block="a"><ruby><rb><rb></rb></rb></ruby></div>', 'M-24', $S];
        yield 'M-24.i rt without ruby' => ['<div data-rq-block="a"><rt>x</rt></div>', 'M-24', $S];
        yield 'M-24.i rp without ruby' => ['<div data-rq-block="a"><rp>x</rp></div>', 'M-24', $S];
        yield 'M-24.i rt while rb is open' => ['<div data-rq-block="a"><ruby><rb><rt></rt></rb></ruby></div>', 'M-24', $S];
        yield 'M-24.i rp while rp is open' => ['<div data-rq-block="a"><ruby><rp><rp></rp></rp></ruby></div>', 'M-24', $S];
        yield 'M-24.i rt while rt is open' => ['<div data-rq-block="a"><ruby><rt><rt></rt></rt></ruby></div>', 'M-24', $S];

        // M-25
        yield 'M-25 trailing comma' => ['<script type="application/json" data-rq-block="a">[1,]</script>', 'M-25', ErrorCode::BadJson];
        yield 'M-25 empty' => ['<script type="application/json" data-rq-block="a"></script>', 'M-25', ErrorCode::BadJson];
    }

    #[DataProvider('invalidCases')]
    public function testInvalidDocument(string $html, string $rule, ErrorCode $code): void
    {
        $result = Parser::parse($html);
        self::assertFalse($result->valid);
        self::assertSame($code, $result->error);
        self::assertSame($rule, $result->rule);
        self::assertSame([], $result->blocks);
        self::assertNull($result->templateRevision);
        self::assertNull($result->pageRevision);
    }

    /**
     * Valid documents with the expected blocks as [id, format, content].
     *
     * @return iterable<string, array{string, list<array{string, string, string}>}>
     */
    public static function validCases(): iterable
    {
        // M-02
        yield 'M-02 BOM' => ["\xEF\xBB\xBF<p data-rq-block=\"a\">x</p>", [['a', 'html', 'x']]];
        yield 'M-02 U+FEFF after offset 0' => ["x\xEF\xBB\xBF<p data-rq-block=\"a\">\xEF\xBB\xBF</p>", [['a', 'html', "\xEF\xBB\xBF"]]];
        yield 'M-02 NUL and controls' => ["<p data-rq-block=\"a\">\0\x01\x7F</p>", [['a', 'html', "\0\x01\x7F"]]];
        yield 'M-02 four-byte character' => ["<p data-rq-block=\"a\">\u{1F600}</p>", [['a', 'html', "\u{1F600}"]]];
        yield 'M-02 largest code point' => ["<p data-rq-block=\"a\">\u{10FFFF}</p>", [['a', 'html', "\u{10FFFF}"]]];

        // M-03
        yield 'M-03 greater-than in quoted value' => ['<p title="a>b" data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-03 block attribute without white space before it' => ['<p class="x"data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-03 duplicate attribute' => ['<p data-rq-block="a" data-rq-block="b">x</p>', [['a', 'html', 'x']]];
        yield 'M-03 CRLF' => ["<p\r\ndata-rq-block=\"a\">x\r\ny</p>", [['a', 'html', "x\r\ny"]]];
        yield 'M-03 bogus comments' => ['<!x <p data-rq-block="b">></ <p data-rq-block="c">><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-03 processing instruction' => ['<?pi <p data-rq-block="b">?><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-03 doctype' => ['<!DOCTYPE html "<p data-rq-block=b>"><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-03 comment forms' => [
            '<!--><p data-rq-block="a">x</p><!---><!-- <p data-rq-block="c"> --!><p data-rq-block="b">y</p>',
            [['a', 'html', 'x'], ['b', 'html', 'y']],
        ];
        yield 'M-03 end tag with attributes' => ['<div><p data-rq-block="a">x</p></div class="<p>">', [['a', 'html', 'x']]];

        // M-04
        yield 'M-04 title text' => ['<title><p data-rq-block="b"></title><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-04 textarea text' => ['<textarea><p data-rq-block="b"></textarea>', []];
        yield 'M-04 style text' => ['<style><p data-rq-block="b"></style>', []];
        yield 'M-04 iframe text' => ['<iframe><p data-rq-block="b"></iframe>', []];
        yield 'M-04 noembed text' => ['<noembed><p data-rq-block="b"></noembed>', []];
        yield 'M-04 noframes text' => ['<noframes><p data-rq-block="b"></noframes>', []];
        yield 'M-04 xmp text' => ['<xmp><p data-rq-block="b"></xmp>', []];
        yield 'M-04 script text' => ['<script>"<p data-rq-block=b>"</script>', []];
        yield 'M-04 script double escaped text' => ['<script><!--<script><p data-rq-block="b"></script>x</script>', []];
        yield 'M-04 plaintext' => ['<plaintext><p data-rq-block="b">x</p>', []];
        yield 'M-04 self-closing title' => ['<title/><p data-rq-block="b"></title>', []];
        yield 'M-04 noscript text' => ['<noscript><img src="x.gif"></noscript><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];

        // M-05
        yield 'M-05 self-closing svg opens no region' => ['<svg/><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-05 self-closing math opens no region' => ['<math/><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-05 void elements are not pushed' => ['<p data-rq-block="a">a<br>b<img src="x"><wbr></p>', [['a', 'html', 'a<br>b<img src="x"><wbr>']]];
        yield 'M-05 self-closing flag ignored outside content' => ['<div/><p data-rq-block="a">x</p></div>', [['a', 'html', 'x']]];
        yield 'M-05 foreign self-closing not pushed' => ['<svg><rect/><g></g></svg><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-05 unmatched end tag ignored' => ['</span></p><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-05 nested svg in svg' => ['<svg><svg></svg></svg><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];

        // M-06
        yield 'M-06 crosses span' => ['<div><span>a</div><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-06 crosses li' => ['<ul><li>a</ul><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-06 tr crosses td' => ['<table><tr><td>a</tr></table><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-06 table crosses table parts' => ['<table><caption>c</table><table><colgroup><col></table><table><tbody><tr><th>h</table><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-06 crosses foreign a' => ['<svg><a><g></svg><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];

        // M-07
        yield 'M-07 text in integration points' => ['<svg><foreignObject>t</foreignObject><desc>d</desc><title>t</title></svg><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-07 self-closing integration point' => ['<svg><desc/><g></g></svg><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-07 crossing inside the region' => ['<svg><g><a><text></g></svg><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-07 text elements with text' => ['<svg><style>a{}</style><script>1</script></svg><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-07 comment in svg' => ['<svg><!-- c --><g></g></svg><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-07 math' => ['<math><mi>x</mi><mo>=</mo><mn>1</mn></math><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-07 integration point at EOF' => ['<p data-rq-block="a">x</p><svg><desc>text', [['a', 'html', 'x']]];

        // M-08
        yield 'M-08 select' => ['<select><option>a</option><optgroup><option>b</optgroup><hr><!-- c --></select><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-08 options without end tags' => ['<select><option>a<option>b</select><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];

        // M-09
        yield 'M-09 table parts' => ['<table><caption>c</caption><colgroup><col></colgroup><col><thead><tr><th>h</th></tr></thead><tbody><tr><td>1</td><td>2</td></tr></tbody><tr><td>3</td></tr><td>4</td><tfoot></tfoot></table><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-09 template stops the search' => ['<table><tr><td><template><td></td></template></td></tr></table><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-09 nested table in a cell' => ['<table><tr><td><table><tr><td>a</td></tr></table></td></tr></table><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-09 table part outside a table' => ['<td>a</td><tr></tr><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];

        // M-11
        yield 'M-11 iframe in noscript' => ['<noscript><iframe src="https://example.com/t"></iframe></noscript><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-11 comment in noscript' => ['<noscript><!-- c --><img src="x"></noscript><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-11 noscript at EOF' => ['<p data-rq-block="a">x</p><noscript><img src="x">', [['a', 'html', 'x']]];
        yield 'M-11 empty noscript' => ['<noscript></noscript><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-11 self-closing noscript' => ['<noscript/></noscript><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-11 script in noscript' => ['<noscript><script>1</script></noscript><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];

        // M-12
        yield 'M-12 CDATA in svg' => ['<svg><![CDATA[a]]></svg><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-12 CDATA in block content' => ['<p data-rq-block="a"><![CDATA[x]]></p>', [['a', 'html', '<![CDATA[x]]>']]];
        yield 'M-12 CDATA in select' => ['<select><![CDATA[x]]></select>', []];
        yield 'M-12 CDATA text in raw text' => ['<style><![CDATA[a>b</style>', []];

        // M-13
        yield 'M-13 manifest text in a script is not a tag' => ['<script>"<script type=rivqen-manifest>"</script>', []];
        yield 'M-13 other type' => ['<script type="module"></script><script type="text/javascript" type="rivqen-manifest"></script>', []];

        // M-14
        yield 'M-14 upper-case attribute name' => ['<P DATA-RQ-BLOCK="a">x</P>', [['a', 'html', 'x']]];
        yield 'M-14 not a marker' => ['<p data-rq-block-x="b" x-data-rq-block="c" title="data-rq-block=d">x</p></p data-rq-block="e">', []];
        yield 'M-14 attribute in a comment' => ['<!-- <p data-rq-block="b"> -->', []];

        // M-15
        yield 'M-15 shortest id' => ['<p data-rq-block="0">x</p>', [['0', 'html', 'x']]];
        yield 'M-15 64 characters' => ['<p data-rq-block="' . str_repeat('z', 64) . '">x</p>', [[str_repeat('z', 64), 'html', 'x']]];
        yield 'M-15 hyphen and underscore' => ['<p data-rq-block="a-_9">x</p>', [['a-_9', 'html', 'x']]];
        yield 'M-15 unquoted value' => ['<p data-rq-block=a>x</p>', [['a', 'html', 'x']]];
        yield 'M-15 single-quoted value' => ["<p data-rq-block='a'>x</p>", [['a', 'html', 'x']]];

        // M-16
        yield 'M-16 every BLOCK_HTML name' => [
            '<article data-rq-block="a1">1</article><aside data-rq-block="a2">2</aside><div data-rq-block="a3">3</div>'
            . '<footer data-rq-block="a4">4</footer><h1 data-rq-block="a5">5</h1><h2 data-rq-block="a6">6</h2>'
            . '<h3 data-rq-block="a7">7</h3><h4 data-rq-block="a8">8</h4><h5 data-rq-block="a9">9</h5>'
            . '<h6 data-rq-block="b1">10</h6><header data-rq-block="b2">11</header><main data-rq-block="b3">12</main>'
            . '<nav data-rq-block="b4">13</nav><p data-rq-block="b5">14</p><section data-rq-block="b6">15</section>'
            . '<span data-rq-block="b7">16</span><title data-rq-block="b8">17</title>',
            [
                ['a1', 'html', '1'], ['a2', 'html', '2'], ['a3', 'html', '3'], ['a4', 'html', '4'],
                ['a5', 'html', '5'], ['a6', 'html', '6'], ['a7', 'html', '7'], ['a8', 'html', '8'],
                ['a9', 'html', '9'], ['b1', 'html', '10'], ['b2', 'html', '11'], ['b3', 'html', '12'],
                ['b4', 'html', '13'], ['b5', 'html', '14'], ['b6', 'html', '15'], ['b7', 'html', '16'],
                ['b8', 'html', '17'],
            ],
        ];
        yield 'M-16 json block' => ['<script type="application/json" data-rq-block="c">{"a":[1]}</script>', [['c', 'json', '{"a":[1]}']]];
        yield 'M-16 title block keeps RCDATA bytes' => ['<title data-rq-block="t">a &amp; <b></title>', [['t', 'html', 'a &amp; <b>']]];

        // M-19
        yield 'M-19 blocks in caption, td and th' => [
            '<table><caption><p data-rq-block="a">1</p></caption><tr><td><div data-rq-block="b">2</div></td><th><span data-rq-block="c">3</span></th></tr></table>',
            [['a', 'html', '1'], ['b', 'html', '2'], ['c', 'html', '3']],
        ];
        yield 'M-19 after a template' => ['<template><p>t</p></template><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-19 after a table' => ['<table><tr><td>1</td></tr></table><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];
        yield 'M-19 after html end' => ['<html><body></body></html><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];

        // M-20
        yield 'M-20 title end tag with attributes' => ['<title data-rq-block="t">x</title class="y">', [['t', 'html', 'x']]];
        yield 'M-20 json end tag after a double escape' => [
            '<script type="application/json" data-rq-block="j">["<!--<script>","</script>"]</script>',
            [['j', 'json', '["<!--<script>","</script>"]']],
        ];

        // M-22
        yield 'M-22 allowed content' => [
            '<div data-rq-block="a"><!-- c --><?pi?><br/><img src="x"/><a href="javascript:x" onclick="y">l</a>&amp;</div>',
            [['a', 'html', '<!-- c --><?pi?><br/><img src="x"/><a href="javascript:x" onclick="y">l</a>&amp;']],
        ];

        // M-23
        yield 'M-23 balanced optional end tags' => [
            '<div data-rq-block="a"><p>1</p><ul><li>2</li></ul><table><tr><td>3</td></tr></table></div>',
            [['a', 'html', '<p>1</p><ul><li>2</li></ul><table><tr><td>3</td></tr></table>']],
        ];
        yield 'M-23 same name inside' => ['<div data-rq-block="a"><div>x</div></div>', [['a', 'html', '<div>x</div>']]];
        yield 'M-23 optional end tags outside blocks' => ['<ul><li>1<li>2</ul><p>a<p data-rq-block="a">x</p>', [['a', 'html', 'x']]];

        // M-24
        yield 'M-24.a div after open p' => ['<p>a<div data-rq-block="a">x</div>', [['a', 'html', 'x']]];
        yield 'M-24.a span in p' => ['<p>a<span data-rq-block="a"><b>x</b></span></p>', [['a', 'html', '<b>x</b>']]];
        yield 'M-24.a block in div' => ['<div data-rq-block="a"><p>1</p><ul></ul><h1>h</h1></div>', [['a', 'html', '<p>1</p><ul></ul><h1>h</h1>']]];
        yield 'M-24.b headings in sequence' => ['<div data-rq-block="a"><h1>1</h1><h2>2</h2></div>', [['a', 'html', '<h1>1</h1><h2>2</h2>']]];
        yield 'M-24.c list items' => ['<div data-rq-block="a"><ol><li>1</li></ol><menu><li>2</li></menu><dl><dt>3</dt><dd>4</dd></dl></div>', [['a', 'html', '<ol><li>1</li></ol><menu><li>2</li></menu><dl><dt>3</dt><dd>4</dd></dl>']]];
        yield 'M-24.d,e,f after close' => ['<div data-rq-block="a"><button>1</button><button>2</button><a>3</a><a>4</a><nobr>5</nobr><nobr>6</nobr></div>', [['a', 'html', '<button>1</button><button>2</button><a>3</a><a>4</a><nobr>5</nobr><nobr>6</nobr>']]];
        yield 'M-24.g table in content' => ['<div data-rq-block="a"><table><caption>c</caption><colgroup><col></colgroup><tbody><tr><td>1</td></tr></tbody></table></div>', [['a', 'html', '<table><caption>c</caption><colgroup><col></colgroup><tbody><tr><td>1</td></tr></tbody></table>']]];
        yield 'M-24.h,i ruby' => ['<div data-rq-block="a"><ruby>x<rb>b</rb><rt>t</rt><rp>(</rp><rtc>c</rtc></ruby></div>', [['a', 'html', '<ruby>x<rb>b</rb><rt>t</rt><rp>(</rp><rtc>c</rtc></ruby>']]];
        yield 'M-24 guards do not apply outside content' => ['<a><a><button><button><nobr><nobr><li><rb><rt></nobr></nobr></button></button></a></a><p data-rq-block="a">x</p>', [['a', 'html', 'x']]];

        // M-25
        yield 'M-25 json edge' => ["<script type=\"application/json\" data-rq-block=\"j\">\r\n{\"a\":1e400,\"a\":\"\\ud83d\\ude00\\/\"}\r\n</script>", [['j', 'json', "\r\n{\"a\":1e400,\"a\":\"\\ud83d\\ude00\\/\"}\r\n"]]];

        // M-27
        yield 'M-27 zero blocks' => ['<!DOCTYPE html><p>x</p>', []];
        yield 'M-27 empty content' => ['<p data-rq-block="a"></p><script type="application/json" data-rq-block="b"> 0 </script>', [['a', 'html', ''], ['b', 'json', ' 0 ']]];
    }

    /**
     * @param list<array{string, string, string}> $expected
     */
    #[DataProvider('validCases')]
    public function testValidDocument(string $html, array $expected): void
    {
        $result = Parser::parse($html);
        self::assertTrue($result->valid, 'error ' . ($result->rule ?? ''));
        self::assertNull($result->error);
        $actual = [];
        foreach ($result->blocks as $block) {
            $actual[] = [$block->id, $block->format, substr($html, $block->start, $block->end - $block->start)];
        }
        self::assertSame($expected, $actual);
    }

    public function testM01InputSizeLimitIsCheckedFirst(): void
    {
        $limit = Parser::MAX_INPUT_BYTES;
        self::assertSame(5_242_880, $limit);
        $over = Parser::parse(str_repeat("\xFF", $limit + 1));
        self::assertSame(ErrorCode::Limit, $over->error);
        self::assertSame('M-01', $over->rule);
    }

    public function testM01InputAtTheLimitIsAccepted(): void
    {
        $result = Parser::parse(str_repeat('a', Parser::MAX_INPUT_BYTES));
        self::assertTrue($result->valid);
    }

    public function testM26BlockAtTheSizeLimitIsAccepted(): void
    {
        $result = Parser::parse('<p data-rq-block="a">' . str_repeat('a', 1_048_576) . '</p>');
        self::assertTrue($result->valid);
        self::assertSame(1_048_576, $result->blocks[0]->end - $result->blocks[0]->start);
    }

    public function testM26BlockOverTheSizeLimit(): void
    {
        $result = Parser::parse('<p data-rq-block="a">' . str_repeat('a', 1_048_577) . '</p>');
        self::assertSame(ErrorCode::Limit, $result->error);
        self::assertSame('M-26', $result->rule);
    }

    public function testM26JsonBlockSizeIsCheckedBeforeJson(): void
    {
        $result = Parser::parse('<script type="application/json" data-rq-block="a">' . str_repeat(' ', Parser::MAX_BLOCK_BYTES + 1) . '</script>');
        self::assertSame(ErrorCode::Limit, $result->error);
    }

    public function testM26BlockCountLimit(): void
    {
        self::assertSame(256, Parser::MAX_BLOCKS);
        self::assertTrue(Parser::parse(self::blocks(256))->valid);
        $over = Parser::parse(self::blocks(257));
        self::assertSame(ErrorCode::Limit, $over->error);
        self::assertSame('M-26', $over->rule);
    }

    public function testM27OffsetsAndHashes(): void
    {
        $html = "\xEF\xBB\xBF<p data-rq-block=\"b\">\u{1F600}</p>\r\n<title data-rq-block=\"a\">T</title>";
        $result = Parser::parse($html);
        self::assertTrue($result->valid);
        self::assertSame(['b', 'a'], array_map(static fn($b) => $b->id, $result->blocks));
        self::assertSame(24, $result->blocks[0]->start);
        self::assertSame(28, $result->blocks[0]->end);
        self::assertSame(rtrim(strtr(base64_encode(hash('sha256', "\u{1F600}", true)), '+/', '-_'), '='), $result->blocks[0]->sha256);
        self::assertSame(43, strlen($result->blocks[0]->sha256));
    }

    public function testM27ResultArrayHasTheContractFields(): void
    {
        $valid = Parser::parse('<p data-rq-block="a">x</p>')->toArray();
        self::assertSame(['valid', 'error', 'blocks', 'template_revision', 'page_revision'], array_keys($valid));
        self::assertSame(['id', 'format', 'start', 'end', 'sha256'], array_keys($valid['blocks'][0]));
        $invalid = Parser::parse('<p data-rq-block="">x</p>')->toArray();
        self::assertSame(
            ['valid' => false, 'error' => 'RQP_MARKUP_INVALID_ID', 'blocks' => [], 'template_revision' => null, 'page_revision' => null],
            $invalid,
        );
    }

    public function testM15IdValidation(): void
    {
        self::assertTrue(Parser::isValidId('a'));
        self::assertTrue(Parser::isValidId('9z_-'));
        self::assertTrue(Parser::isValidId(str_repeat('a', 64)));
        self::assertFalse(Parser::isValidId(''));
        self::assertFalse(Parser::isValidId(str_repeat('a', 65)));
        self::assertFalse(Parser::isValidId('_a'));
        self::assertFalse(Parser::isValidId('-a'));
        self::assertFalse(Parser::isValidId('a/'));
        self::assertFalse(Parser::isValidId("a\n"));
    }

    public function testEveryErrorCodeIsReachable(): void
    {
        $reached = [];
        foreach (self::invalidCases() as [$html, , $code]) {
            $reached[$code->value] = true;
        }
        $reached[ErrorCode::Limit->value] = true; // testM01..., testM26...
        foreach (ErrorCode::cases() as $code) {
            self::assertArrayHasKey($code->value, $reached, $code->value);
        }
    }

    private static function blocks(int $n): string
    {
        $html = '';
        for ($i = 0; $i < $n; $i++) {
            $html .= '<p data-rq-block="b' . $i . '">' . $i . '</p>';
        }
        return $html;
    }

    public static function resultOf(string $html): Result
    {
        return Parser::parse($html);
    }
}
