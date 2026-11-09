// swift-tools-version: 6.0
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The pure part of the WP-04 iOS prototype (tools/platform-spikes/CONTRACT.md): run log, launch
// arguments, probe parsing, chunk markers and splitting, the custom-scheme mapping and the field
// rules. No WebKit here, so `swift test` runs on the Mac.

import PackageDescription

let package = Package(
  name: "SpikeCore",
  platforms: [.iOS(.v18), .macOS(.v15)],
  products: [.library(name: "SpikeCore", targets: ["SpikeCore"])],
  targets: [
    .target(name: "SpikeCore"),
    .testTarget(name: "SpikeCoreTests", dependencies: ["SpikeCore"]),
  ]
)
