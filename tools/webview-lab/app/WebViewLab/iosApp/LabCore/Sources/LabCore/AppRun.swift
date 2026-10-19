// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import Foundation

/// The app part of `results/ios/run.json`. `ios-run.sh` adds the host part (simulator name from
/// `simctl`, Xcode version, hash of `serialize.js`).
public struct AppRun: Equatable, Sendable, Encodable {
  public var platform = "ios"
  public var osName: String
  public var osVersion: String
  public var simulatorDevice: String?
  public var simulatorModel: String?
  public var simulatorRuntime: String?
  public var simulatorRuntimeBuild: String?
  /// `CFBundleShortVersionString` and `CFBundleVersion` of WebKit.framework.
  public var webKitVersion: String?
  public var webKitBundleVersion: String?
  public var userAgent: String?
  public var server: String
  public var settleSeconds: Double
  public var timeoutSeconds: Double
  public var serializerBytes: Int
  public var startedAt: String
  public var finishedAt: String?
  /// Content JavaScript controls (one per view) and the evaluation facts.
  public var controls: [ControlResult]
  public var walkerNotes: [String]
  /// Navigations the delegate cancelled, per case and view (subresource blocks by the content
  /// rule list are not reported by WebKit).
  public var blockedNavigations: [BlockedNavigation]
  public var results: Int
  public var walkerOk: Int
  public var walkerUnavailable: Int
  public var outcome: String?

  public init(
    osName: String, osVersion: String, server: String, settleSeconds: Double,
    timeoutSeconds: Double,
    serializerBytes: Int, startedAt: String
  ) {
    self.osName = osName
    self.osVersion = osVersion
    self.server = server
    self.settleSeconds = settleSeconds
    self.timeoutSeconds = timeoutSeconds
    self.serializerBytes = serializerBytes
    self.startedAt = startedAt
    controls = []
    walkerNotes = []
    blockedNavigations = []
    results = 0
    walkerOk = 0
    walkerUnavailable = 0
  }

  public func encoded() throws(LabError) -> Data {
    do {
      return try LabResult.encoder.encode(self)
    } catch {
      throw LabError(.io, "encode run log: \(error.localizedDescription)")
    }
  }
}
/// One control page per view. It shows what the content JavaScript switch does to the page
/// (does a page script run, how `noscript` parses, which `script` elements and event handler
/// attributes stay in the DOM), and whether the app can evaluate JavaScript in each content world.
public struct ControlResult: Equatable, Sendable, Encodable {
  public let view: LabView
  /// What the app world saw in the page; nil when the read failed (see the evaluation fields).
  public let dom: ControlProbe?
  /// `ok` or the error of `evaluateJavaScript` in `WKContentWorld.defaultClient`.
  public let defaultClientEvaluation: String
  /// `ok` or the error of `evaluateJavaScript` in `WKContentWorld.page`.
  public let pageWorldEvaluation: String
  /// `ok` or the error of `callAsyncJavaScript` in `WKContentWorld.defaultClient`.
  public let defaultClientCallAsync: String

  public init(
    view: LabView, dom: ControlProbe?, defaultClientEvaluation: String, pageWorldEvaluation: String,
    defaultClientCallAsync: String
  ) {
    self.view = view
    self.dom = dom
    self.defaultClientEvaluation = defaultClientEvaluation
    self.pageWorldEvaluation = pageWorldEvaluation
    self.defaultClientCallAsync = defaultClientCallAsync
  }

  /// The control page. parse5 gives: view on, 2 `script` elements (the inline one and the data
  /// block) and `noscript` content as text; view off, 3 `script` elements and a `p#lab-n`.
  /// The `onclick` attribute is in the DOM in both views.
  public static let html = """
    <!DOCTYPE html><html><head><title>control</title></head><body>\
    <script>document.documentElement.setAttribute("data-lab-ran", "1");</script>\
    <script type="application/json" id="lab-data">{}</script>\
    <noscript><p id="lab-n">n</p><script type="application/json" id="lab-ns-data">{}</script></noscript>\
    <p id="lab-h" onclick="return 0">h</p></body></html>
    """

  /// Evaluated in a content world; the value is the JSON of `ControlProbe`.
  public static let probeScript = """
    JSON.stringify({
      pageScriptRan: document.documentElement.getAttribute("data-lab-ran") === "1",
      noscriptParsedAsMarkup: document.getElementById("lab-n") !== null,
      scriptElements: document.getElementsByTagName("script").length,
      dataScriptInDOM: document.getElementById("lab-data") !== null,
      noscriptDataScriptInDOM: document.getElementById("lab-ns-data") !== null,
      onclickAttributeInDOM: (function () {
        var p = document.getElementById("lab-h");
        return p !== null && p.hasAttribute("onclick");
      })()
    })
    """
}

/// The value of `ControlResult.probeScript`.
public struct ControlProbe: Equatable, Sendable, Codable {
  public let pageScriptRan: Bool
  public let noscriptParsedAsMarkup: Bool
  public let scriptElements: Int
  public let dataScriptInDOM: Bool
  public let noscriptDataScriptInDOM: Bool
  public let onclickAttributeInDOM: Bool

  public static let maxBytes = 4_096

  public static func decode(_ text: String) -> ControlProbe? {
    let data = Data(text.utf8)
    guard data.count <= maxBytes else { return nil }
    return try? JSONDecoder().decode(ControlProbe.self, from: data)
  }
}

public struct BlockedNavigation: Equatable, Sendable, Encodable {
  public let caseId: String
  public let view: LabView
  public let url: String

  public init(caseId: String, view: LabView, url: String) {
    self.caseId = caseId
    self.view = view
    self.url = url
  }
}

/// Read a file of at most `limit` bytes. A larger file is an error, not a cut.
public func readBounded(_ url: URL, limit: Int) throws(LabError) -> Data {
  do {
    let handle = try FileHandle(forReadingFrom: url)
    // A close error of a read-only handle loses no data; the read result decides.
    defer { try? handle.close() }
    let data = try handle.read(upToCount: limit + 1) ?? Data()
    guard data.count <= limit else {
      throw LabError(.tooLarge, "\(url.lastPathComponent) has more than \(limit) bytes")
    }
    return data
  } catch let error as LabError {
    throw error
  } catch {
    throw LabError(.io, "read \(url.lastPathComponent): \(error.localizedDescription)")
  }
}
