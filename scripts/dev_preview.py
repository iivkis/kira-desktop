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
        "hotkey-overlay {\n    skip-at-startup\n}\n"
        "prefer-no-csd\n"
        "binds {\n"
        "    Alt+Shift+Q { quit skip-confirmation=true; }\n"
        "    // Mod is Alt in a nested winit Niri; Super remains with host.\n"
        "    // Do not require Alt+Shift: it commonly switches keyboard layout.\n"
        "    Mod+Return repeat=false { spawn \"foot\"; }\n"
        "    Mod+1 { focus-workspace 1; }\n"
        "    Mod+2 { focus-workspace 2; }\n"
        "    Mod+3 { focus-workspace 3; }\n"
        "    Mod+Ctrl+1 repeat=false { move-window-to-workspace focus=true 1; }\n"
        "    Mod+Ctrl+2 repeat=false { move-window-to-workspace focus=true 2; }\n"
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
    if process is None:
        return
    # Every launched child owns a new process group. Even after the leader
    # exits, spawned descendants can remain in that group.
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    if process.poll() is None:
        try:
            process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            pass
    # Graceful leaders may exit while grandchildren keep running.
    time.sleep(0.1)
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    if process.poll() is None:
        process.wait(timeout=2)


def interrupt_preview(_signum: int, _frame: object) -> None:
    """Turn Taskfile's SIGTERM into the same cleanup path as Ctrl+C."""
    raise KeyboardInterrupt


def log_tail(path: Path, limit: int = 80) -> str:
    if not path.exists():
        return ""
    return "\n".join(path.read_text(encoding="utf-8", errors="replace").splitlines()[-limit:])


def print_log_changes(path: Path, offset: int, label: str) -> int:
    """Print newly emitted log lines, preserving the offset across polls."""
    if not path.is_file():
        return offset
    with path.open("rb") as stream:
        stream.seek(offset)
        raw = stream.read(65536)
        current = stream.tell()
    if raw:
        for line in raw.decode("utf-8", errors="replace").splitlines():
            print(f"[preview:{label}] {line}", file=sys.stderr, flush=True)
    return current


