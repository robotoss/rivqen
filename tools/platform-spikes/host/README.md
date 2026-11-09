# Spike host

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

The host of the WP-04 platform lab ([CONTRACT.md](../CONTRACT.md) sections 1 to 5, DL-134). Node.js 22, standard library only. It serves the fault scenarios over TLS on two sites and records the host part of each run.

## Start

```sh
. tools/platform-spikes/lab-env.sh
node tools/platform-spikes/host/main.mjs
# listening 18443 18444 18445
```

`/page/*` needs `rust-reference --site <dir> --protocol rqp|legacy --origin <url> --port 18480` (see `server/rust-reference/README.md`). The other routes work without it.

| Option | Environment | Default |
|---|---|---|
| `--port` | `LAB_HOST_PORT` | 18443 (TLS, good certificate) |
| `--bad-port` | `LAB_HOST_BAD_PORT` | 18444 (TLS, bad certificate) |
| `--control-port` | `LAB_HOST_CONTROL_PORT` | 18445 (plain HTTP, run log) |
| `--site-port` | `LAB_SITE_PORT` | 18480 (`rust-reference --site`) |
| `--tls-dir` | `LAB_TLS` | required |

Port `0` picks a free port; the `listening` line shows the real ports. All listeners bind `127.0.0.1` only. Exit codes: `2` usage error, `1` cannot start.

## Files in `$LAB_TLS`

Made on start with the system `openssl` when any file is missing (a partial set is replaced as a whole). Never committed.

| File | Content |
|---|---|
| `ca.pem`, `ca.key` | CA `CN=Rivqen WP-04 lab CA`, 30 days |
| `leaf.pem`, `leaf.key` | Signed by the CA; SAN `localhost`, `127.0.0.1`, `10.0.2.2`; port 18443 |
| `bad-leaf.pem`, `bad-leaf.key` | Self-signed, same SAN; port 18444 |

Keys have mode `0600`. The code works with OpenSSL 3 and with LibreSSL 3.3 (macOS).

## Routes

Both sites: A is Host `localhost`, B is Host `127.0.0.1` or `10.0.2.2`. Any other Host gives `400`. Only `GET`. Query values are printable ASCII. Routes of CONTRACT.md section 3 plus these details:

| Route | Limits and details |
|---|---|
| `/s/stream` | `chunks` 1 to 1024 (default 4), `size` 1 to 8 MiB (default 1024), `chunks * size` at most 64 MiB, `first`, `delay`, `timeout` 0 to 60000 ms. The CSP nonce is the run id with `.` replaced by `_` (a dot is not a valid nonce character) |
| `/s/redirect` | `n` 1 to 20, `to` is a path that starts with one `/`. With `cross=1` the Location is absolute. The optional `alt=127.0.0.1\|10.0.2.2` names the IP form of site B (default `127.0.0.1`; use `10.0.2.2` on Android) |
| `/s/cookie/set` | `name` and `value` match `[a-z0-9-]{1,64}`; each attribute at most once; `Max-Age` digits only |
| `/s/range` | `size` 1 to 8 MiB. Body byte `i` is `(i * 31 + 7) mod 251`. Forms `bytes=a-b` and `bytes=a-`; everything else is `416` |
| `/s/cut` | `after` (required) is less than `size`; optional `size` (default 4096); optional `hold` 0 to 60000 ms (default 0, S3): the wait after the `after` bytes before the close. With `hold` 0 the body is the S2 body (`<title>cut</title><p>`, then `x`). With `hold` above 0 the body starts with the chunk-0 markup of `/s/stream` (`<p data-k="0">`, the beacon of chunk 0, the script that probes `chunk` 0), so `size` must be above that start; a renderer that got the start before the close shows `host.beacon` before `host.cut`. `host.cut` is logged only when the host closed the connection; a client that closes first during the hold leaves no `host.cut` |
| `/s/sw.js` | Worker with `Service-Worker-Allowed: /s/`; it answers `/s/sw-probe` with `{"sw":"worker"}` and leaves every other request to the network. It calls `skipWaiting` and `clients.claim`, so the page is often controlled without the reload |
| `/s/sw-page` | Registers the worker (scope `/s/`), waits for `ready` (10 s), reloads once with `again=1` when not controlled, probes `sw` with `worker`, `network`, `unsupported` or the error name |
| `/s/sw-probe` | `{"sw":"network"}` |
| `/s/csp` | `mode=header\|meta\|both` (default `header`); policy `script-src 'nonce-<n>'`, `<n>` as in `/s/stream` (`norun` without a run). The meta element is the first element of `head`. Probes `csp.nonce`, `csp.eval` (nonce script) and `csp.inline` (script without nonce) |
| `/s/hist` | `step` 0 to 9 (default 0); link `<a id="next">` to step `step+1` (step 9 links to step 10, which gives `400`). Probes `history` and `location` |
| `/s/cookie/js` | Same `name`, `value` and `attrs` rules as `/s/cookie/set`, but `HttpOnly` gives `400`. The script sets `document.cookie`, then probes `cookie` |
| `/page/<path>` | The path must match the reference server rule (no `..`, no hidden file, no percent-encoding). `502` when the reference server is down |

## Control and run log

`POST /run/start`, `POST /run/end`, `GET /run/log?run=<id>` on the control port. `Content-Type: application/json`, body at most 4 KiB, Host `127.0.0.1` or `localhost`. At most 16 open runs and 100000 events per run. A request with a run that was not started leaves no event. `run/end` answers `application/x-ndjson`: the host events with sorted keys, `src` `host`, `seq` from 0 and `t` in ms. Cookie values outside `[a-z0-9-]` are logged as `[redacted]`.

## Tests

```sh
cd tools/platform-spikes/host && npm test
```

The tests make their own TLS files in a temporary directory, use ephemeral ports and need no network. `OPENSSL=<path>` selects another openssl.
