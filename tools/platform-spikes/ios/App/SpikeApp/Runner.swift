// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import Foundation
import SpikeCore
import UIKit
import WebKit

/// Runs one capability once (tools/platform-spikes/ios/README.md, "What each capability does").
///
/// Output in the data container, `Documents/spike-output/`: `app.jsonl` (the app part of the run
/// log), `metrics.json`, `app-run.json` (user agent, WebKit and simulator facts) and last `DONE`
/// with `ok` or the reason.
@MainActor
final class Runner {
  /// The longest time one run may take before the app gives up and writes `DONE`.
  static let runTimeout = 180.0

  private let model: SpikeModel
  private var recorder = Recorder(labPort: 0)
  private var fetcher = Fetcher(timeout: LaunchConfig.defaultTimeout)
  private var config: LaunchConfig?
  private var paths = LabPaths(run: "")
  private var userAgent: String?

  init(model: SpikeModel) {
    self.model = model
  }

  func runOnce() async {
    guard !model.started else { return }
    model.started = true
    let output: URL
    do {
      output = try prepareOutput()
    } catch {
      model.status = "error: \(describe(error))"
      return
    }
    let parsed: LaunchConfig?
    do {
      parsed = try LaunchConfig.parse(ProcessInfo.processInfo.arguments)
    } catch {
      write("error: \(error)", to: output.appendingPathComponent("DONE"))
      model.status = "error: \(error)"
      return
    }
    guard let config = parsed else {
      model.status = "idle: launch with -rqRun <id> -rqCap <capability> -rqOrigin <origin>"
      return
    }
    self.config = config
    recorder = Recorder(labPort: config.origin.port)
    fetcher = Fetcher(timeout: config.stepTimeout)
    paths = LabPaths(run: config.run)
    model.status = "\(config.run) running"

    var done = false
    let work = Task { @MainActor in
      await self.run(config.capability)
      done = true
    }
    if !(await until(Self.runTimeout) { done }) {
      work.cancel()
      recorder.error("RunTimeout: the run took more than \(Int(Self.runTimeout)) s")
    }
    fetcher.invalidate()
    userAgent = await WebSession.userAgent(container: model.container, timeout: 5)

    let outcome =
      recorder.errors == 0 && recorder.log.dropped == 0
      ? "ok" : "error: \(recorder.errors) app errors"
    write(recorder.log.text, to: output.appendingPathComponent("app.jsonl"))
    write(
      recorder.log.metricsJSON(run: config.run), to: output.appendingPathComponent("metrics.json"))
    write(appRun(config), to: output.appendingPathComponent("app-run.json"))
    write(outcome, to: output.appendingPathComponent("DONE"))
    model.status = "\(config.run) done: \(outcome)"
  }

  // MARK: Capabilities

  private func run(_ capability: Capability) async {
    switch capability {
    case .simulatedFirstRender: await simulatedFirstRender()
    case .simulatedOrigin: await simulatedOrigin()
    case .simulatedCookies: await simulatedCookies()
    case .navigationWithSnapshot: await navigationWithSnapshot()
    case .schemeStreaming: await schemeStreaming()
    case .schemeOrigin: await schemeOrigin()
    case .httpsStreaming: await httpsStreaming()
    }
  }

  /// CAP-I-01: the native fetch starts before the web view exists; `loadSimulatedRequest` gets the
  /// fetched bytes with `text/html; charset=utf-8`; first render metrics.
  private func simulatedFirstRender() async {
    guard let url = https(paths.stream(chunks: 1, size: 4096, gate: .delay, ms: 0)) else { return }
    let page = await fetcher.fetchAll(url, recorder: recorder)
    recorder.probe("metric.fetch", .double(page.ms))
    let web = makeSession()
    defer { close(web) }
    guard await loadSimulated(web, url: url, page: page) else { return }
    await web.probe(
      [
        ["location", "location", ""], ["origin", "origin", ""],
        ["content_type", "content_type", ""],
        ["charset", "charset", ""], ["dom_text", "title", "title"],
        ["paint", "metric.first_paint", ""],
      ], timeout: timeout)
  }

