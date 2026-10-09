// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.tools.markup;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.ValueSource;

/** M-25: JSON text by RFC 8259 §2, depth at most 64. */
class JsonTextTest {

  private static boolean valid(String s) {
    byte[] b = Docs.utf8("<<" + s + ">>");
    return JsonText.isValid(b, 2, b.length - 2);
  }

  @ParameterizedTest
  @ValueSource(
      strings = {
        "0",
        "-0",
        "1",
        "-12.5e+3",
        "1E400",
        "0.5",
        "1e-7",
        "123456789012345678901234567890",
        "true",
        "false",
        "null",
        "\"\"",
        "\"a\\\"\\\\\\/\\b\\f\\n\\r\\t\\u00e9\\uD83D\\uDE00\"",
        "\"é😀\"",
        "[]",
        "{}",
        "[1,2,[3,{}]]",
        " \t\r\n{ \"a\" : [ 1 , true ] , \"a\" : null } \n",
        "{\"\":{\"\":[]}}",
        "[\"\\u0000\"]"
      })
  void validTexts(String s) {
    assertTrue(valid(s), s);
  }

  @ParameterizedTest
  @ValueSource(
      strings = {
        "",
        " ",
        "[1,]",
        "{\"a\":1,}",
        "[1 2]",
        "{\"a\" 1}",
        "{\"a\":}",
        "{a:1}",
        "{1:1}",
        "[",
        "]",
        "{",
        "[1",
        "{\"a\":1",
        "{\"a\"",
        "01",
        "-",
        "+1",
        ".5",
        "1.",
        "1.e1",
        "1e",
        "1e+",
        "0x1",
        "NaN",
        "Infinity",
        "tru",
        "nul",
        "fals",
        "trueX",
        "\"a",
        "\"\\x\"",
        "\"\\u12\"",
        "\"\\u12G4\"",
        "\"\\uD800\"",
        "\"\\uD800\\u0041\"",
        "\"\\uD800x\"",
        "\"\\uDC00\"",
        "\"\\uDBFF\\uDBFF\"",
        "\"\\",
        "\"\t\"",
        "\"\n\"",
        "\u00a0[]",
        "\uFEFF[]",
        "[]\u00a0",
        "[] []",
        "1 2",
        "{\"a\":1}x",
        "[-]",
        "[1,,2]",
        "[,1]"
      })
  void invalidTexts(String s) {
    assertFalse(valid(s), s);
  }

  @Test
  void depthSixtyFourIsTheMaximum() {
    assertTrue(valid("[".repeat(64) + "]".repeat(64)));
    assertFalse(valid("[".repeat(65) + "]".repeat(65)));
    assertTrue(valid("{\"a\":".repeat(63) + "[]" + "}".repeat(63)));
    assertFalse(valid("{\"a\":".repeat(64) + "[]" + "}".repeat(64)));
  }

  @Test
  void depthCountsOpenContainersOnly() {
    assertTrue(valid("[" + "[],".repeat(100) + "[" + "[".repeat(62) + "]".repeat(62) + "]]"));
  }
}
