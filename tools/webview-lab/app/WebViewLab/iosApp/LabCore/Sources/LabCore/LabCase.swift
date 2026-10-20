// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import Foundation

/// The scripting view of one run (CONTRACT section 2.1).
public enum LabView: String, Sendable, CaseIterable, Codable {
  /// Content JavaScript allowed.
  case on
  /// Content JavaScript not allowed.
  case off
}

/// The part of a case file that the app needs: the id, the views and the probe selector.
/// The server owns the body and the header lines.
public struct LabCase: Equatable, Sendable {
  public let id: String
  public let views: [LabView]
  public let probe: String?

  /// The largest case file the app reads. A case file is about 300 bytes.
  public static let maxFileBytes = 64 * 1024
  /// The longest probe selector.
  public static let maxProbeLength = 256

  /// Parse and check a case file. `expectedId` is the id from the case list; the file must
  /// have the same id, so a file can not write results under another name.
  public static func parse(_ data: Data, expectedId: String) throws(LabError) -> LabCase {
    guard data.count <= maxFileBytes else {
      throw LabError(.tooLarge, "case \(expectedId): file has \(data.count) bytes")
    }
    let raw: Any
    do {
      raw = try JSONSerialization.jsonObject(with: data)
    } catch {
      throw LabError(.caseInvalid, "case \(expectedId): not JSON")
    }
    guard let object = raw as? [String: Any] else {
      throw LabError(.caseInvalid, "case \(expectedId): not an object")
    }
    guard let id = object["id"] as? String, isValidId(id), id == expectedId else {
      throw LabError(.caseInvalid, "case \(expectedId): id is missing, malformed or different")
    }
    guard let rawViews = object["views"] as? [Any], !rawViews.isEmpty,
      rawViews.count <= LabView.allCases.count
    else {
      throw LabError(.caseInvalid, "case \(id): bad views")
    }
    var views: [LabView] = []
    for item in rawViews {
      guard let text = item as? String, let view = LabView(rawValue: text), !views.contains(view)
      else {
        throw LabError(.caseInvalid, "case \(id): bad view")
      }
      views.append(view)
    }
    let probe: String?
    switch object["probe"] {
    case nil, is NSNull:
      probe = nil
    case let text as String where !text.isEmpty && text.count <= maxProbeLength:
      probe = text
    default:
      throw LabError(.caseInvalid, "case \(id): bad probe")
    }
    return LabCase(id: id, views: views, probe: probe)
  }

  /// `[A-Z0-9-]{1,64}` (CONTRACT section 2.1). The id names result files, so this check also
  /// keeps `/` and `..` out of every output path.
  public static func isValidId(_ id: String) -> Bool {
    let bytes = Array(id.utf8)
    guard (1...64).contains(bytes.count) else { return false }
    return bytes.allSatisfy { byte in
      (byte >= UInt8(ascii: "A") && byte <= UInt8(ascii: "Z"))
        || (byte >= UInt8(ascii: "0") && byte <= UInt8(ascii: "9"))
        || byte == UInt8(ascii: "-")
    }
  }

  /// The ordered case ids of `GET /cases.json` (CONTRACT section 2.2): a JSON array of valid,
  /// unique ids.
  public static func parseIndex(_ data: Data) throws(LabError) -> [String] {
    guard data.count <= maxIndexBytes else {
      throw LabError(.tooLarge, "case list has \(data.count) bytes")
    }
    let raw: Any
    do {
      raw = try JSONSerialization.jsonObject(with: data)
    } catch {
      throw LabError(.indexInvalid, "case list is not JSON")
    }
    guard let items = raw as? [Any], items.count <= maxCases else {
      throw LabError(.indexInvalid, "case list is not an array of at most \(maxCases) ids")
    }
    var ids: [String] = []
    var seen = Set<String>()
    for item in items {
      guard let id = item as? String, isValidId(id), seen.insert(id).inserted else {
        throw LabError(.indexInvalid, "case list has a malformed or repeated id")
      }
      ids.append(id)
    }
    return ids
  }

  /// The largest case list. The list of S5 has 55 ids (about 1 KiB).
  public static let maxIndexBytes = 1024 * 1024
  /// The most cases in one run.
  public static let maxCases = 10_000
}
