// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import Foundation
import SpikeCore

/// One event of a native fetch, in the order `URLSession` gives it.
enum FetchEvent: Sendable {
  case response(FetchHead)
  case data(Data)
  /// The end of the task: `nil` when it completed, else the error text.
  case finished(String?)
}

struct FetchHead: Sendable {
  let status: Int
  let url: URL?
  let fields: [String: String]
}

/// The task delegate of one fetch. It only forwards the callbacks into an `AsyncStream`, in order;
/// it holds no mutable state, so it is `Sendable`.
final class FetchDelegate: NSObject, URLSessionDataDelegate, Sendable {
  let continuation: AsyncStream<FetchEvent>.Continuation

  init(continuation: AsyncStream<FetchEvent>.Continuation) {
    self.continuation = continuation
  }

  func urlSession(
    _ session: URLSession, dataTask: URLSessionDataTask, didReceive response: URLResponse,
    completionHandler: @escaping @Sendable (URLSession.ResponseDisposition) -> Void
  ) {
    var fields: [String: String] = [:]
    let http = response as? HTTPURLResponse
    for (key, value) in http?.allHeaderFields ?? [:] {
      if let k = key as? String, let v = value as? String { fields[k] = v }
    }
    continuation.yield(
      .response(FetchHead(status: http?.statusCode ?? 0, url: response.url, fields: fields)))
    completionHandler(.allow)
  }

  func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive data: Data) {
    continuation.yield(.data(data))
  }

  /// S1 has no redirect case: the fetch does not follow one (fail closed, DL-042); the 3xx
  /// response itself goes to the caller.
  func urlSession(
    _ session: URLSession, task: URLSessionTask,
    willPerformHTTPRedirection response: HTTPURLResponse,
    newRequest request: URLRequest, completionHandler: @escaping @Sendable (URLRequest?) -> Void
  ) {
    completionHandler(nil)
  }

  func urlSession(
    _ session: URLSession, task: URLSessionTask, didCompleteWithError error: (any Error)?
  ) {
    continuation.yield(.finished(error.map(describe)))
    continuation.finish()
  }
}

/// Native fetches of a run. One ephemeral session per run: its cookies and cache live in memory
/// only, so every run starts empty. TLS is the system default (the lab CA is a root of the lab
/// simulator; no trust override).
@MainActor
final class Fetcher {
  /// The most bytes one fetch may give (the host allows at most 64 MiB; the lab uses less).
  static let maxBytes = 8 * 1024 * 1024

  let session: URLSession
  let timeout: Double

  init(timeout: Double) {
    let config = URLSessionConfiguration.ephemeral
    config.timeoutIntervalForRequest = timeout
    config.timeoutIntervalForResource = timeout * 3
    config.requestCachePolicy = .reloadIgnoringLocalAndRemoteCacheData
    session = URLSession(configuration: config)
    self.timeout = timeout
  }

  func start(_ url: URL) -> (task: URLSessionDataTask, events: AsyncStream<FetchEvent>) {
    let (stream, continuation) = AsyncStream<FetchEvent>.makeStream()
    var request = URLRequest(
      url: url, cachePolicy: .reloadIgnoringLocalAndRemoteCacheData, timeoutInterval: timeout)
    request.httpMethod = "GET"
    let task = session.dataTask(with: request)
    task.delegate = FetchDelegate(continuation: continuation)
    task.resume()
    return (task, stream)
  }

  func invalidate() {
    session.invalidateAndCancel()
  }
}

/// A whole native fetch with its log events (`app.fetch.start`, `head`, `end`).
struct FetchResult: Sendable {
  let head: FetchHead?
  let body: Data
  let complete: Bool
  let ms: Double
}

extension Fetcher {
  func fetchAll(_ url: URL, recorder: Recorder) async -> FetchResult {
    let started = ContinuousClock.now
    recorder.event("app.fetch.start", ["url": .string(url.absoluteString)])
    let (task, events) = start(url)
    var head: FetchHead?
    var body = Data()
    var complete = false
    var overflow = false
    for await event in events {
      switch event {
      case .response(let h):
        head = h
        recorder.event(
          "app.fetch.head",
          [
            "url": .string(url.absoluteString), "status": .int(h.status),
            "fields": .object(Fields.forLog(h.fields)),
          ])
      case .data(let d):
        if overflow { continue }
        guard body.count + d.count <= Self.maxBytes else {
          overflow = true
          task.cancel()
          recorder.error("FetchError: more than \(Self.maxBytes) bytes")
          continue
        }
        body.append(d)
      case .finished(let error):
        complete = error == nil && !overflow
        if let error, !overflow { recorder.error("FetchError: \(error)") }
      }
    }
    recorder.event(
      "app.fetch.end",
      ["url": .string(url.absoluteString), "bytes": .int(body.count), "complete": .bool(complete)])
    return FetchResult(
      head: head, body: body, complete: complete,
      ms: Recorder.msDouble(ContinuousClock.now - started))
  }
}
