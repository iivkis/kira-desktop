#!/usr/bin/env python3
"""Three real isolated Niri logical-scale acceptance scenarios.

Niri's Winit backend uses a fixed physical 1280x800 window in the locked
dependency. Changing outer Weston size cannot change inner Niri resolution.
Test real Niri output *scale* instead, verified from its private IPC before
capturing 34 PNGs for each scale; never certify fake 800x600/1920x1080 pixels.
"""
from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys

from dev_preview import PreviewError
from headless_wayland_acceptance import run_virtual, validate_virtual_viewport
from validate_nested_evidence import MAIN_SHA, validate_bundle

SCALES = (1.0, 1.5, 2.0)
PHYSICAL_VIEWPORT = (1280, 800)
CAPTURES_PER_SCALE = 34


def run_matrix(package: Path, output: Path) -> Path:
    if output.exists() or output.is_symlink():
        raise PreviewError("Refusing to overwrite earlier scale acceptance")
    output.mkdir(mode=0o700, parents=True)
    report = {
        "status": "unreviewed",
        "source_baseline": MAIN_SHA,
        "kind": "three independent private Weston -> nested Niri scale scenarios",
        "physical_png_dimensions": list(PHYSICAL_VIEWPORT),
        "scale_verification": "observed through nested Niri IPC before capture",
        "graphical_input_and_motion_parity": "not verified",
        "scales": [],
    }
    for scale in SCALES:
        scene = output / f"scale-{scale:.1f}"
        print(f"[matrix] Capturing actual Niri output scale {scale:.1f}", flush=True)
        # run_virtual/accept_nested/dev_preview enforce actual output scale
        # through a *live*, private Niri IPC query before launching Kira.
        run_virtual(package, scene, *PHYSICAL_VIEWPORT, profile="smoke", scale=scale)
        captures = validate_bundle(scene)
        validate_virtual_viewport(captures, *PHYSICAL_VIEWPORT)
        frames = sum(len(capture["frames"]) for capture in captures)
        if frames != CAPTURES_PER_SCALE:
            raise PreviewError(f"Missing real PNGs at output scale {scale}: {frames}")
        report["scales"].append({
            "niri_scale": scale,
            "real_physical_png": list(PHYSICAL_VIEWPORT),
            "frames": frames,
            "capture_sequences": len(captures),
            "index": f"{scene.name}/review-index.html",
        })
    manifest = output / "scale-matrix-manifest.json"
    manifest.write_text(
        json.dumps(report, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    print(
        f"[matrix] Verified {len(SCALES)} isolated Niri output scales "
        f"and {sum(row['frames'] for row in report['scales'])} "
        "real, unreviewed PNG frames",
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
    except (PreviewError, OSError, subprocess.TimeoutExpired) as error:
        print(f"[matrix] FAIL CLOSED: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
