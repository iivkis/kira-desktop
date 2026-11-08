#!/usr/bin/env python3
"""Real D-Bus notification probes and timestamped nested-Niri evidence.

This module is callable only by the private-bus preview. It does not create
synthetic UI cards, assert pixel parity, modify KDL or own any bus name.
All events go through the installed Freedesktop Notifications service.
"""
from __future__ import annotations

import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import threading
import time
from typing import Callable

from capture_native_motion import capture_frames
from dev_preview import PreviewError

BUS_NAME = "org.freedesktop.Notifications"
BUS_PATH = "/org/freedesktop/Notifications"
BUS_METHOD = BUS_NAME
SNI_NAME = "org.kde.StatusNotifierWatcher"
SNI_PATH = "/StatusNotifierWatcher"
SNI_FIXTURE = "org.kira.AcceptanceSNI"
BASELINE = "305139949f49176487ef9759fc32c228eadff408"
POLL_SECONDS = 12
FRAME_COUNT = 42
FRAME_INTERVAL_MS = 16


def gdbus_args(method: str, *parameters: str) -> list[str]:
    if method not in {
        "GetServerInformation", "GetCapabilities", "Notify", "CloseNotification"
    }:
        raise PreviewError("Only fixed Notifications D-Bus probes are allowed")
    return [
        "gdbus", "call", "--session", "--dest", BUS_NAME,
        "--object-path", BUS_PATH, "--method", BUS_METHOD + "." + method,
        *parameters,
    ]


def gdbus_call(method: str, args: tuple[str, ...], env: dict[str, str],
               *, runner=subprocess.run, allow_error: bool = False) -> str:
    result = runner(
        gdbus_args(method, *args),
        env=env, text=True, capture_output=True, check=False, timeout=8,
    )
    if result.returncode and not allow_error:
        raise PreviewError(
            f"Private D-Bus call {method} failed: {result.stderr.strip()[:350]}"
        )
    return result.stdout if result.returncode == 0 else result.stderr


def notification_id(answer: str) -> int:
    # gdbus emits "(uint32 2,)" for unsigned IDs. Do not guess an ID
    # when a server returns a different signature or an error.
    match = re.fullmatch(r"\(\s*uint32\s+([0-9]+)\s*,\s*\)", answer.strip())
    if not match:
        raise PreviewError(f"Unexpected Freedesktop Notify reply: {answer[:160]}")
    value = int(match.group(1))
    if not (0 < value <= 0xFFFFFFFF):
        raise PreviewError("Notification server returned an invalid ID")
    return value


def notify(summary: str, replace: int, env: dict[str, str],
           *, timeout: int = 5000, runner=subprocess.run) -> int:
    if not re.fullmatch(r"[A-Za-z0-9_-]{1,48}", summary):
        raise PreviewError("Scenario identifiers are fixed, bounded ASCII")
    return notification_id(gdbus_call(
        "Notify",
        ("Kira Acceptance", str(replace), "", summary,
         "Real private-session D-Bus notification", "[]", "{}", str(timeout)),
        env, runner=runner,
    ))


def close_notification(identifier: int, env: dict[str, str],
                       *, runner=subprocess.run) -> None:
    if not (0 < identifier <= 0xFFFFFFFF):
        raise PreviewError("Never close an untrusted or invalid notification ID")
    answer = gdbus_call(
        "CloseNotification", (str(identifier),), env, runner=runner,
    )
    if answer.strip() != "()":
        raise PreviewError(f"Unexpected CloseNotification reply: {answer[:160]}")


def await_native_server(
    env: dict[str, str], *, timeout: float = POLL_SECONDS,
    runner=subprocess.run, sleep=time.sleep, clock=time.monotonic,
) -> str:
    deadline = clock() + timeout
    while clock() < deadline:
        try:
            result = gdbus_call(
                "GetServerInformation", (), env, runner=runner,
            )
            if "Kira Desktop" not in result or "Kira" not in result:
                raise PreviewError("A foreign Notifications daemon owns the private test bus")
            return result.strip()
        except PreviewError as error:
            if "foreign" in str(error):
                raise
        sleep(0.15)
    raise PreviewError(
        "Native Notification Host did not claim the isolated bus; "
        "inspect Shell logs / monitor input-shape support"
    )