  /// CAP-I-02: location, origin and relative URLs after `loadSimulatedRequest` and after
  /// `loadHTMLString` with the page URL as base URL.
  private func simulatedOrigin() async {
    guard let url = https(paths.echo(tag: "page", html: true)) else { return }
    let page = await fetcher.fetchAll(url, recorder: recorder)

    recorder.native("step", .string("loadSimulatedRequest"))
    let simulated = makeSession()
    if await loadSimulated(simulated, url: url, page: page) {
      await simulated.probe(originSteps(tag: "simulated"), timeout: timeout)
    }
    close(simulated)

    recorder.native("step", .string("loadHTMLString"))
    let string = makeSession()
    defer { close(string) }
    // H-66 for loadHTMLString: decode the bytes as UTF-8 with Foundation (removes a BOM).
    guard let html = String(data: page.body, encoding: .utf8) else {
      recorder.error("DecodeError: the page is not UTF-8")
      return
    }
    if await string.load(timeout: timeout, { $0.loadHTMLString(html, baseURL: url) }) {
      await string.probe(originSteps(tag: "html-string"), timeout: timeout)
    }
  }

  private func originSteps(tag: String) -> [[String]] {
    [
      ["location", "location", ""], ["origin", "origin", ""], ["base", "base", ""],
      ["relative", "relative", "rel/x?y=1"], ["fetch_text", "echo", paths.echo(tag: tag)],
    ]
  }

  /// CAP-I-03: cookies after a cookie set by the native fetch (its `Set-Cookie` is in the simulated
  /// response) and by the web view (an `HttpOnly` cookie from a page fetch, a script cookie).
  private func simulatedCookies() async {
    guard let url = https(paths.cookieSet(name: "rq-native", value: "n1")) else { return }
    let page = await fetcher.fetchAll(url, recorder: recorder)
    let web = makeSession()
    defer { close(web) }
    guard await loadSimulated(web, url: url, page: page) else { return }
    await web.probe(
      [
        ["cookie", "cookie", ""],
        [
          "fetch_status", "webview.cookie_set",
          paths.cookieSet(name: "rq-web", value: "w1", attrs: ["HttpOnly"]),
        ],
        ["set_js_cookie", "cookie", "rq-js=j1; path=/"],
        ["fetch_text", "echo", paths.echo(tag: "webview")],
      ], timeout: timeout)
    await webKitCookies(web)
    await nativeEcho(tag: "native")
  }

