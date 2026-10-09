---
paths:
  - "**/*.php"
  - "**/composer.json"
---
# PHP checklist

Source: `docs/engineering/standards/php.md` (full text; change both together).

- Run: `vendor/bin/php-cs-fixer check --diff`, `vendor/bin/phpstan analyse --level=max`, `vendor/bin/phpunit`.
- `declare(strict_types=1);` in every file. Full types; `readonly`; `final` by default; no `mixed` in public API.
- Pure decision core; PSR-15 middleware and the output-buffer helper only adapt.
- Read headers through PSR-7 (case-insensitive). No superglobals in the core.
- On internal error, pass the original response through unchanged. PSR-3 logging with allowlisted fields.
- Mutation: `vendor/bin/infection --git-diff-lines --git-diff-base=main --min-msi=80`.
