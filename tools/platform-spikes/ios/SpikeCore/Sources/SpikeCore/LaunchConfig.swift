// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import Foundation

/// The capabilities of the iOS prototype (tools/platform-spikes/capabilities.json): S1 CAP-I-01 to
/// CAP-I-07, S2 CAP-I-11.
public enum Capability: String, Sendable, CaseIterable {
  case simulatedFirstRender = "CAP-I-01"
  case simulatedOrigin = "CAP-I-02"
  case simulatedCookies = "CAP-I-03"
  case navigationWithSnapshot = "CAP-I-04"
  case schemeStreaming = "CAP-I-05"
  case schemeOrigin = "CAP-I-06"
  case httpsStreaming = "CAP-I-07"
  case cookieSync = "CAP-I-11"

  /// The prototype of the capability (`prototype` of the catalog row).
  public var prototype: String {
    switch self {
    case .simulatedFirstRender, .simulatedOrigin, .simulatedCookies: "IOS-P1"
    case .navigationWithSnapshot: "IOS-P2"
    case .schemeStreaming, .schemeOrigin: "IOS-P3"
    case .httpsStreaming, .cookieSync: "IOS-ALL"
    }
  }
}

/// The origin of site A or site B of the spike host (CONTRACT.md section 1): `https`, host
/// `localhost` or `127.0.0.1`, a port, nothing else.
public struct LabOrigin: Sendable, Equatable {
  public static let allowedHosts: Set<String> = ["localhost", "127.0.0.1"]

  public let host: String
  public let port: Int

  public init(host: String, port: Int) throws(SpikeError) {
    guard Self.allowedHosts.contains(host) else {
      throw SpikeError(.argument, "origin host must be localhost or 127.0.0.1")
    }
    guard (1...65_535).contains(port) else {
      throw SpikeError(.argument, "origin port is out of range")
    }
    self.host = host
    self.port = port
  }

  /// Parses `https://<host>:<port>` with an optional trailing `/`.
  public static func parse(_ text: String) throws(SpikeError) -> LabOrigin {
    let prefix = "https://"
    guard text.hasPrefix(prefix), text.count <= 64 else {
      throw SpikeError(.argument, "origin must be https://<host>:<port>")
    }
    var rest = Substring(text.dropFirst(prefix.count))
    if rest.hasSuffix("/") { rest = rest.dropLast() }
    guard let colon = rest.lastIndex(of: ":") else {
      throw SpikeError(.argument, "origin needs an explicit port")
    }
    let digits = rest[rest.index(after: colon)...]
    guard !digits.isEmpty, digits.count <= 5, digits.allSatisfy({ $0.isASCII && $0.isNumber }),
      let port = Int(digits)
    else {
      throw SpikeError(.argument, "origin port must be digits")
    }
    return try LabOrigin(host: String(rest[..<colon]), port: port)
  }

  /// `https://<host>:<port>`.
  public var https: String { "https://\(host):\(port)" }
  /// `rqlab://<host>:<port>` (IOS-P3).
  public var scheme: String { "\(LabScheme.name)://\(host):\(port)" }
}

/// The launch arguments of one run: `-rqRun <id> -rqCap <capability> -rqOrigin <site A>`, and
/// optional `-rqTimeout <seconds>` for one step. The run script passes them with `simctl launch`.
public struct LaunchConfig: Sendable, Equatable {
  public static let defaultTimeout = 20.0

  public let run: String
  public let capability: Capability
  public let origin: LabOrigin
  public let stepTimeout: Double

  /// Parses the process arguments. Returns `nil` when `-rqRun` is absent (the app stays idle).
  /// Arguments that do not start with `-rq` are ignored (Xcode and the system add their own).
  public static func parse(_ arguments: [String]) throws(SpikeError) -> LaunchConfig? {
    var values: [String: String] = [:]
    var i = 1
    while i < arguments.count {
      let flag = arguments[i]
      guard flag.hasPrefix("-rq") else {
        i += 1
        continue
      }
      guard ["-rqRun", "-rqCap", "-rqOrigin", "-rqTimeout"].contains(flag) else {
        throw SpikeError(.argument, "unknown flag \(flag.prefix(32))")
      }
      guard i + 1 < arguments.count else { throw SpikeError(.argument, "\(flag) needs a value") }
      guard values[flag] == nil else { throw SpikeError(.argument, "\(flag) is repeated") }
      values[flag] = arguments[i + 1]
      i += 2
    }
    guard let run = values["-rqRun"] else { return nil }
    guard let capText = values["-rqCap"], let capability = Capability(rawValue: capText) else {
      throw SpikeError(.argument, "-rqCap must be one of CAP-I-01...CAP-I-07, CAP-I-11")
    }
    guard isRunId(run), run.hasPrefix("\(capability.rawValue)-ios-") else {
      throw SpikeError(.argument, "-rqRun must be <capability>-ios-<os>-<k> of [A-Za-z0-9.-]")
    }
    guard let originText = values["-rqOrigin"] else {
      throw SpikeError(.argument, "-rqOrigin is missing")
    }
    let origin = try LabOrigin.parse(originText)
    var timeout = defaultTimeout
    if let text = values["-rqTimeout"] {
      guard let t = Double(text), t.isFinite, (1...120).contains(t) else {
        throw SpikeError(.argument, "-rqTimeout must be 1 to 120 seconds")
      }
      timeout = t
    }
    return LaunchConfig(run: run, capability: capability, origin: origin, stepTimeout: timeout)
  }

  /// Run id rule of CONTRACT.md section 4 (`[A-Za-z0-9.-]`, at most 128 characters).
  public static func isRunId(_ s: String) -> Bool {
    !s.isEmpty && s.utf8.count <= 128
      && s.utf8.allSatisfy { b in
        (b >= 0x30 && b <= 0x39) || (b >= 0x41 && b <= 0x5A) || (b >= 0x61 && b <= 0x7A)
          || b == 0x2E
          || b == 0x2D
      }
  }
}
