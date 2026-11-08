// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import Foundation
import Testing

@testable import SpikeCore

@Suite struct CookiePlanTests {
  @Test func oneCookieForEachAttribute() {
    #expect(CookiePlan.cases.map(\.suffix) == ["plain", "http", "strict", "lax", "none"])
    // SameSite=None needs Secure (a browser drops it otherwise).
    for c in CookiePlan.cases where c.attrs.contains("SameSite=None") {
      #expect(c.attrs.contains("Secure"))
    }
    // The host accepts every planned attribute list (CONTRACT.md section 3 attrs rule).
    let allowed = Set(["HttpOnly", "Secure", "SameSite=Strict", "SameSite=Lax", "SameSite=None"])
    for c in CookiePlan.cases {
      #expect(Set(c.attrs).isSubset(of: allowed))
      #expect(Set(c.attrs).count == c.attrs.count)
    }
    // Names and values are synthetic ([a-z0-9-]{1,64}).
    for c in CookiePlan.cases {
      #expect(CookieFacts.safe("nb-" + c.suffix) == "nb-" + c.suffix)
    }
    #expect(CookieFacts.safe(CookiePlan.value) == CookiePlan.value)
  }

  @Test func paths() {
    let p = LabPaths(run: "R-1")
    let c = CookiePlan.cases[2]
    #expect(
      p.cookieSet(name: "na-" + c.suffix, value: CookiePlan.value, attrs: c.attrs)
        == "/s/cookie/set?run=R-1&name=na-strict&value=1&attrs=Secure,SameSite=Strict")
  }
}

@Suite struct CookieFactsTests {
  func facts(
    _ name: String, domain: String = "localhost", httpOnly: Bool = false, secure: Bool = false,
    sameSite: String = "unset", session: Bool = true
  ) -> CookieFacts {
    CookieFacts(
      name: name, domain: domain, path: "/", httpOnly: httpOnly, secure: secure,
      sameSite: sameSite, session: session)
  }

  @Test func descriptorNamesEveryAttribute() {
    #expect(facts("a").descriptor == "a@localhost path=/ SameSite=unset session")
    #expect(
      facts("b", domain: "127.0.0.1", httpOnly: true, secure: true, sameSite: "Strict")
        .descriptor == "b@127.0.0.1 path=/ HttpOnly Secure SameSite=Strict session")
    #expect(
      facts("c", sameSite: "Lax", session: false).descriptor
        == "c@localhost path=/ SameSite=Lax persistent")
  }

  @Test func descriptorRedactsUnexpectedText() {
    #expect(facts("A b").descriptor.hasPrefix("[redacted]@localhost "))
    #expect(facts("x", domain: "").descriptor.hasPrefix("x@[redacted] "))
    #expect(facts(String(repeating: "a", count: 65)).descriptor.hasPrefix("[redacted]@"))
    #expect(facts(String(repeating: "a", count: 64)).descriptor.hasPrefix("aaaa"))
    let slash = CookieFacts(
      name: "a", domain: "a/b", path: "/x/y", httpOnly: false, secure: false, sameSite: "Lax",
      session: true)
    // A slash is allowed in a path only.
    #expect(slash.descriptor == "a@[redacted] path=/x/y SameSite=Lax session")
  }

  @Test(arguments: [
    (String?.none, "unset"), ("lax", "Lax"), ("Lax", "Lax"), ("strict", "Strict"),
    ("STRICT", "Strict"), ("none", "None"), ("", "other"), ("lax ", "other"),
  ])
  func sameSiteWords(_ raw: String?, _ word: String) {
    #expect(CookieFacts.sameSite(raw: raw) == word)
  }

  @Test func descriptorsAreSortedAndBounded() {
    #expect(CookieFacts.descriptors([facts("b"), facts("a")]).map { $0.prefix(1) } == ["a", "b"])
    let many = (0..<70).map { facts("c\($0)") }
    let out = CookieFacts.descriptors(many)
    #expect(out.count == CookieFacts.maxCookies + 1)
    #expect(out.last == "[more]")
    #expect(CookieFacts.descriptors([]) == [])
  }
}

@Suite struct CookieNamesTests {
  @Test func namesOfAField() {
    #expect(CookieNames.fromField(nil) == [])
    #expect(CookieNames.fromField("") == [])
    #expect(CookieNames.fromField("b=1; a=2") == ["a", "b"])
    #expect(CookieNames.fromField("a=1;b=2;") == ["a", "b"])
    #expect(CookieNames.fromField("na-http=1; na-plain=1") == ["na-http", "na-plain"])
    // A value is never needed, and an odd name is not logged.
    #expect(CookieNames.fromField("Secret=x; a=1") == ["[redacted]", "a"])
    #expect(CookieNames.fromField("=x") == ["[redacted]"])
    #expect(CookieNames.fromField("noequals") == ["noequals"])
  }

  @Test func namesAreBounded() {
    let field = (0..<100).map { "c\($0)=1" }.joined(separator: "; ")
    let out = CookieNames.fromField(field)
    #expect(out.count == CookieNames.maxNames + 1)
    #expect(out.last == "[more]")
  }

  @Test func echoSummary() {
    let a = Data(#"{"cookie":"nb-plain=1; nb-http=1","fields":[],"site":"B"}"#.utf8)
    #expect(
      CookieNames.fromEcho(a)
        == .object([
          "site": .string("B"), "cookies": .array([.string("nb-http"), .string("nb-plain")]),
        ]))
    let none = Data(#"{"cookie":null,"site":"A"}"#.utf8)
    #expect(CookieNames.fromEcho(none) == .object(["site": .string("A"), "cookies": .array([])]))
  }

  @Test(arguments: [
    #"{"cookie":"a=1","site":"C"}"#, #"{"cookie":1,"site":"A"}"#, #"[1]"#, "not json",
    #"{"cookie":"a=1"}"#,
  ])
  func echoRefusesOtherText(_ text: String) {
    #expect(CookieNames.fromEcho(Data(text.utf8)) == nil)
  }

  @Test func echoIsBounded() {
    let big = Data(
      (#"{"site":"A","cookie":""# + String(repeating: "a", count: 70_000) + #""}"#).utf8)
    #expect(CookieNames.fromEcho(big) == nil)
  }

  @Test func otherSite() throws {
    let a = try LabOrigin(host: "localhost", port: 18443)
    #expect(a.otherSite.https == "https://127.0.0.1:18443")
    #expect(a.otherSite.otherSite == a)
  }
}
