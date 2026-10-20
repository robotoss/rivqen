// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import Foundation

/// The lab server: `http://127.0.0.1:<port>` (CONTRACT section 3; the simulator shares the
/// network of the host).
public struct LabServer: Equatable, Sendable {
  public let port: Int

  public init(port: Int) throws(LabError) {
    guard (1...65_535).contains(port) else {
      throw LabError(.argument, "server port \(port) is out of range")
    }
    self.port = port
  }

  public var origin: String { "http://127.0.0.1:\(port)" }

  /// The URL of the case list.
  public var indexURL: URL? { URL(string: "\(origin)/cases.json") }

  /// The URL of one case. The id must be valid (it is a path segment).
  public func caseURL(_ id: String) -> URL? {
    guard LabCase.isValidId(id) else { return nil }
    return URL(string: "\(origin)/c/\(id)")
  }
}

/// The launch arguments of the app: `-labServer http://127.0.0.1:<port>`, optional
/// `-labSettle <seconds>` (0 to 2, default 1) and `-labTimeout <seconds>` (1 to 120, default 20).
public struct LabConfig: Equatable, Sendable {
  public let server: LabServer
  /// The wait after `didFinish` (CONTRACT section 4 step 2: at most 2 s).
  public let settleSeconds: Double
  /// The longest wait for one load and for one script evaluation.
  public let timeoutSeconds: Double

  public static let defaultSettle = 1.0
  public static let defaultTimeout = 20.0

  /// Parse the process arguments. Returns nil when `-labServer` is absent (the app then idles).
  public static func parse(_ arguments: [String]) throws(LabError) -> LabConfig? {
    guard let serverText = try value(of: "-labServer", in: arguments) else { return nil }
    let server = try parseServer(serverText)
    let settle = try number(of: "-labSettle", in: arguments, range: 0...2) ?? defaultSettle
    let timeout = try number(of: "-labTimeout", in: arguments, range: 1...120) ?? defaultTimeout
    return LabConfig(server: server, settleSeconds: settle, timeoutSeconds: timeout)
  }

  /// Only `http://127.0.0.1:<port>` with an optional `/` is a lab server. Another host would make
  /// the request policy allow that host.
  public static func parseServer(_ text: String) throws(LabError) -> LabServer {
    guard let parts = URLComponents(string: text),
      parts.scheme == "http",
      parts.host == "127.0.0.1",
      parts.user == nil,
      parts.password == nil,
      parts.query == nil,
      parts.fragment == nil,
      parts.path.isEmpty || parts.path == "/",
      let port = parts.port
    else {
      throw LabError(.argument, "-labServer must be http://127.0.0.1:<port>")
    }
    return try LabServer(port: port)
  }

  private static func value(of name: String, in arguments: [String]) throws(LabError) -> String? {
    guard let index = arguments.firstIndex(of: name) else { return nil }
    let next = arguments.index(after: index)
    guard next < arguments.endIndex else {
      throw LabError(.argument, "\(name) needs a value")
    }
    return arguments[next]
  }

  private static func number(
    of name: String, in arguments: [String], range: ClosedRange<Double>
  ) throws(LabError) -> Double? {
    guard let text = try value(of: name, in: arguments) else { return nil }
    guard let number = Double(text), number.isFinite, range.contains(number) else {
      throw LabError(
        .argument, "\(name) must be a number in \(range.lowerBound)...\(range.upperBound)")
    }
    return number
  }
}
