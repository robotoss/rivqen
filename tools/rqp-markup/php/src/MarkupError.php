<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup;

/**
 * A document breaks a rule of markup.md section 2.
 *
 * Internal control flow of the parser: Parser::parse() converts it to an
 * invalid Result. The message names only the rule, never document content.
 */
final class MarkupError extends \RuntimeException
{
    public function __construct(
        public readonly ErrorCode $errorCode,
        public readonly string $rule,
    ) {
        parent::__construct($rule . ': ' . $errorCode->value);
    }
}
