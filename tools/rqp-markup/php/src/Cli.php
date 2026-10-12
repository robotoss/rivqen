<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup;

/**
 * Command line of tools/rqp-markup/CONTRACT.md section 2.
 *
 *   rqp-markup <path-to-input.html>
 *   rqp-markup --batch <fixtures-dir>
 *
 * Exit codes: 0 results printed (valid and invalid documents); 2 usage or
 * I/O error; 70 internal error (a defect). stderr never gets document
 * content: only paths, exception class names and source lines.
 *
 * A defect in the parser for one document fails closed for that document:
 * its result line is invalid with RQP_MARKUP_STRUCTURE, the defect goes to
 * stderr, the next documents are still processed, and the exit code is 70.
 */
final class Cli
{
    public const EXIT_OK = 0;
    public const EXIT_USAGE = 2;
    public const EXIT_INTERNAL = 70;

    private const USAGE = "usage: rqp-markup <input.html>\n       rqp-markup --batch <fixtures-dir>\n";
    private const JSON_FLAGS = JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE
        | JSON_INVALID_UTF8_SUBSTITUTE | JSON_THROW_ON_ERROR;

    private function __construct() {}

    /**
     * @param list<string> $argv
     * @param resource $out
     * @param resource $err
     * @param (callable(string): Result)|null $parser test seam; Parser::parse by default
     */
    public static function run(array $argv, $out, $err, ?callable $parser = null): int
    {
        // Every PHP warning becomes an exception, so nothing is ignored and
        // nothing is printed to stdout.
        set_error_handler(static function (int $severity, string $message, string $file, int $line): bool {
            throw new \ErrorException($message, 0, $severity, $file, $line);
        });
        try {
            return self::main($argv, $out, $err, $parser ?? Parser::parse(...));
        } catch (UsageError $e) {
            self::writeError($err, 'rqp-markup: ' . $e->getMessage() . "\n");
            return self::EXIT_USAGE;
        } catch (\Throwable $e) {
            // A defect. Print the class and the source line, never the message:
            // a message could contain document bytes.
            self::writeError($err, sprintf(
                "rqp-markup: internal error: %s at %s:%d\n",
                $e::class,
                basename($e->getFile()),
                $e->getLine(),
            ));
            return self::EXIT_INTERNAL;
        } finally {
            restore_error_handler();
        }
    }

    /**
     * @param list<string> $argv
     * @param resource $out
     * @param resource $err
     * @param callable(string): Result $parser
     */
    private static function main(array $argv, $out, $err, callable $parser): int
    {
        $args = array_slice($argv, 1);
        if (count($args) === 2 && $args[0] === '--batch') {
            $defect = self::batch($args[1], $out, $err, $parser);
            return $defect ? self::EXIT_INTERNAL : self::EXIT_OK;
        }
        // Only "--..." is an option; "-page.html" is a path.
        if (count($args) === 1 && !str_starts_with($args[0], '--')) {
            $defect = false;
            $line = self::parseDocument(self::readInput($args[0]), $parser, $err, $defect);
            self::writeLine($out, $line);
            return $defect ? self::EXIT_INTERNAL : self::EXIT_OK;
        }
        throw new UsageError(trim(self::USAGE));
    }

    /**
     * @param resource $out
     * @param resource $err
     * @param callable(string): Result $parser
     * @return bool true when the parser had a defect for at least one document
     */
    private static function batch(string $dir, $out, $err, callable $parser): bool
    {
        $defect = false;
        if (!is_dir($dir)) {
            throw new UsageError('not a directory: ' . $dir);
        }
        try {
            $entries = scandir($dir);
        } catch (\ErrorException) {
            $entries = false;
        }
        if ($entries === false) {
            throw new UsageError('cannot read directory: ' . $dir);
        }
        // Byte order of the names (CONTRACT.md section 2, item 2).
        sort($entries, SORT_STRING);
        foreach ($entries as $name) {
            if ($name === '.' || $name === '..') {
                continue;
            }
            $sub = $dir . DIRECTORY_SEPARATOR . $name;
            $file = $sub . DIRECTORY_SEPARATOR . 'input.html';
            // A missing input.html skips the entry. Any other kind of entry
            // goes to readInput(), which rejects it (CONTRACT.md section 2,
            // item 7).
            if (!is_dir($sub) || !file_exists($file)) {
                continue;
            }
            $line = self::parseDocument(self::readInput($file), $parser, $err, $defect) + ['fixture' => $name];
            self::writeLine($out, $line);
        }
        return $defect;
    }

    /**
     * Parse one document. A Throwable from the parser is a defect: report
     * the class and the source line (never the message, which could hold
     * document bytes) and return the fail-closed result.
     *
     * @param callable(string): Result $parser
     * @param resource $err
     * @return array<string, mixed>
     */
    private static function parseDocument(string $input, callable $parser, $err, bool &$defect): array
    {
        try {
            return $parser($input)->toArray();
        } catch (\Throwable $e) {
            $defect = true;
            self::writeError($err, sprintf(
                "rqp-markup: internal error: %s at %s:%d\n",
                $e::class,
                basename($e->getFile()),
                $e->getLine(),
            ));
            return Result::invalid(ErrorCode::Structure, 'internal')->toArray();
        }
    }

    /**
     * Read at most MAX_INPUT_BYTES + 1 bytes: enough to apply M-01 and
     * bounded for any file size.
     */
    public static function readInput(string $path): string
    {
        if (!is_file($path)) {
            throw new UsageError('not a file: ' . $path);
        }
        try {
            $handle = fopen($path, 'rb');
            if ($handle === false) {
                throw new UsageError('cannot open: ' . $path);
            }
            try {
                // The path may have changed after is_file(): check the handle.
                $stat = fstat($handle);
                if ($stat === false || ($stat['mode'] & 0o170000) !== 0o100000) {
                    throw new UsageError('not a file: ' . $path);
                }
                $data = stream_get_contents($handle, Parser::MAX_INPUT_BYTES + 1);
            } finally {
                fclose($handle);
            }
        } catch (\ErrorException) {
            throw new UsageError('cannot read: ' . $path);
        }
        if ($data === false) {
            throw new UsageError('cannot read: ' . $path);
        }
        return $data;
    }

    /**
     * @param resource $out
     * @param array<string, mixed> $object
     */
    private static function writeLine($out, array $object): void
    {
        $line = json_encode($object, self::JSON_FLAGS) . "\n";
        try {
            $written = fwrite($out, $line);
        } catch (\ErrorException) {
            $written = false;
        }
        if ($written !== strlen($line)) {
            throw new UsageError('cannot write to stdout');
        }
    }

    /**
     * @param resource $err
     */
    private static function writeError($err, string $text): void
    {
        try {
            fwrite($err, $text);
        } catch (\Throwable) {
            // stderr is gone: there is no other channel to report on.
        }
    }
}
