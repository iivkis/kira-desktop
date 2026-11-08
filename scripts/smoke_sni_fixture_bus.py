#!/usr/bin/env python3
"""Headless GIO StatusNotifierItem smoke on a disposable session D-Bus.

Called from CI AFTER nix build. Never binds a user's session bus, starts
a compositor, installs a daemon or claims any UI/motion parity.
"""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import secrets
import shutil
import subprocess
import sys
import tempfile
import time

from accept_nested import PARENT_BUS, TOKEN, verify_private_bus
from acceptance_scenarios import SNI_FIXTURE, sni_property_get_all
from dev_preview import PreviewError
from private_bus import private_bus_config, run_session_arguments


def run_child(fixture: Path) -> None:
    verify_private_bus(os.environ)
    isolated = os.environ.copy()
    isolated.update({
        "KIRA_PREVIEW": "1",
        "KIRA_SNI_PREVIEW": "1",
        "KIRA_ISOLATED_SESSION_BUS": "1",
    })
    with tempfile.TemporaryDirectory(prefix="kira-headless-sni-") as directory:
        log = Path(directory) / "events.log"
        proc = subprocess.Popen(
            [str(fixture), str(log)],
            env=isolated,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
        )
        try:
            deadline = time.monotonic() + 10
            initial = ""
            while time.monotonic() < deadline:
                if proc.poll() is not None:
                    raise PreviewError("SNI GIO fixture exited before exporting D-Bus")
                try:
                    initial = sni_property_get_all(
                        SNI_FIXTURE, "/StatusNotifierItem",
                        "org.kde.StatusNotifierItem", isolated,
                    )
                except PreviewError:
                    pass
                if (
                    "kira-sni-acceptance" in initial
                    and "IconPixmap" in initial
                    and "Status" in initial
                ):
                    break
                time.sleep(0.1)
            else:
                raise PreviewError("No real StatusNotifierItem D-Bus properties on private bus")
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                current = sni_property_get_all(
                    SNI_FIXTURE, "/StatusNotifierItem",
                    "org.kde.StatusNotifierItem", isolated,
                )
                if "NeedsAttention" in current:
                    break
                time.sleep(0.2)
            else:
                raise PreviewError("No real SNI NeedsAttention state after NewStatus")
            if not log.is_file():
                raise PreviewError("Fixture never recorded name ownership")
            emitted = log.read_text(encoding="utf-8")
            if "NameAcquired" not in emitted or "NeedsAttention" not in emitted:
                raise PreviewError("Fixture emitted no true GIO status transition")
            print("[sni-smoke] Private GIO StatusNotifierItem, pixmap, status: success")
        finally:
            if proc.poll() is None:
                proc.terminate()
            try:
                proc.communicate(timeout=2)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.communicate(timeout=2)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixture", required=True, type=Path)
    parser.add_argument("--child", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    try:
        fixture = args.fixture.resolve()
        if not fixture.is_file() or not os.access(fixture, os.X_OK):
            raise PreviewError("No built SNI acceptance fixture in the Nix output")
        if args.child:
            run_child(fixture)
            return 0
        bus_runner = shutil.which("dbus-run-session")
        if not bus_runner or not shutil.which("gdbus"):
            raise PreviewError("dbus-run-session and gdbus are required")
        env = os.environ.copy()
        parent = env.get("DBUS_SESSION_BUS_ADDRESS", "")
        for name in ("KIRA_PREVIEW", "KIRA_SNI_PREVIEW", "KIRA_NOTIFICATIONS_PREVIEW",
                     "KIRA_ISOLATED_SESSION_BUS", TOKEN, PARENT_BUS):
            env.pop(name, None)
        env[PARENT_BUS] = parent
        env[TOKEN] = secrets.token_hex(16)
        with private_bus_config() as config:
            child = [
                *run_session_arguments(bus_runner, config),
                sys.executable, str(Path(__file__).resolve()),
                "--fixture", str(fixture), "--child",
            ]
            return subprocess.run(child, env=env, check=False, timeout=25).returncode
    except (PreviewError, OSError, subprocess.TimeoutExpired) as error:
        print(f"[sni-smoke] {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
