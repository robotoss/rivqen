# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
"""Tests for gen.py. Run: python3 -m unittest tools/rqp-markup/test_gen.py"""

from __future__ import annotations

import importlib.util
import io
import json
import tempfile
import unittest
from pathlib import Path

_SPEC = importlib.util.spec_from_file_location("rqp_markup_gen", Path(__file__).with_name("gen.py"))
assert _SPEC is not None and _SPEC.loader is not None
gen = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(gen)

# Known answer for input `<p data-rq-block="a">x</p>`, computed by hand from
# CONTRACT.md section 4 with the OpenSSL command line (not with this code):
#   printf 'x' | openssl dgst -sha256 -binary | openssl base64 -A
#   printf 'rqp-t1\n<p data-rq-block="a"></p>' | openssl dgst -sha256 -binary | openssl base64 -A
#   printf 'rqp-r1\nt1.<template hash>\na\thtml\t<block hash>\n' | openssl dgst -sha256 -binary | openssl base64 -A
# then '+' -> '-', '/' -> '_', '=' removed.
KAT_INPUT = b'<p data-rq-block="a">x</p>'
KAT_BLOCK_SHA = "LXEWQrcmsEQBYnyp-6wy9chTD7GQPMTbAiWHF5IaSIE"
KAT_TEMPLATE_REV = "t1.53osjF71zEjMw8QchPuphLyF_NPbF_0vl-jnfXKihJw"
KAT_PAGE_REV = "r1.YmTSTJyJ9ntChbQCKNRdfGFONUS604oIThZgjTT8WrU"
# Empty template, no blocks: printf 'rqp-t1\n' | openssl dgst -sha256 -binary | openssl base64 -A
EMPTY_TEMPLATE_REV = "t1.K-1prZ-JAH820HtoFSuiLT6FVsotzMU6M55HaD0iqXA"


def valid_spec(name: str, blocks: list[dict]) -> dict:
    return {
        "id": name,
        "description": "test",
        "rules": ["M-27"],
        "expect": {"valid": True, "blocks": blocks},
    }


class FixtureDir:
    """A temporary fixtures directory with helpers."""

    def __init__(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)

    def close(self) -> None:
        self._tmp.cleanup()

    def add(self, name: str, data: bytes, spec: dict) -> Path:
        d = self.root / name
        d.mkdir()
        (d / "input.html").write_bytes(data)
        (d / "fixture.json").write_text(json.dumps(spec), encoding="utf-8")
        return d

    def run(self, check: bool) -> tuple[int, str, str]:
        out, err = io.StringIO(), io.StringIO()
        code = gen.run(self.root, None, check, generated={}, out=out, err=err)
        return code, out.getvalue(), err.getvalue()


class KnownAnswerTest(unittest.TestCase):
    def test_contract_section_4_known_answer(self) -> None:
        result = gen.compute_result(KAT_INPUT, [("a", "html", 21, 22)])
        self.assertEqual(
            result,
            {
                "valid": True,
                "error": None,
                "blocks": [{"id": "a", "format": "html", "start": 21, "end": 22, "sha256": KAT_BLOCK_SHA}],
                "template_revision": KAT_TEMPLATE_REV,
                "page_revision": KAT_PAGE_REV,
            },
        )
        self.assertEqual(len(KAT_BLOCK_SHA), 43)
        self.assertEqual(len(KAT_TEMPLATE_REV), 46)

    def test_empty_template_known_answer(self) -> None:
        self.assertEqual(gen.compute_result(b"", [])["template_revision"], EMPTY_TEMPLATE_REV)

    def test_fixture_gives_known_answer_and_byte_offsets(self) -> None:
        fx = FixtureDir()
        self.addCleanup(fx.close)
        d = fx.add(
            "FX-RQ-MARKUP-KAT",
            KAT_INPUT,
            valid_spec("FX-RQ-MARKUP-KAT", [{"id": "a", "format": "html", "start_tag": '<p data-rq-block="a">', "content": "x"}]),
        )
        code, _, err = fx.run(check=False)
        self.assertEqual(code, 0, err)
        expected = json.loads((d / "expected.json").read_text(encoding="utf-8"))
        self.assertEqual(expected["page_revision"], KAT_PAGE_REV)

    def test_offsets_are_utf8_bytes(self) -> None:
        fx = FixtureDir()
        self.addCleanup(fx.close)
        # U+1F600 is 4 bytes in UTF-8 (2 units in UTF-16).
        data = '\U0001F600<p data-rq-block="a">é</p>'.encode("utf-8")
        d = fx.add(
            "FX-RQ-MARKUP-U",
            data,
            valid_spec("FX-RQ-MARKUP-U", [{"id": "a", "format": "html", "start_tag": '<p data-rq-block="a">', "content": "é"}]),
        )
        self.assertEqual(fx.run(check=False)[0], 0)
        block = json.loads((d / "expected.json").read_text(encoding="utf-8"))["blocks"][0]
        self.assertEqual((block["start"], block["end"]), (25, 27))

    def test_invalid_fixture_result(self) -> None:
        fx = FixtureDir()
        self.addCleanup(fx.close)
        spec = {"id": "FX-RQ-MARKUP-BAD", "description": "d", "rules": ["M-21"], "expect": {"valid": False, "error": "RQP_MARKUP_DUPLICATE"}}
        d = fx.add("FX-RQ-MARKUP-BAD", b"<p>", spec)
        self.assertEqual(fx.run(check=False)[0], 0)
        self.assertEqual(
            json.loads((d / "expected.json").read_text(encoding="utf-8")),
            {"valid": False, "error": "RQP_MARKUP_DUPLICATE", "blocks": [], "template_revision": None, "page_revision": None},
        )


