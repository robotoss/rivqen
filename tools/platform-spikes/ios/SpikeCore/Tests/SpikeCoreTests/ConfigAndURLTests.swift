// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import Foundation
import Testing

@testable import SpikeCore

@Suite struct LaunchConfigTests {
  let good = [
    "/app", "-rqRun", "CAP-I-01-ios-27.0-1", "-rqCap", "CAP-I-01", "-rqOrigin",
    "https://localhost:18443",
  ]

  @Test func parsesTheRunScriptArguments() throws {
    let c = try #require(try LaunchConfig.parse(good + ["-NSDocumentRevisionsDebugMode", "YES"]))
    #expect(c.run == "CAP-I-01-ios-27.0-1")
    #expect(c.capability == .simulatedFirstRender)
    #expect(c.origin == (try LabOrigin(host: "localhost", port: 18443)))
    #expect(c.stepTimeout == LaunchConfig.defaultTimeout)
    let t = try #require(try LaunchConfig.parse(good + ["-rqTimeout", "5"]))
    #expect(t.stepTimeout == 5)
  }

  @Test func noRunMeansIdle() throws {
    #expect(try LaunchConfig.parse(["/app"]) == nil)
    #expect(try LaunchConfig.parse(["/app", "-rqCap", "CAP-I-01"]) == nil)
  }

  @Test(arguments: [
    [
      "-rqRun", "CAP-I-01-ios-27.0-1", "-rqCap", "CAP-I-99", "-rqOrigin", "https://localhost:18443",
    ],
    [
      "-rqRun", "CAP-I-02-ios-27.0-1", "-rqCap", "CAP-I-01", "-rqOrigin", "https://localhost:18443",
    ],
    [
      "-rqRun", "CAP-I-01-ios-27.0-1;x", "-rqCap", "CAP-I-01", "-rqOrigin",
      "https://localhost:18443",
    ],
    ["-rqRun", "CAP-I-01-ios-27.0-1", "-rqCap", "CAP-I-01"],
    [
      "-rqRun", "CAP-I-01-ios-27.0-1", "-rqCap", "CAP-I-01", "-rqOrigin",
      "https://example.com:18443",
    ],
    [
      "-rqRun", "CAP-I-01-ios-27.0-1", "-rqCap", "CAP-I-01", "-rqOrigin", "https://localhost:18443",
      "-rqTimeout", "0",
    ],
    [
      "-rqRun", "CAP-I-01-ios-27.0-1", "-rqCap", "CAP-I-01", "-rqOrigin", "https://localhost:18443",
      "-rqTimeout", "nan",
    ],
    [
      "-rqRun", "CAP-I-01-ios-27.0-1", "-rqCap", "CAP-I-01", "-rqOrigin", "https://localhost:18443",
      "-rqX", "1",
    ],
    ["-rqRun", "CAP-I-01-ios-27.0-1", "-rqRun", "CAP-I-01-ios-27.0-2", "-rqCap", "CAP-I-01"],
    ["-rqRun"],
  ])
  func refusesBadArguments(_ args: [String]) {
    #expect(throws: SpikeError.self) { try LaunchConfig.parse(["/app"] + args) }
  }

  @Test func runIdRule() {
    #expect(LaunchConfig.isRunId("CAP-I-01-ios-27.0-1"))
    #expect(LaunchConfig.isRunId("azAZ09.-"))
    #expect(!LaunchConfig.isRunId(""))
    #expect(!LaunchConfig.isRunId("a_b"))
    #expect(!LaunchConfig.isRunId("a b"))
    #expect(!LaunchConfig.isRunId("a/b"))
    #expect(!LaunchConfig.isRunId("é"))
    #expect(LaunchConfig.isRunId(String(repeating: "a", count: 128)))
    #expect(!LaunchConfig.isRunId(String(repeating: "a", count: 129)))
  }

  @Test func prototypes() {
    #expect(Capability.simulatedCookies.prototype == "IOS-P1")
    #expect(Capability.navigationWithSnapshot.prototype == "IOS-P2")
    #expect(Capability.schemeOrigin.prototype == "IOS-P3")
    #expect(Capability.httpsStreaming.prototype == "IOS-ALL")
  }
}

@Suite struct LabOriginTests {
  @Test func parsesBothSites() throws {
    let a = try LabOrigin.parse("https://localhost:18443/")
    #expect(a.https == "https://localhost:18443")
    #expect(a.scheme == "rqlab://localhost:18443")
    #expect(try LabOrigin.parse("https://127.0.0.1:1").port == 1)
    #expect(try LabOrigin.parse("https://127.0.0.1:65535").port == 65_535)
  }

