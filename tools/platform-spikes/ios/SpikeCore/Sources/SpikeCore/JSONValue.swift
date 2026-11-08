// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import Foundation

/// A JSON value with the canonical text of the run log (CONTRACT.md section 5): keys sorted by
/// UTF-16 code units at every level, no spaces, the string escapes of JavaScript `JSON.stringify`.
/// The host writes its part with the same rules, so both parts compare byte for byte.
public enum JSONValue: Sendable, Equatable {
  case null
  case bool(Bool)
  case int(Int)
  case double(Double)
  case string(String)
  case array([JSONValue])
  case object([String: JSONValue])

  /// Limits for values that come from a web page (WebKit message bodies, script results).
  public struct Limits: Sendable {
    public var maxDepth = 8
    public var maxItems = 256
    public var maxStringUTF8 = 16 * 1024

    public init() {}
  }

  /// The canonical JSON text.
  public var canonical: String {
    var out = ""
    write(into: &out)
    return out
  }

  private func write(into out: inout String) {
    switch self {
    case .null: out += "null"
    case .bool(let b): out += b ? "true" : "false"
    case .int(let i): out += String(i)
    case .double(let d): out += Self.number(d)
    case .string(let s): Self.quote(s, into: &out)
    case .array(let items):
      out += "["
      for (i, item) in items.enumerated() {
        if i > 0 { out += "," }
        item.write(into: &out)
      }
      out += "]"
    case .object(let members):
      out += "{"
      for (i, key) in Self.sortedKeys(members).enumerated() {
        if i > 0 { out += "," }
        Self.quote(key, into: &out)
        out += ":"
        members[key, default: .null].write(into: &out)
      }
      out += "}"
    }
  }

  /// Keys in the order of JavaScript `Array.prototype.sort` (UTF-16 code units).
  public static func sortedKeys(_ members: [String: JSONValue]) -> [String] {
    members.keys.sorted { $0.utf16.lexicographicallyPrecedes($1.utf16) }
  }

  /// A number as `JSON.stringify` writes it for the values the lab uses: integral values without a
  /// fraction, others in the shortest round-trip form, non-finite values as `null`.
  static func number(_ d: Double) -> String {
    guard d.isFinite else { return "null" }
    if d == d.rounded(), abs(d) < 9_007_199_254_740_992 { return String(Int64(d)) }
    return String(d)
  }

  static func quote(_ s: String, into out: inout String) {
    out += "\""
    for scalar in s.unicodeScalars {
      switch scalar {
      case "\"": out += "\\\""
      case "\\": out += "\\\\"
      case "\u{08}": out += "\\b"
      case "\u{0C}": out += "\\f"
      case "\n": out += "\\n"
      case "\r": out += "\\r"
      case "\t": out += "\\t"
      default:
        if scalar.value < 0x20 {
          let hex = String(scalar.value, radix: 16)
          out += "\\u" + String(repeating: "0", count: 4 - hex.count) + hex
        } else {
          out.unicodeScalars.append(scalar)
        }
      }
    }
    out += "\""
  }
}

extension JSONValue {
  /// Converts a Foundation value from WebKit (`NSString`, `NSNumber`, `NSNull`, `NSArray`,
  /// `NSDictionary`, `NSDate`) into a JSON value, within `limits`. Throws for other types and for
  /// values over a limit; never truncates silently.
  public static func from(foundation value: Any?, limits: Limits = Limits()) throws(SpikeError)
    -> JSONValue
  {
    var budget = limits.maxStringUTF8
    var items = limits.maxItems
    return try convert(value, depth: 0, limits: limits, budget: &budget, items: &items)
  }

  private static func convert(
    _ value: Any?, depth: Int, limits: Limits, budget: inout Int, items: inout Int
  ) throws(SpikeError) -> JSONValue {
    guard depth <= limits.maxDepth else { throw SpikeError(.tooLarge, "value is nested too deep") }
    items -= 1
    guard items >= 0 else { throw SpikeError(.tooLarge, "value has too many items") }
    switch value {
    case nil, is NSNull:
      return .null
    case let s as String:
      budget -= s.utf8.count
      guard budget >= 0 else { throw SpikeError(.tooLarge, "value has too much text") }
      return .string(s)
    case let n as NSNumber:
      return number(n)
    case let a as [Any]:
      var out: [JSONValue] = []
      for item in a {
        out.append(
          try convert(item, depth: depth + 1, limits: limits, budget: &budget, items: &items))
      }
      return .array(out)
    case let d as [String: Any]:
      var out: [String: JSONValue] = [:]
      for (key, item) in d {
        budget -= key.utf8.count
        guard budget >= 0 else { throw SpikeError(.tooLarge, "value has too much text") }
        out[key] = try convert(
          item, depth: depth + 1, limits: limits, budget: &budget, items: &items)
      }
      return .object(out)
    case let date as Date:
      return wholeOrDouble(date.timeIntervalSince1970 * 1000)
    default:
      throw SpikeError(.probe, "unsupported value type \(type(of: value as Any))")
    }
  }

  private static func number(_ n: NSNumber) -> JSONValue {
    if CFGetTypeID(n) == CFBooleanGetTypeID() { return .bool(n.boolValue) }
    if !CFNumberIsFloatType(n), let i = Int(exactly: n.int64Value) { return .int(i) }
    return wholeOrDouble(n.doubleValue)
  }

  /// An integral double in the safe integer range as `.int` (JavaScript has one number type).
  private static func wholeOrDouble(_ d: Double) -> JSONValue {
    if d.isFinite, d == d.rounded(), abs(d) < 9_007_199_254_740_992 { return .int(Int(d)) }
    return .double(d)
  }
}
