<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup;

/**
 * Result object of tools/rqp-markup/CONTRACT.md section 3.
 */
final class Result
{
    /**
     * @param list<Block> $blocks
     * @param ?string $rule The rule ID of the failure (for tests and review; not printed).
     */
    private function __construct(
        public readonly bool $valid,
        public readonly ?ErrorCode $error,
        public readonly array $blocks,
        public readonly ?string $templateRevision,
        public readonly ?string $pageRevision,
        public readonly ?string $rule,
    ) {}

    /**
     * @param list<Block> $blocks in document order
     */
    public static function valid(string $input, array $blocks): self
    {
        $templateRevision = Revision::templateRevision(Revision::template($input, $blocks));
        return new self(true, null, $blocks, $templateRevision, Revision::pageRevision($templateRevision, $blocks), null);
    }

    public static function invalid(ErrorCode $error, string $rule): self
    {
        return new self(false, $error, [], null, null, $rule);
    }

    /**
     * @return array{valid: bool, error: ?string, blocks: list<array{id: string, format: string, start: int, end: int, sha256: string}>, template_revision: ?string, page_revision: ?string}
     */
    public function toArray(): array
    {
        return [
            'valid' => $this->valid,
            'error' => $this->error?->value,
            'blocks' => array_map(static fn(Block $b): array => $b->toArray(), $this->blocks),
            'template_revision' => $this->templateRevision,
            'page_revision' => $this->pageRevision,
        ];
    }
}
