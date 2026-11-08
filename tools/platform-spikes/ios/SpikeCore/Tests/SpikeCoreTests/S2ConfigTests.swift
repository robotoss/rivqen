// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import Testing

@testable import SpikeCore

@Suite struct SupportingSetTests {
  func args(_ cap: String, run: String, set: String? = nil) -> [String] {
    var a = ["/app", "-rqRun", run, "-rqCap", cap, "-rqOrigin", "https://localhost:18443"]
    if let set { a += ["-rqSet", set] }
    return a
  }

  @Test func catalogRunsHaveNoSet() throws {
    let c = try #require(try LaunchConfig.parse(args("CAP-I-09", run: "CAP-I-09-ios-18.5-1")))
    #expect(c.capability == .serviceWorker)
    #expect(c.set == nil)
  }

  @Test func knownSetsAreAccepted() throws {
    let sw = try #require(
      try LaunchConfig.parse(args("CAP-I-09", run: "CAP-I-09-ios-27.0-1", set: "app-bound")))
    #expect(sw.set == "app-bound")
    let ck = try #require(
      try LaunchConfig.parse(args("CAP-I-11", run: "CAP-I-11-ios-27.0-2", set: "initiator")))
    #expect(ck.set == "initiator")
  }

  @Test(arguments: [
    ("CAP-I-09", "initiator"), ("CAP-I-11", "app-bound"), ("CAP-I-01", "app-bound"),
    ("CAP-I-09", ""), ("CAP-I-09", "APP-BOUND"),
  ])
  func otherSetsAreRefused(_ cap: String, _ set: String) {
    #expect(throws: SpikeError.self) {
      try LaunchConfig.parse(self.args(cap, run: "\(cap)-ios-27.0-1", set: set))
    }
  }

  @Test func newCapabilities() {
    #expect(Capability(rawValue: "CAP-I-08") == .contentSecurityPolicy)
    #expect(Capability.contentSecurityPolicy.prototype == "IOS-P1")
    #expect(Capability.serviceWorker.prototype == "IOS-ALL")
    #expect(Capability.history.prototype == "IOS-ALL")
    #expect(Capability.simulatedFirstRender.supportingSets.isEmpty)
  }

  @Test func s2Paths() {
    let p = LabPaths(run: "R-1")
    #expect(p.csp(mode: "meta", tag: "sim") == "/s/csp?run=R-1&mode=meta&tag=sim")
    #expect(p.swPage(tag: "live") == "/s/sw-page?run=R-1&tag=live")
    // The same form as the #next link of the host page, so a click and a load give one URL.
    #expect(p.hist(step: 3) == "/s/hist?run=R-1&step=3")
  }
}
