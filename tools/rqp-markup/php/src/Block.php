<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup;

/**
 * One block of a valid document (markup.md M-27). Content = input bytes [start, end).
 */
final class Block
{
    public function __construct(
        public readonly string $id,
        public readonly string $format,
        public readonly int $start,
        public readonly int $end,
        public readonly string $sha256,
    ) {}

    /**
     * @return array{id: string, format: string, start: int, end: int, sha256: string}
     */
    public function toArray(): array
    {
        return [
            'id' => $this->id,
            'format' => $this->format,
            'start' => $this->start,
            'end' => $this->end,
            'sha256' => $this->sha256,
        ];
    }
}
