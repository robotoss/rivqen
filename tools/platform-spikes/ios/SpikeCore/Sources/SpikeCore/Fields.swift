// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import Foundation

/// Header field rules of the prototype.
public enum Fields {
  /// Fields that describe the transfer, not the document. A simulated or a scheme-handler
  /// response does not carry them: the app gives WebKit the decoded bytes in full or in parts.
  static let transferFields: Set<String> = [
    "connection", "content-encoding", "content-length", "keep-alive", "transfer-encoding",
  ]

  /// The `text/html` type with `utf-8` of H-66.
  public static let htmlUTF8 = "text/html; charset=utf-8"

  /// Fields of the response that the app gives WebKit (`loadSimulatedRequest`,
  /// `WKURLSchemeTask.didReceive(_:)`): the fetched fields without transfer fields, and
  /// `Content-Type` set to `text/html; charset=utf-8` when `html` is true (H-66).
  public static func forWebKit(_ fetched: [String: String], html: Bool) -> [String: String] {
    var out: [String: String] = [:]
    for (name, value) in fetched {
      let lower = name.lowercased()
      if transferFields.contains(lower) { continue }
      if html, lower == "content-type" { continue }
      out[name] = value
    }
    if html { out["Content-Type"] = htmlUTF8 }
    return out
  }

  /// Fields for the run log: lower-case names, `date` removed (its value changes on every run),
  /// a cookie value outside `[a-z0-9-]` replaced by `[redacted]` (the host rule).
  public static func forLog(_ fields: [String: String]) -> [String: JSONValue] {
    var out: [String: JSONValue] = [:]
    for (name, value) in fields {
      let lower = name.lowercased()
      if lower == "date" { continue }
      out[lower] = .string(loggable(lower, value))
    }
    return out
  }

  /// A field value that is safe for the log.
  public static func loggable(_ lowerName: String, _ value: String) -> String {
    guard lowerName == "cookie" || lowerName == "set-cookie" else { return value }
    let segments =
      lowerName == "set-cookie"
      ? [Substring(value.split(separator: ";", omittingEmptySubsequences: false).first ?? "")]
      : value.split(separator: ";", omittingEmptySubsequences: false)
    let ok = segments.allSatisfy { s in
      guard let eq = s.firstIndex(of: "=") else { return false }
      let v = s[s.index(after: eq)...].trimmingCharacters(in: .whitespaces)
      return v.utf8.allSatisfy { b in
        (b >= 0x61 && b <= 0x7A) || (b >= 0x30 && b <= 0x39) || b == 0x2D
      }
    }
    return ok ? value : "[redacted]"
  }
}