  /// CAP-I-04 (IOS-P2): a cached snapshot first (`loadSimulatedRequest` of bytes the app holds),
  /// then the real navigation `load(URLRequest)`; the snapshot stays on screen until the commit.
  /// After the real page loads, the bridge in its own content world asks for a data update, the
  /// app fetches it natively and replies, the bridge writes it into the DOM.
  private func navigationWithSnapshot() async {
    guard let url = https(paths.echo(tag: "page", html: true)) else { return }
    // The snapshot: in the SDK it comes from the cache of an earlier visit; here the app fetches
    // it at the start of the run (the same URL, so the content is a real earlier copy).
    let snapshot = await fetcher.fetchAll(url, recorder: recorder)
    let web = makeSession(bridge: true)
    defer { close(web) }

    recorder.native("step", .string("snapshot"))
    web.bridgeLabel = "snapshot"
    guard await loadSimulated(web, url: url, page: snapshot) else { return }
    _ = await until(timeout) { web.bridgeReady >= 1 }
    await web.probe(
      [["location", "location", ""], ["dom_text", "echo", "#echo"]], timeout: timeout)

    recorder.native("step", .string("live"))
    web.bridgeLabel = "live"
    let readyBefore = web.bridgeReady
    // The update starts after the live didFinish, so the log order does not depend on whether the
    // bridge message or the navigation callback comes first.
    let finishedBefore = web.finished
    web.bridgeUpdate = { [weak self, weak web] in
      guard let self, let web else { return nil }
      _ = await until(self.timeout) { web.finished > finishedBefore }
      return await self.dataUpdate()
    }
    let started = ContinuousClock.now
    guard
      await web.load(
        timeout: timeout,
        {
          $0.load(
            URLRequest(
              url: url, cachePolicy: .reloadIgnoringLocalAndRemoteCacheData,
              timeoutInterval: self.timeout))
        })
    else { return }
    recorder.probe("metric.live_finish", .double(Recorder.msDouble(ContinuousClock.now - started)))
    guard await until(timeout, { web.bridgeReady > readyBefore && web.bridgeApplied >= 1 }) else {
      recorder.error("BridgeTimeout: no data update applied")
      return
    }
    await web.probe(
      [
        ["location", "location", ""], ["origin", "origin", ""], ["dom_text", "echo", "#echo"],
        ["dom_text", "data", "#rq-data"],
        ["typeof", "bridge.page_world", "webkit.messageHandlers.rqBridge"],
        ["typeof", "rqlab.page_world", "rqLab.probe"],
      ], timeout: timeout)
  }

  /// The data update of IOS-P2: a native fetch of `/s/echo` (JSON).
  private func dataUpdate() async -> String? {
    guard let url = https(paths.echo(tag: "data")) else { return nil }
    let data = await fetcher.fetchAll(url, recorder: recorder)
    return data.complete ? String(data: data.body, encoding: .utf8) : nil
  }

  /// CAP-I-05 (IOS-P3): a `rqlab` page streamed by the scheme handler; `gate=beacon` holds chunk
  /// k+1 until the beacon of chunk k arrives, so no `host.gate_timeout` means WebKit parsed and
  /// loaded chunk k before the document ended.
  private func schemeStreaming() async {
    guard let url = scheme(paths.stream(chunks: 4, size: 2048, gate: .beacon, ms: 3000)) else {
      return
    }
    let handler = LabSchemeHandler(recorder: recorder, fetcher: fetcher)
    let web = makeSession(handler: handler)
    defer { close(web) }
    guard await web.load(timeout: timeout, { $0.load(URLRequest(url: url)) }) else { return }
    await web.probe(
      [
        ["location", "location", ""], ["origin", "origin", ""],
        ["paint", "metric.first_paint", ""],
      ], timeout: timeout)
  }

  /// CAP-I-06 (IOS-P3): origin and cookies of a `rqlab` page, and its https subresources. First a
  /// real navigation to site A sets a cookie in the web view (`SameSite=None; Secure`), so the
  /// subresource requests show whether that cookie goes with them.
  private func schemeOrigin() async {
    guard
      let site = https(
        paths.cookieSet(name: "rq-site", value: "a1", attrs: ["Secure", "SameSite=None"])),
      let page = scheme(paths.cookieSet(name: "rq-scheme", value: "s1"))
    else { return }
    let handler = LabSchemeHandler(recorder: recorder, fetcher: fetcher)
    let web = makeSession(handler: handler)
    defer { close(web) }

    recorder.native("step", .string("site"))
    guard await web.load(timeout: timeout, { $0.load(URLRequest(url: site)) }) else { return }

    recorder.native("step", .string("scheme"))
    guard await web.load(timeout: timeout, { $0.load(URLRequest(url: page)) }) else { return }
    let origin = config?.origin.https ?? ""
    await web.probe(
      [
        ["location", "location", ""], ["origin", "origin", ""], ["cookie", "cookie", ""],
        ["set_js_cookie", "cookie", "rq-js=j1; path=/"],
        ["fetch_text", "echo", paths.echo(tag: "relative")],
        ["fetch_text", "https.cors", origin + paths.echo(tag: "cors")],
        ["fetch_no_cors", "https.no_cors", origin + paths.echo(tag: "no-cors")],
        ["image", "https.image", origin + paths.echo(tag: "image")],
      ], timeout: timeout)
    await webKitCookies(web)
  }

