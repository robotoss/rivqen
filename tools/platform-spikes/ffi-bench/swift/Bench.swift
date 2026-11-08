// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Measurement core of the Swift harness (CONTRACT.md section 10). It uses only Darwin,
// Foundation and CryptoKit, so S2 can run it in the iOS simulator.
//
// Timing: the time of a document is the sum of the timed segments open, every read (also the
// last read that reports the end) and close (for UniFFI: the release that runs `deinit`). The
// SHA-256 update of a chunk runs between two segments and is not timed. Clock:
// `clock_gettime_nsec_np(CLOCK_UPTIME_RAW)`.

import CryptoKit
import Darwin
import Foundation
import RqBenchC

enum SwiftPath: String, CaseIterable {
  /// UniFFI. Rust copies the slice into a `Vec` (counted by Rust), the UniFFI scaffolding writes
  /// it into a `RustBuffer` (one copy); the generated `FfiConverterData.read` copies it into a
  /// `[UInt8]` (`readBytes`) and then into a new `Data` (two copies).
  case uniffiSwift = "uniffi-swift"
  /// C header. Rust copies into a buffer that Swift allocated (counted by Rust).
  case cSwift = "c-swift"

  var glueCopiesPerByte: UInt64 {
    switch self {
    case .uniffiSwift: return 3
    case .cSwift: return 0
    }
  }
}

struct BenchFailure: Error, CustomStringConvertible {
  let description: String
}

/// Largest chunk and document of CONTRACT.md section 10.
let maxChunk: UInt32 = 65_536
let maxSize: UInt64 = 256 * 1024 * 1024
let maxChunkTimes = 1 << 24

struct CaseResult {
  let path: String
  let size: UInt64
  let chunk: UInt32
  let docs: Int
  let medianNs: UInt64
  let p95Ns: UInt64
  let chunkMedianNs: UInt64
  let copiedBytes: UInt64
  let rustCopiedBytes: UInt64
  let glueCopiedBytes: UInt64
  let rssDeltaKib: Int64
  let sha256Ok: Bool

  var json: String {
    "{\"chunk\":\(chunk),\"chunk_median_ns\":\(chunkMedianNs),\"copied_bytes\":\(copiedBytes),"
      + "\"docs\":\(docs),\"glue_copied_bytes\":\(glueCopiedBytes),\"median_ns\":\(medianNs),"
      + "\"p95_ns\":\(p95Ns),\"path\":\"\(path)\",\"rss_delta_kib\":\(rssDeltaKib),"
      + "\"rust_copied_bytes\":\(rustCopiedBytes),\"sha256_ok\":\(sha256Ok),\"size\":\(size)}\n"
  }
}

/// Nearest-rank percentile `p` (1...100) of `values`.
func percentile(_ values: ArraySlice<UInt64>, _ p: Int) throws -> UInt64 {
  guard !values.isEmpty, (1...100).contains(p) else {
    throw BenchFailure(description: "bad percentile input")
  }
  let sorted = values.sorted()
  let rank = (p * sorted.count + 99) / 100  // ceil(p * n / 100)
  return sorted[rank - 1]
}

/// SHA-256 of the document computed in Swift from the formula of CONTRACT.md section 10.
func swiftDocSha256(size: UInt64) -> [UInt8] {
  var hasher = SHA256()
  var block = [UInt8](repeating: 0, count: 65_536)
  var i: UInt64 = 0
  while i < size {
    let n = Int(min(UInt64(block.count), size - i))
    for k in 0..<n {
      block[k] = UInt8(((i + UInt64(k)) % 251 * 31 + 7) % 251)
    }
    block.withUnsafeBytes {
      hasher.update(bufferPointer: UnsafeRawBufferPointer(rebasing: $0[0..<n]))
    }
    i += UInt64(n)
  }
  return Array(hasher.finalize())
}

/// Peak resident memory of the process in bytes (`getrusage`, `ru_maxrss` is in bytes on Darwin).
func peakRssBytes() -> Int64? {
  var usage = rusage()
  guard getrusage(RUSAGE_SELF, &usage) == 0 else { return nil }
  return Int64(exactly: usage.ru_maxrss)
}

@inline(__always)
func now() -> UInt64 {
  clock_gettime_nsec_np(CLOCK_UPTIME_RAW)
}

/// The Rust SHA-256 of the document through the C ABI.
func cDocSha256(size: UInt64) -> [UInt8]? {
  var out = [UInt8](repeating: 0, count: 32)
  let rc = out.withUnsafeMutableBufferPointer { rq_bench_sha256(size, $0.baseAddress, $0.count) }
  return rc == RQ_BENCH_OK ? out : nil
}

final class SwiftBench {
  private let path: SwiftPath
  private let size: UInt64
  private let chunk: UInt32
  private let expected: [UInt8]
  private let buffer: UnsafeMutableBufferPointer<UInt8>

  init(path: SwiftPath, size: UInt64, chunk: UInt32) throws {
    // Bounds of CONTRACT.md section 10; checked first, so no buffer is made for a bad value.
    guard (1...maxChunk).contains(chunk), (1...maxSize).contains(size) else {
      throw BenchFailure(description: "chunk must be 1...\(maxChunk) and size 1...\(maxSize)")
    }
    self.path = path
    self.size = size
    self.chunk = chunk
    let rust: [UInt8]?
    switch path {
    case .uniffiSwift: rust = docSha256(size: size).map { Array($0) }
    case .cSwift: rust = cDocSha256(size: size)
    }
    guard let rust else { throw BenchFailure(description: "no Rust SHA-256 for size \(size)") }
    guard rust == swiftDocSha256(size: size) else {
      throw BenchFailure(description: "Rust SHA-256 differs from the Swift SHA-256 of the formula")
    }
    expected = rust
    buffer = .allocate(capacity: Int(chunk))
    buffer.initialize(repeating: 0)
  }

