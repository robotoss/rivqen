<?php

declare(strict_types=1);

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Test harness: serves fixture pages through the UNMODIFIED upstream PHP SDK.
//
// Two routes:
//  - /<page> (traces S01-S21): a stored fixture page, as before.
//  - the control endpoint of server/conformance/CONTRACT.md section 2 (lab exception of DL-081)
//    and, for every other path, the stored case. It is off unless RIVQEN_CONFORMANCE=1.
//
// The PHP built-in server runs every request in a fresh process, so the case is kept in a file
// (written atomically by rename). Requests are sequential.

// Warnings must not reach the response bytes (the runner compares them). Log to stderr instead.
ini_set('display_errors', '0');
ini_set('log_errors', '1');
ini_set('error_log', '/dev/stderr');

const PROFILE = 'legacy-php';
const CONTROL_PREFIX = '/__rivqen/conformance/';
const MAX_CASE_BYTES = 8 * 1024 * 1024;
const DEFAULT_CASE_FILE = '/tmp/rivqen-case.json'; // inside the lab container

function sendJson(int $status, array $value): never
{
    $body = json_encode($value, JSON_THROW_ON_ERROR);
    http_response_code($status);
    header('Content-Type: application/json');
    header('Content-Length: ' . strlen($body));
    echo $body;
    exit;
}

function isControlHost(string $host, string $port): bool
{
    if (preg_match('/^[0-9]{1,5}$/', $port) !== 1) {
        return false; // fails closed when LAB_HOST_PORT is not set
    }
    $h = strtolower($host);

    return $h === "127.0.0.1:$port" || $h === "localhost:$port" || $h === "[::1]:$port";
}

function isJsonContentType(string $value): bool
{
    $type = trim(explode(';', $value)[0], " \t");

    return strtolower($type) === 'application/json';
}

/** RFC 9110 field name (token) and field value (HTAB, SP, VCHAR, obs-text). */
function isWirePair(mixed $pair): bool
{
    return is_array($pair) && array_is_list($pair) && count($pair) === 2
        && is_string($pair[0]) && is_string($pair[1])
        && preg_match("/^[!#$%&'*+\\-.^_`|~0-9A-Za-z]+$/D", $pair[0]) === 1
        && preg_match('/^[\t\x20-\x7e\x80-\xff]*$/D', $pair[1]) === 1;
}

function isIntegral(mixed $v): bool
{
    return is_int($v) || (is_float($v) && is_finite($v) && floor($v) === $v);
}

/**
 * Parse a case body. Returns ['case' => [...]] or ['error' => 'BAD_CASE'|'UNSUPPORTED'].
 * BAD_CASE is checked before UNSUPPORTED.
 *
 * @return array{case?: array{case_id: string, page_base64: string, status: int, headers: list<array{0: string, 1: string}>}, error?: string}
 */