  /// CAP-I-07: https main-document streaming with public API. `handlesURLScheme` shows that WebKit
  /// owns `https` (so `setURLSchemeHandler` for it raises). Then the same `gate=beacon` page through
  /// each prototype: IOS-P1 (the app must have the whole body first), IOS-P2 (WebKit streams, the
  /// app sees no bytes), IOS-P3 (the app streams, but the page is on `rqlab`).
  private func httpsStreaming() async {
    recorder.native("handles_url_scheme.https", .bool(WKWebView.handlesURLScheme("https")))
    recorder.native("handles_url_scheme.rqlab", .bool(WKWebView.handlesURLScheme(LabScheme.name)))
    let stream = paths.stream(chunks: 3, size: 1024, gate: .beacon, ms: 1000)

    recorder.native("step", .string("IOS-P1"))
    guard let p1 = https(stream + "&p=1") else { return }
    let page = await fetcher.fetchAll(p1, recorder: recorder)
    // The beacons of the simulated page carry no information (the body is complete) and would
    // reach the host in parallel; a content rule list blocks them to keep the host order fixed.
    let rules = await beaconBlockList()
    let simulated = makeSession(rules: rules)
    if await loadSimulated(simulated, url: p1, page: page) {
      await simulated.probe([["location", "location", ""]], timeout: timeout)
    }
    close(simulated)

    recorder.native("step", .string("IOS-P2"))
    guard let p2 = https(stream + "&p=2") else { return }
    let live = makeSession()
    _ = await live.load(timeout: timeout, { $0.load(URLRequest(url: p2)) })
    close(live)

    recorder.native("step", .string("IOS-P3"))
    guard let p3 = scheme(stream + "&p=3") else { return }
    let handler = LabSchemeHandler(recorder: recorder, fetcher: fetcher)
    let custom = makeSession(handler: handler)
    if await custom.load(timeout: timeout, { $0.load(URLRequest(url: p3)) }) {
      await custom.probe([["origin", "origin", ""]], timeout: timeout)
    }
    close(custom)
  }

  // MARK: Steps

  private var timeout: Double { config?.stepTimeout ?? LaunchConfig.defaultTimeout }

  private func https(_ path: String) -> URL? {
    url((config?.origin.https ?? "") + path)
  }

  private func scheme(_ path: String) -> URL? {
    url((config?.origin.scheme ?? "") + path)
  }

  private func url(_ text: String) -> URL? {
    guard let u = URL(string: text) else {
      recorder.error("URLError: cannot build a lab URL")
      return nil
    }
    return u
  }

  private func makeSession(
    handler: LabSchemeHandler? = nil, bridge: Bool = false, rules: WKContentRuleList? = nil
  )
    -> WebSession
  {
    let session = WebSession(
      recorder: recorder, container: model.container,
      options: .init(schemeHandler: handler, bridge: bridge, ruleList: rules))
    recorder.native("webview", .string("created"))
    recorder.lastChunkProbe = nil
    return session
  }

  private func close(_ session: WebSession) {
    session.close()
  }

  /// IOS-P1: `loadSimulatedRequest` with the fetched status and fields (transfer fields removed,
  /// `Content-Type: text/html; charset=utf-8`, H-66) and the fetched bytes.
  private func loadSimulated(_ web: WebSession, url: URL, page: FetchResult) async -> Bool {
    guard page.complete, let head = page.head else {
      recorder.error("FetchError: no complete page to load")
      return false
    }
    let fields = Fields.forWebKit(head.fields, html: true)
    guard
      let response = HTTPURLResponse(
        url: url, statusCode: head.status, httpVersion: "HTTP/1.1", headerFields: fields)
    else {
      recorder.error("ResponseError: no HTTPURLResponse")
      return false
    }
    recorder.native(
      "simulated_response",
      .object([
        "mime": .string(response.mimeType ?? ""),
        "encoding": .string(response.textEncodingName ?? ""),
      ]))
    let request = URLRequest(url: url)
    return await web.load(timeout: timeout) {
      $0.loadSimulatedRequest(request, response: response, responseData: page.body)
    }
  }

