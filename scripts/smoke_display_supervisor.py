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
import select
import signal
import subprocess
import sys
import tempfile
import time

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


APPLY = {
    "kind": "apply",
    "value": [{
        "connector": "DP-1", "enabled": True,
        "mode": "1920x1080@60.000", "scale": 1.25,
        "transform": "normal", "x": 0, "y": 0, "vrr": False,
    }],
}


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


def crash_parent(shell: Path) -> None:
    """A disposable process standing in for Shell's supervisor pipe owner."""
    worker = subprocess.Popen(
        [str(shell), "--display-supervisor"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        env=os.environ.copy(),
        text=True,
        bufsize=1,
        process_group=0,  # separate process group, exactly as Rust Shell
    )
    assert worker.stdin is not None and worker.stdout is not None
    worker.stdin.write(json.dumps(APPLY) + "\n")
    worker.stdin.flush()
    for _ in range(8):
        line = worker.stdout.readline()
        if not line:
            raise AssertionError("Supervisor died before pending Apply")
        if json.loads(line).get("state") == "pending":
            print(json.dumps({"worker_pid": worker.pid}), flush=True)
            # Test controller kills THIS process with SIGKILL; the external
            # supervisor must survive and restore the originally observed Niri.
            time.sleep(30)
            raise AssertionError("The simulated parent was not killed")
    raise AssertionError("Supervisor never reported pending Apply")


def assert_sigkill_rollback(shell: Path, env: dict[str, str], log: Path) -> None:
    parent = subprocess.Popen(
        [sys.executable, str(Path(__file__).resolve()),
         "--shell", str(shell), "--crash-parent"],
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
        bufsize=1,
    )
    worker_pid = None
    try:
        assert parent.stdout is not None
        readable, _, _ = select.select([parent.stdout], [], [], 12)
        if not readable:
            raise AssertionError("Simulated Shell never received pending Apply")
        notice = json.loads(parent.stdout.readline())
        worker_pid = int(notice["worker_pid"])
        if log_scales(log) != ["1.25"]:
            raise AssertionError("No actual simulated display mutation before kill")
        os.kill(parent.pid, signal.SIGKILL)
        if parent.wait(timeout=4) != -signal.SIGKILL:
            raise AssertionError("The parent was not terminated by SIGKILL")
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            if log_scales(log) == ["1.25", "1"]:
                print("[display-supervisor] SIGKILL parent / external rollback: PASS")
                return
            time.sleep(0.1)
        raise AssertionError(f"No rollback after SIGKILL: {log_scales(log)!r}")
    finally:
        if parent.poll() is None:
            parent.kill()
            parent.wait(timeout=4)
        if worker_pid is not None:
            # The normal child exits on pipe EOF; never leave a stranded
            # acceptance-only worker in CI, even when a test assertion fails.
            try:
                os.kill(worker_pid, signal.SIGKILL)
            except ProcessLookupError:
                pass


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
        apply = APPLY
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

        # Unlike EOF-only regression, exercise an actual SIGKILL of the
        # parent process while a separate supervisor owns pending Apply.
        log.unlink()
        assert_sigkill_rollback(shell, env, log)
        before = log.read_text(encoding="utf-8")

        env.pop("KIRA_PREVIEW")
        result, _ = invoke(shell, env, [apply])
        if result.returncode == 0 or log.read_text(encoding="utf-8") != before:
            raise AssertionError("Non-preview supervisor must not mutate outputs")
        print("[display-supervisor] EOF rollback, confirmation, command guard: PASS")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--shell", type=Path, required=True)
    parser.add_argument("--crash-parent", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    try:
        if args.crash_parent:
            crash_parent(args.shell.resolve(strict=True))
        else:
            smoke(args.shell.resolve(strict=True))
    except (OSError, ValueError, AssertionError, subprocess.TimeoutExpired) as error:
        print(f"[display-supervisor] {error}", file=sys.stderr)
        raise SystemExit(1) from error
