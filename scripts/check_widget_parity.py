#!/usr/bin/env python3
"""Fail-closed contract for visual, functional and temporal parity with main.

This tool is deliberately NOT a fake screenshot comparator: it checks that every
legacy built-in widget is accounted for, every promised target exists, and no
verified checkbox can be asserted without test/capture/review evidence.
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "docs" / "MAIN_WIDGET_PARITY.json"
MAIN_COMMIT = "305139949f49176487ef9759fc32c228eadff408"
EXPECTED = {
    "frontend/src/App.svelte",
    "frontend/src/lib/components/CalendarWidget.svelte",
    "frontend/src/lib/components/DisplaySettings.svelte",
    "frontend/src/lib/components/LauncherWidget.svelte",
    "frontend/src/lib/components/NotificationCard.svelte",
    "frontend/src/lib/components/NotificationHost.svelte",
    "frontend/src/lib/components/SettingsWidget.svelte",
    "frontend/src/lib/components/SkyMap.svelte",
    "frontend/src/lib/components/SystemCharts.svelte",
    "frontend/src/lib/components/SystemHealth.svelte",
    "frontend/src/lib/components/SystemWidget.svelte",
    "frontend/src/lib/components/TrayBar.svelte",
    "frontend/src/lib/components/TrayWidget.svelte",
    "frontend/src/lib/components/WindowControls.svelte",
    "frontend/src/lib/components/WorkspacePreview.svelte",
}
ACCEPTANCE = {
    "content": ("contract_tests", "review"),
    "interaction": ("contract_tests", "review"),
    "motion": ("motion_capture", "review"),
    "pixels": ("screen_capture", "review"),
}


def validate(document: dict, root: Path) -> list[str]:
    errors: list[str] = []
    if document.get("schema_version") != 1:
        errors.append("unsupported manifest schema")
    base = document.get("baseline", {})
    if base.get("commit") != MAIN_COMMIT:
        errors.append("main reference changed without a baseline review")
    for name in ("MotionSource", "calendarMotionSource", "themeSource"):
        evidence = base.get(name) or {}
        if not re.fullmatch(r"[a-f0-9]{40}", evidence.get("sha", "")):
            errors.append(f"missing pinned source SHA for {name}")

    widgets = document.get("widgets", [])
    paths = [w.get("source", {}).get("path") for w in widgets]
    if len(paths) != len(set(paths)):
        errors.append("duplicate main widget source")
    missing = EXPECTED - set(paths)
    extra = set(paths) - EXPECTED
    if missing or extra:
        errors.append(f"source inventory differs from main: missing={sorted(missing)}, extra={sorted(extra)}")
    ids = [w.get("id") for w in widgets]
    if len(ids) != len(set(ids)):
        errors.append("duplicate widget id")
    for widget in widgets:
        name = widget.get("id", "<unknown>")
        source = widget.get("source", {})
        if not re.fullmatch(r"[a-f0-9]{40}", source.get("sha", "")):
            errors.append(f"{name}: source file has no pinned blob SHA")
        state = widget.get("state")
        if state not in {"missing", "partial", "candidate", "verified"}:
            errors.append(f"{name}: unsupported state {state!r}")
        targets = widget.get("target_files", [])
        if state == "missing" and targets:
            errors.append(f"{name}: 'missing' entry has an implementation target")
        if state != "missing" and not targets:
            errors.append(f"{name}: claimed implementation without target")
        for path in targets:
            if not isinstance(path, str) or not path.startswith(("crates/kira-ui/", "crates/kira-shell/")):
                errors.append(f"{name}: target is outside native GTK boundaries: {path!r}")
            elif not (root / path).is_file():
                errors.append(f"{name}: referenced native target does not exist: {path}")
        for key in ("content_and_interaction_requirements", "animation_requirements"):
            if not widget.get(key):
                errors.append(f"{name}: no {key} contract")
        evidence = widget.get("evidence") or {}
        verified = widget.get("verified") or {}
        if set(verified) != set(ACCEPTANCE):
            errors.append(f"{name}: missing content/interaction/motion/pixels verification fields")
        for dimension, required in ACCEPTANCE.items():
            if verified.get(dimension) is not True:
                continue
            for evidence_type in required:
                files = evidence.get(evidence_type, [])
                if not files:
                    errors.append(f"{name}: {dimension} marked verified without {evidence_type}")
                    continue
                for path in files:
                    if not isinstance(path, str) or not path.startswith("docs/parity-evidence/"):
                        errors.append(f"{name}: untrusted {dimension} evidence path {path!r}")
                    elif not (root / path).is_file():
                        errors.append(f"{name}: missing {dimension} evidence: {path}")
        if state == "verified" and not all(verified.get(dim) is True for dim in ACCEPTANCE):
            errors.append(f"{name}: 'verified' without all four independent gates")
        if state != "verified" and all(verified.get(dim) is True for dim in ACCEPTANCE):
            errors.append(f"{name}: all gates complete but review state is stale")
    return errors


def main() -> int:
    document = json.loads(MANIFEST.read_text(encoding="utf-8"))
    errors = validate(document, ROOT)
    widgets = document["widgets"]
    statuses = {key: sum(w.get("state") == key for w in widgets)
                for key in ("missing", "partial", "candidate", "verified")}
    print(f"[widget parity] main={MAIN_COMMIT[:10]} widgets={len(widgets)}: {statuses}")
    if errors:
        for error in errors:
            print(f"[widget parity] ERROR {error}", file=sys.stderr)
        return 1
    print("[widget parity] inventory is honest; NOT a visual parity certificate.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
