// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import SwiftUI

/// The iOS WebView lab (tools/webview-lab/CONTRACT.md). A pure Swift app.
/// `ios-run.sh` builds, installs and launches it with `-labServer`.
@main
struct WebViewLabApp: App {
  @State private var model = LabModel()

  var body: some Scene {
    WindowGroup {
      ContentView(model: model)
        .task { await LabRunner(model: model).runOnce() }
    }
  }
}
