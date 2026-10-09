<?php
// Test harness: serves fixture pages through the UNMODIFIED upstream PHP SDK.
require '/upstream/sonic-php/sdk/sonic.php';
$name = preg_replace('/[^a-z]/', '', basename(parse_url($_SERVER['REQUEST_URI'], PHP_URL_PATH)));
$f = "/pages/$name.html";
if (!is_file($f)) { http_response_code(404); exit; }
util_sonic::start();
readfile($f);
util_sonic::end();
