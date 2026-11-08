// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import Foundation
import Testing

@testable import SpikeCore

@Suite struct JSONValueTests {
  @Test func canonicalSortsKeysAtEveryLevel() {
    let v = JSONValue.object([
      "b": .int(1), "a": .object(["z": .null, "y": .bool(true)]),
      "B": .array([.string("x"), .bool(false)]),
    ])
    #expect(v.canonical == #"{"B":["x",false],"a":{"y":true,"z":null},"b":1}"#)
  }

  @Test func keysSortByUTF16LikeJavaScript() {
    // U+FF21 (one UTF-16 unit) sorts before U+1F600 (a surrogate pair 0xD83D...) in JavaScript,
    // though its scalar value is lower too; U+E000 sorts after the surrogate pair.
    let v = JSONValue.object(["\u{E000}": .int(1), "\u{1F600}": .int(2), "a": .int(3)])
    #expect(
      JSONValue.sortedKeys(["\u{E000}": .null, "\u{1F600}": .null, "a": .null]) == [
        "a", "\u{1F600}", "\u{E000}",
      ])
    #expect(v.canonical == "{\"a\":3,\"\u{1F600}\":2,\"\u{E000}\":1}")
  }

  @Test func stringEscapesMatchJSONStringify() {
    let s = "q\"b\\s/\u{08}\u{0C}\n\r\t\u{01}\u{1F}é\u{2028}"
    #expect(
      JSONValue.string(s).canonical == "\"q\\\"b\\\\s/\\b\\f\\n\\r\\t\\u0001\\u001fé\u{2028}\"")
  }

  @Test func numbersMatchJSONStringify() {
    #expect(JSONValue.double(12.0).canonical == "12")
    #expect(JSONValue.double(-3.0).canonical == "-3")
    #expect(JSONValue.double(12.5).canonical == "12.5")
    #expect(JSONValue.double(0.1).canonical == "0.1")
    #expect(JSONValue.double(.nan).canonical == "null")
    #expect(JSONValue.double(.infinity).canonical == "null")
    #expect(JSONValue.double(1e300).canonical == String(1e300))
    #expect(JSONValue.int(-7).canonical == "-7")
  }

  @Test func fromFoundationKeepsTypes() throws {
    let body: [String: Any] = [
      "s": "x", "i": NSNumber(value: 3), "d": NSNumber(value: 2.5), "t": NSNumber(value: true),
      "f": kCFBooleanFalse as Any, "n": NSNull(), "a": [NSNumber(value: 1), "y"],
      "whole": NSNumber(value: 4.0),
    ]
    let v = try JSONValue.from(foundation: body)
    #expect(
      v
        == .object([
          "s": .string("x"), "i": .int(3), "d": .double(2.5), "t": .bool(true), "f": .bool(false),
          "n": .null,
          "a": .array([.int(1), .string("y")]), "whole": .int(4),
        ]))
    #expect(try JSONValue.from(foundation: nil) == .null)
    #expect(try JSONValue.from(foundation: Date(timeIntervalSince1970: 2)) == .int(2000))
  }

  @Test func fromFoundationRefusesUnknownTypes() {
    #expect(throws: SpikeError.self) { try JSONValue.from(foundation: Data([1])) }
  }

  @Test func fromFoundationBoundsDepthItemsAndText() throws {
    var limits = JSONValue.Limits()
    limits.maxDepth = 2
    #expect(throws: SpikeError(.tooLarge, "value is nested too deep")) {
      try JSONValue.from(foundation: [[[1]]], limits: limits)
    }
    #expect(try JSONValue.from(foundation: [[1]], limits: limits) == .array([.array([.int(1)])]))

    limits = JSONValue.Limits()
    limits.maxItems = 3
    #expect(try JSONValue.from(foundation: [1, 2], limits: limits) == .array([.int(1), .int(2)]))
    #expect(throws: SpikeError(.tooLarge, "value has too many items")) {
      try JSONValue.from(foundation: [1, 2, 3], limits: limits)
    }

    limits = JSONValue.Limits()
    limits.maxStringUTF8 = 4
    #expect(try JSONValue.from(foundation: "abcd", limits: limits) == .string("abcd"))
    #expect(throws: SpikeError(.tooLarge, "value has too much text")) {
      try JSONValue.from(foundation: "abcde", limits: limits)
    }
    #expect(throws: SpikeError(.tooLarge, "value has too much text")) {
      try JSONValue.from(foundation: ["abc": "de"], limits: limits)
    }
  }
}

@Suite struct EventLogTests {
  @Test func linesHaveSourceSeqAndTimeAndSortedKeys() {
    var log = EventLog()
    log.append("app.fetch.start", t: 0, ["url": .string("https://localhost:18443/s/echo")])
    log.append(
      "app.nav", t: 12, ["kind": .string("start"), "seq": .int(99), "src": .string("host")])
    #expect(
      log.text
        == #"{"ev":"app.fetch.start","seq":0,"src":"app","t":0,"url":"https://localhost:18443/s/echo"}"#
        + "\n"
        + #"{"ev":"app.nav","kind":"start","seq":1,"src":"app","t":12}"# + "\n")
  }

  @Test func probesAndMetrics() {
    var log = EventLog()
    log.probe("location", .string("https://localhost:18443/"), t: 1)
    log.probe("metric.commit", .int(40), t: 2)
    log.probe("metric.commit", .double(41.5), t: 3)
    #expect(log.lines.count == 3)
    #expect(
      log.lines[0]
        == #"{"ev":"app.probe","name":"location","seq":0,"src":"app","t":1,"value":"https://localhost:18443/"}"#
    )
    #expect(log.metrics.count == 2)
    #expect(
      log.metricsJSON(run: "R")
        == #"{"metrics":{"metric.commit":40,"metric.commit#2":41.5},"run":"R"}"# + "\n")
  }

  @Test func eventCountIsBounded() {
    var log = EventLog()
    for i in 0..<(EventLog.maxEvents + 5) { log.append("app.error", t: i) }
    #expect(log.lines.count == EventLog.maxEvents)
    #expect(log.dropped == 5)
  }

  @Test func emptyLogHasNoText() {
    #expect(EventLog().text.isEmpty)
    #expect(EventLog().metricsJSON(run: "x") == #"{"metrics":{},"run":"x"}"# + "\n")
  }
}
