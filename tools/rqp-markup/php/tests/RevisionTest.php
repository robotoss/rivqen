<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup\Tests;

use PHPUnit\Framework\TestCase;
use Rivqen\Tools\RqpMarkup\Block;
use Rivqen\Tools\RqpMarkup\Revision;

/**
 * Hashes and revisions of CONTRACT.md section 4.
 */
final class RevisionTest extends TestCase
{
    public function testB64uHasNoPaddingAndUsesUrlAlphabet(): void
    {
        // RFC 4648 section 10 vectors, URL alphabet, no padding.
        self::assertSame('', Revision::b64u(''));
        self::assertSame('Zg', Revision::b64u('f'));
        self::assertSame('Zm8', Revision::b64u('fo'));
        self::assertSame('Zm9v', Revision::b64u('foo'));
        self::assertSame('-_-_', Revision::b64u("\xFB\xFF\xBF"));
    }

    public function testSha256OfEmptyContent(): void
    {
        self::assertSame('47DEQpj8HBSa-_TImW-5JCeuQeRkm5NMpJWZG3hSuFU', Revision::b64uSha256(''));
    }

    public function testTemplateRemovesBlockContentAndKeepsTags(): void
    {
        $input = 'A<p>xx</p>B<i>yyy</i>C';
        $blocks = [new Block('a', 'html', 4, 6, ''), new Block('b', 'html', 14, 17, '')];
        self::assertSame('A<p></p>B<i></i>C', Revision::template($input, $blocks));
        self::assertSame($input, Revision::template($input, []));
    }

    public function testTemplateRevisionHasPrefixAndDomainSeparation(): void
    {
        self::assertSame('t1.' . Revision::b64uSha256("rqp-t1\nabc"), Revision::templateRevision('abc'));
    }

    public function testPageRevisionListsBlocksInOrder(): void
    {
        $blocks = [new Block('b', 'html', 0, 0, 'H1'), new Block('a', 'json', 0, 0, 'H2')];
        $expected = 'r1.' . Revision::b64uSha256("rqp-r1\nt1.T\nb\thtml\tH1\na\tjson\tH2\n");
        self::assertSame($expected, Revision::pageRevision('t1.T', $blocks));
        self::assertSame('r1.' . Revision::b64uSha256("rqp-r1\nt1.T\n"), Revision::pageRevision('t1.T', []));
    }
}
