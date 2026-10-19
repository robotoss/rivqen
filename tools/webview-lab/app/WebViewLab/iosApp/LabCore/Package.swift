// swift-tools-version: 6.0
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The pure part of the iOS WebView lab app: launch arguments, case files, the request policy and
// the result format (tools/webview-lab/CONTRACT.md section 4). No WebKit here, so `swift test`
// runs on the host.

import PackageDescription

let package = Package(
  name: "LabCore",
  platforms: [.iOS(.v18), .macOS(.v15)],
  products: [.library(name: "LabCore", targets: ["LabCore"])],
  targets: [
    .target(name: "LabCore"),
    .testTarget(name: "LabCoreTests", dependencies: ["LabCore"]),
  ]
)
