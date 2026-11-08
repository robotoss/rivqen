// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import Foundation

/// The own URL scheme of IOS-P3 and its mapping to the spike host.
///
/// `rqlab://<host>:<port>/<path>?<query>` maps to `https://<host>:<port>/<path>?<query>` when the
/// host and the port are those of a lab origin. Every other URL is refused (decision `block`), so
/// the scheme handler never becomes an open proxy.
public enum LabScheme {
  /// Scheme name: lower case, starts with a letter, holds the product name
  /// ([setURLSchemeHandler](https://developer.apple.com/documentation/webkit/wkwebviewconfiguration/seturlschemehandler(_:forurlscheme:))).
  public static let name = "rqlab"

  /// The https URL that a custom-scheme URL stands for, or a reason to refuse it.
  public static func httpsURL(for schemeURL: URL, port: Int) -> Result<URL, SpikeError> {
    guard let parts = URLComponents(url: schemeURL, resolvingAgainstBaseURL: false) else {
      return .failure(SpikeError(.url, "not a URL"))
    }
    guard parts.scheme == name else { return .failure(SpikeError(.url, "scheme is not \(name)")) }
    guard parts.user == nil, parts.password == nil else {
      return .failure(SpikeError(.url, "user or password in the URL"))
    }
    guard let host = parts.host, LabOrigin.allowedHosts.contains(host) else {
      return .failure(SpikeError(.url, "host is not a lab host"))
    }
    guard parts.port == port else { return .failure(SpikeError(.url, "port is not the lab port")) }
    let path = parts.percentEncodedPath
    guard path.hasPrefix("/"), !path.hasPrefix("//") else {
      return .failure(SpikeError(.url, "path must start with one /"))
    }
    var out = URLComponents()
    out.scheme = "https"
    out.host = host
    out.port = port
    out.percentEncodedPath = path
    out.percentEncodedQuery = parts.percentEncodedQuery
    guard let url = out.url else { return .failure(SpikeError(.url, "cannot build the https URL")) }
    return .success(url)
  }
}

/// The origin rule for probe messages: the frame that posts must have a lab origin (`https` or
/// `rqlab`, a lab host, the lab port). This is the iOS form of the origin rule that Android gives
/// `addWebMessageListener`.
public enum ProbeOriginRule {
  public static func allows(scheme: String, host: String, port: Int, labPort: Int) -> Bool {
    (scheme == "https" || scheme == LabScheme.name) && LabOrigin.allowedHosts.contains(host)
      && port == labPort
  }
}
