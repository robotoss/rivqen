<?php

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

declare(strict_types=1);

namespace Rivqen\Tools\RqpMarkup\Tests;

use PHPUnit\Framework\TestCase;
use Rivqen\Tools\RqpMarkup\Cli;
use Rivqen\Tools\RqpMarkup\Parser;
use Rivqen\Tools\RqpMarkup\Result;

/**
 * Command line of CONTRACT.md section 2.
 */
final class CliTest extends TestCase
{
    private string $tmp = '';

    protected function setUp(): void
    {
        $base = sys_get_temp_dir() . '/rqp-markup-php-' . bin2hex(random_bytes(6));
        self::assertTrue(mkdir($base, 0o700));
        $this->tmp = $base;
    }

    protected function tearDown(): void
    {
        self::removeTree($this->tmp);
    }

    /**
     * @param list<string> $args
     * @param (callable(string): Result)|null $parser
     * @return array{int, string, string}
     */
    private static function cli(array $args, ?callable $parser = null): array
    {
        $out = fopen('php://memory', 'w+');
        $err = fopen('php://memory', 'w+');
        self::assertIsResource($out);
        self::assertIsResource($err);
        $code = Cli::run(['rqp-markup', ...$args], $out, $err, $parser);
        rewind($out);
        rewind($err);
        return [$code, (string) stream_get_contents($out), (string) stream_get_contents($err)];
    }

    private function file(string $relative, string $content): string
    {
        $path = $this->tmp . '/' . $relative;
        $dir = dirname($path);
        if (!is_dir($dir)) {
            self::assertTrue(mkdir($dir, 0o700, true));
        }
        self::assertNotFalse(file_put_contents($path, $content));
        return $path;
    }

    public function testSingleModePrintsOneJsonLine(): void
    {
        $path = $this->file('in.html', '<p data-rq-block="a">x</p>');
        [$code, $out, $err] = self::cli([$path]);
        self::assertSame(0, $code);
        self::assertSame('', $err);
        self::assertStringEndsWith("}\n", $out);
        self::assertSame(1, substr_count($out, "\n"));
        $object = json_decode($out, true, 8, JSON_THROW_ON_ERROR);
        self::assertSame(Parser::parse('<p data-rq-block="a">x</p>')->toArray(), $object);
    }

    public function testSingleModeInvalidDocumentExitsZero(): void
    {
        $path = $this->file('in.html', '<p data-rq-block="secret-value-1">x</p><p data-rq-block="secret-value-1">y</p>');
        [$code, $out, $err] = self::cli([$path]);
        self::assertSame(0, $code);
        self::assertSame('', $err);
        self::assertSame(
            '{"valid":false,"error":"RQP_MARKUP_DUPLICATE","blocks":[],"template_revision":null,"page_revision":null}' . "\n",
            $out,
        );
    }

    public function testBatchModeSortsByByteOrderAndSkipsDirectoriesWithoutInput(): void
    {
        $this->file('fx/b/input.html', '<p data-rq-block="b">2</p>');
        $this->file('fx/a/input.html', '<p data-rq-block="a">1</p>');
        // 'Z' and not 'B': on a case-insensitive file system 'B' and 'b' are one directory.
        $this->file('fx/Z/input.html', '');
        $this->file('fx/c/other.html', 'x');
        $this->file('fx/A0/other.html', 'x');
        $this->file('fx/input.html', 'not a fixture');
        $this->file('fx/file', 'x');
        [$code, $out, $err] = self::cli(['--batch', $this->tmp . '/fx']);
        self::assertSame(0, $code);
        self::assertSame('', $err);
        $lines = explode("\n", rtrim($out, "\n"));
        $names = [];
        foreach ($lines as $line) {
            $object = json_decode($line, true, 8, JSON_THROW_ON_ERROR);
            self::assertIsArray($object);
            self::assertSame(['valid', 'error', 'blocks', 'template_revision', 'page_revision', 'fixture'], array_keys($object));
            $names[] = $object['fixture'];
        }
        self::assertSame(['Z', 'a', 'b'], $names);
    }

    public function testBatchModeOnAnEmptyDirectoryPrintsNothing(): void
    {
        self::assertTrue(mkdir($this->tmp . '/empty'));
        self::assertSame([0, '', ''], self::cli(['--batch', $this->tmp . '/empty']));
    }

