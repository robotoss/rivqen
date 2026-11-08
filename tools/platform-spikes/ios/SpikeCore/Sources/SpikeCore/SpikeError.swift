// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

/// An error of the prototype. The message never holds page data, only the reason.
public struct SpikeError: Error, Sendable, Equatable, CustomStringConvertible {
  public enum Kind: String, Sendable {
    case argument
    case probe
    case tooLarge
    case url
  }

  public let kind: Kind
  public let message: String

  public init(_ kind: Kind, _ message: String) {
    self.kind = kind
    self.message = message
  }

  public var description: String { "\(kind.rawValue): \(message)" }
}
