#!/usr/bin/env python3
"""Headless regression for independent native display rollback ownership.

A fake fixed-argv Niri executable records all output changes. No compositor
or production config is touched. Real nested-Niri SIGKILL QA remains required.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

FAKE_NIRI = r'''#!/usr/bin/env python3
import json
import os
import sys
from pathlib import Path

output = {"DP-1": {
    "modes": [{"width": 1920, "height": 1080,
               "refresh_rate": 60000, "is_preferred": True}],
    "current_mode": 0,
    "logical": {"scale": 1.0, "transform": "Normal", "x": 0, "y": 0},
    "vrr_enabled": False, "vrr_supported": False,
}}
args = sys.argv[1:]
if args == ["msg", "--json", "outputs"]:
    print(json.dumps(output))
elif len(args) >= 4 and args[:2] == ["msg", "output"] and args[2] == "DP-1":
    with Path(os.environ["KIRA_TEST_NIRI_LOG"]).open("a", encoding="utf-8") as log:
        log.write(json.dumps(args[3:]) + "\n")
else:
    sys.exit(1)
'''


def invoke(shell: Path, environment: dict[str, str], commands: list[dict]):
    payload = "".join(json.dumps(command) + "\n" for command in commands)
    result = subprocess.run(
        [str(shell), "--display-supervisor"],
        input=payload,
        capture_output=True,
        text=True,
        env=environment,
        timeout=30,
        check=False,
    )
    events = [json.loads(line) for line in result.stdout.splitlines()]
    return result, events


def log_scales(path: Path) -> list[str]:
    if not path.exists():
        return []
    instructions = [
        json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()
    ]
    return [args[1] for args in instructions if args[0] == "scale"]


def smoke(shell: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="kira-display-rollback-ci-") as directory:
        root = Path(directory)
        executable = root / "niri"
        executable.write_text(FAKE_NIRI, encoding="utf-8")
        executable.chmod(0o700)
        log = root / "commands.jsonl"
        env = os.environ.copy()
        env["PATH"] = str(root) + os.pathsep + env.get("PATH", "")
        env["KIRA_TEST_NIRI_LOG"] = str(log)
        env["KIRA_PREVIEW"] = "1"
        env["NIRI_CONFIG"] = str(root / "no-real-niri.kdl")
        env["XDG_CONFIG_HOME"] = str(root)
        apply = {
            "kind": "apply",
            "value": [{
                "connector": "DP-1", "enabled": True,
                "mode": "1920x1080@60.000", "scale": 1.25,
                "transform": "normal", "x": 0, "y": 0, "vrr": False,
            }],
        }
        # The caller's pipe closes immediately after Apply, exactly as when
        # GTK Shell terminates with SIGKILL. The external process must drain
        # the command and restore original geometry after the disconnect.
        result, events = invoke(shell, env, [apply])
        states = [event.get("state") for event in events]
        if result.returncode or "pending" not in states or "reverted" not in states:
            raise AssertionError(
                f"Missing crash-style rollback: code={result.returncode} "
                f"states={states!r} stderr={result.stderr[-500:]}"
            )
        if log_scales(log) != ["1.25", "1"]:
            raise AssertionError(f"Expected apply + baseline restore: {log_scales(log)}")

        # Confirm is an explicit distinct transaction: EOF after Confirm must
        # not rollback a confirmed session change.
        log.unlink()
        result, events = invoke(shell, env, [apply, {"kind": "confirm", "id": 1}])
        states = [event.get("state") for event in events]
        if result.returncode or "confirmed" not in states or "reverted" in states:
            raise AssertionError(f"Confirmed transaction failed: {states!r}")
        if log_scales(log) != ["1.25"]:
            raise AssertionError("Explicit confirmation unexpectedly rolled back")

        # Invalid unrecognized wire messages are fail-closed, not execution.
        before = log.read_text(encoding="utf-8")
        result, _ = invoke(shell, env, [{"kind": "arbitrary-exec", "argv": ["sh"]}])
        if result.returncode or log.read_text(encoding="utf-8") != before:
            raise AssertionError("Unknown command changed Niri")

        env.pop("KIRA_PREVIEW")
        result, _ = invoke(shell, env, [apply])
        if result.returncode == 0 or log.read_text(encoding="utf-8") != before:
            raise AssertionError("Non-preview supervisor must not mutate outputs")
        print("[display-supervisor] EOF rollback, confirmation, command guard: PASS")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--shell", type=Path, required=True)
    args = parser.parse_args()
    try:
        smoke(args.shell.resolve(strict=True))
    except (OSError, ValueError, AssertionError, subprocess.TimeoutExpired) as error:
        print(f"[display-supervisor] {error}", file=sys.stderr)
        raise SystemExit(1) from error
