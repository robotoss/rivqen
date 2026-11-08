// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import SwiftUI
import UIKit

/// The WP-04 iOS prototype (tools/platform-spikes/ios/README.md). `run.sh` launches it with
/// `-rqRun <id> -rqCap <capability> -rqOrigin <site A>`; without `-rqRun` it stays idle.
@main
struct SpikeApp: App {
  @State private var model = SpikeModel()

  var body: some Scene {
    WindowGroup {
      SpikeView(model: model)
        .task { await Runner(model: model).runOnce() }
    }
  }
}

/// The state the screen shows. The web views live in `container`, so each one is in a window while
/// it loads (WebKit may throttle a web view outside a window).
@MainActor
@Observable
final class SpikeModel {
  var status = "idle"
  var started = false
  @ObservationIgnored let container = UIView()
}

struct SpikeView: View {
  let model: SpikeModel

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      Text("Rivqen platform spike").font(.headline)
      Text(model.status).font(.footnote.monospaced()).lineLimit(3)
      WebHost(container: model.container)
    }
    .padding()
  }
}

struct WebHost: UIViewRepresentable {
  let container: UIView

  func makeUIView(context: Context) -> UIView { container }

  func updateUIView(_ uiView: UIView, context: Context) {}
}
