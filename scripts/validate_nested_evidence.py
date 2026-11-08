#!/usr/bin/env python3
"""Validate genuine nested grim PNGs and create an UNREVIEWED offline index.

Deliberately does not compare main/native pixels or modify the parity ledger.
Only known local relative frame names can be embedded in the HTML index.
"""
from __future__ import annotations

import argparse
import html
import json
from pathlib import Path
import re
import struct
import sys
import zlib

from dev_preview import PreviewError

MAIN_SHA = "305139949f49176487ef9759fc32c228eadff408"
PNG_MAGIC = b"\x89PNG\r\n\x1a\n"
MAX_PNG_BYTES = 64 * 1024 * 1024
CAPTURE_NAMES = {
    "entry/toast-entry-manifest.json",
    "exit/toast-exit-manifest.json",
    "tray/tray-live-sni-manifest.json",
}


def png_size(path: Path) -> tuple[int, int]:
    if not path.is_file() or path.is_symlink():
        raise PreviewError("Missing or symlinked screenshot")
    if not 33 <= path.stat().st_size <= MAX_PNG_BYTES:
        raise PreviewError("Invalid PNG screenshot size")
    data = path.read_bytes()
    if data[:8] != PNG_MAGIC:
        raise PreviewError("Evidence frame is not a real PNG")
    position = 8
    dimensions = None
    found_image = False
    finished = False
    while position + 12 <= len(data):
        length = struct.unpack_from(">I", data, position)[0]
        end = position + 12 + length
        if length > MAX_PNG_BYTES or end > len(data):
            raise PreviewError("Truncated or oversized PNG chunk")
        chunk = data[position + 4: position + 8]
        payload = data[position + 8: position + 8 + length]
        expected = struct.unpack_from(">I", data, position + 8 + length)[0]
        actual = zlib.crc32(chunk + payload) & 0xFFFFFFFF
        if expected != actual:
            raise PreviewError("PNG chunk checksum failure")
        if dimensions is None:
            if chunk != b"IHDR" or length != 13:
                raise PreviewError("PNG must begin with a valid IHDR chunk")
            width, height = struct.unpack_from(">II", payload)
            if not 1 <= width <= 16384 or not 1 <= height <= 16384:
                raise PreviewError("Invalid PNG dimensions")
            dimensions = (width, height)
        elif chunk == b"IDAT":
            found_image = True
        elif chunk == b"IEND":
            if length or end != len(data):
                raise PreviewError("PNG contains invalid IEND or trailing data")
            finished = True
            break
        position = end
    if dimensions is None or not found_image or not finished:
        raise PreviewError("Screenshot lacks complete PNG pixel data")
    return dimensions


def validate_capture(manifest: Path, root: Path) -> dict:
    if not manifest.is_file() or manifest.is_symlink():
        raise PreviewError("Missing or symlinked capture manifest")
    capture = json.loads(manifest.read_text(encoding="utf-8"))
    if capture.get("status") != "unreviewed" or capture.get("source_baseline") != MAIN_SHA:
        raise PreviewError("Screenshot manifest has untrusted baseline/status")
    frames = capture.get("frames")
    if not isinstance(frames, list) or not 1 <= len(frames) <= 60:
        raise PreviewError("No bounded captured frames")
    label = capture.get("label")
    if not isinstance(label, str) or not re.fullmatch(r"[A-Za-z0-9_-]{1,48}", label):
        raise PreviewError("Invalid capture label")
    seen = set()
    last = -1.0
    dimensions = None
    for index, frame in enumerate(frames):
        if not isinstance(frame, dict):
            raise PreviewError("Invalid capture frame record")
        name = frame.get("file")
        if name != f"{label}-{index:03}.png" or name in seen:
            raise PreviewError("Frame filenames must match ordered captures")
        seen.add(name)
        elapsed = frame.get("elapsed_ms")
        if not isinstance(elapsed, (float, int)) or not 0 <= elapsed < 60000:
            raise PreviewError("Invalid capture timestamp")
        if elapsed < last:
            raise PreviewError("Capture timestamps are not monotonic")
        last = elapsed
        path = manifest.parent / name
        if path.resolve().parent != manifest.parent.resolve() or path.is_symlink():
            raise PreviewError("Screenshot escapes its private evidence directory")
        dims = png_size(path)
        if dimensions is not None and dims != dimensions:
            raise PreviewError("Frames from one compositor viewport have different dimensions")
        dimensions = dims
    return {"manifest": manifest.relative_to(root).as_posix(),
            "frames": frames, "dimensions": dimensions, "label": label}


