<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup;

/**
 * Token types that the rules need. Character tokens are not emitted: no rule
 * depends on them (markup.md M-07, M-08 and M-22 allow them everywhere).
 */
enum TokenType
{
    case StartTag;
    case EndTag;
    /** A comment, a bogus comment or a processing instruction (M-03). */
    case Comment;
    /** `<![CDATA[` in the data state; boundaries as a bogus comment (M-12). */
    case Cdata;
    case Doctype;
    case Eof;
}
