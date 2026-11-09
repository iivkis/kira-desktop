#!/usr/bin/env python3
"""Fail closed on static Kira Default design and TENET invariants.

This is a source contract, not GTK compilation or paired Wayland evidence.
Keep it fast enough for the two-minute Core/IPC CI job.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PALETTE = frozenset({"#000000", "#14213d", "#fca311", "#e5e5e5", "#ffffff"})
HEX = re.compile(r"#[0-9a-fA-F]{3,8}\b")
RULE = re.compile(r"([^{}]+)\{([^{}]*)\}", re.MULTILINE)
PROPERTY = re.compile(r"(?<![\w-])([\w-]+)\s*:\s*([^;]+)", re.IGNORECASE)
BANNED_PROPERTIES = frozenset(
    {"animation", "transition", "filter", "backdrop-filter", "perspective"}
)


def validate(css: str, bar: str, popup: str, motion: str) -> list[str]:
    errors: list[str] = []
    # Comments can mention old implementation details; only active rules matter.
    active = re.sub(r"/\*.*?\*/", "", css, flags=re.DOTALL)
    colors = {color.lower() for color in HEX.findall(active)}
    for color in sorted(colors - PALETTE):
        errors.append(f"noncanonical Kira Default color: {color}")

    if re.search(r"@(?:-\w+-)?keyframes\b", active, flags=re.IGNORECASE):
        errors.append("CSS keyframes bypass the native GTK timeline")
    for selector, declarations in RULE.findall(active):
        properties = dict(
            (key.lower(), value.strip().lower())
            for key, value in PROPERTY.findall(declarations)
        )
        for property_name in properties:
            if property_name in BANNED_PROPERTIES or any(
                property_name.startswith(prefix + "-")
                for prefix in BANNED_PROPERTIES
            ):
                errors.append(f"browser/CSS-only motion or visual filter: {property_name}")
        outline = properties.get("outline", "")
        if re.fullmatch(r"(?:none|0|0px)", outline):
            errors.append(f"focus outline removed in {selector.strip()!r}")
        if properties.get("background") == "#fca311":
            foreground = properties.get("color")
            if foreground is not None and foreground != "#000000":
                errors.append(
                    f"orange surface must use black text: {selector.strip()!r}"
                )
    if ":focus-visible" not in active:
        errors.append("no visible GTK keyboard focus styling")

    if not re.search(r"pub const BAR_HEIGHT:\s*i32\s*=\s*42\s*;", bar):
        errors.append("Top Bar must reserve exactly 42 logical pixels")
    if "set_exclusive_zone(BAR_HEIGHT)" not in bar:
        errors.append("Top Bar lost its exclusive zone")
    if "set_exclusive_zone(0)" not in popup:
        errors.append("a transient popup must reserve zero exclusive space")

    if "gtk-enable-animations" not in motion:
        errors.append("native animation lost reduced-motion preference")
    if "frame_clock_progress" not in motion or "clock.frame_time()" not in motion:
        errors.append("animation cursor must follow painted GTK frame time")
    return errors


def main() -> int:
    files = (
        "crates/kira-ui/src/style.css",
        "crates/kira-ui/src/bar.rs",
        "crates/kira-ui/src/popup.rs",
        "crates/kira-ui/src/motion.rs",
    )
    values = [(ROOT / name).read_text(encoding="utf-8") for name in files]
    errors = validate(*values)
    for error in errors:
        print(f"FAIL design/motion: {error}", file=sys.stderr)
    if errors:
        return 1
    print("Kira Default palette, GTK focus, 42px bar, popup zone and TENET frame clock: source gate OK")
    print("Not a GTK build or visual/motion parity certificate.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
