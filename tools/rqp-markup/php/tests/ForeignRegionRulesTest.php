<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup\Tests;

use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;
use Rivqen\Tools\RqpMarkup\ErrorCode;
use Rivqen\Tools\RqpMarkup\Parser;

/**
 * The WP-17 S3 rules: foreign regions in the content of `html` blocks (M-22
 * item 5, M-23; H-73, H-75), `svg` in the `noscript` content (M-11 items 5
 * and 9; H-74, H-75), the main pass on the `noscript` content (M-11 item 8;
 * H-71, DL-044) and the mark of the region root end tag (M-06 item 4, H-75).
 * Each invalid case checks the rule ID, so a case does not pass by another
 * rule (a `svg` start tag in the content used to fail at M-22 item 2).
 */
final class ForeignRegionRulesTest extends TestCase
{
    private const BLOCK = '<span data-rq-block="a">';

    /**
     * @return iterable<string, array{string}>
     */
    public static function validDocuments(): iterable
    {
        $B = self::BLOCK;
        yield 'icon with title and path' => [$B . '<svg viewBox="0 0 16 16"><title>Close</title><path d="M0"/></svg> Close</span>'];
        yield 'math with integration points' => [$B . '<math><mi>x</mi><mo>+</mo><mn>1</mn></math></span>'];
        yield 'self-closing svg opens no region' => [$B . '<svg/><math/></span>'];
        yield 'end tag of the region root closes the elements above it' => [$B . '<svg><g><path d="M0"></svg></span>'];
        yield 'nested svg is a foreign element' => [$B . '<svg><svg><g></g></svg></svg></span>'];
        yield 'elements closed one by one' => [$B . '<svg><g><path d="M0"></path></g></svg></span>'];
        yield 'uppercase names are lower-cased' => [$B . '<SVG><LinearGradient><Stop/></LinearGradient></SVG></span>'];
        yield 'html after the region' => [$B . '<svg><path d="M0"></svg><b>x</b></span>'];
        yield 'text with a less-than in an integration point' => [$B . '<svg><desc>a < b</desc></svg></span>'];
        // M-06 item 4 (H-75).
        yield 'region root end tag sets no mark' => ['<a href="#"><svg><path d="M0"></svg><b>x</b></a>'];
        yield 'icon in a link, then a formatting end tag' => ['<a href="#"><b><svg><g><path d="M0"></svg></b></a>'];
        // M-11 items 5 and 9.
        yield 'svg in noscript' => ['<noscript><img src="x"><svg width="1" height="1"><path d="M0"></path></svg></noscript>'];
        yield 'svg in noscript, root end tag crosses' => ['<noscript><a href="/"><svg><path d="M0"></svg>Home</a></noscript>'];
        yield 'self-closing svg in noscript' => ['<noscript><svg/><p></p></noscript>'];
        yield 'nested svg in noscript' => ['<noscript><svg><svg></svg></svg></noscript>'];
        yield 'noscript at EOF with a closed svg' => ['<noscript><svg><path d="M0"></svg>'];
        // H-78: no name list applies in a region of C or of the content.
        foreach (['a href="#x"', 'animate', 'set', 'foreignObject', 'image', 'script', 'style', 'animateTransform', 'textPath', 'feImage'] as $tag) {
            yield "H-78 $tag in svg of the content" => [$B . "<svg><$tag></" . strtok($tag, ' ') . '></svg></span>'];
        }
        yield 'H-78 maction in math of the content' => [$B . '<math><maction></maction></math></span>'];
        yield 'H-78 annotation-xml in math of the content' => [$B . '<math><annotation-xml>t</annotation-xml></math></span>'];
        yield 'H-78 a self-closing name outside the old set' => [$B . '<svg><animate/></svg></span>'];
        yield 'H-78 script after the region' => [$B . '<svg></svg><script></script></span>'];
        yield 'H-78 svg script with CDATA' => [$B . '<svg><script><![CDATA[a<b]]></script></svg></span>'];
        yield 'H-78 html a inside a region-free a' => ['<span data-rq-block="a"><a href="#"><svg><a href="#x"></a></svg></a></span>'];
        yield 'a foreign a in noscript' => ['<noscript><svg><a href="#"></a></svg></noscript>'];
        yield 'a foreign style with CDATA in noscript' => ['<noscript><svg><style><![CDATA[.a{}]]></style></svg></noscript>'];
        yield 'noscript inside a link with an icon' => ['<a href="#"><noscript><svg><path d="M0"></svg></noscript></a><b></b>'];
    }

