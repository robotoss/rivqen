# PHP server SDK

**Package:** `rivqen/rivqen-server` on Packagist. **Work package:** WP-15.

**Status:** <Badge type="info" text="DESIGN" />

## 1. Upstream for comparison

<Badge type="tip" text="FACT" /> Upstream `sonic-php` uses output buffering: `util_sonic::start()` calls `ob_start()`, `util_sonic::end()` reads the buffer, reads headers with `getallheaders()` (with a polyfill that lowercases names), and sends `304`, JSON data or full HTML (`upstream:sonic-php/sdk/sonic.php:14-145`). It uses static class fields for extracted blocks, which is safe in PHP-FPM (one request per process lifecycle) but not in long-running runtimes.

## 2. Design

| Item | Design |
|---|---|
| Interfaces | PSR-15 `MiddlewareInterface`; PSR-7 messages; PSR-17 factories (no hard dependency on one PSR-7 implementation) |
| PHP versions | Supported branches at release time. As of Oct 2026: 8.4 and 8.5 (8.2 security support ends 31 Dec 2026) |
| Runtimes | PHP-FPM; long-running runtimes (RoadRunner, FrankenPHP worker mode, Swoole) — therefore **no static request state** |
| Legacy integration | An adapter for apps without PSR-15, based on output buffering, documented as legacy |
| JSON | `json_encode` with `JSON_UNESCAPED_SLASHES \| JSON_UNESCAPED_UNICODE \| JSON_THROW_ON_ERROR` |
| Extension | No PHP extension in the MVP; measure first, then decide (Rust FFI or sidecar) |

## 3. Rules

1. Header names are case-insensitive (PSR-7 guarantees this; legacy adapter must normalize).
2. Never send headers or output before the middleware runs ("headers already sent" is an error, not a fallback).
3. Respect `memory_limit`: refuse to buffer above `maxDocumentBytes` and pass the response through unchanged.
4. Let the web server or proxy compress after the middleware.

## Related

- [Server SDKs overview](/engineering/server/)
- [Getting started: PHP](/guide/getting-started/server-php)
