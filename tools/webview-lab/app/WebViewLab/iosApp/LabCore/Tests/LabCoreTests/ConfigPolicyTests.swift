// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import Foundation
import Testing

@testable import LabCore

@Suite struct LabConfigTests {
  @Test func idlesWithoutAServerArgument() throws {
    #expect(try LabConfig.parse(["WebViewLab"]) == nil)
  }

  @Test func readsTheServerAndTheDefaults() throws {
    let c = try #require(try LabConfig.parse(["app", "-labServer", "http://127.0.0.1:18091"]))
    #expect(c.server.port == 18091)
    #expect(c.server.origin == "http://127.0.0.1:18091")
    #expect(c.settleSeconds == LabConfig.defaultSettle)
    #expect(c.timeoutSeconds == LabConfig.defaultTimeout)
    #expect(c.server.indexURL?.absoluteString == "http://127.0.0.1:18091/cases.json")
    #expect(c.server.caseURL("CT-UTF8")?.absoluteString == "http://127.0.0.1:18091/c/CT-UTF8")
    #expect(c.server.caseURL("../x") == nil)
  }

  @Test func readsSettleAndTimeoutInRange() throws {
    let args = ["-labTimeout", "120", "-labServer", "http://127.0.0.1:1/", "-labSettle", "2"]
    let c = try #require(try LabConfig.parse(args))
    #expect(c.settleSeconds == 2)
    #expect(c.timeoutSeconds == 120)
    #expect(c.server.port == 1)
    let low = try #require(
      try LabConfig.parse([
        "-labServer", "http://127.0.0.1:2", "-labSettle", "0", "-labTimeout", "1",
      ]))
    #expect(low.settleSeconds == 0)
    #expect(low.timeoutSeconds == 1)
  }

  @Test(arguments: [
    ["-labServer"],
    ["-labServer", "http://127.0.0.1:18091", "-labSettle", "2.5"],
    ["-labServer", "http://127.0.0.1:18091", "-labSettle", "-1"],
    ["-labServer", "http://127.0.0.1:18091", "-labSettle", "nan"],
    ["-labServer", "http://127.0.0.1:18091", "-labSettle"],
    ["-labServer", "http://127.0.0.1:18091", "-labTimeout", "0.5"],
    ["-labServer", "http://127.0.0.1:18091", "-labTimeout", "121"],
    ["-labServer", "http://127.0.0.1:18091", "-labTimeout", "inf"],
  ])
  func rejectsBadArguments(_ args: [String]) {
    #expect(throws: LabError.self) { try LabConfig.parse(args) }
  }

  @Test(arguments: [
    "https://127.0.0.1:18091", "http://localhost:18091", "http://10.0.2.2:18091",
    "http://127.0.0.1",
    "http://127.0.0.1:0", "http://127.0.0.1:65536", "http://u@127.0.0.1:18091",
    "http://u:p@127.0.0.1:18091",
    "http://127.0.0.1:18091/c", "http://127.0.0.1:18091/?a", "http://127.0.0.1:18091/#f",
    "127.0.0.1:18091",
    "",
  ])
  func acceptsOnlyTheLoopbackOrigin(_ text: String) {
    #expect(throws: LabError.self) { try LabConfig.parseServer(text) }
  }

  @Test func acceptsTheEdgePorts() throws {
    #expect(try LabConfig.parseServer("http://127.0.0.1:65535").port == 65_535)
    #expect(try LabConfig.parseServer("http://127.0.0.1:1/").port == 1)
  }
}

@Suite struct RequestPolicyTests {
  private let policy: RequestPolicy

  init() throws {
    policy = RequestPolicy(server: try LabServer(port: 18091))
  }

  private func url(_ text: String) throws -> URL { try #require(URL(string: text)) }

  @Test(arguments: [
    "http://127.0.0.1:18091/c/NOSCRIPT", "http://127.0.0.1:18091/",
    "http://127.0.0.1:18091/c/X?q#f",
    "HTTP://127.0.0.1:18091/x", "about:blank", "about:srcdoc",
  ])
  func allowsTheLabServer(_ text: String) throws {
    #expect(policy.allows(try url(text)))
  }

  @Test(arguments: [
    "https://www.example.com/p.gif", "https://www.example.com/ns.html",
    "http://127.0.0.1:18092/c/X",
    "http://127.0.0.1/c/X", "https://127.0.0.1:18091/c/X", "http://localhost:18091/c/X",
    "http://127.0.0.1:18091@www.example.com/", "http://u@127.0.0.1:18091/",
    "http://127.0.0.2:18091/",
    "data:text/html,x", "blob:http://127.0.0.1:18091/1", "ws://127.0.0.1:18091/",
    "file:///etc/hosts",
    "about:blank#x", "javascript:void(0)",
  ])
  func blocksEverythingElse(_ text: String) throws {
    #expect(!policy.allows(try url(text)))
  }

  @Test func usesPort80WhenTheURLHasNoPort() throws {
    let p80 = RequestPolicy(server: try LabServer(port: 80))
    #expect(p80.allows(try url("http://127.0.0.1/c/X")))
    #expect(!p80.allows(try url("http://127.0.0.1:81/c/X")))
  }

  @Test func ruleListBlocksAllThenExemptsTheLabOrigin() throws {
    let json = try policy.contentRuleListJSON()
    let raw = try JSONSerialization.jsonObject(with: Data(json.utf8))
    let rules = try #require(raw as? [[String: [String: String]]])
    #expect(rules.count == 2)
    #expect(rules[0]["trigger"]?["url-filter"] == ".*")
    #expect(rules[0]["action"]?["type"] == "block")
    #expect(rules[1]["trigger"]?["url-filter"] == policy.labURLFilter)
    #expect(rules[1]["action"]?["type"] == "ignore-previous-rules")
    #expect(rules[0].count == 2 && rules[1].count == 2)
  }

  /// The exemption filter matches exactly the lab origin. WebKit content blocker expressions are
  /// a subset of ICU expressions; this filter uses only `^`, literals and `\.`.
  @Test(arguments: [
    ("http://127.0.0.1:18091/c/X", true), ("http://127.0.0.1:18091/", true),
    ("http://127.0.0.1:180911/", false),
    ("http://127.0.0.1:1809/", false), ("http://127x0x0x1:18091/", false),
    ("https://127.0.0.1:18091/", false),
    ("http://www.example.com/?http://127.0.0.1:18091/", false), ("http://127.0.0.1:18092/", false),
  ])
  func exemptionFilterMatchesOnlyTheLabOrigin(_ text: String, _ expected: Bool) throws {
    let regex = try NSRegularExpression(pattern: policy.labURLFilter)
    let match = regex.firstMatch(in: text, range: NSRange(text.startIndex..., in: text)) != nil
    #expect(match == expected)
  }
}