    public function testUsageErrors(): void
    {
        foreach ([[], ['--batch'], ['a', 'b'], ['--batch', 'a', 'b'], ['--x'], ['--help']] as $args) {
            [$code, $out, $err] = self::cli($args);
            self::assertSame(2, $code, implode(' ', $args));
            self::assertSame('', $out);
            self::assertStringContainsString('usage: rqp-markup', $err);
        }
    }

    private const FAIL_CLOSED = '{"valid":false,"error":"RQP_MARKUP_STRUCTURE","blocks":[],"template_revision":null,"page_revision":null';

    /**
     * Parser seam that fails on documents containing "BOOM".
     */
    private static function defectiveParser(): \Closure
    {
        return static function (string $input): Result {
            if (str_contains($input, 'BOOM')) {
                throw new \RuntimeException('message with ' . $input);
            }
            return Parser::parse($input);
        };
    }

    public function testBatchDefectFailsClosedForThatDocumentAndContinues(): void
    {
        $this->file('fx/a/input.html', '<p data-rq-block="a">1</p>');
        $this->file('fx/b/input.html', '<p data-rq-block="b">SECRET BOOM</p>');
        $this->file('fx/c/input.html', '<p data-rq-block="c">3</p>');
        [$code, $out, $err] = self::cli(['--batch', $this->tmp . '/fx'], self::defectiveParser());
        self::assertSame(70, $code);
        $lines = explode("\n", rtrim($out, "\n"));
        self::assertCount(3, $lines);
        self::assertStringStartsWith('{"valid":true', $lines[0]);
        self::assertSame(self::FAIL_CLOSED . ',"fixture":"b"}', $lines[1]);
        self::assertStringStartsWith('{"valid":true', $lines[2]);
        self::assertStringContainsString('"fixture":"c"', $lines[2]);
        self::assertStringContainsString('RuntimeException', $err);
        self::assertStringNotContainsString('SECRET', $err);
        self::assertStringNotContainsString('BOOM', $err);
    }

    public function testSingleModeDefectFailsClosedWithExit70(): void
    {
        $path = $this->file('in.html', '<p>SECRET BOOM</p>');
        [$code, $out, $err] = self::cli([$path], self::defectiveParser());
        self::assertSame(70, $code);
        self::assertSame(self::FAIL_CLOSED . "}\n", $out);
        self::assertStringContainsString('RuntimeException', $err);
        self::assertStringContainsString('CliTest.php', $err);
        self::assertStringNotContainsString('SECRET', $err);
    }

    public function testPhpWarningInParserIsADefectNotAnEscape(): void
    {
        $path = $this->file('in.html', 'x');
        $parser = static function (string $input): Result {
            trigger_error('warn SECRET', E_USER_WARNING);
            return Parser::parse($input);
        };
        [$code, $out, $err] = self::cli([$path], $parser);
        self::assertSame(70, $code);
        self::assertStringStartsWith(self::FAIL_CLOSED, $out);
        self::assertStringNotContainsString('SECRET', $err);
    }

    public function testDashPrefixedFileNameIsAPathInSingleMode(): void
    {
        $this->file('-page.html', '<title data-rq-block="t">x</title>');
        $cwd = getcwd();
        self::assertNotFalse($cwd);
        self::assertTrue(chdir($this->tmp));
        try {
            [$code, $out, $err] = self::cli(['-page.html']);
        } finally {
            chdir($cwd);
        }
        self::assertSame(0, $code);
        self::assertSame('', $err);
        self::assertStringStartsWith('{"valid":true', $out);
    }

    public function testMissingFileIsAnIoError(): void
    {
        [$code, $out, $err] = self::cli([$this->tmp . '/missing.html']);
        self::assertSame(2, $code);
        self::assertSame('', $out);
        self::assertStringContainsString('not a file', $err);
    }

    public function testDirectoryAsInputIsAnIoError(): void
    {
        [$code] = self::cli([$this->tmp]);
        self::assertSame(2, $code);
    }

    public function testBatchOnMissingDirectoryIsAnIoError(): void
    {
        [$code, $out, $err] = self::cli(['--batch', $this->tmp . '/missing']);
        self::assertSame(2, $code);
        self::assertSame('', $out);
        self::assertStringContainsString('not a directory', $err);
    }

