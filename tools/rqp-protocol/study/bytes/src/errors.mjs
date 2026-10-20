// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

/** An error of the byte study, with a stable code (errors-logging ERR-03). */
export class StudyError extends Error {
  constructor(code, message, options) {
    super(message, options);
    this.name = 'StudyError';
    this.code = code;
  }
}
