// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Foundation runner (macOS and iOS simulator): `main <inputs-file> url|components <stack-name>`.
// Pure parsing, no network. Output: raw records (see lib.mjs).

import Foundation

func enc(_ s: String?) -> String {
    guard let s = s else { return "~" }
    return Data(s.utf8).base64EncodedString()
}

func dec(_ s: Substring) -> String? {
    if s == "~" { return nil }
    guard let d = Data(base64Encoded: String(s)) else { return nil }
    return String(decoding: d, as: UTF8.self)
}

func jsonString(_ s: String) -> String {
    var out = "\""
    for u in s.unicodeScalars {
        if u == "\"" || u == "\\" { out += "\\" + String(u) }
        else if u.value < 0x20 || u.value > 0x7e { out += String(format: "\\u%04x", u.value) }
        else { out += String(u) }
    }
    return out + "\""
}

struct Parsed {
    var href: String
    var host: String?
    var path: String?
    var query: String?
    var extra: String?
}

let args = CommandLine.arguments
guard args.count == 4, let text = try? String(contentsOfFile: args[1], encoding: .utf8) else {
    FileHandle.standardError.write(Data("usage: main <inputs> url|components <stack>\n".utf8))
    exit(2)
}
let mode = args[2]
let stackName = args[3]
let env = ProcessInfo.processInfo
let version = "Foundation of \(env.operatingSystemVersionString); \(env.environment["RQ_SWIFT_VERSION"] ?? "Swift NOT FOUND"); API: "
    + (mode == "url" ? "URL(string:) or URL(string:relativeTo:)?.absoluteURL, absoluteString"
                     : "URLComponents(string:), .string (url(relativeTo:)?.absoluteString for a base)")
var runtime = env.environment["SIMULATOR_DEVICE_NAME"].map { "iOS simulator \($0), \(env.environment["SIMULATOR_RUNTIME_VERSION"] ?? "NOT FOUND")" }
    ?? "macOS \(env.operatingSystemVersionString)"
#if arch(arm64)
runtime += ", arm64"
#endif

var out = "H\tstack\t\(enc(stackName))\nH\tversion\t\(enc(version))\nH\truntime\t\(enc(runtime))\n"

func parseURL(_ input: String, _ base: String?) -> Parsed? {
    let u: URL?
    if let base = base {
        guard let b = URL(string: base) else { return nil }
        u = URL(string: input, relativeTo: b)?.absoluteURL
    } else {
        u = URL(string: input)
    }
    guard let url = u else { return nil }
    let strict = base == nil ? URL(string: input, encodingInvalidCharacters: false)?.absoluteString : nil
    return Parsed(href: url.absoluteString,
                  host: url.host(percentEncoded: true),
                  path: url.path(percentEncoded: true),
                  query: url.query(percentEncoded: true),
                  extra: "{\"absoluteString_noEncodeInvalid\":" + (strict.map(jsonString) ?? "null") + "}")
}

func parseComponents(_ input: String, _ base: String?) -> Parsed? {
    guard let c = URLComponents(string: input) else { return nil }
    var viaURL: String?
    if let base = base {
        guard let b = URL(string: base) else { return nil }
        viaURL = c.url(relativeTo: b)?.absoluteURL.absoluteString
        guard let h = viaURL else { return nil }
        return Parsed(href: h, host: c.percentEncodedHost, path: c.percentEncodedPath, query: c.percentEncodedQuery,
                      extra: "{\"relative_via\":\"url(relativeTo:)\"}")
    }
    viaURL = c.url?.absoluteString
    guard let s = c.string else { return nil }
    return Parsed(href: s, host: c.percentEncodedHost, path: c.percentEncodedPath, query: c.percentEncodedQuery,
                  extra: "{\"url_absoluteString\":" + (viaURL.map(jsonString) ?? "null") + "}")
}

for line in text.split(separator: "\n") {
    let p = line.split(separator: "\t", omittingEmptySubsequences: false)
    guard p.count == 3, let input = dec(p[1]) else { continue }
    let base = dec(p[2])
    let r = mode == "url" ? parseURL(input, base) : parseComponents(input, base)
    if let r = r {
        out += "R\t\(p[0])\t1\t\(enc(r.href))\t\(enc(r.host))\t\(enc(r.path))\t\(enc(r.query))\t~\t\(enc(r.extra))\n"
    } else {
        out += "R\t\(p[0])\t0\t~\t~\t~\t~\t\(enc(mode == "url" ? "URL(string:) returned nil" : "URLComponents(string:) returned nil"))\t~\n"
    }
}
FileHandle.standardOutput.write(Data(out.utf8))
