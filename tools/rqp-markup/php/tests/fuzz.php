<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

// Fuzz target for the tokenizer and the parser (not part of the PHPUnit run).
//
//   php tests/fuzz.php [--seconds N] [--seed N] [--crashes DIR]
//
// Generates documents from fragments, mutates the fixture inputs and
// generated valid documents, then checks the invariants of
// Tests\Fuzz\Harness (for unmutated valid documents also the expected blocks). On a violation it writes the
// input to DIR/crash-<seed>-<n>.html, prints only the file name and the
// violation (never document content), and exits with code 1.

use Random\Engine\Mt19937;
use Random\Randomizer;
use Rivqen\Tools\RqpMarkup\Tests\Fuzz\Harness;

require dirname(__DIR__) . '/vendor/autoload.php';

ini_set('memory_limit', '512M');

$options = getopt('', ['seconds:', 'seed:', 'crashes:']);
$option = static function (string $name, string $default) use ($options): string {
    $value = is_array($options) ? ($options[$name] ?? $default) : $default;
    return is_string($value) ? $value : $default;
};
$seconds = (int) $option('seconds', '60');
$seed = (int) $option('seed', (string) random_int(1, PHP_INT_MAX));
$crashes = $option('crashes', sys_get_temp_dir());

$random = new Randomizer(new Mt19937($seed));
$corpus = Harness::corpus(dirname(__DIR__, 4) . '/fixtures/rqp/markup');
if ($corpus === []) {
    fwrite(STDERR, "fuzz: no fixture corpus found\n");
    exit(2);
}

$deadline = hrtime(true) + $seconds * 1_000_000_000;
$n = 0;
while (hrtime(true) < $deadline) {
    $mode = $random->getInt(0, 3);
    $expected = null;
    if ($mode === 0) {
        $html = Harness::document($random, 200);
    } elseif ($mode === 1) {
        $html = Harness::mutate($random, $corpus[$random->getInt(0, count($corpus) - 1)]);
    } else {
        [$html, $expected] = Harness::validDocument($random);
        if ($mode === 2) {
            $html = Harness::mutate($random, $html);
            $expected = null;
        }
    }
    $violation = $expected === null ? Harness::check($html) : Harness::checkExpected($html, $expected);
    if ($violation !== null) {
        $file = $crashes . '/crash-' . $seed . '-' . $n . '.html';
        file_put_contents($file, $html);
        fwrite(STDERR, "fuzz: violation \"$violation\"; input written to $file\n");
        exit(1);
    }
    $n++;
}
fwrite(STDOUT, "fuzz: seed $seed, $n inputs, no violation\n");
exit(0);
