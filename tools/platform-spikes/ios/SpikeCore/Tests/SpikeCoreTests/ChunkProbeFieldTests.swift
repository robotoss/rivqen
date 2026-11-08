// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import Foundation
import Testing

@testable import SpikeCore

@Suite struct ChunkMarkerTests {
  /// A document like the one of `/s/stream` (host/stream.mjs `buildChunk`).
  static func document(chunks: Int) -> [UInt8] {
    var s = "<!doctype html><html><head></head><body>"
    for k in 0..<chunks {
      s +=
        "<p data-k=\"\(k)\">xxxx</p><img src=\"/s/beacon?run=R&k=\(k)\" hidden><script>p(\(k))</script>"
    }
    return Array(s.utf8)
  }

  @Test func findsTheLastMarkerInOneRead() {
    var s = ChunkMarkerScanner()
    #expect(s.lastK == nil)
    #expect(s.feed(Self.document(chunks: 12)) == 11)
  }

  @Test func markersSplitAtEveryByteBoundary() {
    let doc = Self.document(chunks: 3)
    for cut in 0...doc.count {
      var s = ChunkMarkerScanner()
      s.feed(doc[..<cut])
      s.feed(doc[cut...])
      #expect(s.lastK == 2, "cut at \(cut)")
    }
  }

  @Test func reportsProgressPerRead() {
    var s = ChunkMarkerScanner()
    #expect(s.feed(Array("<p data-k=\"4".utf8)) == nil)
    #expect(s.feed(Array("2\">".utf8)) == 42)
    #expect(s.feed(Array("no marker".utf8)) == 42)
  }

  @Test func ignoresBrokenMarkers() {
    var s = ChunkMarkerScanner()
    #expect(s.feed(Array("data-k=\"\" data-k=\"7x\" ddata-k=\"5\"".utf8)) == 5)
    var t = ChunkMarkerScanner()
    #expect(t.feed(Array("data-k=\"12345678\"".utf8)) == nil)
    #expect(t.feed(Array("data-k=\"1234567\"".utf8)) == 1_234_567)
  }

  @Test func randomSplitsGiveTheSameResult() {
    // Property: any split of a document into reads gives the same last marker.
    var rng = SplitMix(seed: 7)
    let doc = Self.document(chunks: 40)
    for _ in 0..<200 {
      var s = ChunkMarkerScanner()
      var i = 0
      while i < doc.count {
        let n = Int(rng.next() % 50) + 1
        s.feed(doc[i..<min(i + n, doc.count)])
        i += n
      }
      #expect(s.lastK == 39)
    }
  }
}

@Suite struct ChunkSplitterTests {
  @Test func partsConcatenateToTheInput() {
    var rng = SplitMix(seed: 11)
    for _ in 0..<100 {
      let size = Int(rng.next() % 70_000)
      let max = Int(rng.next() % 20_000) + 1
      let data = Data((0..<size).map { UInt8(truncatingIfNeeded: $0 &* 31 &+ 7) })
      let parts = ChunkSplitter.parts(of: data, maxBytes: max)
      #expect(parts.reduce(Data(), +) == data)
      #expect(parts.allSatisfy { $0.count <= max && !$0.isEmpty })
      #expect(parts.count == (size + max - 1) / max)
    }
  }

  @Test func edges() {
    #expect(ChunkSplitter.parts(of: Data()).isEmpty)
    #expect(ChunkSplitter.parts(of: Data([1, 2, 3]), maxBytes: 0).map(\.count) == [1, 1, 1])
    #expect(ChunkSplitter.parts(of: Data([1, 2, 3]), maxBytes: -5).map(\.count) == [1, 1, 1])
    #expect(ChunkSplitter.parts(of: Data(count: 32 * 1024)).map(\.count) == [16_384, 16_384])
    #expect(
      ChunkSplitter.parts(of: Data(count: 3 << 20), maxBytes: Int.max).map(\.count) == [
        1 << 20, 1 << 20, 1 << 20,
      ])
    let slice = Data([9, 8, 7, 6, 5])[2...]
    #expect(ChunkSplitter.parts(of: slice, maxBytes: 2) == [Data([7, 6]), Data([5])])
  }
}

@Suite struct ProbeTests {
  @Test func parsesAMessage() throws {
    let p = try Probe.parseMessage(["name": "chunk", "value": NSNumber(value: 3)])
    #expect(p.name == "chunk")
    #expect(p.value == .int(3))
    let q = try Probe.parseMessage(["name": "cookie"])
    #expect(q.value == .null)
  }

  @Test(arguments: [
    "", "Chunk", "1chunk", "chunk name", "native.webview", String(repeating: "a", count: 65), "a/b",
    "é",
  ])
  func refusesNames(_ name: String) {
    #expect(throws: SpikeError.self) { try Probe.parseMessage(["name": name, "value": 1]) }
  }

