#!/usr/bin/env python3
"""Run packaged Kira in a disposable, windowed Niri compositor.

Only uses Python stdlib; never modifies the user's Niri config or systemd.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import stat
import subprocess
import sys
import tempfile
import time

TIMEOUT = 25.0


class PreviewError(Exception):
    pass


def display_path(display: str, runtime: Path) -> Path:
    if not display:
        raise PreviewError("Empty Wayland display")
    return Path(display) if os.path.isabs(display) else runtime / display


def nested_display_path(nested: str, parent: str, runtime: Path) -> Path:
    candidate = display_path(nested, runtime)
    if candidate.resolve() == display_path(parent, runtime).resolve():
        raise PreviewError("Nested Niri reported the PARENT Wayland socket; refusing to start Shell")
    return candidate


def preview_config(python: Path, script: Path, ready: Path, nonce: str) -> str:
    # JSON-quoted strings are also valid KDL strings.
    args = " ".join(json.dumps(str(value)) for value in
                    (python, script, "report", ready, nonce))
    return (
        "// Kira temporary nested Niri config (no host autostart)\n"
        "hotkey-overlay { skip-at-startup }\n"
        "prefer-no-csd\n"
        "binds {\n"
        "    Alt+Shift+Q { quit skip-confirmation=true; }\n"
        "}\n"
        f"spawn-at-startup {args}\n"
    )


def report_ready(path: Path, nonce: str) -> None:
    # Niri passes its own WAYLAND_DISPLAY into spawned commands.
    display = os.environ.get("WAYLAND_DISPLAY")
    if not display:
        raise PreviewError("Nested Niri did not pass WAYLAND_DISPLAY")
    payload = {"nonce": nonce, "display": display, "niri_socket": os.environ.get("NIRI_SOCKET")}
    staged = path.with_suffix(".partial")
    staged.write_text(json.dumps(payload), encoding="utf-8")
    staged.replace(path)


def ready_socket(path: Path) -> bool:
    try:
        if not stat.S_ISSOCK(path.stat().st_mode):
            return False
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.settimeout(0.3)
            client.connect(str(path))
        return True
    except (OSError, TimeoutError):
        return False


def await_ready(test, process: subprocess.Popen, name: str, timeout: float = TIMEOUT):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise PreviewError(f"{name}: process exited ({process.returncode})")
        result = test()
        if result is not None:
            return result
        time.sleep(0.1)
    raise PreviewError(f"Timed out waiting for {name}")


def stop_process(process: subprocess.Popen | None) -> None:
    if process is None or process.poll() is not None:
        return
    try:
        # Every launched child has its own session. Never use global pkill.
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    try:
        process.wait(timeout=3)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            return
        process.wait(timeout=3)


def log_tail(path: Path) -> str:
    if not path.exists():
        return ""
    return "\n".join(path.read_text(encoding="utf-8", errors="replace").splitlines()[-24:])


def run_preview(package: Path) -> None:
    runtime_value = os.environ.get("XDG_RUNTIME_DIR")
    parent_display = os.environ.get("WAYLAND_DISPLAY")
    if not runtime_value or not parent_display:
        raise PreviewError("An existing Wayland session is required (XDG_RUNTIME_DIR / WAYLAND_DISPLAY)")
    runtime = Path(runtime_value)
    if not ready_socket(display_path(parent_display, runtime)):
        raise PreviewError("Parent Wayland socket is not accepting connections")
    niri_path = shutil.which("niri")
    if not niri_path:
        raise PreviewError("niri is not installed; run from 'nix develop'")

    package = package.resolve()
    binaries = {name: package / "bin" / f"kira-{name}" for name in ("core", "shell")}
    for binary in binaries.values():
        if not binary.is_file() or not os.access(binary, os.X_OK):
            raise PreviewError(f"Missing {binary}; run 'nix build' first")

    with tempfile.TemporaryDirectory(prefix="kira-preview-") as directory:
        temp = Path(directory)
        nonce = os.urandom(16).hex()
        ready = temp / "ready.json"
        config = temp / "niri.kdl"
        socket_file = temp / "core.sock"
        config.write_text(
            preview_config(Path(sys.executable), Path(__file__).resolve(), ready, nonce),
            encoding="utf-8",
        )

        environment = os.environ.copy()
        for key in ("NIRI_SOCKET", "NIRI_CONFIG", "KIRA_CORE_SOCKET", "KIRA_PREVIEW"):
            environment.pop(key, None)

        validation = subprocess.run(
            [niri_path, "validate", "--config", str(config)],
            env=environment, capture_output=True, text=True, check=False,
        )
        if validation.returncode:
            raise PreviewError(f"Niri rejected the preview config:\n{validation.stdout}{validation.stderr}")

        processes: dict[str, subprocess.Popen] = {}
        handles = []
        logs = {name: temp / f"{name}.log" for name in ("niri", "core", "shell")}

        def launch(name: str, argv: list[str], env: dict[str, str]):
            handle = logs[name].open("wb")
            handles.append(handle)
            process = subprocess.Popen(
                argv, env=env, stdout=handle, stderr=subprocess.STDOUT,
                start_new_session=True,
            )
            processes[name] = process
            return process

        try:
            print("[preview] Launching nested Niri as a window (without --session).", flush=True)
            niri = launch("niri", [niri_path, "--config", str(config)], environment)

            def nested_ready():
                if not ready.exists():
                    return None
                try:
                    payload = json.loads(ready.read_text(encoding="utf-8"))
                except (OSError, ValueError):
                    return None
                if payload.get("nonce") != nonce:
                    raise PreviewError("Unexpected Niri startup handshake")
                display = payload.get("display")
                if not isinstance(display, str):
                    raise PreviewError("Missing nested Wayland display")
                socket_path = nested_display_path(display, parent_display, runtime)
                return (display, payload.get("niri_socket")) if ready_socket(socket_path) else None

            nested, nested_niri_socket = await_ready(nested_ready, niri, "nested Wayland socket")
            print(f"[preview] Nested display: {nested}; parent: {parent_display}", flush=True)

            isolated = environment.copy()
            isolated.update({
                "WAYLAND_DISPLAY": nested,
                "KIRA_CORE_SOCKET": str(socket_file),
                "KIRA_PREVIEW": "1",
            })
            # Future Niri IPC consumers must use the child compositor's socket.
            if isinstance(nested_niri_socket, str) and nested_niri_socket:
                parent_niri_socket = os.environ.get("NIRI_SOCKET")
                if parent_niri_socket and Path(nested_niri_socket).resolve() == Path(parent_niri_socket).resolve():
                    raise PreviewError("Nested Niri reported the parent Niri IPC socket")
                isolated["NIRI_SOCKET"] = nested_niri_socket
            core = launch("core", [str(binaries["core"])], isolated)
            await_ready(
                lambda: True if ready_socket(socket_file) else None,
                core, "isolated Kira Core socket", 10.0,
            )
            shell = launch("shell", [str(binaries["shell"])], isolated)
            print("[preview] Kira running inside nested Niri. Ctrl+C to exit.", flush=True)
            print("[preview] Alt+Shift+Q closes the nested Niri window.", flush=True)

            while True:
                for name in ("niri", "core", "shell"):
                    proc = processes[name]
                    if proc.poll() is not None:
                        if name == "niri" and proc.returncode == 0:
                            print("[preview] Nested Niri closed.", flush=True)
                            return
                        raise PreviewError(f"{name} exited with status {proc.returncode}")
                time.sleep(0.2)
        except KeyboardInterrupt:
            print("\n[preview] Stopping...", flush=True)
        except (OSError, PreviewError) as error:
            print(f"[preview] {error}", file=sys.stderr, flush=True)
            for name, path in logs.items():
                tail = log_tail(path)
                if tail:
                    print(f"[preview] {name} logs:\n{tail}", file=sys.stderr, flush=True)
            raise PreviewError(str(error)) from error
        finally:
            for name in ("shell", "core", "niri"):
                stop_process(processes.get(name))
            for handle in handles:
                handle.close()
            print("[preview] Preview processes stopped; temporary files cleaned up.", flush=True)


def validate_preview_config() -> None:
    niri_path = shutil.which("niri")
    if not niri_path:
        raise PreviewError("niri is not available in PATH")
    with tempfile.TemporaryDirectory(prefix="kira-preview-check-") as directory:
        config = Path(directory) / "config.kdl"
        config.write_text(
            preview_config(Path(sys.executable), Path(__file__).resolve(),
                           Path(directory) / "ready.json", "validate"),
            encoding="utf-8",
        )
        result = subprocess.run(
            [niri_path, "validate", "--config", str(config)],
            text=True, capture_output=True, check=False,
        )
        if result.returncode:
            raise PreviewError(f"Invalid preview Niri KDL:\\n{result.stdout}{result.stderr}")
        print("[preview] Niri configuration validated successfully.")


def main() -> int:
    parser = argparse.ArgumentParser(description="Isolated nested-Niri Kira Desktop preview")
    parser.add_argument("--package-dir", type=Path, default=Path("result"))
    parser.add_argument("--validate-config", action="store_true")
    parser.add_argument("mode", nargs="?", help=argparse.SUPPRESS)
    parser.add_argument("ready_path", nargs="?", help=argparse.SUPPRESS)
    parser.add_argument("nonce", nargs="?", help=argparse.SUPPRESS)
    args = parser.parse_args()
    try:
        if args.validate_config:
            validate_preview_config()
        elif args.mode == "report" and args.ready_path and args.nonce:
            report_ready(Path(args.ready_path), args.nonce)
        elif args.mode is None:
            run_preview(args.package_dir)
        else:
            parser.error("Unknown preview mode")
    except PreviewError as error:
        print(f"[preview] {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
