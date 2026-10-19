// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import Foundation

/// One result file `results/ios/<id>.<view>.json` (CONTRACT section 4).
public struct LabResult: Equatable, Sendable, Encodable {
  public enum Walker: String, Sendable, Encodable {
    case ok
    case unavailable
  }

  public let id: String
  public let view: LabView
  public let walker: Walker
  public let tree: String?
  public let characterSet: String?
  public let contentType: String?
  public let probe: String?
  public let error: String?

  /// The longest tree the app keeps. The largest S5 page gives about 20 KiB of tree.
  public static let maxTreeBytes = 16 * 1024 * 1024
  /// The longest error text.
  public static let maxErrorLength = 2_000

  /// The walker ran. `meta` holds `characterSet`, `contentType` and the probe text; `metaError`
  /// is set when only that second read failed.
  public static func ok(
    id: String, view: LabView, tree: String, meta: PageMeta?, metaError: String?
  ) throws(LabError) -> LabResult {
    guard LabCase.isValidId(id) else { throw LabError(.resultInvalid, "bad id") }
    guard tree.utf8.count <= maxTreeBytes else {
      return unavailable(
        id: id, view: view, reason: "tree has \(tree.utf8.count) bytes, more than \(maxTreeBytes)")
    }
    return LabResult(
      id: id, view: view, walker: .ok, tree: tree, characterSet: meta?.characterSet,
      contentType: meta?.contentType, probe: meta?.probe, error: metaError.map(clip))
  }

  /// The walker did not run or the page did not load; `reason` says why. The app never turns
  /// content JavaScript on to get a tree for view `off` (CONTRACT section 4 step 3). `meta`, when
  /// the app could still read it, tells what the WebView made of the page (for example
  /// `text/plain`); compare.mjs does not use it for an unavailable result.
  public static func unavailable(id: String, view: LabView, reason: String, meta: PageMeta? = nil)
    -> LabResult
  {
    LabResult(
      id: id, view: view, walker: .unavailable, tree: nil, characterSet: meta?.characterSet,
      contentType: meta?.contentType, probe: meta?.probe, error: clip(reason))
  }

  /// The file name. The id is checked by the factories, so the name has no path separator.
  public var fileName: String { "\(id).\(view.rawValue).json" }

  /// Stable JSON: sorted keys, `null` for an absent value (the contract shows every key).
  public func encoded() throws(LabError) -> Data {
    do {
      return try LabResult.encoder.encode(self)
    } catch {
      throw LabError(.io, "encode \(fileName): \(error.localizedDescription)")
    }
  }

  enum CodingKeys: String, CodingKey {
    case id, view, walker, tree, characterSet, contentType, probe, error
  }

  public func encode(to encoder: any Encoder) throws {
    var c = encoder.container(keyedBy: CodingKeys.self)
    try c.encode(id, forKey: .id)
    try c.encode(view, forKey: .view)
    try c.encode(walker, forKey: .walker)
    // `encode` (not `encodeIfPresent`) writes `null` for nil.
    try c.encode(tree, forKey: .tree)
    try c.encode(characterSet, forKey: .characterSet)
    try c.encode(contentType, forKey: .contentType)
    try c.encode(probe, forKey: .probe)
    try c.encode(error, forKey: .error)
  }

  static var encoder: JSONEncoder {
    let e = JSONEncoder()
    e.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
    return e
  }

  static func clip(_ text: String) -> String {
    text.count <= maxErrorLength ? text : String(text.prefix(maxErrorLength)) + "..."
  }
}

/// What the app reads besides the tree: `document.characterSet`, `document.contentType` and the
/// `textContent` of the probe element (null when the case has no probe or no element matches).
public struct PageMeta: Equatable, Sendable, Codable {
  public let characterSet: String?
  public let contentType: String?
  public let probe: String?

  public init(characterSet: String?, contentType: String?, probe: String?) {
    self.characterSet = characterSet
    self.contentType = contentType
    self.probe = probe
  }

  /// The function body for `callAsyncJavaScript`; the argument `probe` is the selector or null.
  /// It returns a JSON string, so the app gets one plain value back.
  public static let script = """
    var el = probe === null ? null : document.querySelector(probe);
    return JSON.stringify({
      characterSet: document.characterSet,
      contentType: document.contentType,
      probe: el === null ? null : el.textContent
    });
    """

  /// The longest meta JSON. The probe text of the Q-20 pages is 12 characters.
  public static let maxBytes = 64 * 1024

  public static func decode(_ text: String) throws(LabError) -> PageMeta {
    let data = Data(text.utf8)
    guard data.count <= maxBytes else {
      throw LabError(.tooLarge, "page meta has \(data.count) bytes")
    }
    do {
      return try JSONDecoder().decode(PageMeta.self, from: data)
    } catch {
      throw LabError(.resultInvalid, "page meta is not the expected JSON")
    }
  }
}
