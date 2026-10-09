# Realtime channels

Realtime channels let the server tell the client that a page changed, and optionally send a patch. They are an **optimization**. Rivqen works fully without them.

**Status:** <Badge type="info" text="DESIGN" /> <Badge type="warning" text="RESEARCH" /> RQP feature. WebSocket first; WebTransport experimental (ADR-007).

[[toc]]

## 1. Standards status

| Technology | Status | Source |
|---|---|---|
| WebSocket | RFC 6455; widely available in all WebViews | [RFC 6455](https://www.rfc-editor.org/rfc/rfc6455) |
| WebSocket over HTTP/2 | RFC 8441 (Extended CONNECT) | [RFC 8441](https://www.rfc-editor.org/rfc/rfc8441) |
| WebSocket over HTTP/3 | RFC 9220 — needs Extended CONNECT support on both sides | [RFC 9220](https://www.rfc-editor.org/rfc/rfc9220) |
| HTTP Datagrams | RFC 9297 — a building block, not an application protocol | [RFC 9297](https://www.rfc-editor.org/rfc/rfc9297) |
| WebTransport | W3C API, not a final Recommendation; IETF protocol drafts | [W3C WebTransport](https://www.w3.org/TR/webtransport/) |

<Badge type="tip" text="FACT" /> Browser support for an API does **not** prove support inside Android System WebView or `WKWebView` on a given OS version. Rivqen uses runtime feature detection on the device, never the user-agent string.

## 2. Channel protocol (RQP)

| Step | Client | Server |
|---|---|---|
| 1 | Loads the document over HTTPS; reads the `realtime` capability and endpoint from the manifest | |
| 2 | Opens WSS with a short-lived token from an authenticated HTTPS call | Verifies token, origin, partition |
| 3 | Sends `subscribe { page_id, revision }` | Confirms with `subscribed { sequence }` |
| 4 | | Sends `invalidate { page_id, revision }` or `patch { envelope }` |
| 5 | Validates the message like any HTTPS response (revision, sequence, origin, size) | |
| 6 | On gap or error: revalidate over HTTPS | |

## 3. Message rules

1. Every message has `type`, `sequence`, `page_id`, and a protocol version.
2. A push message is a **signal**. Delivery does not prove freshness. The client checks the revision.
3. A patch received over a channel passes the **same** validation as one received over HTTPS ([Diff engine](/engineering/core/diff-engine#_3-rqp-patch-validation)).
4. The server never sends cookies, tokens or personal data in channel messages beyond what the page already has.
5. Message size ≤ `maxPatchBytes`.

## 4. Lifecycle

| Event | Client behavior |
|---|---|
| App to background | Close the channel (Android and iOS limit background sockets). Revalidate on foreground. |
| Network change | Close and reconnect with backoff. |
| Partition change | Close the channel at once. |
| Battery saver / data saver | Do not open channels. |
| Three failed reconnects | Stay on HTTP revalidation until the next navigation. |

## 5. Costs to measure

- Extra bytes and energy for an open channel per hour.
- Reconnect storms after network changes.
- Server memory per open channel.

## Related

- [Transport overview](/engineering/transport/)
- [ADR-007](/engineering/architecture/adr/#adr-007)