    public function testReadIsBoundedAndOversizedInputIsALimitError(): void
    {
        $path = $this->file('big.html', str_repeat('a', Parser::MAX_INPUT_BYTES + 10));
        self::assertSame(Parser::MAX_INPUT_BYTES + 1, strlen(Cli::readInput($path)));
        [$code, $out] = self::cli([$path]);
        self::assertSame(0, $code);
        self::assertStringContainsString('"RQP_MARKUP_LIMIT"', $out);
    }

    public function testClosedOutputIsAnInternalErrorWithoutContent(): void
    {
        $path = $this->file('in.html', '<p data-rq-block="a">secret-content</p>');
        $out = fopen('php://memory', 'w+');
        $err = fopen('php://memory', 'w+');
        self::assertIsResource($out);
        self::assertIsResource($err);
        fclose($out);
        $code = Cli::run(['rqp-markup', $path], $out, $err);
        rewind($err);
        $text = (string) stream_get_contents($err);
        self::assertSame(70, $code);
        self::assertStringContainsString('internal error', $text);
        self::assertStringNotContainsString('secret', $text);
    }

    public function testReadOnlyOutputIsAnIoError(): void
    {
        $path = $this->file('in.html', 'x');
        $out = fopen('php://memory', 'r');
        $err = fopen('php://memory', 'w+');
        self::assertIsResource($out);
        self::assertIsResource($err);
        self::assertSame(2, Cli::run(['rqp-markup', $path], $out, $err));
    }

    public function testErrorHandlerIsRestored(): void
    {
        self::cli([]);
        $previous = set_error_handler(null);
        restore_error_handler();
        // PHPUnit installs its own handler; Cli must not leave its handler behind.
        self::assertFalse(is_object($previous) && (new \ReflectionFunction(\Closure::fromCallable($previous)))->getClosureScopeClass()?->getName() === Cli::class);
    }

    public function testCommandRunsWithoutComposer(): void
    {
        $path = $this->file('in.html', '<title data-rq-block="t">x</title>');
        $bin = dirname(__DIR__) . '/bin/rqp-markup';
        $process = proc_open([PHP_BINARY, '-n', $bin, $path], [1 => ['pipe', 'w'], 2 => ['pipe', 'w']], $pipes);
        self::assertIsResource($process);
        $out = (string) stream_get_contents($pipes[1]);
        $err = (string) stream_get_contents($pipes[2]);
        fclose($pipes[1]);
        fclose($pipes[2]);
        self::assertSame(0, proc_close($process));
        self::assertSame('', $err);
        self::assertStringStartsWith('{"valid":true,"error":null,"blocks":[{"id":"t","format":"html","start":25,"end":26,', $out);
    }

    public function testCommandUsageErrorExitCode(): void
    {
        $bin = dirname(__DIR__) . '/bin/rqp-markup';
        $process = proc_open([PHP_BINARY, '-n', $bin], [1 => ['pipe', 'w'], 2 => ['pipe', 'w']], $pipes);
        self::assertIsResource($process);
        stream_get_contents($pipes[1]);
        stream_get_contents($pipes[2]);
        fclose($pipes[1]);
        fclose($pipes[2]);
        self::assertSame(2, proc_close($process));
    }

    /**
     * Run the command in a child process with a deadline, so that a read that
     * waits on a FIFO fails the test and does not hang it.
     *
     * @param list<string> $args
     * @return array{int, string, string}
     */
    private static function cliProcess(array $args): array
    {
        $bin = dirname(__DIR__) . '/bin/rqp-markup';
        $process = proc_open([PHP_BINARY, '-n', $bin, ...$args], [1 => ['pipe', 'w'], 2 => ['pipe', 'w']], $pipes);
        self::assertIsResource($process);
        stream_set_blocking($pipes[1], false);
        stream_set_blocking($pipes[2], false);
        $out = '';
        $err = '';
        $deadline = microtime(true) + 10.0;
        while (true) {
            $out .= (string) stream_get_contents($pipes[1]);
            $err .= (string) stream_get_contents($pipes[2]);
            $status = proc_get_status($process);
            if (!$status['running']) {
                break;
            }
            if (microtime(true) > $deadline) {
                proc_terminate($process, 9);
                proc_close($process);
                self::fail('the command waits on its input');
            }
            usleep(10_000);
        }
        $out .= (string) stream_get_contents($pipes[1]);
        $err .= (string) stream_get_contents($pipes[2]);
        fclose($pipes[1]);
        fclose($pipes[2]);
        $code = $status['exitcode'];
        proc_close($process);
        return [$code, $out, $err];
    }

