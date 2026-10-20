<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup;

/**
 * The tokenizer state between tokens (WHATWG HTML 13.2.5.1 to 13.2.5.5).
 * The parser changes it after a start tag by markup.md M-04.
 */
enum ContentState
{
    case Data;
    case Rcdata;
    case Rawtext;
    case ScriptData;
    case Plaintext;
}
