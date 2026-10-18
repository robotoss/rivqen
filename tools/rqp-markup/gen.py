#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
"""Generate expected.json for the RQP markup fixtures.

This tool is an independent oracle. It does not parse HTML. A person writes
the expected blocks (start tag text and content) in fixture.json; this tool
finds the bytes, computes offsets, hashes and revisions
(tools/rqp-markup/CONTRACT.md section 4) and writes expected.json.

Usage:
    python3 tools/rqp-markup/gen.py            # write expected.json files
    python3 tools/rqp-markup/gen.py --check    # fail when a file is stale

Python 3 standard library only. Messages never contain fixture content.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Callable

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_FIXTURES = ROOT / "fixtures" / "rqp" / "markup"
DEFAULT_MARKUP = ROOT / "docs" / "engineering" / "protocol" / "markup.md"

FIXTURE_PREFIX = "FX-RQ-MARKUP-"
ID_RE = re.compile(r"\A[a-z0-9][a-z0-9_-]{0,63}\Z")
RULE_RE = re.compile(r"\AM-[0-9]{2}\Z")
RULE_DEF_RE = re.compile(r"\*\*(M-[0-9]{2}) ")
FORMATS = ("html", "json")
ERROR_CODES = (
    "RQP_MARKUP_ENCODING",
    "RQP_MARKUP_LIMIT",
    "RQP_MARKUP_INVALID_ID",
    "RQP_MARKUP_FORBIDDEN_ELEMENT",
    "RQP_MARKUP_DUPLICATE",
    "RQP_MARKUP_NESTED",
    "RQP_MARKUP_STRUCTURE",
    "RQP_MARKUP_BAD_JSON",
    "RQP_MARKUP_RESERVED",
)
# Generous bound for a hand-written fixture.json; fixture inputs have their own bound.
MAX_FIXTURE_JSON_BYTES = 4 * 1024 * 1024
MAX_INPUT_BYTES = 8 * 1024 * 1024


class FixtureError(Exception):
    """A fixture is malformed. The message names the fixture, never its content."""


def b64u(data: bytes) -> str:
    """base64url without padding (RFC 4648 section 5)."""
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode("ascii")


def sha256_b64u(data: bytes) -> str:
    return b64u(hashlib.sha256(data).digest())


def compute_result(data: bytes, blocks: list[tuple[str, str, int, int]]) -> dict:
    """Result object of CONTRACT.md section 3 for a valid document.

    blocks: (id, format, start, end) in document order, non-overlapping.
    """
    out_blocks = []
    template_parts = []
    last = 0
    for block_id, fmt, start, end in blocks:
        if not (last <= start <= end <= len(data)):
            raise ValueError("block ranges must be ordered and inside the input")
        template_parts.append(data[last:start])
        last = end
        out_blocks.append(
            {
                "id": block_id,
                "format": fmt,
                "start": start,
                "end": end,
                "sha256": sha256_b64u(data[start:end]),
            }
        )
    template_parts.append(data[last:])
    template = b"".join(template_parts)
    template_revision = "t1." + sha256_b64u(b"rqp-t1\n" + template)
    preimage = [b"rqp-r1\n", template_revision.encode("ascii"), b"\n"]
    for b in out_blocks:
        preimage.append(
            b["id"].encode("ascii")
            + b"\t"
            + b["format"].encode("ascii")
            + b"\t"
            + b["sha256"].encode("ascii")
            + b"\n"
        )
    page_revision = "r1." + sha256_b64u(b"".join(preimage))
    return {
        "valid": True,
        "error": None,
        "blocks": out_blocks,
        "template_revision": template_revision,
        "page_revision": page_revision,
    }


def invalid_result(code: str) -> dict:
    return {
        "valid": False,
        "error": code,
        "blocks": [],
        "template_revision": None,
        "page_revision": None,
    }


def serialize(result: dict) -> bytes:
    return (json.dumps(result, indent=2, ensure_ascii=False) + "\n").encode("utf-8")


def find_unique(data: bytes, needle: bytes) -> int:
    """Offset of the only occurrence of needle (overlapping matches count)."""
    first = data.find(needle)
    if first < 0:
        raise LookupError("not found")
    if data.find(needle, first + 1) >= 0:
        raise LookupError("ambiguous")
    return first


def read_bounded(path: Path, limit: int) -> bytes:
    with path.open("rb") as fh:
        data = fh.read(limit + 1)
    if len(data) > limit:
        raise FixtureError(f"{path.name} is larger than {limit} bytes")
    return data


def load_rule_ids(markup: Path | None) -> set[str] | None:
    if markup is None or not markup.is_file():
        return None
    return set(RULE_DEF_RE.findall(markup.read_text(encoding="utf-8")))


def expected_for(fixture_dir: Path, rule_ids: set[str] | None) -> bytes:
    """Validate one fixture directory and return the expected.json bytes."""
    name = fixture_dir.name
    try:
        spec = json.loads(read_bounded(fixture_dir / "fixture.json", MAX_FIXTURE_JSON_BYTES))
    except FileNotFoundError:
        raise FixtureError(f"{name}: fixture.json is missing") from None
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise FixtureError(f"{name}: fixture.json is not valid JSON ({exc.__class__.__name__})") from None
    try:
        data = read_bounded(fixture_dir / "input.html", MAX_INPUT_BYTES)
    except FileNotFoundError:
        raise FixtureError(f"{name}: input.html is missing") from None

    if not isinstance(spec, dict):
        raise FixtureError(f"{name}: fixture.json must be an object")
    if spec.get("id") != name:
        raise FixtureError(f"{name}: field 'id' must equal the directory name")
    if not isinstance(spec.get("description"), str) or not spec["description"].strip():
        raise FixtureError(f"{name}: field 'description' must be a non-empty string")
    rules = spec.get("rules")
    if not isinstance(rules, list) or not rules:
        raise FixtureError(f"{name}: field 'rules' must be a non-empty list")
    for rule in rules:
        if not isinstance(rule, str) or not RULE_RE.match(rule):
            raise FixtureError(f"{name}: rule ids must look like M-01")
        if rule_ids is not None and rule not in rule_ids:
            raise FixtureError(f"{name}: rule {rule} is not defined in markup.md")
    expect = spec.get("expect")
    if not isinstance(expect, dict) or not isinstance(expect.get("valid"), bool):
        raise FixtureError(f"{name}: field 'expect.valid' must be a boolean")

    if not expect["valid"]:
        code = expect.get("error")
        if code not in ERROR_CODES:
            raise FixtureError(f"{name}: 'expect.error' must be a code of CONTRACT.md section 5")
        if "blocks" in expect:
            raise FixtureError(f"{name}: an invalid fixture has no 'expect.blocks'")
        return serialize(invalid_result(code))

    if "error" in expect and expect["error"] is not None:
        raise FixtureError(f"{name}: a valid fixture has no 'expect.error'")
    blocks_spec = expect.get("blocks")
    if not isinstance(blocks_spec, list):
        raise FixtureError(f"{name}: field 'expect.blocks' must be a list")

    blocks: list[tuple[str, str, int, int]] = []
    previous_end = 0
    seen_ids: set[str] = set()
    for index, block in enumerate(blocks_spec):
        where = f"{name}: block {index}"
        if not isinstance(block, dict):
            raise FixtureError(f"{where}: must be an object")
        block_id = block.get("id")
        if not isinstance(block_id, str) or not ID_RE.match(block_id):
            raise FixtureError(f"{where}: 'id' must match the block id syntax")
        if block_id in seen_ids:
            raise FixtureError(f"{where}: valid fixture with duplicate block id (M-21)")
        seen_ids.add(block_id)
        fmt = block.get("format")
        if fmt not in FORMATS:
            raise FixtureError(f"{where}: 'format' must be html or json")
        start_tag = block.get("start_tag")
        if not isinstance(start_tag, str) or not start_tag.startswith("<") or not start_tag.endswith(">"):
            raise FixtureError(f"{where}: 'start_tag' must be the start tag text")
        # Cheap hand-error check without parsing attributes: the id text must occur in the start tag.
        if block_id.encode("utf-8") not in start_tag.encode("utf-8"):
            raise FixtureError(f"{where}: 'id' does not occur in 'start_tag'")
        has_content = "content" in block
        has_file = "content_file" in block
        if has_content == has_file:
            raise FixtureError(f"{where}: give exactly one of 'content' and 'content_file'")
        try:
            if has_content:
                if not isinstance(block["content"], str):
                    raise FixtureError(f"{where}: 'content' must be a string")
                content = block["content"].encode("utf-8")
            else:
                file_name = block["content_file"]
                if not isinstance(file_name, str) or Path(file_name).name != file_name or file_name in (
                    "input.html",
                    "fixture.json",
                    "expected.json",
                ):
                    raise FixtureError(f"{where}: 'content_file' must be a plain file name in the fixture")
                content = read_bounded(fixture_dir / file_name, MAX_INPUT_BYTES)
        except UnicodeEncodeError:
            raise FixtureError(f"{where}: 'content' is not encodable as UTF-8") from None
        except FileNotFoundError:
            raise FixtureError(f"{where}: 'content_file' does not exist") from None

        try:
            tag_at = find_unique(data, start_tag.encode("utf-8"))
        except LookupError as exc:
            raise FixtureError(f"{where}: start_tag is {exc} in input.html (it must occur exactly once)") from None
        if tag_at < previous_end:
            raise FixtureError(f"{where}: blocks must be in document order and must not overlap")
        start = tag_at + len(start_tag.encode("utf-8"))
        end = start + len(content)
        if data[start:end] != content:
            raise FixtureError(f"{where}: content does not follow start_tag in input.html")
        if data[end : end + 2] != b"</":
            raise FixtureError(f"{where}: content must be followed by an end tag ('</')")
        blocks.append((block_id, fmt, start, end))
        previous_end = end

    return serialize(compute_result(data, blocks))


# --- Generated inputs -------------------------------------------------------
# Large fixtures are made by these recipes. The written files are committed;
# --check verifies that they still equal the recipe.

_HEAD = b'<!DOCTYPE html>\n<html><head><title>Blocks</title></head><body>\n'
_TAIL = b"</body></html>\n"


def _many_blocks(count: int) -> bytes:
    rows = b"".join(b'<p data-rq-block="b%03d">%d</p>\n' % (i, i) for i in range(count))
    return _HEAD + rows + _TAIL


def _limit_256_fixture() -> bytes:
    spec = {
        "id": "FX-RQ-MARKUP-LIMIT-256",
        "description": "256 blocks: the maximum number (generated by gen.py)",
        "rules": ["M-26", "M-27"],
        "expect": {
            "valid": True,
            "blocks": [
                {"id": "b%03d" % i, "format": "html", "start_tag": '<p data-rq-block="b%03d">' % i, "content": str(i)}
                for i in range(256)
            ],
        },
    }
    return (json.dumps(spec, indent=2) + "\n").encode("utf-8")


def _block_size() -> bytes:
    # Content of 1 048 577 bytes: one byte more than the limit of M-26.
    return _HEAD + b'<div data-rq-block="big">' + b"a" * (1024 * 1024 + 1) + b"</div>\n" + _TAIL


def _input_size() -> bytes:
    # 5 242 881 bytes: one byte more than the limit of M-01. The padding is a comment.
    total = 5 * 1024 * 1024 + 1
    head = _HEAD + b'<p data-rq-block="real">r</p>\n<!--'
    tail = b"-->\n" + _TAIL
    return head + b"a" * (total - len(head) - len(tail)) + tail


# Depth counter D (markup.md M-26, H-31). The comments give D for each recipe.
_DOCTYPE = b"<!DOCTYPE html>\n"


def _depth_doc(body: bytes) -> bytes:
    return _DOCTYPE + body + b"\n"


def _ignored_end(reps: int, extra_div: bool) -> bytes:
    # Each <span><div></span> adds 2 to D: the end tag crosses the div (no decrease). The browser
    # ignores the end tag ("any other end tag" stops at the special div) and keeps both open.
    # 300 repetitions: D = 600 before the block (LIMIT). 127 repetitions + <div> + block: D = 256.
    shape = b"<span><div></span>" * reps
    wrap_open, wrap_close = (b"<div>", b"</div>") if extra_div else (b"", b"")
    return _depth_doc(shape + wrap_open + b'<div data-rq-block="deep">x</div>' + wrap_close)


def _dirty(reps: int, clean: bool) -> bytes:
    # Not clean: each repetition adds 4. The first </span> crosses the div; the next </span> pop a
    # dirty element and do not decrease D. 64 repetitions reach 256; the 65th makes D 257 (LIMIT).
    # A counter without the dirty mark would stay at 2 per repetition (140 for 70).
    # Clean: the same elements closed in order; D goes back to 0 after each repetition.
    one = b"<span><span><span><div></div></span></span></span>" if clean else b"<span><span><span><div></span></span></span>"
    return _depth_doc(one * reps + b'<p data-rq-block="deep">x</p>')


def _nested_tables(levels: int) -> bytes:
    # Each level <table><td> adds 4: the table, an implied row group, an implied row and the cell.
    # 63 levels: D = 252, the block makes 253 (valid). 64 levels: the block makes 257 (LIMIT).
    return _depth_doc(b"<table><td>" * levels + b'<div data-rq-block="deep">x</div>' + b"</td></table>" * levels)


def _mixed_ignored(reps: int) -> bytes:
    # Shapes of the security review where the browser ignores or limits an end tag. D for one
    # repetition: 3 (h1, span, h2) + 2 (li, ul) + 2 (body, div) + 2 (form, div) = 9. 29 repetitions: 261.
    one = b"<h1><span><h2>x</h1><li><ul></li><body><div></body><form><div></form>"
    return _depth_doc(one * reps + b'<p data-rq-block="deep">x</p>')


def _noscript_depth(divs: int, spans: int = 300) -> bytes:
    # The noscript element and the tags of its content C count in D (M-11 item 8, M-26 item 7; H-71).
    # 255 divs + noscript + 300 nested spans in C: D = 556 (LIMIT). 256 divs + noscript = 257 (LIMIT)
    # before C. 155 divs + noscript + 100 spans = 256 (valid); after </noscript> D is 155 again and the
    # block makes 156. 156 divs + noscript + 100 spans = 257 (LIMIT).
    c = b"<span>" * spans + b"x" + b"</span>" * spans
    return _depth_doc(
        b"<div>" * divs + b"<noscript>" + c + b"</noscript>" + b'<div data-rq-block="deep">x</div>' + b"</div>" * divs
    )


def _noscript_icon_restore() -> bytes:
    # H-71 with H-75: in the pass of M-11 item 8 the icon crossing in C makes the noscript element
    # dirty, so that pass keeps 2 units (D 256 at most). The main pass pops the noscript element
    # exactly: D is 253 after </noscript>, and the block makes 254 (valid). A parser that keeps the
    # state of the pass would have D 256 there and reject the block (D 257).
    return _depth_doc(
        b"<div>" * 253
        + b'<noscript><svg><path d="M0"></svg></noscript><div data-rq-block="deep">x</div>'
        + b"</div>" * 253
    )


def _noscript_icons(reps: int) -> bytes:
    # H-71 with H-75: each icon in C adds 2 to D in the pass of item 8 (svg and path; the crossing
    # gives nothing back). noscript + 127 icons = 255 (valid); + 128 icons = 257 (LIMIT).
    return _depth_doc(
        b"<noscript>" + b'<svg><path d="M0"></svg>' * reps + b'</noscript><div data-rq-block="deep">x</div>'
    )


def _foreign_subtree(gs: int) -> bytes:
    return b"<svg>" + b"<g>" * gs + b"x" + b"</g>" * gs + b"</svg>"


def _content_foreign_depth(gs: int) -> bytes:
    # A foreign region in html block content (H-73): pushes in it count (M-05 item 2, M-26).
    # Block div + svg + 254 g = 256 (valid); 255 g = 257 (LIMIT).
    return _depth_doc(b'<div data-rq-block="deep">' + _foreign_subtree(gs) + b"</div>")


def _icon_depth(reps: int) -> bytes:
    # The icon crossing (H-75) sets no special-crossed mark, but D does not change: each
    # <span><svg><path></svg></span> adds 3 for the rest of the document (the </svg> crossing makes the
    # span dirty, M-26 item 5). 85 repetitions: D = 255, the block makes 256 (valid). 86: 257 (LIMIT).
    return _depth_doc(b'<span><svg><path d="M0"></svg></span>' * reps + b'<div data-rq-block="deep">x</div>')


def _units_then_block(unit: bytes, reps: int) -> bytes:
    # Security review round 2: shapes where the tree builder keeps elements open that the token stack
    # closed by exact pops, so D stays small (3 to 5) while the WHATWG stack grows by 1 per unit.
    # They are invalid by a structure rule (H-36, H-37), not by M-26.
    return _depth_doc(unit * reps + b'<div data-rq-block="d">x</div>')


GENERATED: dict[str, dict[str, Callable[[], bytes]]] = {
    "FX-RQ-MARKUP-LIMIT-INPUT-SIZE": {"input.html": _input_size},
    "FX-RQ-MARKUP-LIMIT-256": {"input.html": lambda: _many_blocks(256), "fixture.json": _limit_256_fixture},
    "FX-RQ-MARKUP-LIMIT-257": {"input.html": lambda: _many_blocks(257)},
    "FX-RQ-MARKUP-LIMIT-BLOCK-SIZE": {"input.html": _block_size},
    "FX-RQ-MARKUP-LIMIT-DEPTH-IGNORED-END": {"input.html": lambda: _ignored_end(300, False)},
    "FX-RQ-MARKUP-LIMIT-DEPTH-IGNORED-END-127": {"input.html": lambda: _ignored_end(127, True)},
    "FX-RQ-MARKUP-LIMIT-DEPTH-DIRTY": {"input.html": lambda: _dirty(70, False)},
    "FX-RQ-MARKUP-LIMIT-DEPTH-DIRTY-CLEAN": {"input.html": lambda: _dirty(70, True)},
    "FX-RQ-MARKUP-LIMIT-DEPTH-TABLE": {"input.html": lambda: _nested_tables(64)},
    "FX-RQ-MARKUP-LIMIT-DEPTH-TABLE-63": {"input.html": lambda: _nested_tables(63)},
    "FX-RQ-MARKUP-LIMIT-DEPTH-MIXED": {"input.html": lambda: _mixed_ignored(29)},
    "FX-RQ-MARKUP-LIMIT-DEPTH-NOSCRIPT": {"input.html": lambda: _noscript_depth(255)},
    "FX-RQ-MARKUP-LIMIT-DEPTH-NOSCRIPT-ELEMENT": {"input.html": lambda: _noscript_depth(256)},
    "FX-RQ-MARKUP-LIMIT-DEPTH-NOSCRIPT-CONTENT-256": {"input.html": lambda: _noscript_depth(155, 100)},
    "FX-RQ-MARKUP-LIMIT-DEPTH-NOSCRIPT-CONTENT-257": {"input.html": lambda: _noscript_depth(156, 100)},
    "FX-RQ-MARKUP-LIMIT-DEPTH-NOSCRIPT-ICON-RESTORE": {"input.html": _noscript_icon_restore},
    "FX-RQ-MARKUP-LIMIT-DEPTH-NOSCRIPT-ICONS-127": {"input.html": lambda: _noscript_icons(127)},
    "FX-RQ-MARKUP-LIMIT-DEPTH-NOSCRIPT-ICONS": {"input.html": lambda: _noscript_icons(128)},
    "FX-RQ-MARKUP-LIMIT-DEPTH-CONTENT-FOREIGN-256": {
        "input.html": lambda: _content_foreign_depth(254),
        "content.html": lambda: _foreign_subtree(254),
    },
    "FX-RQ-MARKUP-LIMIT-DEPTH-CONTENT-FOREIGN-257": {"input.html": lambda: _content_foreign_depth(255)},
    "FX-RQ-MARKUP-LIMIT-DEPTH-ICON-85": {"input.html": lambda: _icon_depth(85)},
    "FX-RQ-MARKUP-LIMIT-DEPTH-ICON": {"input.html": lambda: _icon_depth(86)},
    "FX-RQ-MARKUP-FORM-NESTED-DEPTH": {
        "input.html": lambda: _units_then_block(b"<form><object><form></form></object></form>", 600)
    },
    "FX-RQ-MARKUP-TABLE-FOSTER-MARKER-DEPTH": {
        "input.html": lambda: _units_then_block(
            b"<object><b><table><marquee><table></table></marquee></table></b></object>", 600
        )
    },
}


def fixture_dirs(root: Path) -> list[Path]:
    dirs = [p for p in root.iterdir() if p.is_dir() and p.name.startswith(FIXTURE_PREFIX)]
    return sorted(dirs, key=lambda p: p.name.encode("utf-8"))


def run(
    fixtures: Path,
    markup: Path | None,
    check: bool,
    generated: dict[str, dict[str, Callable[[], bytes]]] | None = None,
    out=sys.stdout,
    err=sys.stderr,
) -> int:
    if not fixtures.is_dir():
        print(f"error: fixtures directory not found: {fixtures}", file=err)
        return 2
    rule_ids = load_rule_ids(markup)
    problems = 0

    for fixture_id, files in (GENERATED if generated is None else generated).items():
        target = fixtures / fixture_id
        if not target.is_dir():
            if check:
                print(f"STALE {fixture_id}: generated fixture directory is missing", file=err)
                problems += 1
                continue
            target.mkdir(parents=True)
        for file_name, make in files.items():
            wanted = make()
            path = target / file_name
            current = path.read_bytes() if path.is_file() else None
            if current == wanted:
                continue
            if check:
                print(f"STALE {fixture_id}: {file_name} differs from its recipe", file=err)
                problems += 1
            else:
                path.write_bytes(wanted)
                print(f"wrote {fixture_id}/{file_name}", file=out)

    count = 0
    for fixture_dir in fixture_dirs(fixtures):
        count += 1
        try:
            wanted = expected_for(fixture_dir, rule_ids)
        except FixtureError as exc:
            print(f"ERROR {exc}", file=err)
            problems += 1
            continue
        path = fixture_dir / "expected.json"
        current = path.read_bytes() if path.is_file() else None
        if current == wanted:
            continue
        if check:
            print(f"STALE {fixture_dir.name}: expected.json is missing or out of date", file=err)
            problems += 1
        else:
            path.write_bytes(wanted)
            print(f"wrote {fixture_dir.name}/expected.json", file=out)

    if problems:
        print(f"{problems} problem(s) in {count} fixture(s)", file=err)
        return 1
    print(f"{count} fixture(s) OK", file=out)
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Generate expected.json for RQP markup fixtures.")
    parser.add_argument("--check", action="store_true", help="do not write; fail when a file is stale")
    parser.add_argument("--fixtures", type=Path, default=DEFAULT_FIXTURES, help="fixtures directory")
    parser.add_argument(
        "--markup", type=Path, default=DEFAULT_MARKUP, help="markup.md with the rule ids (skipped if missing)"
    )
    args = parser.parse_args(argv)
    return run(args.fixtures, args.markup, args.check)


if __name__ == "__main__":
    sys.exit(main())