  deinit {
    buffer.deallocate()
  }

  func run(warmup: Int, docs: Int) throws -> CaseResult {
    guard warmup >= 0, docs > 0 else { throw BenchFailure(description: "bad document counts") }
    let maxChunks = Int((size + UInt64(chunk) - 1) / UInt64(chunk))
    // At most 2^24 chunk times (128 MiB), so the arrays stay bounded.
    let (cells, overflow) = docs.multipliedReportingOverflow(by: maxChunks)
    guard !overflow, cells <= maxChunkTimes, warmup <= maxChunkTimes - docs else {
      throw BenchFailure(description: "too many documents for size \(size) and chunk \(chunk)")
    }
    var docNs = [UInt64](repeating: 0, count: docs)
    var chunkNs = [UInt64](repeating: 0, count: docs * maxChunks)
    var chunkCount = 0
    var copied: (rust: UInt64, glue: UInt64)? = nil
    guard let rssBefore = peakRssBytes() else { throw BenchFailure(description: "no peak RSS") }
    for d in 0..<(warmup + docs) {
      let measured = d >= warmup
      let r = try oneDocument(chunkNs: &chunkNs, record: measured, at: chunkCount)
      guard r.bytes == size else {
        throw BenchFailure(description: "document \(d): \(r.bytes) bytes")
      }
      guard r.rustCopied == size else {
        throw BenchFailure(description: "document \(d): Rust copied \(r.rustCopied)")
      }
      let docCopied = (rust: r.rustCopied, glue: path.glueCopiesPerByte * r.bytes)
      if let c = copied, c != docCopied { throw BenchFailure(description: "copied bytes changed") }
      copied = docCopied
      if measured {
        docNs[d - warmup] = r.docNs
        chunkCount += r.chunks
      }
    }
    guard let rssAfter = peakRssBytes(), let copied else {
      throw BenchFailure(description: "no peak RSS")
    }
    return CaseResult(
      path: path.rawValue, size: size, chunk: chunk, docs: docs,
      medianNs: try percentile(docNs[...], 50),
      p95Ns: try percentile(docNs[...], 95),
      chunkMedianNs: try percentile(chunkNs[0..<chunkCount], 50),
      copiedBytes: copied.rust + copied.glue,
      rustCopiedBytes: copied.rust,
      glueCopiedBytes: copied.glue,
      rssDeltaKib: (rssAfter - rssBefore) / 1024,
      sha256Ok: true)
  }

  private struct DocRun {
    let bytes: UInt64
    let rustCopied: UInt64
    let docNs: UInt64
    let chunks: Int
  }

  /// Reads one document; a SHA-256 mismatch throws.
  private func oneDocument(chunkNs: inout [UInt64], record: Bool, at: Int) throws -> DocRun {
    var hasher = SHA256()
    var docNs: UInt64 = 0
    var bytes: UInt64 = 0
    var chunks = 0
    var rustCopied: UInt64 = 0
    switch path {
    case .uniffiSwift:
      var t0 = now()
      var stream: DocStream? = openStream(size: size, chunk: chunk)
      docNs += now() - t0
      guard let s = stream else { throw BenchFailure(description: "open failed") }
      while true {
        t0 = now()
        let c = s.nextChunk()
        let dt = now() - t0
        docNs += dt
        guard let c else { break }
        if record { chunkNs[at + chunks] = dt }
        chunks += 1
        bytes += UInt64(c.count)
        hasher.update(data: c)
      }
      rustCopied = s.copiedBytes()
      withExtendedLifetime(s) {}
      t0 = now()
      stream = nil  // the last reference: deinit frees the Rust object
      docNs += now() - t0
      _ = stream
    case .cSwift:
      var t0 = now()
      let h = rq_bench_open(size, chunk)
      docNs += now() - t0
      guard let h else { throw BenchFailure(description: "open failed") }
      var n = 0
      while true {
        t0 = now()
        let rc = rq_bench_read(h, buffer.baseAddress, buffer.count, &n)
        let dt = now() - t0
        docNs += dt
        if rc == RQ_BENCH_END { break }
        guard rc == RQ_BENCH_OK else { throw BenchFailure(description: "read error \(rc)") }
        if record { chunkNs[at + chunks] = dt }
        chunks += 1
        bytes += UInt64(n)
        hasher.update(bufferPointer: UnsafeRawBufferPointer(start: buffer.baseAddress, count: n))
      }
      guard rq_bench_copied(h, &rustCopied) == RQ_BENCH_OK else {
        throw BenchFailure(description: "copied failed")
      }
      t0 = now()
      let rc = rq_bench_close(h)
      docNs += now() - t0
      guard rc == RQ_BENCH_OK else { throw BenchFailure(description: "close error \(rc)") }
    }
    guard Array(hasher.finalize()) == expected else {
      throw BenchFailure(description: "SHA-256 mismatch")
    }
    return DocRun(bytes: bytes, rustCopied: rustCopied, docNs: docNs, chunks: chunks)
  }
}
