// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Swift harness CLI (run.sh starts one process per case, so the peak RSS of a case is its own):
//   swift-bench --selfcheck [--status <file>]
//   swift-bench --path <id> --size N --chunk N --warmup N --docs N --out <case.json> [--status <file>]
// Exit 0 on success; 1 when a check fails (also a SHA-256 mismatch); 2 for a usage error.
// The same code is the iOS simulator app (run.sh ios-sim). `simctl launch` does not give the
// exit code, so --status writes "ok", "FAIL: ..." or "usage: ..." to a file. On iOS a file
// name without a leading "/" is in the Documents folder of the app (its data container).

import Foundation
import RqBenchC

func check(_ ok: Bool, _ what: String) throws {
  if !ok { throw BenchFailure(description: "self-check: \(what)") }
}

/// Error paths of the C ABI and the UniFFI path, and the percentile helper.
func selfCheck() throws {
  var buf = [UInt8](repeating: 0, count: 16)
  func read(_ h: OpaquePointer?, cap: Int? = nil) -> (Int32, Int) {
    var n = -1
    let rc = buf.withUnsafeMutableBufferPointer {
      rq_bench_read(h, $0.baseAddress, cap ?? $0.count, &n)
    }
    return (rc, n)
  }
  // Null and forged handles: an error code, not a crash.
  let forged = OpaquePointer(bitPattern: 0xdead_beef)
  for h in [nil, forged] {
    try check(read(h) == (RQ_BENCH_E_HANDLE, 0), "read bad handle")
    try check(rq_bench_close(h) == RQ_BENCH_E_HANDLE, "close bad handle")
  }
  try check(rq_bench_open(1, 0) == nil, "open chunk 0")
  try check(rq_bench_open(1, 65_537) == nil, "open chunk 65537")
  try check(rq_bench_open(UInt64.max, 1) == nil, "open size")
  // Short last chunk, end, close twice, read after close.
  let h = rq_bench_open(10, 4)
  try check(h != nil, "open")
  try check(read(h, cap: 0) == (RQ_BENCH_E_ARG, 0), "cap 0")
  try check(read(h) == (RQ_BENCH_OK, 4), "first read")
  try check(Array(buf[0..<4]) == [7, 38, 69, 100], "first bytes")
  try check(read(h) == (RQ_BENCH_OK, 4), "second read")
  try check(read(h) == (RQ_BENCH_OK, 2), "short last chunk")
  try check(read(h) == (RQ_BENCH_END, 0), "end")
  try check(read(h) == (RQ_BENCH_END, 0), "end again")
  try check(rq_bench_close(h) == RQ_BENCH_OK, "close")
  try check(rq_bench_close(h) == RQ_BENCH_E_HANDLE, "close twice")
  try check(read(h) == (RQ_BENCH_E_HANDLE, 0), "read after close")
  try check(cDocSha256(size: UInt64.max) == nil, "sha size limit")
  // UniFFI: a bad open is an empty stream.
  try check(openStream(size: 10, chunk: 0).nextChunk() == nil, "uniffi bad open")
  try check(openStream(size: 5, chunk: 4).nextChunk()?.count == 4, "uniffi first chunk")
  try check(docSha256(size: UInt64.max) == nil, "uniffi sha size limit")
  // Percentiles (nearest rank).
  let v = (1...1000).reversed().map { UInt64($0) }
  try check(try percentile(v[...], 50) == 500 && percentile(v[...], 95) == 950, "percentile 1000")
  try check(try percentile([3, 1, 2][...], 50) == 2, "percentile odd")
  try check(try percentile([4, 1, 3, 2][...], 50) == 2, "percentile even")
  try check(peakRssBytes() ?? 0 > 0, "peak RSS")
}

func parse(_ argv: [String]) throws -> [String: String] {
  var m: [String: String] = [:]
  var i = 0
  while i < argv.count {
    let k = argv[i]
    guard k.hasPrefix("--") else { throw BenchFailure(description: "unexpected \(k)") }
    if k == "--selfcheck" {
      m["selfcheck"] = "1"
      i += 1
      continue
    }
    guard i + 1 < argv.count else { throw BenchFailure(description: "\(k) needs a value") }
    m[String(k.dropFirst(2))] = argv[i + 1]
    i += 2
  }
  return m
}

/// The path of an output file: on iOS a relative name is in the Documents folder of the app.
func outputPath(_ name: String) -> String {
  #if os(iOS)
    if !name.hasPrefix("/"),
      let docs = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first
    {
      return docs.appendingPathComponent(name).path
    }
  #endif
  return name
}

/// Writes the status file (when --status is set), prints `line` and exits with `code`.
func finish(_ status: String?, _ code: Int32, _ line: String) -> Never {
  if let status {
    do {
      try "\(line)\n".write(toFile: outputPath(status), atomically: true, encoding: .utf8)
    } catch {
      FileHandle.standardError.write(Data("FAIL: status file: \(error)\n".utf8))
      exit(1)
    }
  }
  if code == 0 {
    print(line)
  } else {
    FileHandle.standardError.write(Data("\(line)\n".utf8))
  }
  exit(code)
}

let options: [String: String]
do {
  options = try parse(Array(CommandLine.arguments.dropFirst()))
} catch {
  finish(nil, 2, "usage: \(error)")
}
let status = options["status"]

func usage(_ message: String) -> Never {
  finish(status, 2, "usage: \(message)")
}

do {
  if options["selfcheck"] != nil {
    try selfCheck()
    finish(status, 0, "ok")
  } else {
    guard let path = options["path"].flatMap(SwiftPath.init(rawValue:)) else { usage("--path") }
    guard let size = options["size"].flatMap(UInt64.init) else { usage("--size") }
    guard let chunk = options["chunk"].flatMap(UInt32.init) else { usage("--chunk") }
    guard let out = options["out"] else { usage("--out") }
    let warmup = options["warmup"].flatMap(Int.init) ?? 200
    let docs = options["docs"].flatMap(Int.init) ?? 1000
    let r = try SwiftBench(path: path, size: size, chunk: chunk).run(warmup: warmup, docs: docs)
    try r.json.write(toFile: outputPath(out), atomically: true, encoding: .utf8)
    print("\(r.path) chunk \(r.chunk): median \(r.medianNs) ns, p95 \(r.p95Ns) ns")
    finish(status, 0, "ok")
  }
} catch {
  finish(status, 1, "FAIL: \(error)")
}
