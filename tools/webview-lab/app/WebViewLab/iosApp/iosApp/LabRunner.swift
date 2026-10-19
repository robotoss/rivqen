// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import Foundation
import LabCore
import UIKit
import WebKit

/// Runs every case of the lab server in each view of the case (CONTRACT section 4).
///
/// Input (written by `ios-run.sh` into the data container): `Documents/lab-input/serialize.js`
/// and `Documents/lab-input/cases/<id>.json`. Output: `Documents/lab-output/<id>.<view>.json`,
/// `Documents/lab-output/app-run.json`, and last `Documents/lab-output/DONE` with `ok` or the
/// error. The case list comes from `GET /cases.json` of the lab server.
@MainActor
final class LabRunner {
  private let model: LabModel

  /// The largest `serialize.js` the app accepts (the file has about 3.5 KiB).
  static let maxSerializerBytes = 256 * 1024
  /// The most blocked navigations kept in the run log.
  static let maxBlockedInLog = 1_000

  init(model: LabModel) {
    self.model = model
  }

  func runOnce() async {
    guard !model.started else { return }
    model.started = true
    let output: URL
    do {
      output = try prepareOutput()
    } catch {
      model.status = "error: \(error)"
      return
    }
    let outcome: String
    do {
      guard let config = try LabConfig.parse(ProcessInfo.processInfo.arguments) else {
        model.status = "idle: launch with -labServer http://127.0.0.1:<port>"
        return
      }
      try await run(config, output: output)
      outcome = "ok"
    } catch {
      outcome = "error: \(error)"
    }
    model.status = "done: \(outcome)"
    do {
      try Data(outcome.utf8).write(to: output.appendingPathComponent("DONE"), options: .atomic)
    } catch {
      model.status = "done: \(outcome); DONE not written: \(describe(error))"
    }
  }

  // MARK: Run

  private func run(_ config: LabConfig, output: URL) async throws {
    let input = try documents().appendingPathComponent("lab-input", isDirectory: true)
    let serializerData = try readBounded(
      input.appendingPathComponent("serialize.js"), limit: Self.maxSerializerBytes)
    guard let serializer = String(data: serializerData, encoding: .utf8) else {
      throw LabError(.io, "serialize.js is not UTF-8")
    }
    let policy = RequestPolicy(server: config.server)
    var log = AppRun(
      osName: UIDevice.current.systemName, osVersion: UIDevice.current.systemVersion,
      server: config.server.origin, settleSeconds: config.settleSeconds,
      timeoutSeconds: config.timeoutSeconds,
      serializerBytes: serializerData.count, startedAt: Date().formatted(.iso8601))
    fillPlatform(&log)

    model.status = "case list"
    let ids = try await fetchIndex(config)
    let rules = try await compileRules(policy)

    for view in LabView.allCases {
      model.status = "control \(view.rawValue)"
      log.controls.append(
        await control(view, config: config, policy: policy, rules: rules, log: &log))
    }

    var offOk = 0
    var offTotal = 0
    for (index, id) in ids.enumerated() {
      let caseData = try readBounded(
        input.appendingPathComponent("cases/\(id).json"), limit: LabCase.maxFileBytes)
      let labCase = try LabCase.parse(caseData, expectedId: id)
      for view in labCase.views {
        model.status = "\(index + 1)/\(ids.count) \(id) \(view.rawValue)"
        let (result, blocked) = await runCase(
          labCase, view: view, serializer: serializer, config: config, policy: policy, rules: rules)
        try result.encoded().write(
          to: output.appendingPathComponent(result.fileName), options: .atomic)
        log.results += 1
        if result.walker == .ok { log.walkerOk += 1 } else { log.walkerUnavailable += 1 }
        if view == .off {
          offTotal += 1
          if result.walker == .ok { offOk += 1 }
        }
        for url in blocked where log.blockedNavigations.count < Self.maxBlockedInLog {
          log.blockedNavigations.append(BlockedNavigation(caseId: id, view: view, url: url))
        }
      }
    }

    log.walkerNotes = notes(
      config: config, serializerBytes: serializerData.count, offOk: offOk, offTotal: offTotal)
    log.finishedAt = Date().formatted(.iso8601)
    log.outcome = "ok"
    try log.encoded().write(to: output.appendingPathComponent("app-run.json"), options: .atomic)
  }

