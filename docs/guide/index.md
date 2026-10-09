# What is Rivqen?

Rivqen is an open-source engine that makes HTML pages inside mobile apps **open faster**, **use less data**, and **work offline safely**. It works with Android System WebView, iOS `WKWebView`, and web pages built with React or plain JavaScript. Server SDKs for Node.js, Java and PHP prepare the pages.

::: warning Status
Rivqen is **pre-alpha**. This documentation describes the planned product. No package is published yet. See [Project status](/guide/status).
:::

## The problem

Many apps show content in a WebView: product pages, news, offers, help pages, forms. A WebView page often feels slow:

1. The app creates the WebView. This takes time.
2. Only then does the WebView start to download the HTML.
3. The page downloads again in full, even when only a price or a counter changed.
4. Without network, the page does not open at all.

## What Rivqen does

| Problem | Rivqen solution |
|---|---|
| WebView start delays the download | Rivqen starts the HTML download **at the same time** as the WebView starts. |
| Full page downloads every time | Rivqen keeps a **template** of the page and downloads only the changed **data blocks**. |
| Repeat visits wait for the network | Rivqen shows the **cached page at once** and checks for changes in the background. |
| No network, no page | Rivqen shows an **allowed offline copy**, if the server permits it. |
| A failure gives a blank screen | Rivqen **falls back** to a normal page load. |

## Who is Rivqen for?

| You are | Rivqen gives you |
|---|---|
| Android or iOS app developer | A small SDK that you attach to your WebView |
| Web developer (React, SSR) | Hooks and components that receive data updates safely |
| Backend developer | Middleware for Node.js, Java or PHP that marks templates and answers update requests |
| Architect | A documented protocol, a security model and measurable performance goals |

## What Rivqen is not

- **Not a browser engine.** Rivqen uses the system WebView.
- **Not a proxy for other people's sites.** Full acceleration needs Rivqen on your server. For third-party sites, Rivqen does not change content.
- **Not a security bypass.** Rivqen never disables TLS, cookie rules or content security policy.
- **Not a replacement for HTTP caching.** Rivqen follows HTTP caching rules and adds template and data updates on top.

## Relation to Tencent VasSonic

Rivqen is an **independent** project. It offers a **temporary** compatibility mode for the legacy protocol of [Tencent VasSonic](https://github.com/Tencent/VasSonic), which was last updated in 2019. Rivqen is written from scratch under a clean-room policy. It is not affiliated with, sponsored by, or endorsed by Tencent. See [Migrate from VasSonic](/guide/migrate-from-vassonic).

## Next steps

1. Read [How it works](/guide/how-it-works).
2. Learn the [concepts](/guide/concepts/).
3. Choose your [getting-started path](/guide/getting-started/).
