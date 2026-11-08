// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import SpikeCore
import UIKit
import WebKit

/// One `WKWebView` of a run with its navigation delegate, the probe handler (`rqLab`, page world)
/// and the bridge (`rqBridge`, own content world). Every callback runs on the main actor and
/// writes the run log in arrival order.
@MainActor
final class WebSession: NSObject, WKNavigationDelegate, WKScriptMessageHandler,
  WKScriptMessageHandlerWithReply
{
  static let bridgeWorld = WKContentWorld.world(name: "rivqen-bridge")

  struct Options {
    var schemeHandler: LabSchemeHandler?
    var bridge = false
    var ruleList: WKContentRuleList?
  }

  let webView: WKWebView
  private let recorder: Recorder
  private(set) var finished = 0
  private(set) var failed = 0
  private(set) var bridgeApplied = 0
  private(set) var bridgeReady = 0
  private var loadStarted: ContinuousClock.Instant?

  /// The bridge answer for the next `ready` message: `nil` answers `null` (no update).
  var bridgeUpdate: (@MainActor () async -> String?)?
  /// The label of the page that the bridge serves (`snapshot`, `live`), for the log.
  var bridgeLabel = ""

  init(recorder: Recorder, container: UIView, options: Options) {
    self.recorder = recorder
    let configuration = WKWebViewConfiguration()
    // A new data store per web view in memory: every run starts without cookies or cache.
    configuration.websiteDataStore = .nonPersistent()
    configuration.dataDetectorTypes = []
    configuration.preferences.javaScriptCanOpenWindowsAutomatically = false
    if let handler = options.schemeHandler {
      configuration.setURLSchemeHandler(handler, forURLScheme: LabScheme.name)
    }
    if let list = options.ruleList {
      configuration.userContentController.add(list)
    }
    let frame =
      container.bounds.isEmpty ? CGRect(x: 0, y: 0, width: 390, height: 600) : container.bounds
    webView = WKWebView(frame: frame, configuration: configuration)
    super.init()
    let controller = configuration.userContentController
    controller.addUserScript(
      WKUserScript(
        source: Scripts.rqLab, injectionTime: .atDocumentStart, forMainFrameOnly: true, in: .page))
    controller.add(self, contentWorld: .page, name: "rqLab")
    if options.bridge {
      controller.addUserScript(
        WKUserScript(
          source: Scripts.bridge, injectionTime: .atDocumentEnd, forMainFrameOnly: true,
          in: Self.bridgeWorld))
      controller.addScriptMessageHandler(self, contentWorld: Self.bridgeWorld, name: "rqBridge")
    }
    webView.navigationDelegate = self
    container.addSubview(webView)
  }

  /// Removes the web view and breaks the retain cycle of the message handlers.
  func close() {
    webView.stopLoading()
    webView.configuration.userContentController.removeAllScriptMessageHandlers()
    webView.navigationDelegate = nil
    webView.removeFromSuperview()
  }

  /// Starts a load and waits for `didFinish` or a failure. Returns `true` on `didFinish`.
  func load(timeout: Double, _ start: (WKWebView) -> Void) async -> Bool {
    let done = finished
    let bad = failed
    loadStarted = ContinuousClock.now
    start(webView)
    let ended = await until(timeout) { self.finished > done || self.failed > bad }
    if !ended { recorder.error("NavigationTimeout: no didFinish after \(Int(timeout)) s") }
    return finished > done
  }

  /// Runs the probe steps in the page world and logs each result as `app.probe`.
  func probe(_ steps: [[String]], timeout: Double) async {
    let result = await callAsync(Scripts.probeSteps, arguments: ["steps": steps], timeout: timeout)
    switch result {
    case .success(let pairs):
      for (name, value) in pairs { recorder.probe(name, value) }
    case .failure(let error):
      recorder.error("ProbeScriptError: \(error)")
    }
  }

  /// `callAsyncJavaScript` in the page world, reduced to Sendable probe pairs, with a timeout.
  func callAsync(_ source: String, arguments: [String: Any], timeout: Double) async -> Result<
    [(name: String, value: JSONValue)], SpikeError
  > {
    let gate = OneShot<Result<[(name: String, value: JSONValue)], SpikeError>>()
    return await gate.wait(
      timeout: timeout, onTimeout: .failure(SpikeError(.probe, "script timeout"))
    ) {
      webView.callAsyncJavaScript(source, arguments: arguments, in: nil, in: .page) { result in
        switch result {
        case .success(let value):
          gate.resume(Result(catching: { () throws(SpikeError) in try Probe.parseList(value) }))
        case .failure(let error):
          gate.resume(.failure(SpikeError(.probe, describe(error))))
        }
      }
    }
  }

  /// `navigator.userAgent`, read in the app world of a separate web view after the run, so the
  /// read never overlaps a measured load and writes no run event.
  static func userAgent(container: UIView, timeout: Double) async -> String? {
    let configuration = WKWebViewConfiguration()
    configuration.websiteDataStore = .nonPersistent()
    let webView = WKWebView(
      frame: CGRect(x: 0, y: 0, width: 10, height: 10), configuration: configuration)
    container.addSubview(webView)
    defer { webView.removeFromSuperview() }
    let gate = OneShot<String?>()
    return await gate.wait(timeout: timeout, onTimeout: nil) {
      webView.evaluateJavaScript("navigator.userAgent", in: nil, in: .defaultClient) { result in
        if case .success(let value) = result {
          gate.resume(value as? String)
        } else {
          gate.resume(nil)
        }
      }
    }
  }

  // MARK: WKNavigationDelegate

  func webView(
    _ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction,
    decisionHandler: @escaping @MainActor (WKNavigationActionPolicy) -> Void
  ) {
    // Main-frame navigations go only to the lab origins; subframes follow the same rule.
    guard let url = navigationAction.request.url else { return decisionHandler(.cancel) }
    let scheme = url.scheme ?? ""
    if scheme == "about" { return decisionHandler(.allow) }
    let port = url.port ?? 0
    if ProbeOriginRule.allows(
      scheme: scheme, host: url.host ?? "", port: port, labPort: recorder.labPort)
    {
      return decisionHandler(.allow)
    }
    recorder.error("NavigationBlocked: a navigation outside the lab origins")
    decisionHandler(.cancel)
  }

  func webView(_ webView: WKWebView, didStartProvisionalNavigation navigation: WKNavigation?) {
    nav("start", webView.url)
  }

  func webView(
    _ webView: WKWebView, didReceiveServerRedirectForProvisionalNavigation navigation: WKNavigation?
  ) {
    nav("redirect", webView.url)
  }

  func webView(_ webView: WKWebView, didCommit navigation: WKNavigation?) {
    nav("commit", webView.url)
    if let started = loadStarted {
      recorder.probe("metric.commit", .double(Recorder.msDouble(ContinuousClock.now - started)))
    }
  }

  func webView(_ webView: WKWebView, didFinish navigation: WKNavigation?) {
    nav("finish", webView.url)
    finished += 1
  }

  func webView(_ webView: WKWebView, didFail navigation: WKNavigation?, withError error: any Error)
  {
    nav("fail", webView.url, error: error)
    failed += 1
  }

  func webView(
    _ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation?,
    withError error: any Error
  ) {
    nav("fail", webView.url, error: error)
    failed += 1
  }

  func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
    recorder.error("WebContentProcessTerminated")
    failed += 1
  }

  private func nav(_ kind: String, _ url: URL?, error: (any Error)? = nil) {
    var members: [String: JSONValue] = [
      "kind": .string(kind), "url": url.map { .string($0.absoluteString) } ?? .null,
    ]
    members["error"] = error.map { .string(describe($0)) } ?? .null
    recorder.event("app.nav", members)
  }

  // MARK: Messages

  /// `true` when the frame that posted has a lab origin.
  private func fromLab(_ message: WKScriptMessage) -> Bool {
    let origin = message.frameInfo.securityOrigin
    return ProbeOriginRule.allows(
      scheme: origin.protocol, host: origin.host, port: origin.port, labPort: recorder.labPort)
  }

  func userContentController(
    _ controller: WKUserContentController, didReceive message: WKScriptMessage
  ) {
    guard message.name == "rqLab", message.world == .page else { return }
    guard fromLab(message) else {
      let o = message.frameInfo.securityOrigin
      recorder.error(
        "ProbeRefused: frame origin \(o.protocol)://\(o.host):\(o.port) is not a lab origin")
      return
    }
    do {
      let probe = try Probe.parseMessage(message.body)
      recorder.probe(probe.name, probe.value)
    } catch {
      recorder.error("ProbeRefused: \(error)")
    }
  }

  func userContentController(
    _ controller: WKUserContentController, didReceive message: WKScriptMessage,
    replyHandler: @escaping @MainActor @Sendable (Any?, String?) -> Void
  ) {
    guard message.name == "rqBridge", message.world == Self.bridgeWorld, fromLab(message) else {
      recorder.error("BridgeRefused: wrong world or origin")
      return replyHandler(nil, "refused")
    }
    let body = message.body as? [String: Any]
    switch body?["op"] as? String {
    case "ready":
      bridgeReady += 1
      recorder.probe("bridge.ready", .string(bridgeLabel))
      guard let update = bridgeUpdate else { return replyHandler(NSNull(), nil) }
      bridgeUpdate = nil
      Task { @MainActor in
        let text = await update()
        replyHandler(text ?? NSNull(), nil)
      }
    case "applied":
      bridgeApplied += 1
      let value = (try? JSONValue.from(foundation: body?["text"])) ?? .string("[too large]")
      recorder.probe("bridge.applied", value)
      replyHandler(NSNull(), nil)
    default:
      recorder.error("BridgeRefused: unknown op")
      replyHandler(nil, "unknown op")
    }
  }
}

/// Waits for the first of a value and a timeout. Only the first `resume` counts, so a late WebKit
/// callback after a timeout does nothing. One instance serves one wait.
@MainActor
final class OneShot<Value: Sendable> {
  private var continuation: CheckedContinuation<Value, Never>?
  private var timer: Task<Void, Never>?
  private var used = false

  func wait(timeout: Double, onTimeout: Value, start: () -> Void) async -> Value {
    guard !used else { return onTimeout }
    used = true
    return await withCheckedContinuation { continuation in
      self.continuation = continuation
      timer = Task { [weak self] in
        do {
          try await Task.sleep(for: .milliseconds(Int(timeout * 1000)))
        } catch {
          return
        }
        self?.resume(onTimeout)
      }
      start()
    }
  }

  func resume(_ value: Value) {
    guard let continuation else { return }
    self.continuation = nil
    timer?.cancel()
    timer = nil
    continuation.resume(returning: value)
  }
}