  @Test func nameRule() {
    #expect(Probe.isName("metric.first_paint"))
    #expect(Probe.isName("csp.inline"))
    #expect(Probe.isName("a-b_c.d9"))
    #expect(Probe.isName(String(repeating: "a", count: 64)))
  }

  @Test func refusesBodies() {
    #expect(throws: SpikeError.self) { try Probe.parseMessage("chunk") }
    #expect(throws: SpikeError.self) { try Probe.parseMessage(nil) }
    #expect(throws: SpikeError.self) { try Probe.parseMessage(["name": 1, "value": 1]) }
    #expect(throws: SpikeError.self) { try Probe.parseMessage(["name": "a", "value": 1, "x": 2]) }
    #expect(throws: SpikeError.self) {
      try Probe.parseMessage(["name": "a", "value": String(repeating: "x", count: 20_000)])
    }
  }

  @Test func parsesAList() throws {
    let list = try Probe.parseList([
      ["location", "https://localhost:18443/"], ["metric.first_paint", 12.5],
    ])
    #expect(list.map(\.name) == ["location", "metric.first_paint"])
    #expect(list.map(\.value) == [.string("https://localhost:18443/"), .double(12.5)])
    #expect(try Probe.parseList([Any]()).isEmpty)
  }

  @Test func refusesLists() throws {
    #expect(throws: SpikeError.self) { try Probe.parseList(["location", "x"]) }
    #expect(throws: SpikeError.self) { try Probe.parseList([["location"]]) }
    #expect(throws: SpikeError.self) { try Probe.parseList([["native.step", "x"]]) }
    #expect(throws: SpikeError.self) { try Probe.parseList(["a": 1]) }
    let many = (0...Probe.maxPairs).map { _ in ["a", 1] as [Any] }
    #expect(throws: SpikeError(.tooLarge, "probe script returned too many pairs")) {
      try Probe.parseList(many)
    }
    #expect(try Probe.parseList(Array(many.dropLast())).count == Probe.maxPairs)
  }
}

@Suite struct FieldTests {
  @Test func webKitFieldsDropTransferFieldsAndForceHTML() {
    let fetched = [
      "Content-Type": "text/html; charset=iso-8859-1", "Content-Length": "10",
      "Transfer-Encoding": "chunked",
      "Connection": "keep-alive", "Keep-Alive": "timeout=5", "Content-Encoding": "gzip",
      "Set-Cookie": "a=b; Path=/",
      "Cache-Control": "no-store",
    ]
    #expect(
      Fields.forWebKit(fetched, html: true) == [
        "Content-Type": "text/html; charset=utf-8", "Set-Cookie": "a=b; Path=/",
        "Cache-Control": "no-store",
      ])
    #expect(
      Fields.forWebKit(["content-type": "image/gif", "content-length": "3"], html: false) == [
        "content-type": "image/gif"
      ])
  }

  @Test func logFieldsAreLowerCaseWithoutDateAndRedacted() {
    let log = Fields.forLog([
      "Date": "Sun, 08 Nov 2026 10:00:00 GMT", "Content-Type": "text/html",
      "Set-Cookie": "rq-a=v1; Path=/; HttpOnly",
      "Cookie": "a=b; c=D",
    ])
    #expect(
      log == [
        "content-type": .string("text/html"), "set-cookie": .string("rq-a=v1; Path=/; HttpOnly"),
        "cookie": .string("[redacted]"),
      ])
  }

  @Test func cookieRedaction() {
    #expect(Fields.loggable("cookie", "a=b; c=d-1") == "a=b; c=d-1")
    #expect(Fields.loggable("cookie", "a=B") == "[redacted]")
    #expect(Fields.loggable("cookie", "a") == "[redacted]")
    #expect(Fields.loggable("cookie", "a=") == "a=")
    #expect(Fields.loggable("set-cookie", "a=x y; Path=/") == "[redacted]")
    #expect(Fields.loggable("set-cookie", "a=xy; Path=/") == "a=xy; Path=/")
    #expect(Fields.loggable("set-cookie", "") == "[redacted]")
    #expect(Fields.loggable("x-other", "Secret Value") == "Secret Value")
  }
}

/// A small deterministic generator for the property tests (no dependency).
struct SplitMix {
  var state: UInt64
  init(seed: UInt64) { state = seed }
  mutating func next() -> UInt64 {
    state &+= 0x9E37_79B9_7F4A_7C15
    var z = state
    z = (z ^ (z >> 30)) &* 0xBF58_476D_1CE4_E5B9
    z = (z ^ (z >> 27)) &* 0x94D0_49BB_1331_11EB
    return z ^ (z >> 31)
  }
}
