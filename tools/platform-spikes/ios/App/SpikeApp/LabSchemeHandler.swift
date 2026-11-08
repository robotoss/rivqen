// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import SpikeCore
import WebKit

/// IOS-P3: the handler of the own scheme `rqlab`. It maps a request to the https URL of the spike
/// host (`LabScheme`), fetches it with the native session and passes each read to WebKit as it
/// arrives: `didReceive(_:)` for the response, `didReceive(_:)` for every part of every read
/// (`ChunkSplitter`), then `didFinish()` or `didFailWithError(_:)`.
///
/// Relative URLs of a `rqlab` page (the beacons `/s/beacon?...` of `/s/stream`) resolve against
/// the `rqlab` base, so they come back here and are forwarded the same way.
///
/// WebKit forbids any call on a task after `webView(_:stop:)`; the handler cancels the fetch and
/// never touches a stopped task again.
@MainActor
final class LabSchemeHandler: NSObject, WKURLSchemeHandler {
  /// The longest wait of a beacon for its chunk probe (see `forward`).
  static let beaconWait = 2.0

  private let recorder: Recorder
  private let fetcher: Fetcher
  private var tasks: [ObjectIdentifier: Task<Void, Never>] = [:]
  private var stopped: Set<ObjectIdentifier> = []

  init(recorder: Recorder, fetcher: Fetcher) {
    self.recorder = recorder
    self.fetcher = fetcher
  }

  func webView(_ webView: WKWebView, start urlSchemeTask: any WKURLSchemeTask) {
    let id = ObjectIdentifier(urlSchemeTask)
    let request = urlSchemeTask.request
    let method = request.httpMethod ?? "GET"
    guard let url = request.url else {
      urlSchemeTask.didFailWithError(URLError(.badURL))
      return
    }
    let mainFrame = request.mainDocumentURL == url
    let target: URL
    switch LabScheme.httpsURL(for: url, port: recorder.labPort) {
    case .success(let https) where method == "GET":
      target = https
    case .success, .failure:
      intercept(url, method: method, mainFrame: mainFrame, decision: "block", head: nil)
      urlSchemeTask.didFailWithError(URLError(.unsupportedURL))
      return
    }
    tasks[id] = Task { @MainActor in
      await self.forward(
        urlSchemeTask, id: id, from: url, to: target, method: method, mainFrame: mainFrame)
      self.tasks[id] = nil
      self.stopped.remove(id)
    }
  }

  func webView(_ webView: WKWebView, stop urlSchemeTask: any WKURLSchemeTask) {
    let id = ObjectIdentifier(urlSchemeTask)
    stopped.insert(id)
    tasks[id]?.cancel()
  }

  private func forward(
    _ schemeTask: any WKURLSchemeTask, id: ObjectIdentifier, from url: URL, to target: URL,
    method: String,
    mainFrame: Bool
  ) async {
    // Lab ordering rule: the beacon of chunk k goes to the host only after the `chunk` probe k
    // arrived (or after `beaconWait` seconds). The beacon and the probe come from the same chunk
    // and reach the app in either order; with this rule the log order is fixed, and a run with all
    // beacons on time shows that the script of chunk k ran before the host sent chunk k+1.
    if let k = LabPaths.beaconIndex(path: url.path, query: url.query) {
      _ = await until(Self.beaconWait) { (self.recorder.lastChunkProbe ?? -1) >= k }
      if stopped.contains(id) || Task.isCancelled { return }
    }
    let (fetch, events) = fetcher.start(target)
    var scanner = ChunkMarkerScanner()
    var total = 0
    var answered = false
    for await event in events {
      if stopped.contains(id) || Task.isCancelled {
        fetch.cancel()
        return
      }
      switch event {
      case .response(let head):
        intercept(url, method: method, mainFrame: mainFrame, decision: "stream", head: head)
        // H-66: a main document goes to WebKit as text/html; utf-8. Subresources keep their type.
        let fields = Fields.forWebKit(head.fields, html: mainFrame)
        guard
          let response = HTTPURLResponse(
            url: url, statusCode: head.status, httpVersion: "HTTP/1.1", headerFields: fields)
        else {
          fetch.cancel()
          recorder.error("SchemeError: no HTTPURLResponse")
          schemeTask.didFailWithError(URLError(.cannotParseResponse))
          return
        }
        schemeTask.didReceive(response)
        answered = true
      case .data(let data):
        total += data.count
        guard answered, total <= Fetcher.maxBytes else {
          fetch.cancel()
          recorder.error(
            "SchemeError: data before the response or more than \(Fetcher.maxBytes) bytes")
          schemeTask.didFailWithError(URLError(.dataLengthExceedsMaximum))
          return
        }
        let k = scanner.feed(data)
        if mainFrame {
          recorder.event(
            "app.stream.read", ["bytes": .int(data.count), "k": k.map { .int($0) } ?? .null])
        }
        for part in ChunkSplitter.parts(of: data) { schemeTask.didReceive(part) }
      case .finished(let error):
        if let error {
          recorder.error("SchemeFetchError: \(error)")
          schemeTask.didFailWithError(URLError(.networkConnectionLost))
        } else if !answered {
          recorder.error("SchemeError: no response")
          schemeTask.didFailWithError(URLError(.badServerResponse))
        } else {
          schemeTask.didFinish()
        }
        return
      }
    }
    // The stream ended without `finished`: the run task was cancelled.
    fetch.cancel()
  }

  private func intercept(
    _ url: URL, method: String, mainFrame: Bool, decision: String, head: FetchHead?
  ) {
    recorder.event(
      "app.intercept",
      [
        "url": .string(url.absoluteString), "method": .string(method),
        "main_frame": .bool(mainFrame),
        "decision": .string(decision), "status": head.map { .int($0.status) } ?? .null,
        "fields": .object(head.map { Fields.forLog($0.fields) } ?? [:]),
      ])
  }
}
