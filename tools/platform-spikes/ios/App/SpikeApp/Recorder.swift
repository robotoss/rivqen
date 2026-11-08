// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import Foundation
import SpikeCore
import os

/// The app part of the run log, on the main actor. Every app event of a run goes through here, in
/// the order the app sees it.
@MainActor
final class Recorder {
  private(set) var log = EventLog()
  private(set) var errors = 0
  let labPort: Int
  private let start = ContinuousClock.now
  private let logger = Logger(subsystem: "dev.rivqen.platformspike", category: "RivqenSpike")

  init(labPort: Int) {
    self.labPort = labPort
  }

  /// Milliseconds since the run start (app clock).
  var now: Int { Self.ms(ContinuousClock.now - start) }

  static func ms(_ d: Duration) -> Int {
    let (s, atto) = d.components
    return Int(s) * 1000 + Int(atto / 1_000_000_000_000_000)
  }

  static func msDouble(_ d: Duration) -> Double {
    let (s, atto) = d.components
    return Double(s) * 1000 + Double(atto) / 1e15
  }

  func event(_ ev: String, _ members: [String: JSONValue] = [:]) {
    log.append(ev, t: now, members)
  }

  func probe(_ name: String, _ value: JSONValue) {
    log.probe(name, value, t: now)
    if probeCounts.count < 256 || probeCounts[name] != nil { probeCounts[name, default: 0] += 1 }
    if name == "chunk", case .int(let k) = value { lastChunkProbe = max(lastChunkProbe ?? k, k) }
  }

  /// How many probes of each name the run logged (at most 256 names). CAP-I-09 waits for `sw`.
  private(set) var probeCounts: [String: Int] = [:]

  /// The highest `chunk` probe of the current page (`nil` before the first one). The scheme
  /// handler uses it to order beacons after their chunk probe; a new web view resets it.
  var lastChunkProbe: Int?

  /// An app-side observation (`native.*` probe, see the README).
  func native(_ name: String, _ value: JSONValue) {
    probe(Probe.nativePrefix + name, value)
  }

  /// `app.error` with a class name and a message; no page data.
  func error(_ text: String) {
    errors += 1
    logger.error("\(text, privacy: .public)")
    event("app.error", ["error": .string(String(text.prefix(512)))])
  }
}

/// An error as text: domain, code and description (no user data in the lab).
func describe(_ error: any Error) -> String {
  let ns = error as NSError
  var text = "\(ns.domain) \(ns.code): \(ns.localizedDescription)"
  if let message = ns.userInfo["WKJavaScriptExceptionMessage"] as? String {
    text += " (\(message))"
  }
  return text
}

/// Waits on the main actor until `condition` holds or `timeout` seconds pass. Returns whether the
/// condition holds. The lab polls every 20 ms; WebKit callbacks change the state in between.
@MainActor
func until(_ timeout: Double, _ condition: () -> Bool) async -> Bool {
  let deadline = ContinuousClock.now + .milliseconds(Int(timeout * 1000))
  while !condition() {
    if ContinuousClock.now >= deadline { return false }
    do {
      try await Task.sleep(for: .milliseconds(20))
    } catch {
      return condition()
    }
  }
  return true
}
