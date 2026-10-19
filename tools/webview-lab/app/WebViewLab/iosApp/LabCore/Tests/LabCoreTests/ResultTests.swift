// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import Foundation
import Testing

@testable import LabCore

@Suite struct LabResultTests {
  private func object(_ r: LabResult) throws -> [String: Any] {
    let raw = try JSONSerialization.jsonObject(with: try r.encoded())
    return try #require(raw as? [String: Any])
  }

  /// CONTRACT section 4 shows every key; compare.mjs reads them. A nil value is `null`, not absent.
  @Test func writesEveryContractKeyWithNull() throws {
    let r = LabResult.unavailable(id: "NOSCRIPT", view: .off, reason: "no walker")
    let o = try object(r)
    #expect(
      Set(o.keys) == [
        "id", "view", "walker", "tree", "characterSet", "contentType", "probe", "error",
      ])
    #expect(o["id"] as? String == "NOSCRIPT")
    #expect(o["view"] as? String == "off")
    #expect(o["walker"] as? String == "unavailable")
    #expect(o["tree"] is NSNull)
    #expect(o["characterSet"] is NSNull)
    #expect(o["error"] as? String == "no walker")
    #expect(r.fileName == "NOSCRIPT.off.json")
  }

  @Test func writesTheTreeAndTheMeta() throws {
    let meta = PageMeta(characterSet: "UTF-8", contentType: "text/html", probe: "Grüße – 日本")
    let tree = "| <html>\n|   <head>\n|   <body>"
    let r = try LabResult.ok(id: "CT-UTF8", view: .on, tree: tree, meta: meta, metaError: nil)
    let o = try object(r)
    #expect(o["walker"] as? String == "ok")
    #expect(o["tree"] as? String == tree)
    #expect(o["characterSet"] as? String == "UTF-8")
    #expect(o["contentType"] as? String == "text/html")
    #expect(o["probe"] as? String == "Grüße – 日本")
    #expect(o["error"] is NSNull)
    #expect(r.fileName == "CT-UTF8.on.json")
  }

  @Test func keepsTheTreeWhenOnlyTheMetaFailed() throws {
    let r = try LabResult.ok(id: "X", view: .off, tree: "", meta: nil, metaError: "meta failed")
    #expect(r.walker == .ok)
    #expect(r.tree == "")
    #expect(r.characterSet == nil)
    #expect(r.error == "meta failed")
  }

  @Test func refusesABadIdAndAHugeTree() throws {
    #expect(throws: LabError.self) {
      try LabResult.ok(id: "../x", view: .on, tree: "", meta: nil, metaError: nil)
    }
    let huge = String(repeating: "x", count: LabResult.maxTreeBytes + 1)
    let r = try LabResult.ok(id: "X", view: .on, tree: huge, meta: nil, metaError: nil)
    #expect(r.walker == .unavailable)
    #expect(r.tree == nil)
    let edge = String(repeating: "x", count: LabResult.maxTreeBytes)
    #expect(
      try LabResult.ok(id: "X", view: .on, tree: edge, meta: nil, metaError: nil).walker == .ok)
  }

  @Test func clipsLongErrors() {
    let long = String(repeating: "e", count: LabResult.maxErrorLength + 5)
    let r = LabResult.unavailable(id: "X", view: .on, reason: long)
    #expect(r.error?.count == LabResult.maxErrorLength + 3)
    let exact = String(repeating: "e", count: LabResult.maxErrorLength)
    #expect(LabResult.unavailable(id: "X", view: .on, reason: exact).error == exact)
  }

  @Test func keepsTheMetaOfAnUnavailableResult() {
    let meta = PageMeta(characterSet: "UTF-8", contentType: "text/plain", probe: nil)
    let r = LabResult.unavailable(id: "BOM-NONE", view: .on, reason: "walker failed", meta: meta)
    #expect(r.walker == .unavailable)
    #expect(r.tree == nil)
    #expect(r.characterSet == "UTF-8")
    #expect(r.contentType == "text/plain")
    #expect(r.probe == nil)
  }

  @Test func outputIsStable() throws {
    let r = LabResult.unavailable(id: "X", view: .on, reason: "r")
    let text = String(decoding: try r.encoded(), as: UTF8.self)
    #expect(
      text == """
        {
          "characterSet" : null,
          "contentType" : null,
          "error" : "r",
          "id" : "X",
          "probe" : null,
          "tree" : null,
          "view" : "on",
          "walker" : "unavailable"
        }
        """)
  }
}

