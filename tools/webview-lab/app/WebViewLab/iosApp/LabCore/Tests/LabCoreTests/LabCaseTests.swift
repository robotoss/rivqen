// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import Foundation
import Testing

@testable import LabCore

@Suite struct LabCaseTests {
  private func data(_ text: String) -> Data { Data(text.utf8) }

  @Test func parsesACaseFileOfTheHostTools() throws {
    let text = """
      {"id":"NOSCRIPT","question":"Q-19","source":"fixtures/rqp/markup/FX-RQ-MARKUP-NOSCRIPT/input.html",
       "headers":[["Content-Type","text/html; charset=utf-8"]],"views":["on","off"],"probe":null}
      """
    let c = try LabCase.parse(data(text), expectedId: "NOSCRIPT")
    #expect(c == LabCase(id: "NOSCRIPT", views: [.on, .off], probe: nil))
  }

  @Test func keepsTheProbeAndTheViewOrder() throws {
    let c = try LabCase.parse(
      data(##"{"id":"CT-UTF8","views":["on"],"probe":"#m"}"##), expectedId: "CT-UTF8")
    #expect(c.views == [.on])
    #expect(c.probe == "#m")
    let d = try LabCase.parse(data(#"{"id":"X","views":["off","on"]}"#), expectedId: "X")
    #expect(d.views == [.off, .on])
    #expect(d.probe == nil)
  }

  @Test(arguments: [
    #"[]"#,
    #"not json"#,
    #"{"id":"OTHER","views":["on"],"probe":null}"#,
    #"{"id":"a","views":["on"],"probe":null}"#,
    #"{"views":["on"],"probe":null}"#,
    #"{"id":"X","views":[],"probe":null}"#,
    #"{"id":"X","views":["on","on"],"probe":null}"#,
    #"{"id":"X","views":["on","off","on"],"probe":null}"#,
    #"{"id":"X","views":["maybe"],"probe":null}"#,
    #"{"id":"X","views":"on","probe":null}"#,
    #"{"id":"X","views":["on"],"probe":""}"#,
    #"{"id":"X","views":["on"],"probe":1}"#,
  ])
  func rejectsMalformedCaseFiles(_ text: String) {
    #expect(throws: LabError.self) { try LabCase.parse(data(text), expectedId: "X") }
  }

  @Test func rejectsALongProbeAndABigFile() {
    let probe = String(repeating: "p", count: LabCase.maxProbeLength + 1)
    #expect(throws: LabError.self) {
      try LabCase.parse(data(#"{"id":"X","views":["on"],"probe":"\#(probe)"}"#), expectedId: "X")
    }
    let okProbe = String(repeating: "p", count: LabCase.maxProbeLength)
    #expect(
      (try? LabCase.parse(
        data(#"{"id":"X","views":["on"],"probe":"\#(okProbe)"}"#), expectedId: "X")) != nil)
    // A valid object padded to one byte over the limit, and exactly to the limit.
    let body = #"{"id":"X","views":["on"]}"#
    let over = body + String(repeating: " ", count: LabCase.maxFileBytes + 1 - body.utf8.count)
    #expect(throws: LabError(.tooLarge, "case X: file has \(LabCase.maxFileBytes + 1) bytes")) {
      try LabCase.parse(data(over), expectedId: "X")
    }
    let exact = body + String(repeating: " ", count: LabCase.maxFileBytes - body.utf8.count)
    #expect((try? LabCase.parse(data(exact), expectedId: "X")) != nil)
  }

  @Test(arguments: ["A", "NOSCRIPT-SVG", "CT-UTF8", "0-9", String(repeating: "Z", count: 64)])
  func acceptsValidIds(_ id: String) {
    #expect(LabCase.isValidId(id))
  }

  @Test(arguments: [
    "", "a", "NOSCRIPT_SVG", "../X", "X/Y", "X.on", " X", "İ", "Ａ",
    String(repeating: "Z", count: 65),
  ])
  func rejectsInvalidIds(_ id: String) {
    #expect(!LabCase.isValidId(id))
  }

  /// Property: `isValidId` agrees with the regular expression of the contract on random strings
  /// over an alphabet with valid, near-valid and path characters. `\z`: in ICU, `$` also matches
  /// before a final line feed; in the JavaScript expression of the host tools it does not.
  @Test func idCheckAgreesWithTheContractExpression() throws {
    let regex = try NSRegularExpression(pattern: "^[A-Z0-9-]{1,64}\\z")
    let alphabet = Array("AZ09-az_./ \u{0130}\u{FF21}\n")
    var rng = SplitMix64(seed: 0x5EED_0019)
    for _ in 0..<20_000 {
      let length = Int(rng.next() % 70)
      let id = String((0..<length).map { _ in alphabet[Int(rng.next() % UInt64(alphabet.count))] })
      let range = NSRange(id.startIndex..., in: id)
      let expected = regex.firstMatch(in: id, range: range) != nil
      #expect(LabCase.isValidId(id) == expected, "id \(id.debugDescription)")
    }
  }

  @Test func parsesTheCaseList() throws {
    #expect(try LabCase.parseIndex(data(#"["NOSCRIPT","CT-UTF8"]"#)) == ["NOSCRIPT", "CT-UTF8"])
    #expect(try LabCase.parseIndex(data("[]")) == [])
  }

  @Test(arguments: [#"{}"#, #"["A","A"]"#, #"["a"]"#, #"[1]"#, #"["../A"]"#, "x"])
  func rejectsMalformedCaseLists(_ text: String) {
    #expect(throws: LabError.self) { try LabCase.parseIndex(data(text)) }
  }

  @Test func rejectsAHugeCaseList() {
    let ids = (0...LabCase.maxCases).map { "\"C\($0)\"" }.joined(separator: ",")
    #expect(throws: LabError.self) { try LabCase.parseIndex(data("[\(ids)]")) }
    let big = "[" + String(repeating: " ", count: LabCase.maxIndexBytes - 1) + "]"
    #expect(throws: LabError.self) { try LabCase.parseIndex(data(big)) }
    let edge = "[" + String(repeating: " ", count: LabCase.maxIndexBytes - 2) + "]"
    #expect(try LabCase.parseIndex(data(edge)) == [])
    let most = (0..<LabCase.maxCases).map { "\"C\($0)\"" }.joined(separator: ",")
    #expect((try? LabCase.parseIndex(data("[\(most)]")))?.count == LabCase.maxCases)
  }
}

/// A small deterministic generator for the property tests (no dependency).
struct SplitMix64 {
  private var state: UInt64
  init(seed: UInt64) { state = seed }
  mutating func next() -> UInt64 {
    state &+= 0x9E37_79B9_7F4A_7C15
    var z = state
    z = (z ^ (z >> 30)) &* 0xBF58_476D_1CE4_E5B9
    z = (z ^ (z >> 27)) &* 0x94D0_49BB_1331_11EB
    return z ^ (z >> 31)
  }
}