def sni_property_get_all(
    destination: str,
    path: str,
    interface: str,
    env: dict[str, str],
    *, runner=subprocess.run,
) -> str:
    if (destination, path, interface) not in {
        (SNI_NAME, SNI_PATH, SNI_NAME),
        (SNI_FIXTURE, "/StatusNotifierItem", "org.kde.StatusNotifierItem"),
    }:
        raise PreviewError("SNI probes accept only the private acceptance fixture")
    result = runner(
        [
            "gdbus", "call", "--session", "--dest", destination,
            "--object-path", path,
            "--method", "org.freedesktop.DBus.Properties.GetAll",
            interface,
        ],
        env=env, text=True, capture_output=True, check=False, timeout=8,
    )
    if result.returncode:
        raise PreviewError("Private SNI property query failed: " + result.stderr[:250])
    return result.stdout.strip()


def await_real_sni_fixture(
    env: dict[str, str], *, timeout: float = POLL_SECONDS,
    runner=subprocess.run, sleep=time.sleep, clock=time.monotonic,
) -> dict[str, str]:
    deadline = clock() + timeout
    while clock() < deadline:
        try:
            roster = sni_property_get_all(
                SNI_NAME, SNI_PATH, SNI_NAME, env, runner=runner,
            )
            properties = sni_property_get_all(
                SNI_FIXTURE, "/StatusNotifierItem",
                "org.kde.StatusNotifierItem", env, runner=runner,
            )
            if (
                "/StatusNotifierItem" in roster
                and "RegisteredStatusNotifierItems" in roster
                and "ProtocolVersion" in roster
                and "IconPixmap" in properties
                and "kira-sni-acceptance" in properties
            ):
                return {"watcher": roster, "item": properties}
        except PreviewError:
            pass
        sleep(0.15)
    raise PreviewError(
        "Native watcher did not register the real GIO SNI fixture on the private bus"
    )


def capture_event(
    display: str, output: Path, label: str,
    parent_environment: dict[str, str],
    session_environment: dict[str, str],
    action: Callable[[], int | None],
    *, runner=subprocess.run, clock=time.monotonic,
) -> tuple[Path, dict[str, object]]:
    """Start real capture before the D-Bus event, with no fake frame timing."""
    result: dict[str, object] = {}
    error: list[Exception] = []
    started = clock()

    def emit() -> None:
        time.sleep(0.12)
        try:
            result["elapsed_ms"] = round((clock() - started) * 1000.0, 3)
            result["id"] = action()
        except Exception as failure:
            error.append(failure)

    producer = threading.Thread(target=emit, name="kira-acceptance-dbus", daemon=True)
    producer.start()
    try:
        manifest = capture_frames(
            display, output, label, FRAME_COUNT, FRAME_INTERVAL_MS,
            parent_environment, grim="grim", runner=runner,
        )
    finally:
        producer.join(timeout=10)
    if producer.is_alive():
        raise PreviewError("Private D-Bus event was not completed")
    if error:
        raise PreviewError(f"Private D-Bus stimulus failed: {error[0]}") from error[0]
    if "elapsed_ms" not in result:
        raise PreviewError("Missing actual D-Bus event time")
    return manifest, result