    private function fifo(string $relative): string
    {
        if (!function_exists('posix_mkfifo')) {
            self::markTestSkipped('posix_mkfifo is not available');
        }
        $path = $this->tmp . '/' . $relative;
        $dir = dirname($path);
        if (!is_dir($dir)) {
            self::assertTrue(mkdir($dir, 0o700, true));
        }
        if (!posix_mkfifo($path, 0o600)) {
            self::markTestSkipped('cannot create a FIFO here');
        }
        return $path;
    }

    public function testSingleModeFifoIsAnIoErrorWithoutWaiting(): void
    {
        [$code, $out, $err] = self::cliProcess([$this->fifo('pipe.html')]);
        self::assertSame(2, $code);
        self::assertSame('', $out);
        self::assertStringContainsString('not a file', $err);
    }

    public function testSingleModeSymlinkToFifoIsAnIoErrorWithoutWaiting(): void
    {
        $fifo = $this->fifo('pipe.html');
        self::assertTrue(symlink($fifo, $this->tmp . '/link.html'));
        [$code, $out, $err] = self::cliProcess([$this->tmp . '/link.html']);
        self::assertSame(2, $code);
        self::assertSame('', $out);
        self::assertStringContainsString('not a file', $err);
    }

    public function testSingleModeSymlinkToRegularFileIsRead(): void
    {
        $path = $this->file('real.html', '<title data-rq-block="t">x</title>');
        self::assertTrue(symlink($path, $this->tmp . '/link.html'));
        [$code, $out] = self::cli([$this->tmp . '/link.html']);
        self::assertSame(0, $code);
        self::assertStringStartsWith('{"valid":true', $out);
    }

    public function testBatchFifoInputIsAnIoErrorWithoutWaiting(): void
    {
        $this->fifo('fx/a/input.html');
        [$code, $out, $err] = self::cliProcess(['--batch', $this->tmp . '/fx']);
        self::assertSame(2, $code);
        self::assertSame('', $out);
        self::assertStringContainsString('not a file', $err);
    }

    public function testBatchSymlinkToFifoInputIsAnIoErrorWithoutWaiting(): void
    {
        $fifo = $this->fifo('pipe');
        self::assertTrue(mkdir($this->tmp . '/fx/a', 0o700, true));
        self::assertTrue(symlink($fifo, $this->tmp . '/fx/a/input.html'));
        [$code, , $err] = self::cliProcess(['--batch', $this->tmp . '/fx']);
        self::assertSame(2, $code);
        self::assertStringContainsString('not a file', $err);
    }

    public function testBatchDirectoryNamedInputHtmlIsAnIoError(): void
    {
        self::assertTrue(mkdir($this->tmp . '/fx/a/input.html', 0o700, true));
        [$code, $out, $err] = self::cli(['--batch', $this->tmp . '/fx']);
        self::assertSame(2, $code);
        self::assertSame('', $out);
        self::assertStringContainsString('not a file', $err);
    }

    public function testBatchErrorMessageHasNoDocumentContent(): void
    {
        $this->file('fx/a/input.html', '<title data-rq-block="t">SECRET</title>');
        self::assertTrue(mkdir($this->tmp . '/fx/b/input.html', 0o700, true));
        [$code, , $err] = self::cli(['--batch', $this->tmp . '/fx']);
        self::assertSame(2, $code);
        self::assertStringNotContainsString('SECRET', $err);
    }

    private static function removeTree(string $path): void
    {
        if ($path === '' || !file_exists($path)) {
            return;
        }
        if (is_dir($path) && !is_link($path)) {
            foreach (scandir($path) ?: [] as $entry) {
                if ($entry !== '.' && $entry !== '..') {
                    self::removeTree($path . '/' . $entry);
                }
            }
            rmdir($path);
            return;
        }
        unlink($path);
    }
}
