# Example: iOS

The iOS demo opens the catalog page from the [server demo](/guide/examples/server) in a `WKWebView` and shows what Rivqen does on each visit.

**Status:** <Badge type="info" text="DESIGN" /> planned for P5 (WP-11, WP-22) · code uses the planned API

[[toc]]

## 1. Screens

The screens are the same as in the [Android demo](/guide/examples/android#_1-screens): Cold, Warm, Live diff, Template changed and Offline.

## 2. Planned project layout

```text
examples/ios-demo/
├── RivqenDemo/
│   ├── RivqenDemoApp.swift   engine setup
│   ├── CatalogView.swift     WKWebView + control panel
│   └── EventLogView.swift    session events and timings
└── RivqenDemo/Info.plist      local-network exception (debug only)
```

## 3. Planned code

```swift
// Planned API — not released
@main
struct RivqenDemoApp: App {
    let engine = try! RivqenEngine(configuration: .init(
        allowedOrigins: ["http://127.0.0.1:8787"]  // the simulator reaches the host as 127.0.0.1
    ))

    var body: some Scene {
        WindowGroup { CatalogView(engine: engine) }
    }
}
```

```swift
// Planned API — not released
@MainActor
final class CatalogModel: ObservableObject {
    @Published var events: [RivqenEvent] = []
    private var session: RivqenSession?

    func open(engine: RivqenEngine, webView: WKWebView) async throws {
        session?.cancel()
        let s = try await engine.open(.init(
            url: URL(string: "http://127.0.0.1:8787/catalog")!,
            partition: .user(id: "demo")
        ))
        s.attach(to: webView)
        session = s
        for await event in s.events { events.append(event) }
    }
}
```

::: warning App Transport Security
The demo uses `http://` to the local server. Add an ATS exception for `127.0.0.1` only in the debug configuration. Production apps must use HTTPS. Do not use `try!` in production code.
:::

## 4. What to measure

1. Time to first content: native marker and `WKNavigationDelegate` plus web FCP.
2. Transferred bytes per visit.
3. Patch apply time.
4. Energy for 30 visits (Xcode Energy Organizer), with Rivqen on and off.

## Related

- [Getting started: iOS](/guide/getting-started/ios)
- [Engineering: iOS SDK](/engineering/platforms/ios)