def run_acceptance(
    nested_display: str,
    parent_environment: dict[str, str],
    isolated_environment: dict[str, str],
    output: Path,
) -> Path:
    """The preview owns lifecycle: run only after its Shell is running."""
    from accept_nested import verify_private_bus

    # Both the sender and receiver must use the new dbus-run-session
    # address. We never forward the parent's bus into this scenario.
    verify_private_bus(parent_environment)
    if (
        parent_environment.get("DBUS_SESSION_BUS_ADDRESS")
        != isolated_environment.get("DBUS_SESSION_BUS_ADDRESS")
        or isolated_environment.get("KIRA_NOTIFICATIONS_PREVIEW") != "1"
        or isolated_environment.get("KIRA_ISOLATED_SESSION_BUS") != "1"
    ):
        raise PreviewError("Preview did not opt in to the verified private bus")

    if not shutil.which("grim") or not shutil.which("gdbus"):
        raise PreviewError("Both grim and gdbus are required")
    if output.exists():
        raise PreviewError("Do not overwrite prior unreviewed evidence")
    output.mkdir(parents=True)
    server = await_native_server(isolated_environment)
    sni_initial = await_real_sni_fixture(isolated_environment)
    arrival, event_in = capture_event(
        nested_display, output / "entry", "toast-entry",
        parent_environment, isolated_environment,
        lambda: notify("entry_real", 0, isolated_environment, timeout=10000),
    )
    identifier = event_in.get("id")
    if not isinstance(identifier, int):
        raise PreviewError("Missing first private D-Bus notification ID")

    # The queue guarantees at least 3 seconds of painted dwell. Give
    # compositor scheduling margin before measuring the actual exit.
    time.sleep(3.8)
    departure, event_out = capture_event(
        nested_display, output / "exit", "toast-exit",
        parent_environment, isolated_environment,
        lambda: (close_notification(identifier, isolated_environment), identifier)[1],
    )

    # Contract probes are real D-Bus calls; they are *not* claims about
    # visual reception or correspondence to the pinned main implementation.
    second = notify("replacement_a", 0, isolated_environment, timeout=0)
    replaced = notify("replacement_b", second, isolated_environment, timeout=0)
    if replaced != second:
        raise PreviewError("Freedesktop replace_id returned a different notification ID")
    third = notify("close_by_id", 0, isolated_environment, timeout=5000)
    close_notification(third, isolated_environment)

    # This is a *live* SNI registration, not a fake application in GTK.
    # Capture its actual status and themed/pixmap state on the nested
    # compositor. Opening its popup and comparing source frames remain
    # manual paired-capture gates.
    sni_final = await_real_sni_fixture(isolated_environment)
    if "NeedsAttention" not in sni_final["item"]:
        raise PreviewError("Real SNI Status/NewStatus did not reach private D-Bus")
    tray_capture = capture_frames(
        nested_display, output / "tray", "tray-live-sni", 20, 33,
        parent_environment, grim="grim",
    )

    summary = {
        "status": "unreviewed",
        "source_baseline": BASELINE,
        "scenario": "private-dbus-notification-host",
        "session_bus": "new dbus-run-session (no address recorded)",
        "nested_wayland": nested_display,
        "server_information": server,
        "captures": [
            str(arrival.relative_to(output)),
            str(departure.relative_to(output)),
            str(tray_capture.relative_to(output)),
        ],
        "sni_fixture": {
            "protocol": "org.kde.StatusNotifierItem registered through actual watcher",
            "registered_on_private_bus": True,
            "sent_real_status_changed": "NeedsAttention" in sni_final["item"],
            "initial_item_properties": sni_initial["item"][:1200],
        },
        "observed_events": [
            {"type": "notify", "id": identifier, **event_in},
            {"type": "close", "id": identifier, **event_out},
            {"type": "replace_id", "id": second, "same_id": replaced == second},
            {"type": "close_by_id", "id": third},
        ],
        "limitations": [
            "No source-main capture was performed",
            "No pixel, input hit-testing or motion parity is certified",
            "GDBus call times are not compositor presentation times",
            "The tray popup and menu click navigation were not automated",
            "Visual review and scaled multi-output Niri tests remain mandatory",
        ],
    }
    manifest = output / "scenario-manifest.json"
    staging = output / "scenario-manifest.json.tmp"
    staging.write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    staging.replace(manifest)
    return manifest
