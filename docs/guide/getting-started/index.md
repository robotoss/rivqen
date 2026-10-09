# Choose your path

Rivqen has a client side (in the app and in the page) and a server side. You get the full benefit when both sides use Rivqen.

::: warning Planned API
Packages are not published yet. The steps show the planned installation. They will work when the first alpha is released.
:::

## 1. Pick your components

| You build | Install | Guide |
|---|---|---|
| Android app with a WebView | Android SDK | [Android](/guide/getting-started/android) |
| iOS app with a WKWebView | iOS SDK | [iOS](/guide/getting-started/ios) |
| Web page shown in the WebView | Web core (+ React adapter) | [Web and React](/guide/getting-started/web-react) |
| Node.js backend (Express, Fastify, Next.js) | Node.js server SDK | [Server: Node.js](/guide/getting-started/server-node) |
| Java backend (Spring Boot, Servlet) | Java server SDK | [Server: Java](/guide/getting-started/server-java) |
| PHP backend (PSR-15 frameworks) | PHP server SDK | [Server: PHP](/guide/getting-started/server-php) |

## 2. Typical order

1. Install the server SDK and mark data blocks in one page.
2. Check the page with a browser. Normal visitors must see the same HTML as before.
3. Install the mobile SDK and open the page through Rivqen.
4. Add the web SDK if your page must update blocks without a reload.
5. Measure the result with the built-in metrics.

## 3. Requirements

See the [Support matrix](/guide/support-matrix) for platform versions.

## 4. Try it in a lab first

Use the demo apps (planned in WP-22). They show all outcomes: cold, warm, live data update, template change, offline.