  /// One case in one view, in a new web view with a new non-persistent data store.
  private func runCase(
    _ labCase: LabCase, view: LabView, serializer: String, config: LabConfig, policy: RequestPolicy,
    rules: WKContentRuleList
  ) async -> (LabResult, [String]) {
    let session = PageSession(policy: policy, allowsContentJavaScript: view == .on)
    let webView = makeWebView(view: view, rules: rules, session: session)
    defer { tearDown(webView) }
    let id = labCase.id
    guard let url = config.server.caseURL(id) else {
      return (.unavailable(id: id, view: view, reason: "no URL for the case id"), session.blocked)
    }
    let request = URLRequest(
      url: url, cachePolicy: .reloadIgnoringLocalAndRemoteCacheData,
      timeoutInterval: config.timeoutSeconds)
    let outcome = await session.load(webView, timeout: config.timeoutSeconds) { $0.load(request) }
    switch outcome {
    case .failed(let reason):
      return (.unavailable(id: id, view: view, reason: "load failed: \(reason)"), session.blocked)
    case .finished(let status) where status != 200:
      let text = status.map(String.init) ?? "none"
      return (.unavailable(id: id, view: view, reason: "HTTP status \(text)"), session.blocked)
    case .finished:
      break
    }
    do {
      try await Task.sleep(for: .seconds(config.settleSeconds))
    } catch {
      return (.unavailable(id: id, view: view, reason: "cancelled"), session.blocked)
    }

    // CONTRACT section 4 step 3: the walker runs in the app world. For view off, content
    // JavaScript stays off; a failure here is a result ("unavailable"), not a retry.
    let tree = await evaluate(
      serializer, in: webView, world: .defaultClient, timeout: config.timeoutSeconds)
    let (meta, metaError) = await readMeta(
      labCase.probe, in: webView, timeout: config.timeoutSeconds)
    guard case .string(let treeText) = tree else {
      let reason = "evaluateJavaScript in WKContentWorld.defaultClient: \(tree.summary)"
      return (.unavailable(id: id, view: view, reason: reason, meta: meta), session.blocked)
    }
    do {
      return (
        try LabResult.ok(id: id, view: view, tree: treeText, meta: meta, metaError: metaError),
        session.blocked
      )
    } catch {
      return (.unavailable(id: id, view: view, reason: "\(error)", meta: meta), session.blocked)
    }
  }

  /// `characterSet`, `contentType` and the probe text, read in the app world.
  private func readMeta(_ probe: String?, in webView: WKWebView, timeout: Double) async -> (
    PageMeta?, String?
  ) {
    let outcome = await callAsync(
      PageMeta.script, arguments: ["probe": probe ?? NSNull()], in: webView, world: .defaultClient,
      timeout: timeout)
    guard case .string(let text) = outcome else {
      return (nil, "callAsyncJavaScript in WKContentWorld.defaultClient: \(outcome.summary)")
    }
    do {
      return (try PageMeta.decode(text), nil)
    } catch {
      return (nil, "page meta: \(error)")
    }
  }

  /// Content JavaScript control: a `loadHTMLString` page with the lab origin as base URL (see
  /// `ControlResult.html`). It also records which evaluation paths work in the view.
  private func control(
    _ view: LabView, config: LabConfig, policy: RequestPolicy, rules: WKContentRuleList,
    log: inout AppRun
  ) async -> ControlResult {
    let session = PageSession(policy: policy, allowsContentJavaScript: view == .on)
    let webView = makeWebView(view: view, rules: rules, session: session)
    defer { tearDown(webView) }
    let base = URL(string: "\(config.server.origin)/")
    let outcome = await session.load(webView, timeout: config.timeoutSeconds) {
      $0.loadHTMLString(ControlResult.html, baseURL: base)
    }
    if case .failed(let reason) = outcome {
      let text = "control load failed: \(reason)"
      return ControlResult(
        view: view, dom: nil, defaultClientEvaluation: text, pageWorldEvaluation: text,
        defaultClientCallAsync: text)
    }
    let timeout = config.timeoutSeconds
    let client = await evaluate(
      ControlResult.probeScript, in: webView, world: .defaultClient, timeout: timeout)
    let page = await evaluate(
      ControlResult.probeScript, in: webView, world: .page, timeout: timeout)
    let async = await callAsync(
      "return typeof document;", arguments: [:], in: webView, world: .defaultClient,
      timeout: timeout)
    if log.userAgent == nil,
      case .string(let agent) = await evaluate(
        "navigator.userAgent", in: webView, world: .defaultClient, timeout: timeout)
    {
      log.userAgent = agent
    }
    var dom: ControlProbe?
    if case .string(let text) = client { dom = ControlProbe.decode(text) }
    return ControlResult(
      view: view, dom: dom, defaultClientEvaluation: client.summary,
      pageWorldEvaluation: page.summary,
      defaultClientCallAsync: async.summary)
  }

  // MARK: Web view

