<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup;

/**
 * Hashes and revisions of tools/rqp-markup/CONTRACT.md section 4 (rqp/1).
 */
final class Revision
{
    private function __construct() {}

    /** B64U(x): base64url without padding (RFC 4648 section 5). */
    public static function b64u(string $bytes): string
    {
        return rtrim(strtr(base64_encode($bytes), '+/', '-_'), '=');
    }

    public static function b64uSha256(string $bytes): string
    {
        return self::b64u(hash('sha256', $bytes, true));
    }

    /**
     * The input with the content [start, end) of every block removed. The
     * start and end tags stay.
     *
     * @param list<Block> $blocks in document order
     */
    public static function template(string $input, array $blocks): string
    {
        $parts = [];
        $from = 0;
        foreach ($blocks as $block) {
            $parts[] = substr($input, $from, $block->start - $from);
            $from = $block->end;
        }
        $parts[] = substr($input, $from);
        return implode('', $parts);
    }

    public static function templateRevision(string $template): string
    {
        return 't1.' . self::b64uSha256("rqp-t1\n" . $template);
    }

    /**
     * @param list<Block> $blocks in document order
     */
    public static function pageRevision(string $templateRevision, array $blocks): string
    {
        $p = "rqp-r1\n" . $templateRevision . "\n";
        foreach ($blocks as $block) {
            $p .= $block->id . "\t" . $block->format . "\t" . $block->sha256 . "\n";
        }
        return 'r1.' . self::b64uSha256($p);
    }
}
