#!/usr/bin/env python3
"""Capture timestamped motion frames from an explicitly isolated nested Wayland display.

Run separately from dev_preview.py, on the HOST terminal. Never guesses a
display, never captures the current host compositor and never changes KDL.
Captured PNGs are evidence candidates, not a claim of visual parity.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time

from dev_preview import PreviewError, nested_display_path, ready_socket

MAX_FRAMES = 60
MAX_INTERVAL_MS = 500


def validate_capture_environment(
    nested_display: str, environment: dict[str, str]
) -> dict[str, str]:
    runtime = environment.get("XDG_RUNTIME_DIR")
    parent = environment.get("WAYLAND_DISPLAY")
    if not runtime or not parent:
        raise PreviewError("Capture requires the host Wayland session environment")
    runtime_dir = Path(runtime)
    nested_socket = nested_display_path(nested_display, parent, runtime_dir)
    if not ready_socket(nested_socket):
        raise PreviewError("Nested Wayland socket is missing or not accepting clients")
    if not ready_socket((runtime_dir / parent) if not Path(parent).is_absolute() else Path(parent)):
        raise PreviewError("Host Wayland session socket is not accepting clients")
    isolated = environment.copy()
    isolated["WAYLAND_DISPLAY"] = nested_display
    isolated.pop("NIRI_SOCKET", None)
    isolated.pop("NIRI_CONFIG", None)
    return isolated


def validate_capture_args(label: str, frames: int, interval_ms: int) -> None:
    if not re.fullmatch(r"[a-zA-Z0-9_-]{1,48}", label):
        raise PreviewError("Use 1-48 letters, digits, underscores or dashes for label")
    if not (1 <= frames <= MAX_FRAMES):
        raise PreviewError(f"Frame count must be between 1 and {MAX_FRAMES}")
    if not (10 <= interval_ms <= MAX_INTERVAL_MS):
        raise PreviewError(f"Frame interval must be between 10 and {MAX_INTERVAL_MS}ms")


def capture_frames(
    display: str,
    output: Path,
    label: str,
    frames: int,
    interval_ms: int,
    environment: dict[str, str],
    *,
    grim: str,
    runner=subprocess.run,
) -> Path:
    validate_capture_args(label, frames, interval_ms)
    isolated = validate_capture_environment(display, environment)
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    start = time.monotonic()
    metadata = {
        "status": "unreviewed",
        "display": display,
        "label": label,
        "requested_interval_ms": interval_ms,
        "source_baseline": "305139949f49176487ef9759fc32c228eadff408",
        "frames": [],
    }
    for index in range(frames):
        scheduled = start + index * interval_ms / 1000.0
        time.sleep(max(0.0, scheduled - time.monotonic()))
        actual_ms = round((time.monotonic() - start) * 1000.0, 3)
        filename = f"{label}-{index:03}.png"
        destination = output / filename
        # Explicit argv, no shell; the child's WAYLAND_DISPLAY is always
        # validated as different from the parent socket.
        result = runner(
            [grim, "-t", "png", str(destination)],
            env=isolated,
            capture_output=True,
            text=True,
            check=False,
            timeout=10,
        )
        if result.returncode != 0 or not destination.is_file() or destination.stat().st_size == 0:
            raise PreviewError(f"Nested grim capture failed at frame {index}: {result.stderr}")
        metadata["frames"].append({"file": filename, "elapsed_ms": actual_ms})
    manifest = output / f"{label}-manifest.json"
    staged = manifest.with_suffix(".json.tmp")
    staged.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    staged.replace(manifest)
    return manifest


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--display", required=True, help="Nested display printed by dev_preview.py")
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--label", default="motion")
    parser.add_argument("--frames", type=int, default=14)
    parser.add_argument("--interval-ms", type=int, default=33)
    args = parser.parse_args()
    try:
        grim = shutil.which("grim")
        if not grim:
            raise PreviewError("grim is not installed; use the default Nix dev shell")
        manifest = capture_frames(
            args.display,
            args.output,
            args.label,
            args.frames,
            args.interval_ms,
            os.environ,
            grim=grim,
        )
        print(f"[capture] Unreviewed nested frames and timestamps: {manifest}")
        return 0
    except PreviewError as error:
        print(f"[capture] {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