  /// The cookie names in the `WKHTTPCookieStore` of the web view (names only, sorted).
  private func webKitCookies(_ web: WebSession) async {
    let store = web.webView.configuration.websiteDataStore.httpCookieStore
    let gate = OneShot<[String]>()
    let names = await gate.wait(timeout: timeout, onTimeout: ["timeout"]) {
      store.getAllCookies { cookies in
        gate.resume(
          cookies.map { "\($0.name)@\($0.domain)\($0.isHTTPOnly ? ";HttpOnly" : "")" }.sorted())
      }
    }
    recorder.native("wk_cookies", .array(names.map { .string($0) }))
  }

  /// A native fetch of `/s/echo` with the run session: the cookies the native side sends.
  private func nativeEcho(tag: String) async {
    guard let url = https(paths.echo(tag: tag)) else { return }
    let echo = await fetcher.fetchAll(url, recorder: recorder)
    let text = String(data: echo.body, encoding: .utf8) ?? ""
    recorder.native("echo", .string(String(text.prefix(4096))))
  }

  private func beaconBlockList() async -> WKContentRuleList? {
    guard let store = WKContentRuleListStore.default() else {
      recorder.error("RuleListError: no store")
      return nil
    }
    let json = #"[{"trigger":{"url-filter":"/s/beacon"},"action":{"type":"block"}}]"#
    do {
      return try await store.compileContentRuleList(
        forIdentifier: "rivqen-spike-beacon-block", encodedContentRuleList: json)
    } catch {
      recorder.error("RuleListError: \(describe(error))")
      return nil
    }
  }

  // MARK: Files

  private func prepareOutput() throws -> URL {
    guard let docs = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first
    else {
      throw CocoaError(.fileNoSuchFile)
    }
    let output = docs.appendingPathComponent("spike-output", isDirectory: true)
    if FileManager.default.fileExists(atPath: output.path) {
      try FileManager.default.removeItem(at: output)
    }
    try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
    return output
  }

  private func write(_ text: String, to url: URL) {
    do {
      try Data(text.utf8).write(to: url, options: .atomic)
    } catch {
      model.status = "cannot write \(url.lastPathComponent): \(describe(error))"
    }
  }

  private func appRun(_ config: LaunchConfig) -> String {
    let env = ProcessInfo.processInfo.environment
    let webKit = Bundle(for: WKWebView.self)
    func text(_ v: String?) -> JSONValue { v.map { .string($0) } ?? .null }
    let run: [String: JSONValue] = [
      "run": .string(config.run),
      "capability": .string(config.capability.rawValue),
      "prototype": .string(config.capability.prototype),
      "os_name": .string(UIDevice.current.systemName),
      "os_version": .string(UIDevice.current.systemVersion),
      "simulator_model": text(env["SIMULATOR_MODEL_IDENTIFIER"]),
      "simulator_runtime": text(env["SIMULATOR_RUNTIME_VERSION"]),
      "simulator_runtime_build": text(env["SIMULATOR_RUNTIME_BUILD_VERSION"]),
      "webkit_version": text(
        webKit.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String),
      "webkit_build": text(webKit.object(forInfoDictionaryKey: "CFBundleVersion") as? String),
      "user_agent": text(userAgent),
      "app_errors": .int(recorder.errors),
      "events_dropped": .int(recorder.log.dropped),
    ]
    return JSONValue.object(run).canonical + "\n"
  }
}
