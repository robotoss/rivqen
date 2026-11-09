// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

/// Probe input from a web page (CONTRACT.md section 6). Everything here is untrusted page data:
/// names have a fixed form, values are bounded, and a page cannot write an app-side probe.
public enum Probe {
  /// Prefix of probes that the app writes itself (not a page). A page probe with it is refused.
  public static let nativePrefix = "native."
  /// The most pairs one probe script may return.
  public static let maxPairs = 64

  /// A probe name: a lower-case letter, then `[a-z0-9._-]`, at most 64 characters.
  public static func isName(_ s: String) -> Bool {
    guard let first = s.utf8.first, first >= 0x61, first <= 0x7A, s.utf8.count <= 64 else {
      return false
    }
    return s.utf8.allSatisfy { b in
      (b >= 0x61 && b <= 0x7A) || (b >= 0x30 && b <= 0x39) || b == 0x2E || b == 0x5F || b == 0x2D
    }
  }

  /// The body of a `window.rqLab.probe(name, value)` message: `{"name": <name>, "value": <any>}`.
  public static func parseMessage(_ body: Any?, limits: JSONValue.Limits = JSONValue.Limits())
    throws(SpikeError) -> (name: String, value: JSONValue)
  {
    guard let dict = body as? [String: Any] else {
      throw SpikeError(.probe, "message body is not an object")
    }
    guard dict.count <= 2 else { throw SpikeError(.probe, "message body has extra members") }
    return try pair(name: dict["name"], value: dict["value"], limits: limits)
  }

  /// The result of an app probe script: an array of `[name, value]` pairs, in order.
  public static func parseList(_ result: Any?, limits: JSONValue.Limits = JSONValue.Limits())
    throws(SpikeError) -> [(name: String, value: JSONValue)]
  {
    guard let list = result as? [Any] else {
      throw SpikeError(.probe, "probe script result is not an array")
    }
    guard list.count <= maxPairs else {
      throw SpikeError(.tooLarge, "probe script returned too many pairs")
    }
    var out: [(name: String, value: JSONValue)] = []
    for item in list {
      guard let p = item as? [Any], p.count == 2 else {
        throw SpikeError(.probe, "probe pair is not [name, value]")
      }
      out.append(try pair(name: p[0], value: p[1], limits: limits))
    }
    return out
  }

  private static func pair(name: Any?, value: Any?, limits: JSONValue.Limits) throws(SpikeError)
    -> (
      name: String, value: JSONValue
    )
  {
    guard let name = name as? String, isName(name) else {
      throw SpikeError(.probe, "probe name must match [a-z][a-z0-9._-]{0,63}")
    }
    guard !name.hasPrefix(nativePrefix) else {
      throw SpikeError(.probe, "a page cannot write a native probe")
    }
    return (name, try JSONValue.from(foundation: value, limits: limits))
  }
}
