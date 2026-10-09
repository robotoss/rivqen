// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

/** Error codes of {@code tools/rqp-markup/CONTRACT.md} §5. The wire form never changes. */
public enum ErrorCode {
  ENCODING("RQP_MARKUP_ENCODING"),
  LIMIT("RQP_MARKUP_LIMIT"),
  INVALID_ID("RQP_MARKUP_INVALID_ID"),
  FORBIDDEN_ELEMENT("RQP_MARKUP_FORBIDDEN_ELEMENT"),
  DUPLICATE("RQP_MARKUP_DUPLICATE"),
  NESTED("RQP_MARKUP_NESTED"),
  STRUCTURE("RQP_MARKUP_STRUCTURE"),
  BAD_JSON("RQP_MARKUP_BAD_JSON"),
  RESERVED("RQP_MARKUP_RESERVED");

  private final String wire;

  ErrorCode(String wire) {
    this.wire = wire;
  }

  /** The stable code as it is printed in the result object. */
  public String wire() {
    return wire;
  }
}
