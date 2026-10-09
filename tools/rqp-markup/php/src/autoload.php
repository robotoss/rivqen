<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

// Class loader for bin/rqp-markup. The command runs without `composer install`
// (no build step, no runtime dependency).
spl_autoload_register(static function (string $class): void {
    $prefix = 'Rivqen\\Tools\\RqpMarkup\\';
    if (!str_starts_with($class, $prefix)) {
        return;
    }
    $relative = substr($class, strlen($prefix));
    if (preg_match('/^[A-Za-z0-9]+$/D', $relative) !== 1) {
        return;
    }
    $file = __DIR__ . '/' . $relative . '.php';
    if (is_file($file)) {
        require $file;
    }
});