function parseCase(string $text): array
{
    try {
        $json = json_decode($text, false, 64, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        return ['error' => 'BAD_CASE'];
    }
    if (!is_object($json)) {
        return ['error' => 'BAD_CASE'];
    }
    $suite = $json->suite ?? null;
    if ($suite !== 'rqp' && $suite !== 'legacy') {
        return ['error' => 'BAD_CASE'];
    }
    if (!is_string($json->case_id ?? null)) {
        return ['error' => 'BAD_CASE'];
    }
    $profile = $json->profile ?? null;
    if ($suite === 'legacy' && !is_string($profile)) {
        return ['error' => 'BAD_CASE'];
    }
    $page = $json->page_base64 ?? null;
    if (!is_string($page) || strlen($page) % 4 !== 0 || preg_match('/^[A-Za-z0-9+\/]*={0,2}$/D', $page) !== 1
        || base64_decode($page, true) === false) {
        return ['error' => 'BAD_CASE'];
    }
    $app = $json->app ?? null;
    if (!is_object($app) || !isIntegral($app->status ?? null) || $app->status < 100 || $app->status > 599) {
        return ['error' => 'BAD_CASE'];
    }
    $headers = $app->headers ?? null;
    if (!is_array($headers) || !array_is_list($headers)) {
        return ['error' => 'BAD_CASE'];
    }
    foreach ($headers as $pair) {
        if (!isWirePair($pair)) {
            return ['error' => 'BAD_CASE'];
        }
    }
    if (property_exists($json, 'server_config') && !is_object($json->server_config)) {
        return ['error' => 'BAD_CASE'];
    }
    if ($suite !== 'legacy' || $profile !== PROFILE) {
        return ['error' => 'UNSUPPORTED'];
    }

    return ['case' => [
        'case_id' => $json->case_id,
        'page_base64' => $page,
        'status' => (int) $app->status,
        'headers' => $headers,
    ]];
}

/** Read the request body, at most MAX_CASE_BYTES + 1 bytes. Null when it is larger. */
function readCaseBody(): ?string
{
    $length = $_SERVER['CONTENT_LENGTH'] ?? null;
    if (is_string($length) && ctype_digit($length) && (int) $length > MAX_CASE_BYTES) {
        return null;
    }
    $text = file_get_contents('php://input', false, null, 0, MAX_CASE_BYTES + 1);
    if ($text === false) {
        sendJson(500, ['error' => 'HARNESS_FAILED']);
    }

    return strlen($text) > MAX_CASE_BYTES ? null : $text;
}

function caseFile(): string
{
    $f = getenv('LAB_CASE_FILE');

    return $f === false || $f === '' ? DEFAULT_CASE_FILE : $f;
}

function storeCase(array $case): void
{
    $file = caseFile();
    $tmp = $file . '.' . bin2hex(random_bytes(6)) . '.tmp';
    $ok = file_put_contents($tmp, json_encode($case, JSON_THROW_ON_ERROR)) !== false && rename($tmp, $file);
    if (!$ok) {
        @unlink($tmp);
        fwrite(STDERR, "lab error: CASE_STORE_FAILED\n");
        sendJson(500, ['error' => 'HARNESS_FAILED']);
    }
}

function loadCase(): ?array
{
    $text = @file_get_contents(caseFile());
    if ($text === false) {
        return null;
    }
    try {
        $case = json_decode($text, true, 64, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        return null;
    }

    return is_array($case) ? $case : null;
}

function handleControl(string $path): never
{
    if (getenv('RIVQEN_CONFORMANCE') !== '1') {
        sendJson(404, ['error' => 'NOT_FOUND']);
    }
    // Lab exception (DL-081): the peer is the bridge address, so check Host and Content-Type only.
    $port = getenv('LAB_HOST_PORT');
    if (!isControlHost((string) ($_SERVER['HTTP_HOST'] ?? ''), $port === false ? '' : $port)) {
        sendJson(403, ['error' => 'FORBIDDEN']);
    }
    $method = $_SERVER['REQUEST_METHOD'] ?? '';
    if ($path === CONTROL_PREFIX . 'info') {
        if ($method !== 'GET') {
            sendJson(405, ['error' => 'METHOD_NOT_ALLOWED']);
        }
        sendJson(200, ['contract' => 1, 'suites' => ['legacy'], 'profiles' => [PROFILE]]);
    }
    if ($path === CONTROL_PREFIX . 'case') {
        if ($method !== 'POST') {
            sendJson(405, ['error' => 'METHOD_NOT_ALLOWED']);
        }
        if (!isJsonContentType((string) ($_SERVER['CONTENT_TYPE'] ?? ''))) {
            sendJson(415, ['error' => 'UNSUPPORTED_MEDIA_TYPE']);
        }
        $body = readCaseBody();
        if ($body === null) {
            sendJson(413, ['error' => 'TOO_LARGE']);
        }
        $parsed = parseCase($body);
        if (($parsed['error'] ?? '') === 'BAD_CASE') {
            sendJson(400, ['error' => 'BAD_CASE']);
        }
        if (($parsed['error'] ?? '') === 'UNSUPPORTED' || !isset($parsed['case'])) {
            sendJson(422, ['error' => 'UNSUPPORTED']);
        }
        storeCase($parsed['case']);
        http_response_code(204);
        exit;
    }
    sendJson(404, ['error' => 'NOT_FOUND']);
}

$path = parse_url((string) ($_SERVER['REQUEST_URI'] ?? '/'), PHP_URL_PATH);
$path = is_string($path) ? $path : '/';

if (str_starts_with($path, CONTROL_PREFIX)) {
    handleControl($path);
}

if (str_starts_with($path, '/p/')) {
    // Stored fixture page (traces S01-S21).
    require '/upstream/sonic-php/sdk/sonic.php';
    $name = preg_replace('/[^a-z]/', '', basename($path));
    $f = "/pages/$name.html";
    if (!is_file($f)) {
        http_response_code(404);
        exit;
    }
    util_sonic::start();
    readfile($f);
    util_sonic::end();
    exit;
}

// The stored case: what an application does is status, field lines, then the body bytes.
// The upstream code acts on it.
$case = getenv('RIVQEN_CONFORMANCE') === '1' ? loadCase() : null;
if ($case === null) {
    sendJson(503, ['error' => 'NO_CASE']);
}
$page = base64_decode((string) $case['page_base64'], true);
if ($page === false) {
    sendJson(500, ['error' => 'HARNESS_FAILED']);
}
require '/upstream/sonic-php/sdk/sonic.php';
http_response_code((int) $case['status']);
foreach ($case['headers'] as [$name, $value]) {
    header("$name: $value", false);
}
util_sonic::start();
echo $page;
util_sonic::end();
