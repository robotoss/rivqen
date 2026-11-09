// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import Testing

@testable import SpikeCore

@Suite struct RecordFlagTests {
  func args(_ extra: [String]) -> [String] {
    let base = [
      "/app", "-rqRun", "CAP-I-05-ios-27.0-4", "-rqCap", "CAP-I-05",
      "-rqOrigin", "https://localhost:18443",
    ]
    return base + extra
  }

  @Test func normalRunsAreNotRecorded() throws {
    let c = try #require(try LaunchConfig.parse(args([])))
    #expect(c.record == false)
  }

  @Test func recordOneMarksTheRun() throws {
    let c = try #require(try LaunchConfig.parse(args(["-rqRecord", "1"])))
    #expect(c.record == true)
    #expect(c.capability == .schemeStreaming)
  }

  @Test(arguments: ["0", "true", "", "11"])
  func otherRecordValuesAreRefused(_ value: String) {
    #expect(throws: SpikeError.self) { try LaunchConfig.parse(self.args(["-rqRecord", value])) }
  }

  @Test func recordNeedsAValueAndComesOnce() {
    #expect(throws: SpikeError.self) { try LaunchConfig.parse(self.args(["-rqRecord"])) }
    #expect(throws: SpikeError.self) {
      try LaunchConfig.parse(self.args(["-rqRecord", "1", "-rqRecord", "1"]))
    }
  }
}