class RefusalTest(unittest.TestCase):
    def setUp(self) -> None:
        self.fx = FixtureDir()
        self.addCleanup(self.fx.close)

    def assert_refused(self, data: bytes, blocks: list[dict], fragment: str) -> None:
        d = self.fx.add("FX-RQ-MARKUP-T", data, valid_spec("FX-RQ-MARKUP-T", blocks))
        code, _, err = self.fx.run(check=False)
        self.assertEqual(code, 1)
        self.assertIn(fragment, err)
        self.assertFalse((d / "expected.json").exists())

    def test_ambiguous_start_tag(self) -> None:
        data = b'<p data-rq-block="a">x</p><!-- <p data-rq-block="a">x</p> -->'
        self.assert_refused(data, [{"id": "a", "format": "html", "start_tag": '<p data-rq-block="a">', "content": "x"}], "ambiguous")

    def test_missing_start_tag(self) -> None:
        self.assert_refused(b"<p>x</p>", [{"id": "a", "format": "html", "start_tag": '<p data-rq-block="a">', "content": "x"}], "not found")

    def test_content_mismatch(self) -> None:
        self.assert_refused(KAT_INPUT, [{"id": "a", "format": "html", "start_tag": '<p data-rq-block="a">', "content": "y"}], "content does not follow")

    def test_content_must_end_at_end_tag(self) -> None:
        data = b'<p data-rq-block="a">xy</p>'
        self.assert_refused(data, [{"id": "a", "format": "html", "start_tag": '<p data-rq-block="a">', "content": "x"}], "followed by an end tag")

    def test_order(self) -> None:
        data = b'<p data-rq-block="a">x</p><p data-rq-block="b">y</p>'
        blocks = [
            {"id": "b", "format": "html", "start_tag": '<p data-rq-block="b">', "content": "y"},
            {"id": "a", "format": "html", "start_tag": '<p data-rq-block="a">', "content": "x"},
        ]
        self.assert_refused(data, blocks, "document order")

    def test_bad_id_and_format(self) -> None:
        self.assert_refused(KAT_INPUT, [{"id": "A", "format": "html", "start_tag": '<p data-rq-block="a">', "content": "x"}], "'id'")

    def test_unknown_error_code(self) -> None:
        spec = {"id": "FX-RQ-MARKUP-T", "description": "d", "rules": ["M-01"], "expect": {"valid": False, "error": "NOPE"}}
        self.fx.add("FX-RQ-MARKUP-T", b"", spec)
        code, _, err = self.fx.run(check=False)
        self.assertEqual(code, 1)
        self.assertIn("expect.error", err)

    def test_id_must_match_directory(self) -> None:
        self.fx.add("FX-RQ-MARKUP-T", KAT_INPUT, valid_spec("FX-RQ-MARKUP-OTHER", []))
        self.assertEqual(self.fx.run(check=False)[0], 1)

    def test_unknown_rule_id(self) -> None:
        d = self.fx.add("FX-RQ-MARKUP-T", KAT_INPUT, valid_spec("FX-RQ-MARKUP-T", []))
        with self.assertRaises(gen.FixtureError):
            gen.expected_for(d, {"M-01"})

    def test_messages_do_not_contain_content(self) -> None:
        secret = "secret-content-123"
        data = ('<p data-rq-block="a">' + secret + "</p>").encode()
        self.assert_refused(data, [{"id": "a", "format": "html", "start_tag": '<p data-rq-block="a">', "content": secret + "x"}], "content")
        self.assertNotIn(secret, self.fx.run(check=False)[2])


class CheckModeTest(unittest.TestCase):
    def setUp(self) -> None:
        self.fx = FixtureDir()
        self.addCleanup(self.fx.close)
        self.dir = self.fx.add(
            "FX-RQ-MARKUP-T",
            KAT_INPUT,
            valid_spec("FX-RQ-MARKUP-T", [{"id": "a", "format": "html", "start_tag": '<p data-rq-block="a">', "content": "x"}]),
        )

    def test_check_fails_when_missing(self) -> None:
        code, _, err = self.fx.run(check=True)
        self.assertEqual(code, 1)
        self.assertIn("STALE", err)
        self.assertFalse((self.dir / "expected.json").exists())

    def test_check_passes_after_write(self) -> None:
        self.assertEqual(self.fx.run(check=False)[0], 0)
        self.assertEqual(self.fx.run(check=True)[0], 0)

    def test_check_fails_when_stale(self) -> None:
        self.assertEqual(self.fx.run(check=False)[0], 0)
        (self.dir / "input.html").write_bytes(KAT_INPUT + b"\n")
        code, _, err = self.fx.run(check=True)
        self.assertEqual(code, 1)
        self.assertIn("STALE FX-RQ-MARKUP-T", err)

    def test_check_detects_generated_input_drift(self) -> None:
        recipe = {"FX-RQ-MARKUP-T": {"input.html": lambda: KAT_INPUT}}
        out, err = io.StringIO(), io.StringIO()
        self.assertEqual(gen.run(self.fx.root, None, False, generated=recipe, out=out, err=err), 0)
        (self.dir / "input.html").write_bytes(b"changed")
        self.assertEqual(gen.run(self.fx.root, None, True, generated=recipe, out=out, err=err), 1)
        self.assertIn("differs from its recipe", err.getvalue())

    def test_missing_fixtures_dir_is_usage_error(self) -> None:
        out, err = io.StringIO(), io.StringIO()
        self.assertEqual(gen.run(self.fx.root / "none", None, True, generated={}, out=out, err=err), 2)


if __name__ == "__main__":
    unittest.main()
