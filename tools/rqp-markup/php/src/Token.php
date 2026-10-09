<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup;

/**
 * One token with its byte span [start, end) in the input.
 *
 * `start` is the offset of the `<`. `end` is the offset after the closing `>`,
 * or the input length when the token ends at EOF.
 */
final class Token
{
    /**
     * @param array<string, string> $attributes Raw values (DL-010) of the first
     *        occurrence of each watched attribute name (Tokenizer::WATCHED).
     *        An attribute without a value has the empty value.
     */
    public function __construct(
        public readonly TokenType $type,
        public readonly int $start,
        public readonly int $end,
        public readonly string $name = '',
        public readonly bool $selfClosing = false,
        public readonly array $attributes = [],
        public readonly bool $cdataClosed = false,
    ) {}

    public function attribute(string $name): ?string
    {
        return $this->attributes[$name] ?? null;
    }
}
