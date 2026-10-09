# Android

This guide shows how to add Rivqen to an Android app that shows pages in a `WebView`.

::: warning Planned API — not released
The artifact coordinates and the API can change before the first alpha.
:::

## Requirements

- An Android app module that uses `android.webkit.WebView` (Android System WebView).
- Kotlin. Java callers are supported through a Java-friendly API.
- Minimum Android version: see the [Support matrix](/guide/support-matrix).
- A server that uses a Rivqen server SDK, for template and data updates. (Legacy VasSonic servers work with the optional legacy module until Rivqen 1.5.)

## Step 1 — Add the dependency

1. Open the `build.gradle.kts` file of your app module.
2. Add the Rivqen dependency to the `dependencies` block.

```kotlin
// Planned API — not released
dependencies {
    implementation("dev.rivqen:rivqen-android:0.1.0")
}
```

3. Synchronize the project.

## Step 2 — Create the engine

1. Create one engine for the whole app. Do this in your `Application` class.
2. List the origins that Rivqen may accelerate.

```kotlin
// Planned API — not released
class App : Application() {
    lateinit var rivqen: RivqenEngine

    override fun onCreate() {
        super.onCreate()
        rivqen = Rivqen.create(this, RivqenConfig {
            allowedOrigins = listOf("https://shop.example.com")
            // RQP is the default. Add the optional legacy module only for migration.
        })
    }
}
```

## Step 3 — Open a page

1. Create the session **before** you create or show the WebView. This starts the download early.
2. Attach the session to the WebView.

```kotlin
// Planned API — not released
val session = app.rivqen.open(
    SessionRequest(
        url = "https://shop.example.com/catalog",
        partition = Partition.forUser(currentUserId), // non-secret ID
    )
)

val webView = WebView(this)
session.attach(webView)
setContentView(webView)
```

## Step 4 — Close the session

1. Cancel the session when the screen closes.

```kotlin
// Planned API — not released
override fun onDestroy() {
    session.cancel()
    super.onDestroy()
}
```

## Step 5 — Handle logout

::: danger Risk of data leak between accounts
Tell Rivqen when the user logs out. If you do not, cached pages of the old user can stay readable.
:::

1. Call `clearCache` with the old user's partition when the user logs out or switches accounts.

```kotlin
// Planned API — not released
app.rivqen.clearCache(Partition.forUser(oldUserId))
```

## Step 6 — Check the result

1. Open the page two times.
2. Read the session events in Logcat (debug builds only) or in your metrics sink.
3. The second visit must show `CacheHitNotModified` or `DataUpdated`.

## Next steps

- [Troubleshooting](/guide/troubleshooting)
- [Engineering: Android SDK](/engineering/platforms/android)
