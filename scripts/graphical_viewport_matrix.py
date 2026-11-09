#!/usr/bin/env python3
"""Capture a bounded multi-viewport Niri matrix in separate private sessions.

Each viewport gets its OWN Weston -> nested Niri -> private D-Bus session,
its own 34 real screenshots, and a screenshot-dimension check. The summary
does not certify main/native parity, user input, or framebuffer scale factors.
The default 1280x800 scene is preserved alongside a compact and wide scene.
"""
from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys

from dev_preview import PreviewError
from headless_wayland_acceptance import run_virtual, validate_virtual_viewport
from validate_nested_evidence import MAIN_SHA, validate_bundle

VIEWPORTS: tuple[tuple[int, int], ...] = (
    (800, 600),
    (1280, 800),
    (1920, 1080),
)
CAPTURES_PER_VIEWPORT = 34


def run_matrix(package: Path, output: Path) -> Path:
    """No reuse of a live Wayland session, or previously captured evidence."""
    if output.exists() or output.is_symlink():
        raise PreviewError("Refusing to overwrite any prior viewport evidence")
    output.mkdir(mode=0o700, parents=True)
    report = {
        "status": "unreviewed",
        "source_baseline": MAIN_SHA,
        "kind": "three isolated Weston -> nested Niri sessions",
        "graphical_input_and_motion_parity": "not verified",
        "viewports": [],
    }
    for width, height in VIEWPORTS:
        name = f"{width}x{height}"
        scene = output / name
        print(f"[matrix] Capturing genuine nested-Niri viewport {name}", flush=True)
        run_virtual(package, scene, width, height, profile="smoke")
        # Read actual nested PNG manifests rather than copying CLI flags
        # into a report that merely *claims* a tested viewport.
        captures = validate_bundle(scene)
        validate_virtual_viewport(captures, width, height)
        frames = sum(len(capture["frames"]) for capture in captures)
        if frames != CAPTURES_PER_VIEWPORT:
            raise PreviewError(f"Wrong number of real smoke frames in {name}: {frames}")
        report["viewports"].append({
            "requested_and_measured": [width, height],
            "frames": frames,
            "capture_sequences": len(captures),
            "index": f"{name}/review-index.html",
        })
    manifest = output / "matrix-manifest.json"
    manifest.write_text(
        json.dumps(report, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    print(
        "[matrix] Verified 3 real private Niri sessions and "
        f"{sum(item['frames'] for item in report['viewports'])} unreviewed frames",
        flush=True,
    )
    return manifest


def main() -> int:
    import argparse

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package-dir", type=Path, default=Path("result"))
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    try:
        run_matrix(args.package_dir.resolve(), args.output.resolve())
        return 0
    except (PreviewError, OSError, subprocess.TimeoutExpired) as exc:
        print(f"[matrix] FAIL CLOSED: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
