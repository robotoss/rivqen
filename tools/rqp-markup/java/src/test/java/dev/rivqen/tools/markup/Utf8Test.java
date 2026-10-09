// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.HexFormat;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.ValueSource;

/** M-02: RFC 3629 UTF-8. */
class Utf8Test {

  @ParameterizedTest
  @ValueSource(
      strings = {
        "",
        "00417f",
        "c280",
        "dfbf",
        "e0a080",
        "ed9fbf",
        "ee8080",
        "efbfbf",
        "f0908080",
        "f48fbfbf",
        "f3bfbfbf",
        "efbbbf41"
      })
  void validSequences(String hex) {
    assertTrue(Utf8.isValid(HexFormat.of().parseHex(hex)));
  }

  @ParameterizedTest
  @ValueSource(
      strings = {
        "80", // continuation byte alone
        "bf",
        "c0af", // overlong two-byte
        "c1bf",
        "c2", // truncated
        "c241", // bad continuation
        "c2c0",
        "e09f80", // overlong three-byte
        "eda080", // surrogate
        "edbfbf",
        "e282", // truncated at the end
        "e28241",
        "e2c0a0",
        "e282c0",
        "f08f8080", // overlong four-byte
        "f4908080", // above U+10FFFF
        "f0908041",
        "f09080",
        "f5808080",
        "f8",
        "ff",
        "41e2"
      })
  void invalidSequences(String hex) {
    assertFalse(Utf8.isValid(HexFormat.of().parseHex(hex)));
  }
}