  private func makeWebView(view: LabView, rules: WKContentRuleList, session: PageSession)
    -> WKWebView
  {
    let configuration = WKWebViewConfiguration()
    configuration.websiteDataStore = .nonPersistent()
    configuration.defaultWebpagePreferences.allowsContentJavaScript = view == .on
    configuration.preferences.javaScriptCanOpenWindowsAutomatically = false
    configuration.userContentController.add(rules)
    // Data detectors would add `a` elements to the DOM.
    configuration.dataDetectorTypes = []
    configuration.mediaTypesRequiringUserActionForPlayback = .all
    let bounds = model.container.bounds
    let frame = bounds.isEmpty ? CGRect(x: 0, y: 0, width: 390, height: 600) : bounds
    let webView = WKWebView(frame: frame, configuration: configuration)
    webView.navigationDelegate = session
    webView.uiDelegate = session
    model.container.addSubview(webView)
    return webView
  }

  private func tearDown(_ webView: WKWebView) {
    webView.stopLoading()
    webView.navigationDelegate = nil
    webView.uiDelegate = nil
    webView.removeFromSuperview()
  }

  private func compileRules(_ policy: RequestPolicy) async throws -> WKContentRuleList {
    guard let store = WKContentRuleListStore.default() else {
      throw LabError(.io, "no content rule list store")
    }
    let json = try policy.contentRuleListJSON()
    let identifier = "rivqen-webview-lab-\(policy.server.port)"
    do {
      guard
        let list = try await store.compileContentRuleList(
          forIdentifier: identifier, encodedContentRuleList: json)
      else {
        throw LabError(.io, "content rule list: no list")
      }
      return list
    } catch let error as LabError {
      throw error
    } catch {
      throw LabError(.io, "content rule list: \(describe(error))")
    }
  }

  // MARK: Files and server

  private func documents() throws -> URL {
    guard let url = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first
    else {
      throw LabError(.io, "no Documents directory")
    }
    return url
  }

  /// A new, empty output directory, so a stale result or DONE file never survives a run.
  private func prepareOutput() throws -> URL {
    let output = try documents().appendingPathComponent("lab-output", isDirectory: true)
    let files = FileManager.default
    if files.fileExists(atPath: output.path) {
      try files.removeItem(at: output)
    }
    try files.createDirectory(at: output, withIntermediateDirectories: true)
    return output
  }

  private func fetchIndex(_ config: LabConfig) async throws -> [String] {
    guard let url = config.server.indexURL else { throw LabError(.argument, "no case list URL") }
    let sessionConfig = URLSessionConfiguration.ephemeral
    sessionConfig.timeoutIntervalForRequest = config.timeoutSeconds
    sessionConfig.timeoutIntervalForResource = config.timeoutSeconds
    let session = URLSession(configuration: sessionConfig)
    defer { session.invalidateAndCancel() }
    let (bytes, response) = try await session.bytes(from: url)
    guard (response as? HTTPURLResponse)?.statusCode == 200 else {
      throw LabError(.io, "GET /cases.json: status is not 200")
    }
    var data = Data()
    for try await byte in bytes {
      guard data.count < LabCase.maxIndexBytes else {
        throw LabError(.tooLarge, "GET /cases.json: more than \(LabCase.maxIndexBytes) bytes")
      }
      data.append(byte)
    }
    return try LabCase.parseIndex(data)
  }

  // MARK: Run log

  private func fillPlatform(_ log: inout AppRun) {
    let env = ProcessInfo.processInfo.environment
    log.simulatorDevice = env["SIMULATOR_DEVICE_NAME"]
    log.simulatorModel = env["SIMULATOR_MODEL_IDENTIFIER"]
    log.simulatorRuntime = env["SIMULATOR_RUNTIME_VERSION"]
    log.simulatorRuntimeBuild = env["SIMULATOR_RUNTIME_BUILD_VERSION"]
    let webKit = Bundle(for: WKWebView.self)
    log.webKitVersion = webKit.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String
    log.webKitBundleVersion = webKit.object(forInfoDictionaryKey: "CFBundleVersion") as? String
  }

  private func notes(config: LabConfig, serializerBytes: Int, offOk: Int, offTotal: Int) -> [String]
  {
    [
      "Walker: host/serialize.js (\(serializerBytes) bytes) evaluated with evaluateJavaScript(_:in:in:) in "
        + "WKContentWorld.defaultClient, \(config.settleSeconds) s after didFinish of the main frame.",
      "View off: allowsContentJavaScript = false in defaultWebpagePreferences and in the preferences of every "
        + "navigation policy decision. The app never turns content JavaScript on for an off result.",
      "View off: the walker ran in \(offOk) of \(offTotal) runs.",
      "Meta: characterSet, contentType and the probe text read with callAsyncJavaScript in "
        + "WKContentWorld.defaultClient.",
      "Requests: the navigation delegate allows only \(config.server.origin), about:blank and about:srcdoc; "
        + "a WKContentRuleList blocks every other load (WebKit does not report subresource blocks to the app).",
      "Isolation: a new WKWebView and a new non-persistent website data store per case and view; data "
        + "detectors off; no new windows.",
    ]
  }
}
