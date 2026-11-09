#!/usr/bin/env python3
"""Genuine, isolated virtual Wayland smoke: Weston -> Niri -> packaged Kira.

No host Wayland, Niri, KDL, HOME, or D-Bus session may be reused. PNGs are
unreviewed evidence, never a main/native parity or keyboard-motion claim.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import stat
import subprocess
import sys
import tempfile

from dev_preview import PreviewError, await_ready, log_tail, ready_socket, stop_process
from validate_nested_evidence import MAIN_SHA, validate_bundle

MAX_SECONDS = 240


def validate_virtual_viewport(
    captures: list[dict], width: int, height: int,
) -> None:
    """Inspect REAL grim PNG dimensions, never trust the Weston CLI alone.

    Niri runs inside the outer headless Weston. The Niri window can have
    different dimensions from Weston, so each requested viewport has to be
    checked against the actual three independent capture manifests.
    """
    if len(captures) != 3:
        raise PreviewError("Incomplete nested viewport evidence")
    expected = (width, height)
    for capture in captures:
        actual = tuple(capture.get("dimensions", ()))
        if actual != expected:
            raise PreviewError(
                f"Nested Niri screenshot viewport {actual} differs from "
                f"requested {expected}; do not certify the wrong layout"
            )


def weston_command(binary: str, socket: str, log: Path, width: int, height: int) -> list[str]:
    if not re.fullmatch(r"kira-virtual-[0-9a-f]{12}", socket):
        raise PreviewError("Unbounded virtual Wayland socket name")
    if not (640 <= width <= 2560 and 480 <= height <= 1600):
        raise PreviewError("Invalid virtual display dimensions")
    return [
        binary, "--backend=headless-backend.so", "--renderer=gl",
        "--no-config", "--idle-time=0", "--socket=" + socket,
        "--width=" + str(width), "--height=" + str(height),
        "--log=" + str(log),
    ]


def isolated_virtual_environment(
    ambient: dict[str, str], root: Path, socket: str,
) -> dict[str, str]:
    if ambient.get("WAYLAND_DISPLAY") or ambient.get("NIRI_SOCKET"):
        raise PreviewError("Refusing a headless runner inside an existing desktop session")
    if not root.is_absolute() or root.is_symlink():
        raise PreviewError("Virtual session root must be an absolute real directory")
    if not re.fullmatch(r"kira-virtual-[0-9a-f]{12}", socket):
        raise PreviewError("Invalid private Wayland socket")
    env = ambient.copy()
    for name in (
        "WAYLAND_DISPLAY", "NIRI_SOCKET", "NIRI_CONFIG", "KIRA_CONFIG_FILE",
        "KIRA_CORE_SOCKET", "KIRA_PREVIEW", "KIRA_NOTIFICATIONS_PREVIEW",
        "KIRA_SNI_PREVIEW", "KIRA_ISOLATED_SESSION_BUS",
        "KIRA_ACCEPTANCE_PRIVATE_BUS_DIR", "KIRA_ACCEPTANCE_PARENT_BUS",
        "KIRA_ACCEPTANCE_TOKEN", "DBUS_SESSION_BUS_ADDRESS",
        "XDG_RUNTIME_DIR", "XDG_CONFIG_HOME", "XDG_STATE_HOME",
        "XDG_CACHE_HOME", "XDG_DATA_HOME", "HOME",
    ):
        env.pop(name, None)
    for name, directory in (
        ("XDG_RUNTIME_DIR", "runtime"), ("XDG_CONFIG_HOME", "config"),
        ("XDG_STATE_HOME", "state"), ("XDG_CACHE_HOME", "cache"),
        ("XDG_DATA_HOME", "data"), ("HOME", "home"),
    ):
        target = root / directory
        target.mkdir(mode=0o700)
        target.chmod(0o700)
        env[name] = str(target)
    env["WAYLAND_DISPLAY"] = socket
    env["XDG_SESSION_TYPE"] = "wayland"
    env["LIBGL_ALWAYS_SOFTWARE"] = "1"
    env["GALLIUM_DRIVER"] = "llvmpipe"
    return env


def verify_virtual_socket(env: dict[str, str], socket: str) -> Path:
    runtime = Path(env["XDG_RUNTIME_DIR"])
    st = runtime.lstat()
    if (not stat.S_ISDIR(st.st_mode) or st.st_uid != os.getuid()
            or stat.S_IMODE(st.st_mode) != 0o700):
        raise PreviewError("Virtual runtime must be an owned 0700 directory")
    path = runtime / socket
    if path.is_symlink() or path.parent != runtime:
        raise PreviewError("Weston socket escaped the virtual runtime")
    if not path.is_socket() or path.stat().st_uid != os.getuid():
        raise PreviewError("Weston did not create an owned Unix socket")
    if not ready_socket(path):
        raise PreviewError("Weston socket refused a real Wayland connection")
    return path


def accept_command(
    script: Path, package: Path, output: Path, *, profile: str = "smoke",
) -> list[str]:
    if profile not in ("smoke", "full"):
        raise PreviewError("Unknown headless capture profile")
    return [
        sys.executable, str(script), "--package-dir", str(package),
        "--output", str(output), "--profile", profile,
    ]


def run_virtual(
    package: Path, output: Path, width: int, height: int,
    *, timeout_seconds: int = MAX_SECONDS, profile: str = "smoke",
    ambient: dict[str, str] | None = None,
) -> Path:
    ambient = os.environ if ambient is None else ambient
    if profile not in ("smoke", "full"):
        raise PreviewError("Unknown headless capture profile")
    if not 30 <= timeout_seconds <= MAX_SECONDS:
        raise PreviewError("Acceptance timeout outside safe bounds")
    if ambient.get("WAYLAND_DISPLAY") or ambient.get("NIRI_SOCKET"):
        raise PreviewError("Refusing to start virtual Weston inside a live desktop")
    if output.exists() or output.is_symlink():
        raise PreviewError("Previous graphical evidence must never be overwritten")
    if not (package / "bin/kira-shell").is_file():
        raise PreviewError("Packaged GTK Shell is missing; run nix build .#default")
    programs = ("weston", "niri", "grim", "dbus-run-session", "gdbus")
    for program in programs:
        if shutil.which(program) is None:
            raise PreviewError("Missing graphical CI dependency: " + program)
    with tempfile.TemporaryDirectory(prefix="kira-virtual-wayland-") as temporary:
        root = Path(temporary).resolve()
        socket = "kira-virtual-" + secrets.token_hex(6)
        env = isolated_virtual_environment(ambient, root, socket)
        weston_env = env.copy()
        weston_env.pop("WAYLAND_DISPLAY")
        log = root / "weston.log"
        compositor = subprocess.Popen(
            weston_command(shutil.which("weston") or "weston", socket, log, width, height),
            env=weston_env, stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            start_new_session=True,
        )
        try:
            await_ready(
                lambda: verify_virtual_socket(env, socket)
                if (root / "runtime" / socket).exists() else None,
                compositor, "virtual Weston Wayland socket", timeout=20.0,
            )
            print("[virtual] Real outer Weston and private 0700 runtime ready", flush=True)
            # The existing coordinator opens an entirely separate private
            # D-Bus, nested Niri IPC and Wayland display before launching Kira.
            script = Path(__file__).resolve().parent / "accept_nested.py"
            result = subprocess.run(
                accept_command(script, package, output, profile=profile),
                env=env, text=True, capture_output=True,
                check=False, timeout=timeout_seconds,
            )
            if result.stdout:
                print(result.stdout[-12000:], flush=True)
            if result.returncode:
                raise PreviewError(
                    "Real nested Niri capture failed: " + result.stderr[-2500:]
                )
            if compositor.poll() is not None:
                raise PreviewError("Weston died before capture validation")
            captures = validate_bundle(output)
            validate_virtual_viewport(captures, width, height)
            from check_native_visual_smoke import verify_toast_transitions
            visual = verify_toast_transitions(output)
            (output / "visual-smoke.json").write_text(
                json.dumps(visual, ensure_ascii=False, indent=2) + "\n",
                encoding="utf-8",
            )
            index = output / "review-index.html"
            if len(captures) != 3 or not index.is_file():
                raise PreviewError("Missing validated native-only review index")
            (output / "virtual-session.json").write_text(
                json.dumps({
                    "status": "unreviewed", "source_baseline": MAIN_SHA,
                    "kind": "headless Weston -> nested Niri -> packaged GTK",
                    "viewport": [width, height],
                    "captures": [capture["manifest"] for capture in captures],
                    "limitations": [
                        "No source main comparison or visual human approval",
                        "No keyboard, motion, or input-region acceptance",
                    ],
                }, indent=2) + "\n", encoding="utf-8",
            )
            print(
                f"[virtual] Verified actual nested PNG viewport "
                f"{width}x{height} across {len(captures)} capture sequences",
                flush=True,
            )
            print("[virtual] Actual nested Wayland PNG frames: " + str(index))
            return index
        except (PreviewError, subprocess.TimeoutExpired):
            print("[virtual] Weston log tail:\n" + log_tail(log, 30), file=sys.stderr)
            raise
        finally:
            stop_process(compositor)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package-dir", type=Path, default=Path("result"))
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--width", type=int, default=1280)
    parser.add_argument("--height", type=int, default=800)
    parser.add_argument("--profile", choices=("smoke", "full"), default="smoke")
    parser.add_argument("--timeout-seconds", type=int, default=MAX_SECONDS)
    args = parser.parse_args()
    try:
        run_virtual(
            args.package_dir.resolve(), args.output.resolve(),
            args.width, args.height, timeout_seconds=args.timeout_seconds,
            profile=args.profile,
        )
        return 0
    except (PreviewError, OSError, subprocess.TimeoutExpired) as error:
        print("[virtual] FAIL CLOSED: " + str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
