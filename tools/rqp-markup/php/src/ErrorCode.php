<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup;

/**
 * Error codes of tools/rqp-markup/CONTRACT.md section 5.
 */
enum ErrorCode: string
{
    case Encoding = 'RQP_MARKUP_ENCODING';
    case Limit = 'RQP_MARKUP_LIMIT';
    case InvalidId = 'RQP_MARKUP_INVALID_ID';
    case ForbiddenElement = 'RQP_MARKUP_FORBIDDEN_ELEMENT';
    case Duplicate = 'RQP_MARKUP_DUPLICATE';
    case Nested = 'RQP_MARKUP_NESTED';
    case Structure = 'RQP_MARKUP_STRUCTURE';
    case BadJson = 'RQP_MARKUP_BAD_JSON';
    case Reserved = 'RQP_MARKUP_RESERVED';
}
