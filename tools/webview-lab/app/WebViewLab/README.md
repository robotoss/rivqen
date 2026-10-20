# WebViewLab apps

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

The two apps of the WebView lab. The full procedure, the contract and the results are in [`tools/webview-lab/README.md`](../../README.md) and [`tools/webview-lab/CONTRACT.md`](../../CONTRACT.md).

| Directory | App | Build |
|---|---|---|
| [`iosApp`](./iosApp) | Pure Swift app with `WKWebView`; logic in the local Swift package `iosApp/LabCore` | `tools/webview-lab/ios-run.sh` (Xcode) |
| [`androidApp`](./androidApp) | Plain `Activity` with `android.webkit.WebView` | `tools/webview-lab/android-run.sh` (`./gradlew :androidApp:assembleDebug`) |

Tests:

- iOS logic: `swift test` in `iosApp/LabCore`.
- Android: `./gradlew :androidApp:lintDebug`; the app self-test runs in `android-run.sh`.
