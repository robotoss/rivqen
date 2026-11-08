// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import Foundation

/// Finds the chunk markers `data-k="<k>"` of `/s/stream` (CONTRACT.md section 3) in a byte stream
/// that arrives in reads of any size. A marker can be split across reads. Memory is constant: the
/// scanner keeps only its match position and the digits so far.
public struct ChunkMarkerScanner: Sendable {
  private static let pattern = Array("data-k=\"".utf8)
  /// A chunk index has at most 7 digits (the host allows 1024 chunks).
  static let maxDigits = 7

  private var matched = 0
  private var digits = 0
  private var value = 0
  /// The `k` of the last complete marker, or `nil` before the first one.
  public private(set) var lastK: Int?

  public init() {}

  /// Feeds one read and returns `lastK` after it.
  @discardableResult
  public mutating func feed<Bytes: Sequence>(_ bytes: Bytes) -> Int? where Bytes.Element == UInt8 {
    for byte in bytes { step(byte) }
    return lastK
  }

  private mutating func step(_ byte: UInt8) {
    let pattern = Self.pattern
    if matched < pattern.count {
      if byte == pattern[matched] {
        matched += 1
      } else {
        // The pattern has no border, so a mismatch restarts at 0 with this byte.
        matched = byte == pattern[0] ? 1 : 0
      }
      return
    }
    // Inside the digits after `data-k="`.
    if byte >= 0x30, byte <= 0x39, digits < Self.maxDigits {
      value = value * 10 + Int(byte - 0x30)
      digits += 1
      return
    }
    if byte == 0x22, digits > 0 { lastK = value }
    matched = byte == pattern[0] ? 1 : 0
    digits = 0
    value = 0
  }
}

/// Splits a read into parts for `WKURLSchemeTask.didReceive(_:)`. Streaming chunks of the core are
/// 8 to 64 KiB ([streaming](/engineering/core/streaming)); the prototype passes at most
/// `maxBytes` per call and keeps the byte order.
public enum ChunkSplitter {
  public static let defaultMaxBytes = 16 * 1024
  public static let limit = 1...(1 << 20)

  /// Parts of at most `maxBytes` (clamped to `limit`) that concatenate to `data`. Empty data gives
  /// no part.
  public static func parts(of data: Data, maxBytes: Int = defaultMaxBytes) -> [Data] {
    let size = min(max(maxBytes, limit.lowerBound), limit.upperBound)
    var out: [Data] = []
    out.reserveCapacity(data.count / size + 1)
    var start = data.startIndex
    while start < data.endIndex {
      let end = data.index(start, offsetBy: size, limitedBy: data.endIndex) ?? data.endIndex
      out.append(data.subdata(in: start..<end))
      start = end
    }
    return out
  }
}
