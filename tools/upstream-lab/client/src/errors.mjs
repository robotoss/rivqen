// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

/** Error of the conductor. `code` is a stable upper-case token; `cause` keeps the original error. */
export class LabError extends Error {
  /**
   * @param {string} code
   * @param {string} message
   * @param {{ cause?: unknown }} [options]
   */
  constructor(code, message, options) {
    super(message, options);
    this.name = 'LabError';
    this.code = code;
  }
}
