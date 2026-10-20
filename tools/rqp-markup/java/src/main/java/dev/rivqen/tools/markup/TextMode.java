// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

/** Tokenizer states for raw text after a start tag in HTML context ({@code markup.md} M-04). */
enum TextMode {
  /** {@code title}, {@code textarea}. */
  RCDATA,
  /**
   * {@code iframe}, {@code noembed}, {@code noframes}, {@code noscript}, {@code style}, {@code
   * xmp}.
   */
  RAWTEXT,
  /** {@code script}, with the escaped and double-escaped states. */
  SCRIPT_DATA,
  /** {@code plaintext}: never ends. */
  PLAINTEXT
}
