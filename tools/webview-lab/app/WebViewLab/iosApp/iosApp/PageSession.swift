// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import Foundation
import LabCore
import WebKit

/// Waits for the first of a value and a timeout. Only the first `resume` counts, so a late
/// WebKit callback after a timeout does nothing. One instance serves one wait.
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
          try await Task.sleep(for: .seconds(timeout))
        } catch {
          return  // Cancelled: a value came first.
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

/// The value of one script evaluation, reduced to Sendable data.
enum ScriptOutcome: Sendable, Equatable {
  case string(String)
  case nonString(String)
  case failure(String)

  static func from(_ result: Result<Any, any Error>) -> ScriptOutcome {
    switch result {
    case .success(let value):
      if let text = value as? String { return .string(text) }
      return .nonString(String(describing: type(of: value)))
    case .failure(let error):
      return .failure(describe(error))
    }
  }

  /// `ok` for a string value, else the reason.
  var summary: String {
    switch self {
    case .string: "ok"
    case .nonString(let type): "returned a non-string value (\(type))"
    case .failure(let reason): reason
    }
  }
}

/// An error as text: domain, code, description and the JavaScript exception message, if any.
func describe(_ error: any Error) -> String {
  let ns = error as NSError
  var text = "\(ns.domain) \(ns.code): \(ns.localizedDescription)"
  if let message = ns.userInfo["WKJavaScriptExceptionMessage"] as? String {
    text += " (\(message))"
  }
  return text
}

@MainActor
func evaluate(_ source: String, in webView: WKWebView, world: WKContentWorld, timeout: Double) async
  -> ScriptOutcome
{
  let shot = OneShot<ScriptOutcome>()
  return await shot.wait(timeout: timeout, onTimeout: .failure("no answer in \(timeout) s")) {
    webView.evaluateJavaScript(source, in: nil, in: world) { result in
      shot.resume(ScriptOutcome.from(result))
    }
  }
}

@MainActor
func callAsync(
  _ body: String, arguments: [String: Any], in webView: WKWebView, world: WKContentWorld,
  timeout: Double
) async -> ScriptOutcome {
  let shot = OneShot<ScriptOutcome>()
  return await shot.wait(timeout: timeout, onTimeout: .failure("no answer in \(timeout) s")) {
    webView.callAsyncJavaScript(body, arguments: arguments, in: nil, in: world) { result in
      shot.resume(ScriptOutcome.from(result))
    }
  }
}

/// The navigation and UI delegate of one web view: applies the scripting view and the request
/// policy, and reports the end of the main-frame load.
@MainActor
final class PageSession: NSObject, WKNavigationDelegate, WKUIDelegate {
  enum LoadOutcome: Sendable, Equatable {
    /// `didFinish` of the main navigation; the HTTP status of the main-frame response, if any.
    case finished(status: Int?)
    case failed(String)
  }

  static let maxBlocked = 32

  private let policy: RequestPolicy
  private let allowsContentJavaScript: Bool
  private var mainNavigation: WKNavigation?
  private var mainStatus: Int?
  private var shot: OneShot<LoadOutcome>?
  private(set) var blocked: [String] = []

  init(policy: RequestPolicy, allowsContentJavaScript: Bool) {
    self.policy = policy
    self.allowsContentJavaScript = allowsContentJavaScript
  }

  /// Start a load with `start` and wait for its end, at most `timeout` seconds.
  func load(_ webView: WKWebView, timeout: Double, start: (WKWebView) -> WKNavigation?) async
    -> LoadOutcome
  {
    let shot = OneShot<LoadOutcome>()
    self.shot = shot
    defer { self.shot = nil }
    return await shot.wait(timeout: timeout, onTimeout: .failed("no didFinish in \(timeout) s")) {
      guard let navigation = start(webView) else {
        shot.resume(.failed("the load did not start"))
        return
      }
      mainNavigation = navigation
    }
  }

  // MARK: WKNavigationDelegate

  func webView(
    _ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction,
    preferences: WKWebpagePreferences,
    decisionHandler: @escaping @MainActor (WKNavigationActionPolicy, WKWebpagePreferences) -> Void
  ) {
    // The same value as the configuration: the view never changes during a run.
    preferences.allowsContentJavaScript = allowsContentJavaScript
    guard let url = navigationAction.request.url, policy.allows(url) else {
      if blocked.count < Self.maxBlocked {
        blocked.append(navigationAction.request.url?.absoluteString ?? "(no URL)")
      }
      decisionHandler(.cancel, preferences)
      return
    }
    decisionHandler(.allow, preferences)
  }

  func webView(
    _ webView: WKWebView, decidePolicyFor navigationResponse: WKNavigationResponse,
    decisionHandler: @escaping @MainActor (WKNavigationResponsePolicy) -> Void
  ) {
    if navigationResponse.isForMainFrame {
      mainStatus = (navigationResponse.response as? HTTPURLResponse)?.statusCode
    }
    decisionHandler(.allow)
  }

  func webView(_ webView: WKWebView, didFinish navigation: WKNavigation?) {
    guard let navigation, navigation === mainNavigation else { return }
    shot?.resume(.finished(status: mainStatus))
  }

  func webView(_ webView: WKWebView, didFail navigation: WKNavigation?, withError error: any Error)
  {
    guard let navigation, navigation === mainNavigation else { return }
    shot?.resume(.failed("didFail: \(describe(error))"))
  }

  func webView(
    _ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation?,
    withError error: any Error
  ) {
    guard let navigation, navigation === mainNavigation else { return }
    shot?.resume(.failed("didFailProvisionalNavigation: \(describe(error))"))
  }

  func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
    shot?.resume(.failed("the web content process terminated"))
  }

  // MARK: WKUIDelegate

  /// No new windows.
  func webView(
    _ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration,
    for navigationAction: WKNavigationAction, windowFeatures: WKWindowFeatures
  ) -> WKWebView? {
    nil
  }
}