    #[DataProvider('validDocuments')]
    public function testValidDocument(string $html): void
    {
        $result = Parser::parse($html);
        self::assertTrue($result->valid, 'rule ' . ($result->rule ?? ''));
    }

    /**
     * @return iterable<string, array{string, string}>
     */
    public static function invalidDocuments(): iterable
    {
        $B = self::BLOCK;
        // M-07 still applies in the region.
        yield 'M-07.1 breakout tag in the region' => [$B . '<svg><div></svg></span>', 'M-07'];
        yield 'M-07.3 tag in an integration point' => [$B . '<svg><desc><g></g></desc></svg></span>', 'M-07'];
        yield 'M-07.3 comment in an integration point' => [$B . '<svg><title><!--x--></title></svg></span>', 'M-07'];
        yield 'M-07.3 a text element in svg is not RCDATA' => [$B . '<svg><title><g></g></title></svg></span>', 'M-07'];
        yield 'M-12 CDATA in a region of the content' => [$B . '<svg><desc><![CDATA[a>b]]></desc></svg></span>', 'M-12'];
        // M-23 in a region.
        yield 'M-23 the end tag of an inner element crosses' => [$B . '<svg><g><path d="M0"></g></svg></span>', 'M-23'];
        yield 'M-23 the nearest svg is not the region root' => [$B . '<svg><svg><g></svg></svg></span>', 'M-23'];
        yield 'M-23 stray end tag in a region' => [$B . '<svg></g></svg></span>', 'M-23'];
        yield 'M-23 end tag br in a region matches nothing' => [$B . '<svg></br></svg></span>', 'M-23'];
        yield 'M-23 the block end tag inside the region' => [$B . '<svg></span>', 'M-23'];
        yield 'M-23 end tag of an element before the region' => ['<div data-rq-block="a"><b><svg></b></svg></div>', 'M-23'];
        yield 'M-20 the region is open at EOF' => [$B . '<svg><g>', 'M-20'];
        yield 'M-17 block start tag in a region' => [$B . '<svg><g data-rq-block="b"></g></svg></span>', 'M-17'];
        yield 'M-22.3 self-closing html element' => [$B . '<svg/><b/></span>', 'M-22'];
        yield 'M-22.1 DOCTYPE in a region of the content' => [$B . '<svg><!DOCTYPE html></svg></span>', 'M-22'];
        // M-26: pushes in a region of the content count.
        yield 'M-26 region of the content' => ['<div data-rq-block="a"><svg>' . str_repeat('<g>', 255) . str_repeat('</g>', 255) . '</svg></div>', 'M-26'];
        // M-06 item 4 (H-75): other crossings still set the mark.
        yield 'M-06.4 an inner end tag marks, the root end tag passes the mark' => ['<a href="#"><svg><g><path d="M0"></g></svg></a>', 'M-06'];
        yield 'M-06.4 the nested svg is not the region root' => ['<a href="#"><svg><svg><path d="M0"></svg></svg></a>', 'M-06'];
        yield 'M-06.2 formatting end tag crosses a foreign element' => ['<svg><a><text>t</a></svg>', 'M-06'];
        yield 'M-06.4 a mark passed by a root end tag' => ['<a><span><div></span><svg><path></svg></a>', 'M-06'];
        // M-11 items 5 and 9.
        yield 'M-11.5 inner end tag crosses in C' => ['<noscript><svg><g><path d="M0"></g></svg></noscript>', 'M-11'];
        yield 'M-11.5 the nested svg is not the region root' => ['<noscript><svg><svg></svg></noscript>', 'M-11'];
        yield 'M-11.5 stray end tag svg' => ['<noscript></svg></noscript>', 'M-11'];
        yield 'M-11.5 stray end tag after a self-closing svg' => ['<noscript><svg/></svg></noscript>', 'M-11'];
        yield 'M-11.5 svg open at the end of C' => ['<noscript><svg>', 'M-11'];
        yield 'M-11.5 end tag of a name that is not open, region root directly above noscript' => ['<noscript><svg><g></path></svg></noscript>', 'M-11'];
        yield 'M-11.6 table part in a region of C' => ['<noscript><svg><tr></tr></svg></noscript>', 'M-11'];
        yield 'M-11.7 html in a region of C' => ['<noscript><svg><html></svg></noscript>', 'M-11'];
        yield 'M-11.7 body start tag in C' => ['<noscript><body></body></noscript>', 'M-11'];
        yield 'M-11.7 head end tag in C' => ['<noscript></head></noscript>', 'M-11'];
        yield 'M-11.3 math in noscript' => ['<noscript><math></math></noscript>', 'M-11'];
        yield 'M-11.2 block start tag in a region of C' => ['<noscript><svg><g data-rq-block="a"></g></svg></noscript>', 'M-11'];
        yield 'M-11.3 frameset in a region of C' => ['<noscript><svg><frameset/></svg></noscript>', 'M-11'];
        // M-11 item 8: the rules of the main pass on C.
        yield 'M-07.1 breakout tag in a region of C' => ['<noscript><svg><p>x</p></svg></noscript>', 'M-07'];
        yield 'M-07.3 integration point in C' => ['<noscript><svg><title><g></g></title></svg></noscript>', 'M-07'];
        yield 'M-12 CDATA in C' => ['<noscript><![CDATA[a>b]]></noscript>', 'M-12'];
        yield 'M-09 table in a table around noscript' => ['<table><noscript><table></table></noscript></table>', 'M-09'];
        yield 'M-09 marquee between tables in C' => ['<object><a href="1"><noscript><table><marquee><table></table></marquee></table></noscript>x</a></object>', 'M-09'];
        yield 'M-10.2 form around noscript' => ['<form><noscript><form>x</form></noscript></form>', 'M-10'];
    }