def validate_bundle(root: Path) -> list[dict]:
    root = root.resolve(strict=True)
    report = root / "scenario-manifest.json"
    if report.is_symlink() or not report.is_file():
        raise PreviewError("No original scenario manifest")
    scenario = json.loads(report.read_text(encoding="utf-8"))
    if (scenario.get("status") != "unreviewed"
            or scenario.get("source_baseline") != MAIN_SHA
            or set(scenario.get("captures", [])) != CAPTURE_NAMES
            or len(scenario.get("captures", [])) != len(CAPTURE_NAMES)):
        raise PreviewError("Scenario lacks all pinned unreviewed real captures")
    sni = scenario.get("sni_fixture")
    if not isinstance(sni, dict) or sni.get("registered_on_private_bus") is not True:
        raise PreviewError("No real private-bus SNI registration")
    if sni.get("sent_real_status_changed") is not True:
        raise PreviewError("No live SNI status update")
    captures = []
    for relative in sorted(CAPTURE_NAMES):
        path = root / relative
        if path.resolve().parent != (root / Path(relative).parent).resolve():
            raise PreviewError("Capture manifest escapes evidence directory")
        captures.append(validate_capture(path, root))
    width_height = {tuple(item["dimensions"]) for item in captures}
    if len(width_height) != 1:
        raise PreviewError("Nested viewport dimensions changed across the scenario")
    return captures


def write_index(root: Path, captures: list[dict]) -> Path:
    root = root.resolve()
    sections = []
    for capture in captures:
        relative = Path(capture["manifest"])
        directory = relative.parent
        tiles = []
        for frame in capture["frames"]:
            url = html.escape((directory / frame["file"]).as_posix(), quote=True)
            label = html.escape(f"{capture['label']} at {frame['elapsed_ms']} ms")
            tiles.append(
                f'<figure><img loading="lazy" src="{url}" alt="{label}">'
                f'<figcaption>{label}</figcaption></figure>'
            )
        sections.append(
            f"<section><h2>{html.escape(capture['label'])}</h2>"
            f"<p>{capture['dimensions'][0]} × {capture['dimensions'][1]} real pixels; "
            "unreviewed timing</p><div class=\"frames\">"
            + "".join(tiles) + "</div></section>"
        )
    content = (
        '<!doctype html><html lang="en"><meta charset="utf-8">'
        '<meta name="viewport" content="width=device-width">'
        '<title>Kira nested Niri evidence, UNREVIEWED</title>'
        '<style>body{font:14px system-ui;background:#101725;color:white;padding:24px}'
        'h1{color:#fca311}.frames{display:flex;gap:12px;overflow:auto}'
        'figure{margin:0;min-width:320px}img{width:320px;height:auto;display:block}'
        'figcaption{font-size:12px;padding:8px 0}</style>'
        '<h1>Unreviewed, native-only capture</h1>'
        '<p>Not a main/native comparison, motion certificate, or parity claim.</p>'
        + "".join(sections) + '</html>'
    )
    output = root / "review-index.html"
    if output.exists():
        raise PreviewError("Refusing to replace an earlier visual review")
    output.write_text(content, encoding="utf-8")
    return output


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path, help="Explicit private-bus evidence directory")
    args = parser.parse_args()
    try:
        captures = validate_bundle(args.directory)
        index = write_index(args.directory, captures)
        print(f"[evidence] {index} (UNREVIEWED, no parity claim)")
        return 0
    except (OSError, ValueError, KeyError, PreviewError) as error:
        print(f"[evidence] {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
