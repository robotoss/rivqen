// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import Foundation

/// The cookie cases of CAP-I-11 (cookie sync, Q-12). One cookie for each attribute, so a log line
/// shows the effect of one attribute: no attribute, `HttpOnly`, `SameSite=Strict`, `SameSite=Lax`
/// and `SameSite=None`. `SameSite=None` needs `Secure`; `Strict` has `Secure` too, so both
/// `Secure` and non-`Secure` cookies are in the set. Names are `<prefix>-<case>`, values `1`
/// (synthetic, CONTRACT.md section 3).
public enum CookiePlan {
  public struct Case: Sendable, Equatable {
    public let suffix: String
    public let attrs: [String]
  }

  public static let cases: [Case] = [
    Case(suffix: "plain", attrs: []),
    Case(suffix: "http", attrs: ["HttpOnly"]),
    Case(suffix: "strict", attrs: ["Secure", "SameSite=Strict"]),
    Case(suffix: "lax", attrs: ["SameSite=Lax"]),
    Case(suffix: "none", attrs: ["Secure", "SameSite=None"]),
  ]

  /// The synthetic value of every planned cookie.
  public static let value = "1"
}

/// The attributes of one cookie as the app sees them in `HTTPCookieStorage` or
/// `WKHTTPCookieStore`, for the run log. No value: the names and attributes are the facts.
public struct CookieFacts: Sendable, Equatable {
  public let name: String
  public let domain: String
  public let path: String
  public let httpOnly: Bool
  public let secure: Bool
  /// `Strict`, `Lax`, `None`, `unset` (no policy) or `other`.
  public let sameSite: String
  public let session: Bool

  public init(
    name: String, domain: String, path: String, httpOnly: Bool, secure: Bool, sameSite: String,
    session: Bool
  ) {
    self.name = name
    self.domain = domain
    self.path = path
    self.httpOnly = httpOnly
    self.secure = secure
    self.sameSite = sameSite
    self.session = session
  }

  /// `HTTPCookie.sameSitePolicy?.rawValue` as a log word. Foundation names `lax` and `strict`;
  /// any other text is `other` (a log never holds a free text from a store).
  public static func sameSite(raw: String?) -> String {
    guard let raw else { return "unset" }
    switch raw.lowercased() {
    case "lax": return "Lax"
    case "strict": return "Strict"
    case "none": return "None"
    default: return "other"
    }
  }

  /// The most cookies the app logs or copies in one step. The lab sets at most 12 per store.
  public static let maxCookies = 64

  /// One log word: `name@domain path=/ HttpOnly Secure SameSite=Lax session`. A name or domain
  /// outside the lab form (`[a-z0-9.-]`, at most 64 bytes) is `[redacted]`.
  public var descriptor: String {
    var parts = ["\(Self.safe(name))@\(Self.safe(domain))", "path=\(Self.safe(path, slash: true))"]
    if httpOnly { parts.append("HttpOnly") }
    if secure { parts.append("Secure") }
    parts.append("SameSite=\(sameSite)")
    parts.append(session ? "session" : "persistent")
    return parts.joined(separator: " ")
  }

  /// The descriptors of a list, sorted, at most `maxCookies` (then one `[more]` word).
  public static func descriptors(_ list: [CookieFacts]) -> [String] {
    let sorted = list.map(\.descriptor).sorted()
    guard sorted.count > maxCookies else { return sorted }
    return Array(sorted.prefix(maxCookies)) + ["[more]"]
  }

  static func safe(_ s: String, slash: Bool = false) -> String {
    let ok =
      !s.isEmpty && s.utf8.count <= 64
      && s.utf8.allSatisfy { b in
        (b >= 0x61 && b <= 0x7A) || (b >= 0x30 && b <= 0x39) || b == 0x2D || b == 0x2E
          || (slash && b == 0x2F)
      }
    return ok ? s : "[redacted]"
  }
}

/// Cookie names in the request echo of the host (`/s/echo`, CONTRACT.md section 3).
public enum CookieNames {
  /// The most names one field gives.
  public static let maxNames = 64

  /// The sorted names of a `Cookie` field value (`a=1; b=2`); `nil` gives `[]`. A name outside
  /// `[a-z0-9-]` (at most 64 bytes) is `[redacted]`; more than `maxNames` names end in `[more]`.
  public static func fromField(_ value: String?) -> [String] {
    guard let value, !value.isEmpty else { return [] }
    var names: [String] = []
    for segment in value.split(separator: ";", omittingEmptySubsequences: true) {
      if names.count == maxNames {
        names.append("[more]")
        break
      }
      let pair = segment.trimmingCharacters(in: .whitespaces)
      let name =
        pair.split(separator: "=", maxSplits: 1, omittingEmptySubsequences: false).first
        .map(String.init) ?? ""
      let ok =
        !name.isEmpty && name.utf8.count <= 64
        && name.utf8.allSatisfy { b in
          (b >= 0x61 && b <= 0x7A) || (b >= 0x30 && b <= 0x39) || b == 0x2D
        }
      names.append(ok ? name : "[redacted]")
    }
    let more = names.last == "[more]"
    let sorted = names.filter { $0 != "[more]" }.sorted()
    return more ? sorted + ["[more]"] : sorted
  }

  /// The echo summary for the log: `{"site": "A"|"B", "cookies": [sorted names]}`, or `nil` when
  /// the text is not the echo JSON of the host (at most 64 KiB).
  public static func fromEcho(_ data: Data) -> JSONValue? {
    guard data.count <= 64 * 1024,
      let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
      let site = object["site"] as? String, site == "A" || site == "B"
    else { return nil }
    let cookie: String?
    switch object["cookie"] {
    case let s as String: cookie = s
    case is NSNull, nil: cookie = nil
    default: return nil
    }
    return .object([
      "site": .string(site), "cookies": .array(fromField(cookie).map { .string($0) }),
    ])
  }

  /// The CAP-I-11 form of a page probe value: `cookie` (a `document.cookie` text) becomes the
  /// sorted names; `echo` and `frame.echo` (the echo JSON text) become the echo summary. Cookies
  /// that WebKit creates in the same millisecond come in any order, so the sorted form is the one
  /// that compares across runs. Any other probe, or a value of another form, is unchanged.
  public static func pageView(name: String, value: JSONValue) -> JSONValue {
    switch (name, value) {
    case ("cookie", .string(let text)):
      return .array(fromField(text).map { .string($0) })
    case ("echo", .string(let text)), ("frame.echo", .string(let text)):
      return fromEcho(Data(text.utf8)) ?? value
    default:
      return value
    }
  }
}

extension LabOrigin {
  /// The other site of the host (CONTRACT.md section 1): `127.0.0.1` for `localhost` (site B for
  /// site A), and `localhost` for `127.0.0.1`. Same port.
  public var otherSite: LabOrigin {
    // The two hosts are the only allowed ones and the port is already checked, so the init
    // cannot fail; the fallback keeps the function total.
    let host = self.host == "localhost" ? "127.0.0.1" : "localhost"
    return (try? LabOrigin(host: host, port: port)) ?? self
  }
}