    #[DataProvider('invalidDocuments')]
    public function testInvalidDocument(string $html, string $rule): void
    {
        $result = Parser::parse($html);
        self::assertFalse($result->valid, 'the document is valid');
        self::assertSame($rule, $result->rule);
        self::assertSame(
            match ($rule) {
                'M-26' => ErrorCode::Limit,
                'M-17' => ErrorCode::Nested,
                default => ErrorCode::Structure,
            },
            $result->error,
        );
    }

    public function testManifestScriptInARegionOfCIsReserved(): void
    {
        $result = Parser::parse('<noscript><svg><script type="application/rivqen-manifest+json"></script></svg></noscript>');
        self::assertSame(ErrorCode::Reserved, $result->error);
        self::assertSame('M-13', $result->rule);
    }

    public function testReservedScriptInCIsReportedBeforeTheNoscriptRules(): void
    {
        // The block attribute is also an M-11 item 2 violation; M-13 is checked first (item 4).
        $result = Parser::parse('<noscript><script type="application/rivqen-manifest+json" data-rq-block="a"></script></noscript>');
        self::assertSame(ErrorCode::Reserved, $result->error);
        self::assertSame('M-13', $result->rule);
    }

    public function testDepthLimitInARegionOfC(): void
    {
        $within = '<noscript><svg>' . str_repeat('<g>', 254) . '</svg></noscript>';
        $over = '<noscript><svg>' . str_repeat('<g>', 255) . '</svg></noscript>';
        self::assertTrue(Parser::parse($within)->valid);
        $result = Parser::parse($over);
        self::assertSame(ErrorCode::Limit, $result->error);
        self::assertSame('M-26', $result->rule);
    }

    public function testRegionRootEndTagInContentKeepsItsUnits(): void
    {
        // M-26 item 5: the crossing makes the span dirty. The block element and each icon add 1 + 3 n.
        $icons = static fn(int $n): string => str_repeat('<span><svg><path></svg></span>', $n);
        self::assertTrue(Parser::parse('<div data-rq-block="a">' . $icons(85) . '</div>')->valid);
        $result = Parser::parse('<div data-rq-block="a">' . $icons(86) . '</div>');
        self::assertSame(ErrorCode::Limit, $result->error);
    }

    public function testBlocksAndOffsetsAroundARegionAndNoscript(): void
    {
        $html = '<noscript><svg><path d="M0"></svg></noscript><span data-rq-block="a"><svg><g></svg>x</span>';
        $result = Parser::parse($html);
        self::assertTrue($result->valid);
        self::assertCount(1, $result->blocks);
        $block = $result->blocks[0];
        self::assertSame('<svg><g></svg>x', substr($html, $block->start, $block->end - $block->start));
    }

    public function testNoscriptContentLeavesTheMainPassUnchanged(): void
    {
        // After </noscript> the main pass has its own stack, as without C.
        self::assertTrue(Parser::parse('<form><noscript><svg><path></svg></noscript></form><form></form>')->valid);
        $result = Parser::parse('<a><noscript><b></b></noscript><div></a>');
        self::assertSame('M-06', $result->rule);
    }
}
