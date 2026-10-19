<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup;

/**
 * Name sets of markup.md section 2.1, as sets (name => true).
 */
final class Names
{
    public const VOID = [
        'area' => true, 'base' => true, 'basefont' => true, 'bgsound' => true, 'br' => true,
        'col' => true, 'embed' => true, 'frame' => true, 'hr' => true, 'image' => true,
        'img' => true, 'input' => true, 'keygen' => true, 'link' => true, 'meta' => true,
        'param' => true, 'source' => true, 'track' => true, 'wbr' => true,
    ];

    /** TEXT with the state that M-04 sets. */
    public const TEXT = [
        'title' => ContentState::Rcdata,
        'textarea' => ContentState::Rcdata,
        'iframe' => ContentState::Rawtext,
        'noembed' => ContentState::Rawtext,
        'noframes' => ContentState::Rawtext,
        'noscript' => ContentState::Rawtext,
        'style' => ContentState::Rawtext,
        'xmp' => ContentState::Rawtext,
        'script' => ContentState::ScriptData,
        'plaintext' => ContentState::Plaintext,
    ];

    public const PCLOSE = [
        'address' => true, 'article' => true, 'aside' => true, 'blockquote' => true,
        'center' => true, 'dd' => true, 'details' => true, 'dialog' => true, 'dir' => true,
        'div' => true, 'dl' => true, 'dt' => true, 'fieldset' => true, 'figcaption' => true,
        'figure' => true, 'footer' => true, 'form' => true, 'h1' => true, 'h2' => true,
        'h3' => true, 'h4' => true, 'h5' => true, 'h6' => true, 'header' => true,
        'hgroup' => true, 'hr' => true, 'li' => true, 'listing' => true, 'main' => true,
        'menu' => true, 'nav' => true, 'ol' => true, 'p' => true, 'plaintext' => true,
        'pre' => true, 'search' => true, 'section' => true, 'summary' => true, 'table' => true,
        'ul' => true, 'xmp' => true,
    ];

    public const GUARDED = [
        'a' => true, 'applet' => true, 'button' => true, 'caption' => true, 'colgroup' => true,
        'marquee' => true, 'math' => true, 'nobr' => true, 'object' => true, 'select' => true,
        'svg' => true, 'table' => true, 'tbody' => true, 'td' => true, 'template' => true,
        'tfoot' => true, 'th' => true, 'thead' => true, 'tr' => true,
    ];

    public const TABLE_PART = [
        'caption' => true, 'colgroup' => true, 'tbody' => true, 'td' => true,
        'tfoot' => true, 'th' => true, 'thead' => true, 'tr' => true,
    ];

    public const TABLE_FAMILY = self::TABLE_PART + ['table' => true];

    public const BREAKOUT = [
        'b' => true, 'big' => true, 'blockquote' => true, 'body' => true, 'br' => true,
        'center' => true, 'code' => true, 'dd' => true, 'div' => true, 'dl' => true,
        'dt' => true, 'em' => true, 'embed' => true, 'font' => true, 'h1' => true,
        'h2' => true, 'h3' => true, 'h4' => true, 'h5' => true, 'h6' => true,
        'head' => true, 'hr' => true, 'i' => true, 'img' => true, 'li' => true,
        'listing' => true, 'menu' => true, 'meta' => true, 'nobr' => true, 'ol' => true,
        'p' => true, 'pre' => true, 'ruby' => true, 's' => true, 'small' => true,
        'span' => true, 'strike' => true, 'strong' => true, 'sub' => true, 'sup' => true,
        'table' => true, 'tt' => true, 'u' => true, 'ul' => true, 'var' => true,
    ];

    public const INTEGRATION = [
        'annotation-xml' => true, 'desc' => true, 'foreignobject' => true, 'mi' => true,
        'mn' => true, 'mo' => true, 'ms' => true, 'mtext' => true, 'title' => true,
    ];

    public const BLOCK_HTML = [
        'article' => true, 'aside' => true, 'div' => true, 'footer' => true, 'h1' => true,
        'h2' => true, 'h3' => true, 'h4' => true, 'h5' => true, 'h6' => true,
        'header' => true, 'main' => true, 'nav' => true, 'p' => true, 'section' => true,
        'span' => true, 'title' => true,
    ];

    /** M-22 item 2 (H-78, H-94, H-95): start tag names not allowed in the content. */
    public const FORBIDDEN_CONTENT = [
        'body' => true, 'frame' => true, 'frameset' => true, 'head' => true, 'html' => true,
        'noembed' => true, 'noframes' => true, 'optgroup' => true,
        'option' => true, 'plaintext' => true, 'select' => true,
        'textarea' => true, 'title' => true, 'xmp' => true,
    ];

    public const HEADING = [
        'h1' => true, 'h2' => true, 'h3' => true, 'h4' => true, 'h5' => true, 'h6' => true,
    ];

