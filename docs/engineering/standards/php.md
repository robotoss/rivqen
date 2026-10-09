# PHP

PHP is used for the PHP server SDK (`server/php`, Composer package `rivqen/rivqen-server`): PSR-15 middleware and an output-buffer helper.

**Status:** <Badge type="info" text="DESIGN" /> · <Badge type="tip" text="FACT" /> versions checked 2026-10-09; pin them in WP-15

[[toc]]

## 1. References

- [PER Coding Style 3.x](https://www.php-fig.org/per/coding-style/) (extends and replaces PSR-12)
- PSR-7 (HTTP messages), PSR-15 (middleware), PSR-17 (factories), PSR-3 (logger)
- [PHP server SDK](/engineering/server/php), [Server conformance](/engineering/server/conformance)

## 2. Toolchain

| Tool | Version (2026-10-09) | License | Command |
|---|---|---|---|
| PHP | 8.2+ (final range in Q-09) | PHP License | — |
| PHP-CS-Fixer | 3.95.27 | MIT | `vendor/bin/php-cs-fixer check --diff` with `'@PER-CS' => true` |
| PHPStan | 2.3.1 | MIT | `vendor/bin/phpstan analyse --level=max` |
| PHPUnit | 13.4.1 (latest) | BSD-3-Clause | `vendor/bin/phpunit` |
| Infection | 0.35.6 | BSD-3-Clause | See [Mutation testing](/engineering/quality/mutation) |

::: warning Check at WP-15 kickoff
The newest PHPUnit major can require a newer PHP than our minimum. Pick the PHPUnit major that runs on the lowest supported PHP version, or test the lowest version with an older major. Decide together with Q-09.
:::

## 3. Design rules

1. `declare(strict_types=1);` in every file.
2. **Pure core.** A framework-free class decides `HTML in → response out`. The PSR-15 middleware and the output-buffer helper only adapt.
3. **Types everywhere:** parameter, return and property types; `readonly` properties; `final` classes by default. No `mixed` in the public API.
4. **Errors.** Exceptions with a stable code and the previous exception. On any internal error, the original response passes through unchanged ([Errors and logging](/engineering/architecture/errors-logging)).
5. **Header names are case-insensitive.** Always read headers through PSR-7 (upstream trace T-03 showed a case-sensitive bug).
6. **No global state, no superglobals** in the core.
7. **Logging** through PSR-3 with allowlisted fields.

## 4. Tests

- PHPUnit unit tests for the core; middleware tests with PSR-7 fakes.
- The cross-language conformance suite runs against the PHP server.

## Related

- [Coding standards](/engineering/standards/)
- [Getting started: PHP](/guide/getting-started/server-php)
