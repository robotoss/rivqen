# Example: Android

The Android demo opens the catalog page from the [server demo](/guide/examples/server) in a `WebView` and shows what Rivqen does on each visit.

**Status:** <Badge type="info" text="DESIGN" /> planned for P5 (WP-10, WP-22) · code uses the planned API

[[toc]]

## 1. Screens

| Screen | What the user sees | Event in the log |
|---|---|---|
| **Cold** | Page loads from the network | `FirstLoad` |
| **Warm** | Page shows at once from cache | `CacheHitNotModified` |
| **Live diff** | Cached page, then the price changes in place | `DataUpdated` (4 blocks) |
| **Template changed** | New menu item appears after a full load | `TemplateChanged` |
| **Offline** | Cached page with an "offline" banner | `ServedStale` |

## 2. Planned project layout

```text
examples/android-demo/
├── app/src/main/java/dev/rivqen/demo/
│   ├── DemoApp.kt          engine setup
│   ├── CatalogActivity.kt  WebView + control panel
│   └── EventLogView.kt     session events and timings
└── app/src/main/res/xml/network_security_config.xml
```

## 3. Planned code

```kotlin
// Planned API — not released
class DemoApp : Application() {
    lateinit var rivqen: RivqenEngine

    override fun onCreate() {
        super.onCreate()
        rivqen = Rivqen.create(this, RivqenConfig {
            // 10.0.2.2 is the host machine from the Android emulator.
            allowedOrigins = listOf("http://10.0.2.2:8787")
            debugEvents = BuildConfig.DEBUG
        })
    }
}
```

```kotlin
// Planned API — not released
class CatalogActivity : AppCompatActivity() {
    private var session: RivqenSession? = null

    private fun openPage() {
        val app = application as DemoApp
        session?.cancel()
        session = app.rivqen.open(
            SessionRequest(url = "http://10.0.2.2:8787/catalog", partition = Partition.forUser("demo"))
        ).also { s ->
            s.events.onEach { eventLog.add(it) }.launchIn(lifecycleScope)
            s.attach(webView)
        }
    }

    override fun onDestroy() {
        session?.cancel()
        super.onDestroy()
    }
}
```

::: warning Cleartext only in the demo
The demo uses `http://` to the local server. Allow cleartext only for `10.0.2.2` in `network_security_config.xml`, and only in the debug build. Production apps must use HTTPS.
:::

## 4. What to measure

1. Time from **Open page** to first content (native marker plus `PerformanceObserver` FCP).
2. Transferred bytes per visit (from session events).
3. Patch apply time.
4. The same values with **Rivqen off**, for comparison.

## Related

- [Getting started: Android](/guide/getting-started/android)
- [Engineering: Android SDK](/engineering/platforms/android)
