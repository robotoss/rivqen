# iOS

This guide shows how to add Rivqen to an iOS app that shows pages in a `WKWebView`.

::: warning Planned API — not released
The package and the API can change before the first alpha. The iOS loading strategy is decided after a feasibility spike (ADR-003). Some Android features may work differently on iOS. See the [Support matrix](/guide/support-matrix).
:::

## Requirements

- An iOS app that uses `WKWebView`.
- Swift with Swift Concurrency.
- Minimum iOS version: see the [Support matrix](/guide/support-matrix).

## Step 1 — Add the package

1. In Xcode, select **File → Add Package Dependencies**.
2. Enter the repository URL `https://github.com/robotoss/rivqen`.
3. Select the `Rivqen` library for your app target.

Or add it to `Package.swift`:

```swift
// Planned API — not released
dependencies: [
    .package(url: "https://github.com/robotoss/rivqen", from: "0.1.0")
]
```

## Step 2 — Create the engine

1. Create one engine for the app.
2. List the allowed origins.

```swift
// Planned API — not released
let engine = try RivqenEngine(configuration: .init(
    allowedOrigins: ["https://shop.example.com"],
    // RQP is the default. The optional RivqenLegacy product exists until 1.5.
))
```

## Step 3 — Open a page

1. Start the session before the web view appears.
2. Attach the session to the web view.

```swift
// Planned API — not released
let session = try await engine.open(.init(
    url: URL(string: "https://shop.example.com/catalog")!,
    partition: .user(id: currentUserID)   // non-secret ID
))

let webView = WKWebView(frame: .zero, configuration: session.webViewConfiguration)
session.attach(to: webView)
```

## Step 4 — Close the session

1. Cancel the session when the view goes away.

```swift
// Planned API — not released
session.cancel()
```

## Step 5 — Handle logout

::: danger Risk of data leak between accounts
Tell Rivqen when the user logs out. If you do not, cached pages of the old user can stay readable.
:::

```swift
// Planned API — not released
await engine.clearCache(partition: .user(id: oldUserID))
```

## Step 6 — Privacy manifest

1. Check that your app's privacy report includes the Rivqen privacy manifest.
2. Do not remove `PrivacyInfo.xcprivacy` from the package.

## Next steps

- [Troubleshooting](/guide/troubleshooting)
- [Engineering: iOS SDK](/engineering/platforms/ios)
