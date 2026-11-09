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
from private_bus import PRIVATE_BUS_DIR, private_bus_config, run_session_arguments


MENU_PATH = "/KiraAcceptanceMenu"
MENU_IFACE = "com.canonical.dbusmenu"


def menu_call(method: str, env: dict[str, str]) -> str:
    # All method/argument pairs are fixed. Only the disposable fixture name
    # on the separately attested D-Bus session may be contacted.
    arguments = {
        "GetLayout": ["0", "-1", "[]"],
        "AboutToShow": ["20"],
        "ClickLeaf": ["21", "clicked", "<int32 0>", "0"],
        "ClickCheck": ["40", "clicked", "<int32 0>", "0"],
    }
    if method not in arguments:
        raise PreviewError("Unapproved SNI menu test method")
    actual = "Event" if method.startswith("Click") else method
    result = subprocess.run(
        [
            "gdbus", "call", "--session", "--dest", SNI_FIXTURE,
            "--object-path", MENU_PATH, "--method", f"{MENU_IFACE}.{actual}",
            "--", *arguments[method],
        ],
        env=env, text=True, capture_output=True, timeout=8, check=False,
    )
    if result.returncode:
        raise PreviewError(f"Private SNI menu {method} failed: {result.stderr[:220]}")
    return result.stdout.strip()


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
            # GIO must really export a typed (u(ia{sv}av)) menu; checking
            # only an SNI IconPixmap does not exercise a native cascade.
            if MENU_PATH not in current:
                raise PreviewError("SNI did not advertise a typed live menu object")
            before = menu_call("GetLayout", isolated)
            if not all(name in before for name in ("Open", "Tools", "Disabled", "Checkbox")):
                raise PreviewError("Private GetLayout omitted real root menu entries")
            result = menu_call("AboutToShow", isolated)
            if "true" not in result.lower():
                raise PreviewError("Lazy AboutToShow did not mutate the real menu")
            after = menu_call("GetLayout", isolated)
            if not all(name in after for name in ("Clear", "Advanced", "Deep")):
                raise PreviewError("Real submenu did not appear after AboutToShow")
            if menu_call("ClickLeaf", isolated) != "()":
                raise PreviewError("Menu leaf Event returned an invalid reply")
            if menu_call("ClickCheck", isolated) != "()":
                raise PreviewError("Menu checkbox Event returned an invalid reply")
            changed = menu_call("GetLayout", isolated)
            if changed == after or "toggle-state" not in changed:
                raise PreviewError("Real checkbox Event failed to update its menu layout")
            if not log.is_file():
                raise PreviewError("Fixture never recorded name ownership")
            emitted = log.read_text(encoding="utf-8")
            if not all(
                marker in emitted
                for marker in (
                    "NameAcquired", "NeedsAttention", "MenuAboutToShow:20",
                    "MenuClick:21", "MenuClick:40",
                )
            ):
                raise PreviewError("Fixture omitted a real SNI menu/status transition")
            print(
                "[sni-smoke] Private GIO SNI, pixmap/status and real "
                "dbusmenu lazy hierarchy/actions: success"
            )
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
                     "KIRA_ISOLATED_SESSION_BUS", TOKEN, PARENT_BUS, PRIVATE_BUS_DIR):
            env.pop(name, None)
        env[PARENT_BUS] = parent
        env[TOKEN] = secrets.token_hex(16)
        with private_bus_config() as config:
            env[PRIVATE_BUS_DIR] = str(config.parent)
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
