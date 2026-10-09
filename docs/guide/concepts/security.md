# Security guarantees

This page lists what Rivqen promises about security and privacy, and what it never does.

## Rivqen always

| Guarantee | Meaning |
|---|---|
| Uses TLS with system validation | Certificates are checked by the operating system. Rivqen adds no trust exceptions. |
| Isolates accounts | Each account has its own cache partition. Logout removes access at once. |
| Validates before it updates | A data update is checked (version, size, origin, format) before it changes the page or the cache. |
| Falls back safely | Any failure leads to a normal page load, not a blank screen. |
| Respects `no-store` | Pages that the server marks `no-store` are never stored. |
| Limits resources | Size, memory and time limits protect your app. |
| Keeps telemetry local | No data leaves the device unless your app exports it. |

## Rivqen never

| Never | Why |
|---|---|
| Bypasses TLS or certificate errors | Prevents man-in-the-middle attacks |
| Uses private iOS or Android APIs | Keeps your app compliant and stable on new OS versions |
| Runs code from data updates | Prevents script injection through updates |
| Exposes native functions to unknown origins | Only origins in your allowlist can use the JS bridge |
| Logs cookies, tokens or page content | Protects user privacy |
| Sends an SDK-identifying header by default | Avoids unnecessary fingerprinting |
| Changes behavior when it is being tested or reviewed | Keeps behavior transparent |

## Your responsibilities

1. List the allowed origins in the app configuration.
2. Set correct `Cache-Control` headers on your server, especially `private` and `no-store` for personal pages.
3. Tell Rivqen when the user logs in, logs out or switches accounts.
4. Keep Rivqen up to date. Read security advisories.

## Report a vulnerability

Do not open a public issue. Follow the [security policy](https://github.com/robotoss/rivqen/blob/main/SECURITY.md).

## Related

- [Engineering: Security](/engineering/security/)