  @Test(arguments: [
    "http://localhost:18443", "https://localhost", "https://localhost:", "https://localhost:0",
    "https://localhost:65536", "https://localhost:18443/x", "https://evil.test:18443",
    "https://localhost:1a",
    "https://localhost:+1", "https://10.0.2.2:18443",
  ])
  func refuses(_ text: String) {
    #expect(throws: SpikeError.self) { try LabOrigin.parse(text) }
  }
}

@Suite struct LabSchemeTests {
  @Test func mapsToHttpsWithPathAndQuery() throws {
    let u = try #require(URL(string: "rqlab://localhost:18443/s/stream?run=R&k=1#frag"))
    #expect(
      try LabScheme.httpsURL(for: u, port: 18443).get().absoluteString
        == "https://localhost:18443/s/stream?run=R&k=1")
    let b = try #require(URL(string: "rqlab://127.0.0.1:18443/s/echo"))
    #expect(
      try LabScheme.httpsURL(for: b, port: 18443).get().absoluteString
        == "https://127.0.0.1:18443/s/echo")
    let e = try #require(URL(string: "rqlab://localhost:18443/a%20b?q=%2F"))
    #expect(
      try LabScheme.httpsURL(for: e, port: 18443).get().absoluteString
        == "https://localhost:18443/a%20b?q=%2F")
  }

  @Test(arguments: [
    "https://localhost:18443/s/echo", "rqlab://evil.test:18443/s/echo",
    "rqlab://localhost:18444/s/echo",
    "rqlab://localhost/s/echo", "rqlab://u:p@localhost:18443/s/echo", "rqlab://localhost:18443",
    "rqlab://localhost:18443//evil.test/x", "RQLAB://localhost:18443/s/echo",
  ])
  func refuses(_ text: String) throws {
    let u = try #require(URL(string: text))
    #expect(throws: SpikeError.self) { try LabScheme.httpsURL(for: u, port: 18443).get() }
  }

  @Test func probeOriginRule() {
    #expect(ProbeOriginRule.allows(scheme: "https", host: "localhost", port: 18443, labPort: 18443))
    #expect(ProbeOriginRule.allows(scheme: "rqlab", host: "127.0.0.1", port: 18443, labPort: 18443))
    #expect(!ProbeOriginRule.allows(scheme: "http", host: "localhost", port: 18443, labPort: 18443))
    #expect(
      !ProbeOriginRule.allows(scheme: "https", host: "evil.test", port: 18443, labPort: 18443))
    #expect(!ProbeOriginRule.allows(scheme: "https", host: "localhost", port: 443, labPort: 18443))
    #expect(!ProbeOriginRule.allows(scheme: "", host: "", port: 0, labPort: 18443))
  }

  @Test func beaconIndex() {
    #expect(LabPaths.beaconIndex(path: "/s/beacon", query: "run=R&k=3") == 3)
    #expect(LabPaths.beaconIndex(path: "/s/beacon", query: "k=0&run=R") == 0)
    #expect(LabPaths.beaconIndex(path: "/s/beacon", query: "k=1234567") == 1_234_567)
    #expect(LabPaths.beaconIndex(path: "/s/beacon", query: "k=12345678") == nil)
    #expect(LabPaths.beaconIndex(path: "/s/beacon", query: "k=") == nil)
    #expect(LabPaths.beaconIndex(path: "/s/beacon", query: "k=-1") == nil)
    #expect(LabPaths.beaconIndex(path: "/s/beacon", query: "run=R") == nil)
    #expect(LabPaths.beaconIndex(path: "/s/beacon", query: nil) == nil)
    #expect(LabPaths.beaconIndex(path: "/s/echo", query: "k=1") == nil)
  }

  @Test func paths() {
    let p = LabPaths(run: "R-1")
    #expect(
      p.stream(chunks: 4, size: 2048, gate: .beacon, ms: 3000)
        == "/s/stream?run=R-1&chunks=4&size=2048&gate=beacon&timeout=3000")
    #expect(
      p.stream(chunks: 1, size: 9, gate: .delay, ms: 0)
        == "/s/stream?run=R-1&chunks=1&size=9&gate=delay&delay=0")
    #expect(p.echo(tag: "page", html: true) == "/s/echo?run=R-1&tag=page&html=1")
    #expect(p.echo(tag: "x") == "/s/echo?run=R-1&tag=x")
    #expect(p.cookieSet(name: "a", value: "b") == "/s/cookie/set?run=R-1&name=a&value=b")
    #expect(
      p.cookieSet(name: "a", value: "b", attrs: ["HttpOnly", "Secure"])
        == "/s/cookie/set?run=R-1&name=a&value=b&attrs=HttpOnly,Secure")
  }
}
