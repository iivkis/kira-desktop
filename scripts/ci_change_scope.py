#!/usr/bin/env python3
"""Fail-open CI impact classifier. Docs-only changes skip expensive native Nix.

The regular Core/IPC and Python parity job always runs. On any uncertain
revision, fetch failure, unexpected status or invalid diff, rebuild anyway.
"""
from __future__ import annotations

import os
import re
import subprocess
import sys

SHA = re.compile(r"[a-fA-F0-9]{40}\Z")
NATIVE_PREFIXES = ("crates/", "nix/", "config/", "scripts/")
NATIVE_ROOTS = {
    "Cargo.toml", "Cargo.lock", "flake.nix", "flake.lock",
    "Taskfile.yml", ".github/workflows/ci.yml",
}


def needs_native_build(paths: list[str]) -> bool:
    """Only documentation/artifact/workflow-graphical changes skip Nix build."""
    return any(
        path in NATIVE_ROOTS or path.startswith(NATIVE_PREFIXES)
        for path in paths
    )


def changed_paths(
    base: str, head: str, *, runner=subprocess.run,
) -> list[str] | None:
    """Return None when the comparison cannot be trusted: build by default."""
    if not SHA.fullmatch(base or "") or not SHA.fullmatch(head or ""):
        return None
    try:
        known = runner(
            ["git", "cat-file", "-e", base + "^{commit}"],
            check=False, capture_output=True, timeout=10,
        )
        if known.returncode:
            fetched = runner(
                ["git", "fetch", "--quiet", "--no-tags", "--depth=1",
                 "origin", base],
                check=False, capture_output=True, timeout=30,
            )
            if fetched.returncode:
                return None
        diff = runner(
            ["git", "diff", "--no-ext-diff", "--name-only", "-z", base, head],
            check=False, capture_output=True, timeout=20,
        )
        if diff.returncode:
            return None
        return [
            name for name in diff.stdout.decode("utf-8", "surrogateescape").split("\0")
            if name
        ]
    except (OSError, ValueError, UnicodeError, subprocess.TimeoutExpired):
        return None


def main() -> None:
    paths = changed_paths(
        os.environ.get("CI_BASE_SHA", ""),
        os.environ.get("GITHUB_SHA", ""),
    )
    required = paths is None or needs_native_build(paths)
    # GitHub step output: no untrusted filenames are ever appended here.
    print("native_changed=" + str(required).lower())
    print(
        f"[ci-scope] {('unknown base; fail open' if paths is None else str(len(paths)) + ' paths')}; "
        f"native build {'required' if required else 'skipped for docs-only changes'}",
        file=sys.stderr,
    )


if __name__ == "__main__":
    main()
