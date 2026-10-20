// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import SwiftUI
import UIKit

/// The state the screen shows. The runner owns the web views; they live in `container` so that
/// each one is in a window while it loads (WebKit may throttle a web view outside a window).
@MainActor
@Observable
final class LabModel {
  var status = "idle"
  var started = false
  @ObservationIgnored let container = UIView()
}

struct ContentView: View {
  let model: LabModel

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      Text("Rivqen WebView lab").font(.headline)
      Text(model.status).font(.footnote.monospaced()).lineLimit(4)
      WebHost(container: model.container)
    }
    .padding()
  }
}

/// Shows the container view of the runner.
struct WebHost: UIViewRepresentable {
  let container: UIView

  func makeUIView(context: Context) -> UIView { container }

  func updateUIView(_ uiView: UIView, context: Context) {}
}