@Suite struct PageMetaTests {
  @Test func decodesTheMetaJSON() throws {
    let m = try PageMeta.decode(
      #"{"characterSet":"windows-1252","contentType":"text/html","probe":null}"#)
    #expect(m == PageMeta(characterSet: "windows-1252", contentType: "text/html", probe: nil))
  }

  @Test(arguments: ["", "[]", "null", #"{"characterSet":1}"#])
  func rejectsOtherValues(_ text: String) {
    #expect(throws: LabError.self) { try PageMeta.decode(text) }
  }

  @Test func rejectsHugeMeta() {
    // Valid JSON padded with spaces: one byte over the limit fails, the limit itself passes.
    let body = #"{"probe":null}"#
    let over = body + String(repeating: " ", count: PageMeta.maxBytes + 1 - body.utf8.count)
    #expect(throws: LabError.self) { try PageMeta.decode(over) }
    let edge = body + String(repeating: " ", count: PageMeta.maxBytes - body.utf8.count)
    #expect(try PageMeta.decode(edge) == PageMeta(characterSet: nil, contentType: nil, probe: nil))
  }

  @Test func scriptReadsOnlyTheContractFields() {
    #expect(PageMeta.script.contains("document.characterSet"))
    #expect(PageMeta.script.contains("document.contentType"))
    #expect(PageMeta.script.contains("textContent"))
  }
}

@Suite struct ControlTests {
  @Test func decodesTheControlProbe() throws {
    let text = """
      {"pageScriptRan":false,"noscriptParsedAsMarkup":true,"scriptElements":0,"dataScriptInDOM":false,\
      "noscriptDataScriptInDOM":false,"onclickAttributeInDOM":false}
      """
    let probe = try #require(ControlProbe.decode(text))
    #expect(!probe.pageScriptRan && probe.noscriptParsedAsMarkup)
    #expect(probe.scriptElements == 0)
    #expect(
      !probe.dataScriptInDOM && !probe.noscriptDataScriptInDOM && !probe.onclickAttributeInDOM)
  }

  @Test(arguments: ["", "1|true", "{}", #"{"pageScriptRan":"yes"}"#])
  func rejectsOtherProbeValues(_ text: String) {
    #expect(ControlProbe.decode(text) == nil)
  }

  @Test func rejectsAHugeProbe() {
    // A valid probe padded with spaces past the limit is refused before decoding.
    let body = """
      {"pageScriptRan":true,"noscriptParsedAsMarkup":false,"scriptElements":2,"dataScriptInDOM":true,\
      "noscriptDataScriptInDOM":false,"onclickAttributeInDOM":true}
      """
    #expect(ControlProbe.decode(body) != nil)
    let over = body + String(repeating: " ", count: ControlProbe.maxBytes + 1 - body.utf8.count)
    #expect(ControlProbe.decode(over) == nil)
    let edge = body + String(repeating: " ", count: ControlProbe.maxBytes - body.utf8.count)
    #expect(ControlProbe.decode(edge) != nil)
  }

  /// The control page has the signals that the probe reads.
  @Test func controlPageHasEverySignal() {
    for id in ["data-lab-ran", "lab-data", "lab-n", "lab-ns-data", "lab-h"] {
      #expect(ControlResult.html.contains(id))
      #expect(ControlResult.probeScript.contains(id))
    }
    #expect(ControlResult.html.contains("onclick="))
  }

  @Test func encodesTheRunLog() throws {
    var run = AppRun(
      osName: "iOS", osVersion: "26.5", server: "http://127.0.0.1:18091", settleSeconds: 1,
      timeoutSeconds: 20,
      serializerBytes: 3000, startedAt: "2026-10-19T00:00:00Z")
    let probe = ControlProbe(
      pageScriptRan: false, noscriptParsedAsMarkup: true, scriptElements: 0, dataScriptInDOM: false,
      noscriptDataScriptInDOM: false, onclickAttributeInDOM: false)
    run.controls.append(
      ControlResult(
        view: .off, dom: probe, defaultClientEvaluation: "ok", pageWorldEvaluation: "ok",
        defaultClientCallAsync: "ok"))
    run.blockedNavigations.append(
      BlockedNavigation(caseId: "NOSCRIPT", view: .off, url: "https://www.example.com/ns.html"))
    let raw = try JSONSerialization.jsonObject(with: try run.encoded())
    let o = try #require(raw as? [String: Any])
    #expect(o["platform"] as? String == "ios")
    let controls = try #require(o["controls"] as? [[String: Any]])
    #expect(controls.count == 1)
    #expect((controls[0]["dom"] as? [String: Any])?["scriptElements"] as? Int == 0)
    #expect(
      (o["blockedNavigations"] as? [[String: Any]])?.first?["url"] as? String
        == "https://www.example.com/ns.html")
  }
}

@Suite struct ReadBoundedTests {
  private func temp(_ bytes: Int) throws -> URL {
    let url = FileManager.default.temporaryDirectory.appendingPathComponent(
      "labcore-\(UUID().uuidString)")
    try Data(repeating: 0x41, count: bytes).write(to: url)
    return url
  }

  @Test func readsUpToTheLimit() throws {
    let url = try temp(10)
    defer { try? FileManager.default.removeItem(at: url) }
    #expect(try readBounded(url, limit: 10).count == 10)
    #expect(try readBounded(url, limit: 11).count == 10)
    #expect(throws: LabError.self) { try readBounded(url, limit: 9) }
  }

  @Test func readsAnEmptyFile() throws {
    let url = try temp(0)
    defer { try? FileManager.default.removeItem(at: url) }
    #expect(try readBounded(url, limit: 1).isEmpty)
  }

  @Test func reportsAMissingFile() {
    let url = FileManager.default.temporaryDirectory.appendingPathComponent(
      "labcore-missing-\(UUID().uuidString)")
    #expect(throws: LabError.self) { try readBounded(url, limit: 1) }
  }
}
