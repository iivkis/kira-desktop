#!/usr/bin/env python3
"""Exercise real GIO Freedesktop RPCs on an ephemeral private D-Bus in CI.

The disposable policy fixture reuses production Rust queue/owner logic.
It is NOT the GTK Shell, and this smoke test is NOT Wayland, visual,
notification presentation or source-main parity acceptance.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import secrets
import shutil
import subprocess
import sys
import time

from accept_nested import PARENT_BUS, TOKEN, verify_private_bus
from dev_preview import PreviewError
from private_bus import PRIVATE_BUS_DIR, private_bus_config, run_session_arguments


def require_binary(path: Path) -> Path:
    resolved = path.resolve()
    if not resolved.is_file() or not os.access(resolved, os.X_OK):
        raise PreviewError("Private notification policy smoke executable is missing")
    return resolved


def expected_proof(answer: str) -> dict[str, bool]:
    try:
        value = json.loads(answer)
    except (ValueError, TypeError) as error:
        raise PreviewError("Private GIO sender probe returned invalid JSON") from error
    checks = (
        "same_sender_replaced", "foreign_sender_isolated",
        "stale_id_rejected", "private_bus",
    )
    if (not isinstance(value, dict)
            or set(value) != set(checks)
            or any(value.get(key) is not True for key in checks)):
        raise PreviewError("Private GIO probe missed a required sender-isolation assertion")
    return {key: True for key in checks}


def run_child(fixture: Path, probe: Path) -> None:
    verify_private_bus(os.environ)
    isolated = os.environ.copy()
    isolated.update({
        "KIRA_PREVIEW": "1",
        "KIRA_NOTIFICATIONS_PREVIEW": "1",
        "KIRA_ISOLATED_SESSION_BUS": "1",
        "KIRA_POLICY_FIXTURE_PREVIEW": "1",
    })
    server = subprocess.Popen(
        [str(fixture)], env=isolated, stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE, text=True,
    )
    try:
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            if server.poll() is not None:
                raise PreviewError("Private Freedesktop policy fixture exited before ownership")
            ready = subprocess.run(
                [
                    "gdbus", "call", "--session",
                    "--dest", "org.freedesktop.Notifications",
                    "--object-path", "/org/freedesktop/Notifications",
                    "--method", "org.freedesktop.Notifications.GetServerInformation",
                ],
                env=isolated, capture_output=True, text=True, check=False, timeout=3,
            )
            if (ready.returncode == 0
                    and "Kira Policy Fixture" in ready.stdout
                    and "Kira Test Policy" in ready.stdout):
                break
            time.sleep(0.1)
        else:
            raise PreviewError("Policy fixture never acquired the private D-Bus name")

        result = subprocess.run(
            [str(probe), "--policy-fixture"],
            env=isolated, text=True, capture_output=True, check=False, timeout=15,
        )
        if result.returncode:
            raise PreviewError(
                "Real GIO dual-peer policy probe failed: " + result.stderr.strip()[:400]
            )
        proof = expected_proof(result.stdout.strip())
        print("[notification-policy-smoke] Real private GIO D-Bus methods: " +
              json.dumps(proof, sort_keys=True))
    finally:
        if server.poll() is None:
            server.terminate()
        try:
            server.communicate(timeout=2)
        except subprocess.TimeoutExpired:
            server.kill()
            server.communicate(timeout=2)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixture", required=True, type=Path)
    parser.add_argument("--probe", required=True, type=Path)
    parser.add_argument("--child", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    try:
        fixture = require_binary(args.fixture)
        probe = require_binary(args.probe)
        if args.child:
            run_child(fixture, probe)
            return 0
        runner = shutil.which("dbus-run-session")
        if runner is None or not shutil.which("gdbus"):
            raise PreviewError("dbus-run-session and gdbus are required")
        env = os.environ.copy()
        parent = env.get("DBUS_SESSION_BUS_ADDRESS", "")
        for name in (
            "KIRA_PREVIEW", "KIRA_NOTIFICATIONS_PREVIEW",
            "KIRA_ISOLATED_SESSION_BUS", "KIRA_POLICY_FIXTURE_PREVIEW",
            TOKEN, PARENT_BUS, PRIVATE_BUS_DIR,
        ):
            env.pop(name, None)
        env[PARENT_BUS] = parent
        env[TOKEN] = secrets.token_hex(16)
        with private_bus_config() as config:
            env[PRIVATE_BUS_DIR] = str(config.parent)
            command = [
                *run_session_arguments(runner, config),
                sys.executable, str(Path(__file__).resolve()),
                "--fixture", str(fixture), "--probe", str(probe), "--child",
            ]
            return subprocess.run(
                command, env=env, check=False, timeout=35,
            ).returncode
    except (PreviewError, OSError, subprocess.TimeoutExpired) as error:
        print(f"[notification-policy-smoke] {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