def run_preview(
    package: Path,
    bin_dir: Path | None = None,
    ui_dir: Path | None = None,
    debug: bool = False,
    qa_output_dir: Path | None = None,
    qa_smoke: bool = False,
) -> None:
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
    binaries_directory = (bin_dir if bin_dir is not None else package / "bin").resolve()
    binaries = {
        name: binaries_directory / f"kira-{name}" for name in ("core", "shell")
    }
    for binary in binaries.values():
        if not binary.is_file() or not os.access(binary, os.X_OK):
            raise PreviewError(f"Missing {binary}; build the preview binaries first")

    if bin_dir is not None and ui_dir is None:
        raise PreviewError("Debug preview requires --ui-dir with compiled frontend assets")
    if ui_dir is not None:
        ui_dir = ui_dir.resolve()
        if not (ui_dir / "index.html").is_file():
            raise PreviewError(f"Missing {ui_dir / 'index.html'}; run 'task frontend:build'")

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
        for key in ("NIRI_SOCKET", "NIRI_CONFIG", "KIRA_CORE_SOCKET", "KIRA_PREVIEW", "KIRA_UI_DIR"):
            environment.pop(key, None)

        validation = subprocess.run(
            [niri_path, "validate", "--config", str(config)],
            env=environment, capture_output=True, text=True, check=False, timeout=10,
        )
        if validation.returncode:
            raise PreviewError(f"Niri rejected the preview config:\n{validation.stdout}{validation.stderr}")

        processes: dict[str, subprocess.Popen] = {}
        handles = []
        logs = {name: temp / f"{name}.log" for name in ("niri", "core", "shell")}
        debug_offsets = {"core": 0, "shell": 0}

        def launch(name: str, argv: list[str], env: dict[str, str]):
            handle = logs[name].open("wb")
            handles.append(handle)
            process = subprocess.Popen(
                argv, env=env, stdout=handle, stderr=subprocess.STDOUT,
                start_new_session=True,
            )
            processes[name] = process
            return process

        previous_sigterm_handler = signal.signal(signal.SIGTERM, interrupt_preview)
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
            if ui_dir is not None:
                isolated["KIRA_UI_DIR"] = str(ui_dir)
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
            if not qa_smoke and qa_output_dir is None:
                print("[preview] Nested Mod=Alt: Alt+Enter opens foot; Alt+1/2/3 switches workspace.", flush=True)
                print("[preview] Alt+Ctrl+1/2 moves the focused window to workspace 1/2 AND follows it.", flush=True)
                print("[preview] If a shortcut does not work, check host layout shortcuts and run 'niri msg --json workspaces' in the nested foot terminal.", flush=True)

            if qa_smoke or qa_output_dir is not None:
                from qa_visual import capture, verify_smoke
                try:
                    if qa_output_dir is not None:
                        print("[preview] OPT-IN VISUAL CAPTURE: nested Niri will change scales and save four PNGs.", flush=True)
                        evidence = capture(
                            niri_path, isolated, os.environ.get("NIRI_SOCKET"),
                            logs["shell"], socket_file, qa_output_dir,
                            packaged=(bin_dir is None),
                        )
                        print(f"[preview] QA evidence captured: {evidence}", flush=True)
                        print("[preview] Visual review is still pending.", flush=True)
                    else:
                        print("[preview] READ-ONLY QA: checking Niri, Core and Svelte without changing scale or taking screenshots.", flush=True)
                        state, output = verify_smoke(
                            niri_path, isolated, os.environ.get("NIRI_SOCKET"),
                            logs["shell"], socket_file,
                        )
                        print(f"[preview] QA smoke PASS: nested output={output}; Core={state['status']}; WebKit/Svelte DOM verified.", flush=True)
                except Exception as error:
                    raise PreviewError(f"QA smoke/capture failed: {error}") from error
                return

            while True:
                if debug:
                    for name in ("core", "shell"):
                        debug_offsets[name] = print_log_changes(
                            logs[name], debug_offsets[name], name
                        )
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
            signal.signal(signal.SIGTERM, previous_sigterm_handler)
            for name in ("shell", "core", "niri"):
                stop_process(processes.get(name))
            for handle in handles:
                handle.close()

            if debug:
                for name in ("core", "shell"):
                    print_log_changes(logs[name], debug_offsets[name], name)
            else:
                # A successful Wayland surface is not evidence that the UI
                # mounted. Preserve clues on normal Ctrl+C as well as crashes.
                shell_log = logs["shell"].read_text(
                    encoding="utf-8", errors="replace"
                ) if logs["shell"].exists() else ""
                if "Svelte entrypoint mounted" not in shell_log:
                    tail = log_tail(logs["shell"])
                    if tail:
                        print(f"[preview] Shell diagnostics (UI mount not confirmed):\n{tail}", file=sys.stderr)

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
    parser.add_argument("--bin-dir", type=Path, help="Cargo debug binaries directory")
    parser.add_argument("--ui-dir", type=Path, help="Compiled frontend assets directory")
    parser.add_argument("--debug", action="store_true", help="Stream Core and WebKit Shell logs")
    qa_mode = parser.add_mutually_exclusive_group()
    qa_mode.add_argument("--qa-smoke", action="store_true", help="Read-only Niri/Core/Svelte smoke, no screenshots or scaling")
    qa_mode.add_argument("--qa-output-dir", type=Path, help="Explicit opt-in: change Niri scale and save PNGs with grim")
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
            run_preview(args.package_dir, args.bin_dir, args.ui_dir, args.debug, args.qa_output_dir, args.qa_smoke)
        else:
            parser.error("Unknown preview mode")
    except PreviewError as error:
        print(f"[preview] {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
