# JS bridge contract

The JS bridge carries messages between the web SDK in the page and the native SDK. It is a security boundary: web content is untrusted.

**Status:** <Badge type="info" text="DESIGN" />

[[toc]]

## 1. Transport per platform

| Platform | Page → native | Native → page | Origin source |
|---|---|---|---|
| Android | `WebViewCompat.addWebMessageListener` (`JavaScriptReplyProxy`) | `replyProxy.postMessage` | `sourceOrigin` and `isMainFrame` from the platform callback |
| iOS | `WKScriptMessageHandlerWithReply` in a dedicated `WKContentWorld` | Reply handler / `callAsyncJavaScript` in the same world | `message.frameInfo.securityOrigin`, `frameInfo.isMainFrame` |
| Fallback (old Android WebView) | None | None | The bridge is disabled; pages use HTTP revalidation only |

<Badge type="warning" text="RESEARCH" /> Confirm minimum WebView and iOS versions for these APIs (WP-04). Do not use `addJavascriptInterface` for untrusted content: it exposes methods to every frame.

## 2. Message envelope

```json
{
  "protocol_version": 1,
  "navigation_id": "nav-123",
  "session_generation": 4,
  "request_id": 18,
  "type": "rivqen.update",
  "payload": { }
}
```

| Field | Rule |
|---|---|
| `protocol_version` | Integer. Receiver rejects unknown majors. |
| `navigation_id` | Must match the current navigation. |
| `session_generation` | Must match the current generation. Old messages are ignored. |
| `request_id` | Correlates a reply with a request. |
| `type` | From a closed list (section 3). Unknown types are rejected. |
| `payload` | Validated against the schema of the type. |

The native side **never** trusts an `origin` field inside a message. It uses the origin given by the platform API.

## 3. Message types

| Type | Direction | Payload | Notes |
|---|---|---|---|
| `rivqen.hello` | page → native | `sdk_version`, `capabilities` | First message. Native answers with session info. |
| `rivqen.session` | native → page | `navigation_id`, `page_revision`, `template_revision`, `mode` | |
| `rivqen.update` | native → page | `base_revision`, `next_revision`, `blocks[]` | Validated patch or legacy diff |
| `rivqen.ack` | page → native | `request_id`, `status` (`applied`, `rejected`, `reload`) | Native commits or rolls back |
| `rivqen.reload` | page → native | `reason` | Page asks for a full snapshot (for example hydration mismatch) |
| `rivqen.metric` | page → native | Closed set of timing marks | Optional |

There is no message type that calls an arbitrary native method or runs code.

## 4. Rules

1. Accept messages only from origins in `trust.bridgeOrigins`, and only from the main frame unless the app allows a frame origin.
2. Limit payload size (`maxPatchBytes`) and message rate (for example 50 per second).
3. On schema error: drop the message, increment `rivqen.security.violation`, keep the page working.
4. On partition change: close the bridge for the old session at once.

## 5. Legacy bridge

<Badge type="tip" text="FACT" /> Upstream pages obtained diff data through a JS interface exposed by the native SDK and a callback. The exact names are in [legacy client behavior](/engineering/protocol/legacy-client). Rivqen can expose a **compatibility shim** with the same names for legacy pages, implemented on top of the secure transport above, with the same origin checks.
