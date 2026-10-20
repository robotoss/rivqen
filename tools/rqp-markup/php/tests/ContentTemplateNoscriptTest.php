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
 * The WP-17 S5 rules (H-94 to H-97, DL-055): `template` and `noscript` are
 * allowed in block content (M-22 item 2), M-22 item 6 (`shadowrootmode`, and
 * `shadowroot` since DL-056),
 * M-24 item g with "F is none", and M-11 item 10 (M-22 and M-24 on the tokens
 * of C in a block). Each invalid case checks the rule ID.
 */
final class ContentTemplateNoscriptTest extends TestCase
{
    private const BLOCK = '<div data-rq-block="a">';

    /**
     * @return iterable<string, array{string}>
     */
    public static function validDocuments(): iterable
    {
        $B = self::BLOCK;
        yield 'M-22.2 template in the content' => [$B . '<template><p>x</p></template></div>'];
        yield 'M-22.2 noscript in the content' => [$B . '<noscript><b>x</b></noscript></div>'];
        yield 'M-22.6 other shadowroot attributes' => [$B . '<template shadowrootclonable shadowrootdelegatesfocus shadowrootserializable data-shadowrootmode="open"></template></div>'];
        yield 'M-22.6 shadowrootmode outside the content' => ['<template shadowrootmode="open"></template>' . $B . 'x</div>'];
        yield 'M-22.6 a foreign template with shadowrootmode' => [$B . '<svg><template shadowrootmode="open"></template></svg></div>'];
        yield 'M-22.6 names like shadowroot (DL-056)' => [$B . '<template shadowrootx="open" shadowroots data-shadowroot shadowroo title="shadowroot"></template></div>'];
        yield 'M-22.6 a non-ASCII look-alike of shadowroot (DL-056)' => [$B . "<template \u{17F}hadowroot></template></div>"];
        yield 'M-22.6 shadowroot outside the content (DL-056)' => ['<template shadowroot="open"></template>' . $B . 'x</div>'];
        yield 'M-22.6 a foreign template with shadowroot (DL-056)' => [$B . '<svg><template shadowroot="open"></template></svg></div>'];
        yield 'M-22.6 shadowroot on another element (DL-056)' => [$B . '<div shadowroot="open"></div></div>'];
        yield 'M-24.g table in a template' => [$B . '<template><table><tr><td>x</td></tr></table></template></div>'];
        yield 'M-24.g table in the content, template in a cell' => [$B . '<table><tr><td><template><p>x</p></template></td></tr></table></div>'];
        yield 'M-11.10 table in noscript in the content' => [$B . '<noscript><table><tr><td>x</td></tr></table></noscript></div>'];
        yield 'M-11.10 phrasing and an icon in noscript' => ['<span data-rq-block="a"><noscript><img src="x"><svg><path d="M0"></svg></noscript></span>'];
        yield 'M-11.10 noscript in a template of the content' => [$B . '<template><noscript><b>x</b></noscript></template></div>'];
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
        // M-22 item 6.
        yield 'M-22.6 shadowrootmode with a value' => [$B . '<template shadowrootmode="open"></template></div>', 'M-22'];
        yield 'M-22.6 shadowrootmode without a value' => [$B . '<template shadowrootmode></template></div>', 'M-22'];
        yield 'M-22.6 shadowrootmode in upper case' => [$B . '<template ShadowRootMode="x"></template></div>', 'M-22'];
        yield 'M-22.6 shadowrootmode after other attributes' => [$B . '<template id="t" shadowrootclonable shadowrootmode></template></div>', 'M-22'];
        yield 'M-22.6 a duplicate shadowrootmode' => [$B . '<template shadowrootmode shadowrootmode="open"></template></div>', 'M-22'];
        yield 'M-22.6 nested template' => [$B . '<template><template shadowrootmode="open"></template></template></div>', 'M-22'];
        // M-22 item 6, the old Chromium name (DL-056).
        yield 'M-22.6 shadowroot with a value' => [$B . '<template shadowroot="open"></template></div>', 'M-22'];
        yield 'M-22.6 shadowroot without a value' => [$B . '<template shadowroot></template></div>', 'M-22'];
        yield 'M-22.6 shadowroot in upper case' => [$B . '<template SHADOWROOT=closed></template></div>', 'M-22'];
        yield 'M-22.6 shadowroot after other attributes' => [$B . '<template id="t" class="c" shadowrootclonable shadowroot="open"></template></div>', 'M-22'];
        yield 'M-22.6 a duplicate shadowroot' => [$B . '<template ShadowRoot="" class="b" shadowroot="open"></template></div>', 'M-22'];
        yield 'M-22.6 shadowroot in a nested template' => [$B . '<template><b><template shadowroot></template></b></template></div>', 'M-22'];
        // M-24 item g with F none.
        yield 'M-24.g tr in a template without a table' => [$B . '<template><tr></tr></template></div>', 'M-24'];
        yield 'M-24.g td in a template of a table' => [$B . '<table><template><td>x</td></template></table></div>', 'M-24'];
        yield 'M-24.g tr in a template inside a cell' => [$B . '<table><tr><td><template><tr></tr></template></td></tr></table></div>', 'M-24'];
        yield 'M-09.1 col in a template' => [$B . '<template><col></template></div>', 'M-09'];
        // The other rules of the content in the template contents.
        yield 'M-22.2 textarea in a template' => [$B . '<template><textarea></textarea></template></div>', 'M-22'];
        yield 'M-23 the block end tag before the template end tag' => [$B . '<template></div>', 'M-23'];
        yield 'M-23 the template end tag inside another element' => [$B . '<template><b></template></b></div>', 'M-23'];
        yield 'M-10.3 template for' => [$B . '<template for="p"></template></div>', 'M-10'];
        // M-11 item 10.
        yield 'M-24.a p-closing tag in noscript in a p' => ['<p data-rq-block="a"><noscript><div>x</div></noscript></p>', 'M-24'];
        yield 'M-24.b heading in noscript in a heading' => ['<h2 data-rq-block="h"><noscript><h3>x</h3></noscript></h2>', 'M-24'];
        yield 'M-24.c li in noscript without a list' => [$B . '<noscript><li>x</li></noscript></div>', 'M-24'];
        yield 'M-24.e a in noscript in an a' => [$B . '<a href="#"><noscript><a href="#">x</a></noscript></a></div>', 'M-24'];
        yield 'M-24.k foster parent in noscript' => [$B . '<noscript><table><b>x</b></table></noscript></div>', 'M-24'];
        yield 'M-22.2 textarea in noscript' => [$B . '<noscript><textarea>t</textarea></noscript></div>', 'M-22'];
        yield 'M-22.1 DOCTYPE in noscript' => [$B . '<noscript><!DOCTYPE html></noscript></div>', 'M-22'];
        yield 'M-22.3 self-closing html element in noscript' => [$B . '<noscript><b/></noscript></div>', 'M-22'];
        yield 'M-22.6 shadowrootmode after noscript in a template' => [$B . '<template><noscript><i>x</i></noscript><template shadowrootmode></template></template></div>', 'M-22'];
        yield 'M-11.10 end tag of the block element in C' => [$B . '<noscript></div></noscript></div>', 'M-11'];
        yield 'M-11.6 table part in noscript without a table' => [$B . '<noscript><tr></tr></noscript></div>', 'M-11'];
        yield 'M-11.3 template in C' => [$B . '<noscript><template></template></noscript></div>', 'M-11'];
        yield 'M-24.a p-closing tag in noscript in a template' => ['<p data-rq-block="a"><template><noscript><div>x</div></noscript></template></p>', 'M-24'];
    }

    #[DataProvider('invalidDocuments')]
    public function testInvalidDocument(string $html, string $rule): void
    {
        $result = Parser::parse($html);
        self::assertFalse($result->valid, 'the document is valid');
        self::assertSame($rule, $result->rule);
        self::assertSame(ErrorCode::Structure, $result->error);
    }

    public function testManifestScriptInNoscriptInContentIsReserved(): void
    {
        $result = Parser::parse(self::BLOCK . '<noscript><script type="application/rivqen-manifest+json"></script></noscript></div>');
        self::assertSame(ErrorCode::Reserved, $result->error);
        self::assertSame('M-13', $result->rule);
    }

    public function testBlockBytesAreTheContentWithTemplateAndNoscript(): void
    {
        $html = self::BLOCK . '<template><b>t</b></template><noscript><i>n</i></noscript></div>';
        $result = Parser::parse($html);
        self::assertTrue($result->valid);
        self::assertCount(1, $result->blocks);
        $block = $result->blocks[0];
        self::assertSame(
            '<template><b>t</b></template><noscript><i>n</i></noscript>',
            substr($html, $block->start, $block->end - $block->start),
        );
    }
}