    /** M-24 items c and j: lists. */
    public const LIST = ['ul' => true, 'ol' => true, 'menu' => true, 'dl' => true];

    /** M-24 items c and j: list items. */
    public const LIST_ITEM = ['li' => true, 'dd' => true, 'dt' => true];

    /**
     * Values of F (M-09) of table context. M-24 item k: only TABLE_PART and
     * `col` start tags are allowed in the content. M-09 condition 2: a
     * TABLE_PART, `col` or `table` start tag needs F as the current node.
     */
    public const TABLE_CONTEXT = [
        'table' => true, 'tbody' => true, 'thead' => true, 'tfoot' => true, 'tr' => true, 'colgroup' => true,
    ];

    /** M-08: start tags allowed inside `select`. */
    public const SELECT_START = ['option' => true, 'optgroup' => true, 'hr' => true];

    /** M-08: end tags allowed inside `select`. */
    public const SELECT_END = ['option' => true, 'optgroup' => true, 'select' => true];

    /** M-11 item 3: start tags not allowed in `noscript` content (`svg` starts a region, item 9). */
    public const NOSCRIPT_FORBIDDEN = [
        'frameset' => true, 'math' => true, 'noscript' => true, 'plaintext' => true,
        'select' => true, 'template' => true,
    ];

    /** M-11 item 7: tags not allowed in `noscript` content, start and end. */
    public const NOSCRIPT_DOCUMENT_TAGS = ['html' => true, 'body' => true, 'head' => true];

    /** M-09: allowed F for each table part start tag. */
    public const TABLE_START_ALLOWED_F = [
        'caption' => ['table' => true],
        'colgroup' => ['table' => true],
        'tbody' => ['table' => true],
        'thead' => ['table' => true],
        'tfoot' => ['table' => true],
        'col' => ['table' => true, 'colgroup' => true],
        'tr' => ['table' => true, 'tbody' => true, 'thead' => true, 'tfoot' => true],
        'td' => ['table' => true, 'tbody' => true, 'thead' => true, 'tfoot' => true, 'tr' => true],
        'th' => ['table' => true, 'tbody' => true, 'thead' => true, 'tfoot' => true, 'tr' => true],
    ];

    /** M-19: allowed F for a block start tag (none is also allowed). */
    public const BLOCK_ALLOWED_F = ['td' => true, 'th' => true, 'caption' => true];

    /** M-06 items 2 and 4: the formatting category of WHATWG 13.2.4.2. */
    public const FORMATTING = [
        'a' => true, 'b' => true, 'big' => true, 'code' => true, 'em' => true, 'font' => true,
        'i' => true, 'nobr' => true, 's' => true, 'small' => true, 'strike' => true,
        'strong' => true, 'tt' => true, 'u' => true,
    ];

    /** M-06 items 2 and 4: the HTML part of the special category of WHATWG 13.2.4.2. */
    public const SPECIAL = [
        'address' => true, 'applet' => true, 'area' => true, 'article' => true, 'aside' => true,
        'base' => true, 'basefont' => true, 'bgsound' => true, 'blockquote' => true, 'body' => true,
        'br' => true, 'button' => true, 'caption' => true, 'center' => true, 'col' => true,
        'colgroup' => true, 'dd' => true, 'details' => true, 'dir' => true, 'div' => true,
        'dl' => true, 'dt' => true, 'embed' => true, 'fieldset' => true, 'figcaption' => true,
        'figure' => true, 'footer' => true, 'form' => true, 'frame' => true, 'frameset' => true,
        'h1' => true, 'h2' => true, 'h3' => true, 'h4' => true, 'h5' => true, 'h6' => true,
        'head' => true, 'header' => true, 'hgroup' => true, 'hr' => true, 'html' => true,
        'iframe' => true, 'img' => true, 'input' => true, 'keygen' => true, 'li' => true,
        'link' => true, 'listing' => true, 'main' => true, 'marquee' => true, 'menu' => true,
        'meta' => true, 'nav' => true, 'noembed' => true, 'noframes' => true, 'noscript' => true,
        'object' => true, 'ol' => true, 'p' => true, 'param' => true, 'plaintext' => true,
        'pre' => true, 'script' => true, 'search' => true, 'section' => true, 'select' => true,
        'source' => true, 'style' => true, 'summary' => true, 'table' => true, 'tbody' => true,
        'td' => true, 'template' => true, 'textarea' => true, 'tfoot' => true, 'th' => true,
        'thead' => true, 'title' => true, 'tr' => true, 'track' => true, 'ul' => true,
        'wbr' => true, 'xmp' => true,
    ];

    /** M-06 item 3: end tags that must match an element in HTML context. */
    public const STRAY_TABLE_END = [
        'tbody' => true, 'thead' => true, 'tfoot' => true, 'tr' => true, 'table' => true,
    ];

    private function __construct() {}
}
