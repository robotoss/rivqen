// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

/// A lab failure with a stable code and a context text. The text never holds secrets: the lab
/// reads only test data (case files, the lab server on 127.0.0.1).
public struct LabError: Error, Equatable, Sendable, CustomStringConvertible {
  public enum Code: String, Sendable {
    case argument = "ARGUMENT"
    case caseInvalid = "CASE_INVALID"
    case indexInvalid = "INDEX_INVALID"
    case tooLarge = "TOO_LARGE"
    case io = "IO"
    case resultInvalid = "RESULT_INVALID"
  }

  public let code: Code
  public let message: String

  public init(_ code: Code, _ message: String) {
    self.code = code
    self.message = message
  }

  public var description: String { "\(code.rawValue): \(message)" }
}
