<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup;

/**
 * A usage error or an I/O error (exit code 2). The message has paths only,
 * never document content.
 */
final class UsageError extends \RuntimeException {}
