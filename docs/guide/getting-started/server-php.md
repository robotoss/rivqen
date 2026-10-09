# Server: PHP

This guide adds Rivqen to a PHP application that uses PSR-15 middleware.

::: warning Planned API — not released
:::

## Requirements

- A supported PHP version (see [Support matrix](/guide/support-matrix)).
- A framework or stack with PSR-7 and PSR-15 support (for example Slim, Mezzio, or Laravel/Symfony through a PSR bridge).

## Step 1 — Install

```bash
composer require rivqen/rivqen-server
```

## Step 2 — Add the middleware

1. Add `RivqenMiddleware` to the middleware pipeline, before the handler that renders HTML.

```php
<?php
// Planned API — not released
use Rivqen\Server\RivqenMiddleware;
use Rivqen\Server\Config;

$app->add(new RivqenMiddleware(Config::defaults()));
```

## Step 3 — Check output buffering

1. Do not send headers or output before the middleware runs.
2. If your server compresses responses (gzip, brotli), let the reverse proxy do it after the middleware.

## Step 4 — Set cache headers and check normal visitors

1. Add `Cache-Control: private` to personal pages.
2. Open the page in a desktop browser. Check that the HTML did not change.

## Next steps

- [Engineering: PHP server SDK](/engineering/server/php)
