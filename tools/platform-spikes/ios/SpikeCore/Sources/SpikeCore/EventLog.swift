// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

/// The app part of a run log (CONTRACT.md section 5): one canonical JSON line per event with
/// `src` `app`, `seq` from 0 in append order and `t` in milliseconds of the app clock.
public struct EventLog: Sendable {
  /// The most events one run keeps. Later events are counted in `dropped`, not stored.
  public static let maxEvents = 20_000

  public private(set) var lines: [String] = []
  public private(set) var dropped = 0
  /// Metric probes (`metric.*`) in append order, for `run-<k>.metrics.json`.
  public private(set) var metrics: [(name: String, value: JSONValue)] = []

  public init() {}

  /// Adds an event. `ev`, `src`, `seq` and `t` in `members` are replaced by the log values.
  public mutating func append(_ ev: String, t: Int, _ members: [String: JSONValue] = [:]) {
    guard lines.count < Self.maxEvents else {
      dropped += 1
      return
    }
    var all = members
    all["ev"] = .string(ev)
    all["src"] = .string("app")
    all["seq"] = .int(lines.count)
    all["t"] = .int(t)
    lines.append(JSONValue.object(all).canonical)
  }

  /// Adds an `app.probe` event; a `metric.*` probe also goes to `metrics`.
  public mutating func probe(_ name: String, _ value: JSONValue, t: Int) {
    append("app.probe", t: t, ["name": .string(name), "value": value])
    if name.hasPrefix("metric.") { metrics.append((name, value)) }
  }

  /// JSON Lines text: every line ends with LF.
  public var text: String {
    lines.map { $0 + "\n" }.joined()
  }

  /// The metrics file of the run: `{"metrics": {...}, "run": "<id>"}`; a repeated name keeps the
  /// first value and gets `#2`, `#3` for the next ones.
  public func metricsJSON(run: String) -> String {
    var members: [String: JSONValue] = [:]
    for (name, value) in metrics {
      var key = name
      var n = 1
      while members[key] != nil {
        n += 1
        key = "\(name)#\(n)"
      }
      members[key] = value
    }
    return JSONValue.object(["metrics": .object(members), "run": .string(run)]).canonical + "\n"
  }
}
