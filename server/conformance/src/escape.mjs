// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Escaping of every text that goes to a terminal (stdout or stderr). A text can
// hold response bytes or a name from the other side. C0 and C1 controls, the
// line and paragraph separators and the bidirectional controls are written as
// \uXXXX, so the terminal cannot act on them or hide text (DL-078, I1).
// The JSON report keeps raw strings.

const UNSAFE = /[\u0000-\u001f\u007f-\u009f\u2028\u2029\u202a-\u202e\u2066-\u2069]/g;

/** The text with every unsafe character written as \uXXXX. Idempotent. */
export function escapeText(s) {
  return String(s).replace(UNSAFE, (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, '0')}`);
}
