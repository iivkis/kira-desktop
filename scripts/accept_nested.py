#!/usr/bin/env python3
"""Run native motion acceptance only inside a NEW private D-Bus session.

The parent Wayland display is used solely as the window for a disposable
nested Niri. The parent D-Bus session is NEVER used by Kira's Notification
or StatusNotifier watcher. No user's Niri/Kira configuration is modified.
This command does not certify main/native parity or review any PNG.
"""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import re
import secrets
import shutil
import subprocess
import sys

from dev_preview import PreviewError
from private_bus import (
    PRIVATE_BUS_DIR, private_bus_config, run_session_arguments,
    verify_private_bus_socket,
)

TOKEN = "KIRA_ACCEPTANCE_TOKEN"
PARENT_BUS = "KIRA_ACCEPTANCE_PARENT_BUS"


def child_environment(parent: dict[str, str], nonce: str) -> dict[str, str]:
    """Strip inherited preview opt-ins, then seal a private-bus handshake."""
    if not re.fullmatch(r"[0-9a-f]{32}", nonce):
        raise PreviewError("The acceptance handshake needs a fresh 128-bit token")
    if not parent.get("XDG_RUNTIME_DIR") or not parent.get("WAYLAND_DISPLAY"):
        raise PreviewError("A real parent Wayland session is required")
    env = parent.copy()
    for name in (
        "KIRA_PREVIEW",
        "KIRA_NOTIFICATIONS_PREVIEW",
        "KIRA_SNI_PREVIEW",
        "KIRA_ISOLATED_SESSION_BUS",
        "NIRI_SOCKET",
        "NIRI_CONFIG",
        "KIRA_CORE_SOCKET",
        TOKEN,
        PARENT_BUS,
        PRIVATE_BUS_DIR,
    ):
        env.pop(name, None)
    env[PARENT_BUS] = parent.get("DBUS_SESSION_BUS_ADDRESS", "")
    env[TOKEN] = nonce
    return env


def verify_private_bus(env: dict[str, str]) -> None:
    """Call before enabling either Freedesktop name in native preview."""
    nonce = env.get(TOKEN, "")
    current = env.get("DBUS_SESSION_BUS_ADDRESS", "")
    parent = env.get(PARENT_BUS, "")
    if not re.fullmatch(r"[0-9a-f]{32}", nonce):
        raise PreviewError("Missing private-bus acceptance handshake")
    if not current.startswith("unix:"):
        raise PreviewError("No isolated Unix D-Bus session address")
    if current == parent:
        raise PreviewError("Nested acceptance inherited the user's D-Bus bus; refusing")
    try:
        verify_private_bus_socket(env)
    except ValueError as error:
        raise PreviewError(str(error)) from error
    if env.get("KIRA_NOTIFICATIONS_PREVIEW") or env.get("KIRA_SNI_PREVIEW"):
        raise PreviewError("An inherited preview ownership override is forbidden")


def acceptance_command(
    script: Path,
    package: Path,
    output: Path,
    bus_runner: str,
    config: Path,
    bin_dir: Path | None = None,
) -> list[str]:
    argv = [
        *run_session_arguments(bus_runner, config),
        sys.executable, str(script),
        "--acceptance-notifications",
        "--evidence-dir", str(output),
        "--package-dir", str(package),
    ]
    if bin_dir is not None:
        argv.extend(["--bin-dir", str(bin_dir)])
    return argv


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package-dir", type=Path, default=Path("result"))
    parser.add_argument("--bin-dir", type=Path, help="Built kira-core and kira-shell directory")
    parser.add_argument("--output", type=Path, required=True, help="NEW evidence directory")
    args = parser.parse_args()
    try:
        bus_runner = shutil.which("dbus-run-session")
        if bus_runner is None:
            raise PreviewError("dbus-run-session is required (enter nix develop)")
        if shutil.which("gdbus") is None or shutil.which("grim") is None:
            raise PreviewError("gdbus and grim are required (enter nix develop)")
        output = args.output.resolve()
        if output.exists():
            raise PreviewError("Evidence directory already exists; never overwrite an earlier run")
        env = child_environment(os.environ, secrets.token_hex(16))
        # The isolated daemon cannot depend on a host /etc/dbus-1/session.conf,
        # activate an external user service, or create a socket outside its
        # mode-0700 throwaway directory.
        with private_bus_config() as config:
            env[PRIVATE_BUS_DIR] = str(config.parent)
            command = acceptance_command(
                Path(__file__).resolve().parent / "dev_preview.py",
                args.package_dir.resolve(),
                output, bus_runner, config,
                args.bin_dir.resolve() if args.bin_dir else None,
            )
            return subprocess.run(command, env=env, check=False).returncode
    except (PreviewError, OSError) as error:
        print(f"[acceptance] {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
