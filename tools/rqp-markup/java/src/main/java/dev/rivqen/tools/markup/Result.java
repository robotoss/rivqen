// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import java.util.List;
import org.jspecify.annotations.Nullable;

/**
 * The result of one document ({@code CONTRACT.md} §3).
 *
 * @param error null for a valid document
 * @param rule the rule that failed (for example {@code "M-06"}), null for a valid document; for an
 *     internal error, {@code "internal-error:"} and the exception class name; not printed, used by
 *     tests and diagnostics
 * @param blocks the blocks in document order; empty for an invalid document
 * @param templateRevision {@code t1.…}, null for an invalid document
 * @param pageRevision {@code r1.…}, null for an invalid document
 */
public record Result(
    @Nullable ErrorCode error,
    @Nullable String rule,
    List<Block> blocks,
    @Nullable String templateRevision,
    @Nullable String pageRevision) {

  /** A block of a valid document. */
  public record Block(String id, String format, int start, int end, String sha256) {}

  public Result {
    blocks = List.copyOf(blocks);
  }

  static Result invalid(ErrorCode error, String rule) {
    return new Result(error, rule, List.of(), null, null);
  }

  /** True when the document is valid for RQP. */
  public boolean valid() {
    return error == null;
  }

  /**
   * True when the result comes from an internal error (a defect) at the boundary of {@link
   * Analyzer#analyze}. The result is invalid (fail closed); the command exits with code 3.
   */
  public boolean internalError() {
    return rule != null && rule.startsWith(Analyzer.INTERNAL_ERROR);
  }
}
