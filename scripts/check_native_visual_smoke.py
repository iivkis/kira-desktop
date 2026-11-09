#!/usr/bin/env python3
"""Verify a REAL Kira toast visually enters and leaves genuine Niri PNGs.

This is a low-level presence/absence smoke test, NOT pixel fidelity, source
main parity, compositor presentation timing, or keyboard acceptance. It
can only consume the validated nested Niri capture bundle and never renders
or manufactures frames.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

from PIL import Image

from dev_preview import PreviewError
from validate_nested_evidence import MAIN_SHA, validate_bundle

PRUSSIAN = (20, 33, 61)
ACCENT = (252, 163, 17)
BLACK = (0, 0, 0)
WHITE = (255, 255, 255)


def palette_counts(path: Path, *, bottom_right: bool) -> dict[tuple[int, int, int], int]:
    with Image.open(path) as screenshot:
        image = screenshot.convert("RGB")
        if bottom_right:
            image = image.crop((
                image.width // 2, image.height // 2,
                image.width, image.height,
            ))
        else:
            image = image.crop((0, 0, image.width, min(42, image.height)))
        frequencies = image.getcolors(maxcolors=image.width * image.height)
        if frequencies is None:
            raise PreviewError("Too many independent colors in native smoke evidence")
        return {color: count for count, color in frequencies}


def toast_present(counts: dict[tuple[int, int, int], int]) -> bool:
    # Exact canonical solid fill / accent border. Thresholds tolerate partial
    # animation and variable viewport sizes, not a fabricated blank screen.
    return counts.get(PRUSSIAN, 0) >= 500 and counts.get(ACCENT, 0) >= 20


def bar_present(counts: dict[tuple[int, int, int], int]) -> bool:
    # Verify actual top bar paint on the same compositor screenshot. A blank
    # solid-color Niri image must never count as a successful GTK smoke.
    return (
        counts.get(BLACK, 0) >= 100
        and counts.get(PRUSSIAN, 0) >= 100
        and counts.get(WHITE, 0) >= 20
    )


def verify_toast_transitions(root: Path) -> dict:
    captures = validate_bundle(root)
    by_label = {capture["label"]: capture for capture in captures}
    expected = {"toast-entry", "toast-exit", "tray-live-sni"}
    if set(by_label) != expected:
        raise PreviewError("Missing real notification entry/exit or live SNI captures")

    def frame(label: str, index: int) -> Path:
        capture = by_label[label]
        filename = capture["frames"][index]["file"]
        return root / Path(capture["manifest"]).parent / filename

    # The coordinator emits a real Notify at 120ms and closes it after a
    # 3.8s painted dwell; inspect independently captured first/last frames.
    samples = {
        "entry_before": frame("toast-entry", 0),
        "entry_after": frame("toast-entry", -1),
        "exit_before": frame("toast-exit", 0),
        "exit_after": frame("toast-exit", -1),
        "tray_after": frame("tray-live-sni", -1),
    }
    observations = {}
    for name, png in samples.items():
        counts = palette_counts(png, bottom_right=True)
        observations[name] = {
            "blue_pixels": counts.get(PRUSSIAN, 0),
            "accent_pixels": counts.get(ACCENT, 0),
            "toast_visible": toast_present(counts),
        }
    for name in ("entry_before", "exit_after", "tray_after"):
        if observations[name]["toast_visible"]:
            raise PreviewError("A real toast is still visible at unexpected " + name)
    for name in ("entry_after", "exit_before"):
        if not observations[name]["toast_visible"]:
            raise PreviewError("No real painted notification at expected " + name)
    if not bar_present(palette_counts(samples["entry_after"], bottom_right=False)):
        raise PreviewError("The native 42px Top Bar is not painted in the Niri PNG")
    return {
        "status": "unreviewed",
        "source_baseline": MAIN_SHA,
        "measurement": "actual top-42px and bottom-right PNG RGB colors",
        "transitions": observations,
        "limitations": [
            "Only real native toast appearance/disappearance and bar paint",
            "No source main pixel, focus, input or TENET parity verdict",
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("evidence", type=Path)
    args = parser.parse_args()
    try:
        report = verify_toast_transitions(args.evidence)
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 0
    except (OSError, ValueError, PreviewError) as error:
        print("[visual] " + str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
