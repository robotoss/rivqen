// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import Foundation

/// CONTRACT section 6: case pages never load anything from the network; the app blocks every
/// request that is not to the lab server. Two layers use this type:
/// - the navigation delegate (`allows`) for main-frame and subframe navigations;
/// - a content rule list (`contentRuleListJSON`) for subresources (images, scripts, styles,
///   fetch, media, fonts, frames, WebSocket).
public struct RequestPolicy: Equatable, Sendable {
  public let server: LabServer

  public init(server: LabServer) {
    self.server = server
  }

  /// True for a URL of the lab server, and for `about:blank` and `about:srcdoc` (no request).
  public func allows(_ url: URL) -> Bool {
    let text = url.absoluteString
    if text == "about:blank" || text == "about:srcdoc" { return true }
    guard let parts = URLComponents(url: url, resolvingAgainstBaseURL: false),
      parts.scheme?.lowercased() == "http",
      parts.host == "127.0.0.1",
      parts.user == nil,
      parts.password == nil
    else {
      return false
    }
    return (parts.port ?? 80) == server.port
  }

  /// A WebKit content rule list: block every load, then stop blocking loads whose URL starts
  /// with the lab origin. Rules apply in order; `ignore-previous-rules` cancels the block.
  public func contentRuleListJSON() throws(LabError) -> String {
    let rules: [[String: [String: String]]] = [
      ["trigger": ["url-filter": ".*"], "action": ["type": "block"]],
      ["trigger": ["url-filter": labURLFilter], "action": ["type": "ignore-previous-rules"]],
    ]
    do {
      let data = try JSONSerialization.data(
        withJSONObject: rules, options: [.sortedKeys, .withoutEscapingSlashes])
      return String(decoding: data, as: UTF8.self)
    } catch {
      throw LabError(.io, "content rule list: \(error.localizedDescription)")
    }
  }

  /// The `url-filter` (a WebKit content blocker regular expression) of the lab origin.
  public var labURLFilter: String { "^http://127\\.0\\.0\\.1:\(server.port)/" }
}
