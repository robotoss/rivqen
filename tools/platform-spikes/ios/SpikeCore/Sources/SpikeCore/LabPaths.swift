// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

/// Paths of the host routes (CONTRACT.md section 3) with the run id, so the host logs every
/// request under the run. Run ids and the other values are `[A-Za-z0-9.-]`, so no escaping is
/// needed; `LaunchConfig` checks the run id.
public struct LabPaths: Sendable, Equatable {
  public enum Gate: String, Sendable {
    case beacon
    case delay
  }

  public let run: String

  public init(run: String) {
    self.run = run
  }

  /// `/s/stream` with `chunks`, `size`, the gate and its time (`timeout` for `beacon`, `delay` for
  /// `delay`), in milliseconds.
  public func stream(chunks: Int, size: Int, gate: Gate, ms: Int) -> String {
    let timeName = gate == .beacon ? "timeout" : "delay"
    return
      "/s/stream?run=\(run)&chunks=\(chunks)&size=\(size)&gate=\(gate.rawValue)&\(timeName)=\(ms)"
  }

  /// `/s/echo`; `tag` makes requests of one run tell apart in the host log; `html` gives the
  /// `text/html` form.
  public func echo(tag: String, html: Bool = false) -> String {
    "/s/echo?run=\(run)&tag=\(tag)" + (html ? "&html=1" : "")
  }

  /// The `k` of a beacon URL (`/s/beacon?...&k=<k>`), or `nil` for any other URL. At most 7
  /// digits (the host allows 1024 chunks).
  public static func beaconIndex(path: String, query: String?) -> Int? {
    guard path == "/s/beacon", let query else { return nil }
    for item in query.split(separator: "&") where item.hasPrefix("k=") {
      let digits = item.dropFirst(2)
      guard !digits.isEmpty, digits.count <= 7, digits.utf8.allSatisfy({ $0 >= 0x30 && $0 <= 0x39 })
      else {
        return nil
      }
      return Int(digits)
    }
    return nil
  }

  /// `/s/csp` (S2): the CSP page in `mode` (`header`, `meta`, `both`); `tag` tells the loads apart.
  public func csp(mode: String, tag: String) -> String {
    "/s/csp?run=\(run)&mode=\(mode)&tag=\(tag)"
  }

  /// `/s/sw-page` (S2); `tag` tells the loads apart (the page drops it when it reloads itself).
  public func swPage(tag: String) -> String {
    "/s/sw-page?run=\(run)&tag=\(tag)"
  }

  /// `/s/hist` (S2) step `step`, in the form of the page's own `#next` link.
  public func hist(step: Int) -> String {
    "/s/hist?run=\(run)&step=\(step)"
  }

  /// `/s/cookie/set` with a synthetic name and value and the attribute list.
  public func cookieSet(name: String, value: String, attrs: [String] = []) -> String {
    let base = "/s/cookie/set?run=\(run)&name=\(name)&value=\(value)"
    return attrs.isEmpty ? base : base + "&attrs=" + attrs.joined(separator: ",")
  }
}
