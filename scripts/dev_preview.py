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
        "    Mod+T repeat=false { spawn \"foot\"; }\n"
        "    Mod+1 { focus-workspace 1; }\n"
        "    Mod+2 { focus-workspace 2; }\n"
        "    Mod+3 { focus-workspace 3; }\n"
        "    Mod+Ctrl+1 repeat=false { move-window-to-workspace focus=false 1; }\n"
        "    Mod+Ctrl+2 repeat=false { move-window-to-workspace focus=false 2; }\n"
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


def isolated_shell_environment(
    environment: dict[str, str],
    nested_display: str,
    nested_niri_socket: str | None,
    parent_niri_socket: str | None,
    temp: Path,
    config: Path,
    socket_file: Path,
    active_config: Path,
) -> dict[str, str]:
    """Bind display actions and any KDL writes to *only* the nested preview."""
    if not nested_niri_socket:
        raise PreviewError("Nested Niri did not report its IPC socket; refusing unsafe preview")
    path = Path(nested_niri_socket)
    if not path.is_absolute() or not path.is_socket():
        raise PreviewError("Nested Niri IPC socket is missing or not a Unix socket")
    if parent_niri_socket and path.resolve() == Path(parent_niri_socket).resolve():
        raise PreviewError("Nested Niri reported the parent compositor IPC socket")
    isolated_config = temp / "xdg-config"
    isolated_config.mkdir(mode=0o700, parents=True, exist_ok=True)
    isolated = environment.copy()
    isolated.update({
        "WAYLAND_DISPLAY": nested_display,
        "NIRI_SOCKET": nested_niri_socket,
        "NIRI_CONFIG": str(config),
        "XDG_CONFIG_HOME": str(isolated_config),
        "KIRA_CORE_SOCKET": str(socket_file),
        "KIRA_PREVIEW": "1",
        "KIRA_CONFIG_FILE": str(active_config),
    })
    return isolated


def output_signature(outputs: object) -> dict[str, dict]:
    """Compare observed output state, not incidental Niri metadata.

    Never call a restoration PASS without a valid connected output and an
    observed mode/scale/position/rotation/VRR change.
    """
    if not isinstance(outputs, dict) or not outputs or len(outputs) > 8:
        raise PreviewError("Nested Niri returned no bounded output snapshot")
    signature = {}
    for connector, output in outputs.items():
        if not isinstance(connector, str) or not connector or not isinstance(output, dict):
            raise PreviewError("Malformed output in nested Niri snapshot")
        if "logical" not in output or "current_mode" not in output:
            raise PreviewError("Incomplete nested Niri output state")
        logical = output["logical"]
        actual_mode = None
        if logical is not None:
            if not isinstance(logical, dict) or not all(
                key in logical for key in ("scale", "transform", "x", "y")
            ):
                raise PreviewError("Incomplete nested display geometry")
            logical = {key: logical[key] for key in ("scale", "transform", "x", "y")}
            # An index alone is insufficient: modes can be reordered across
            # hotplug. Compare the underlying resolution and mHz refresh.
            modes = output.get("modes")
            index = output["current_mode"]
            if (not isinstance(modes, list) or len(modes) > 80
                    or not isinstance(index, int) or isinstance(index, bool)
                    or not 0 <= index < len(modes)):
                raise PreviewError("No valid active mode for nested output")
            mode = modes[index]
            if not isinstance(mode, dict) or not all(
                isinstance(mode.get(key), int) and not isinstance(mode[key], bool)
                for key in ("width", "height", "refresh_rate")
            ):
                raise PreviewError("Malformed current nested display mode")
            actual_mode = {
                key: mode[key] for key in ("width", "height", "refresh_rate")
            }
        signature[connector] = {
            "logical": logical,
            "mode": actual_mode,
            "vrr_enabled": output.get("vrr_enabled"),
        }
    if not any(item["logical"] is not None for item in signature.values()):
        raise PreviewError("No enabled display in nested Niri")
    return signature


def nested_output_snapshot(niri: str, environment: dict[str, str]) -> dict[str, dict]:
    """Read only the verified nested NIRI_SOCKET, never the live compositor."""
    socket_path = Path(environment.get("NIRI_SOCKET", ""))
    if not socket_path.is_absolute() or not socket_path.is_socket():
        raise PreviewError("Refusing display inspection without the nested Niri socket")
    result = subprocess.run(
        [niri, "msg", "--json", "outputs"],
        env=environment, text=True, capture_output=True, check=False, timeout=3,
    )
    if result.returncode:
        raise PreviewError("Nested Niri did not answer output inspection")
    try:
        return output_signature(json.loads(result.stdout))
    except (TypeError, ValueError) as error:
        raise PreviewError("Invalid nested Niri output JSON") from error


def await_display_restore(
    niri: str,
    environment: dict[str, str],
    baseline: dict[str, dict],
    compositor: subprocess.Popen,
) -> None:
    """Give the independent rollback owner time after a real GTK Shell kill."""
    deadline = time.monotonic() + 10
    last_error = "No output query"
    while time.monotonic() < deadline:
        if compositor.poll() is not None:
            raise PreviewError("Nested Niri died before rollback could be observed")
        try:
            current = nested_output_snapshot(niri, environment)
            if current == baseline:
                return
            last_error = "Nested output state still differs from baseline"
        except (PreviewError, subprocess.TimeoutExpired) as error:
            last_error = str(error)
        time.sleep(0.15)
    raise PreviewError("Display rollback after Shell SIGKILL was not observed: " + last_error)


def run_preview(
    package: Path,
    bin_dir: Path | None = None,
    debug: bool = False,
    *,
    acceptance_notifications: bool = False,
    evidence_dir: Path | None = None,
    accept_display_crash: bool = False,
    acceptance_profile: str = "full",
) -> None:
    runtime_value = os.environ.get("XDG_RUNTIME_DIR")
    parent_display = os.environ.get("WAYLAND_DISPLAY")
    if not runtime_value or not parent_display:
        raise PreviewError("An existing Wayland session is required (XDG_RUNTIME_DIR / WAYLAND_DISPLAY)")
    if acceptance_notifications and accept_display_crash:
        raise PreviewError("Display-crash QA and notification capture are separate runs")
    if acceptance_notifications:
        from accept_nested import verify_private_bus

        verify_private_bus(os.environ)
        if evidence_dir is None:
            raise PreviewError("An explicit new evidence directory is required")
        if evidence_dir.exists():
            raise PreviewError("Existing evidence must never be overwritten")
    elif evidence_dir is not None:
        raise PreviewError("Evidence output is only available in private-bus acceptance mode")
    runtime = Path(runtime_value)
    if not ready_socket(display_path(parent_display, runtime)):
        raise PreviewError("Parent Wayland socket is not accepting connections")
    niri_path = shutil.which("niri")
    if not niri_path:
        raise PreviewError("niri is not installed; run from 'nix develop'")

    package = package.resolve()
    binaries_directory = (bin_dir if bin_dir is not None else package / "bin").resolve()
    names = (
        ("core", "shell", "sni-acceptance", "notification-acceptance")
        if acceptance_notifications else ("core", "shell")
    )
    binaries = {name: binaries_directory / f"kira-{name}" for name in names}
    for binary in binaries.values():
        if not binary.is_file() or not os.access(binary, os.X_OK):
            raise PreviewError(f"Missing {binary}; build the preview binaries first")

    with tempfile.TemporaryDirectory(prefix="kira-preview-") as directory:
        temp = Path(directory)
        nonce = os.urandom(16).hex()
        ready = temp / "ready.json"
        config = temp / "niri.kdl"
        socket_file = temp / "core.sock"
        # Kira's own KDL is separate from Niri's temporary compositor KDL.
        # Default preview never reads or writes the user's ~/.config tree.
        demo_config = temp / "kira.kdl"
        sample = Path(__file__).resolve().parents[1] / "config/kira.kdl.example"
        demo_config.write_text(sample.read_text(encoding="utf-8"), encoding="utf-8")
        requested = os.environ.get("KIRA_CONFIG_FILE")
        if requested and not Path(requested).is_absolute():
            raise PreviewError("KIRA_CONFIG_FILE must be an absolute path")
        active_config = Path(requested) if requested else demo_config
        config.write_text(
            preview_config(Path(sys.executable), Path(__file__).resolve(), ready, nonce),
            encoding="utf-8",
        )

        environment = os.environ.copy()
        for key in ("NIRI_SOCKET", "NIRI_CONFIG", "KIRA_CORE_SOCKET", "KIRA_PREVIEW", "KIRA_CONFIG_FILE"):
            environment.pop(key, None)

        validation = subprocess.run(
            [niri_path, "validate", "--config", str(config)],
            env=environment, capture_output=True, text=True, check=False, timeout=10,
        )
        if validation.returncode:
            raise PreviewError(f"Niri rejected the preview config:\n{validation.stdout}{validation.stderr}")

        processes: dict[str, subprocess.Popen] = {}
        handles = []
        logs = {name: temp / f"{name}.log" for name in ("niri", "core", "shell", "sni")}
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

            isolated = isolated_shell_environment(
                environment, nested, nested_niri_socket,
                os.environ.get("NIRI_SOCKET"), temp, config, socket_file,
                active_config,
            )
            if acceptance_notifications:
                # Only the coordinator can attest a fresh dbus-run-session.
                # Keep SNI pins, caches and state out of the user's HOME.
                state_dir = temp / "xdg-state"
                cache_dir = temp / "xdg-cache"
                data_dir = temp / "xdg-data"
                for directory in (state_dir, cache_dir, data_dir):
                    directory.mkdir(mode=0o700)
                isolated.update({
                    "KIRA_NOTIFICATIONS_PREVIEW": "1",
                    "KIRA_SNI_PREVIEW": "1",
                    "KIRA_ISOLATED_SESSION_BUS": "1",
                    "XDG_STATE_HOME": str(state_dir),
                    "XDG_CACHE_HOME": str(cache_dir),
                    "XDG_DATA_HOME": str(data_dir),
                })
            core = launch("core", [str(binaries["core"])], isolated)
            await_ready(
                lambda: True if ready_socket(socket_file) else None,
                core, "isolated Kira Core socket", 10.0,
            )
            # Capture before the GTK Shell can apply anything. The compositor
            # and its output socket are already verified as separate from host.
            baseline_outputs = (
                nested_output_snapshot(niri_path, isolated)
                if accept_display_crash else None
            )
            shell = launch("shell", [str(binaries["shell"])], isolated)
            if accept_display_crash:
                print(f"[preview:crash] GTK Shell PID: {shell.pid}", flush=True)
                print("[preview:crash] Apply a reversible display change in the nested Settings, "
                      f"then run 'kill -KILL {shell.pid}' from the host terminal within 15s.", flush=True)
                print("[preview:crash] The nested Niri stays alive long enough to verify "
                      "rollback; an unobserved change is INCONCLUSIVE, never PASS.", flush=True)
            if acceptance_notifications:
                # This executable is a real StatusNotifierItem over GIO,
                # never a mocked GTK icon. It can see only the new private bus.
                launch(
                    "sni",
                    [str(binaries["sni-acceptance"]), str(temp / "sni-events.log")],
                    isolated,
                )
                from acceptance_scenarios import run_acceptance

                print("[preview] Running real private D-Bus acceptance and PNG capture.", flush=True)
                assert evidence_dir is not None
                manifest = run_acceptance(
                    nested, environment, isolated, evidence_dir,
                    binaries["notification-acceptance"],
                    profile=acceptance_profile,
                )
                print(f"[preview] Unreviewed evidence: {manifest}", flush=True)
                return
            print("[preview] Kira running inside nested Niri. Ctrl+C to exit.", flush=True)
            print(f"[preview] Live Kira KDL config: {active_config}", flush=True)
            print(f"[preview] Isolated Niri KDL: {config}", flush=True)
            print(f"[preview] Isolated display ownership: {isolated['XDG_CONFIG_HOME']}/niri", flush=True)
            print("[preview] Edit this file to hot-reload the bar; invalid KDL retains last valid config.", flush=True)
            print("[preview] Alt+Shift+Q closes the nested Niri window.", flush=True)
            print("[preview] Nested Mod=Alt: Alt+T opens foot; Alt+1/2/3 switches workspace.", flush=True)
            print("[preview] Alt+Ctrl+1/2 moves the focused window without changing the focused workspace; Alt+1/2 switches.", flush=True)
            print("[preview] If a shortcut does not work, check host layout shortcuts and run 'niri msg --json workspaces' in the nested foot terminal.", flush=True)

            observed_changed = False
            last_outputs = baseline_outputs
            while True:
                if accept_display_crash and shell.poll() is None:
                    last_outputs = nested_output_snapshot(niri_path, isolated)
                    observed_changed |= last_outputs != baseline_outputs
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
                        if name == "shell" and accept_display_crash:
                            if proc.returncode != -signal.SIGKILL:
                                raise PreviewError("Crash acceptance requires GTK Shell SIGKILL")
                            if not observed_changed or last_outputs == baseline_outputs:
                                raise PreviewError(
                                    "INCONCLUSIVE: no pending display change was observed at Shell death"
                                )
                            assert baseline_outputs is not None
                            await_display_restore(niri_path, isolated, baseline_outputs, niri)
                            print("[preview:crash] PASS: real nested Niri output state "
                                  "returned to its pre-Apply baseline after Shell SIGKILL.", flush=True)
                            print("[preview:crash] This is NOT multi-output, "
                                  "hotplug or compositor pixel-parity acceptance.", flush=True)
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
            for name in ("sni", "shell", "core", "niri"):
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
                if shell_log.strip():
                    tail = log_tail(logs["shell"])
                    if tail:
                        print(f"[preview] Native Shell diagnostics:\n{tail}", file=sys.stderr)

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
    parser.add_argument("--debug", action="store_true", help="Stream Core and native GTK Shell logs")
    parser.add_argument("--validate-config", action="store_true")
    parser.add_argument(
        "--accept-display-crash", action="store_true",
        help="Observe a real nested Niri rollback after GTK Shell SIGKILL",
    )
    parser.add_argument(
        "--acceptance-notifications", action="store_true",
        help="Private dbus-run-session smoke only (use accept_nested.py)",
    )
    parser.add_argument("--acceptance-profile", choices=("smoke", "full"), default="full",
                        help=argparse.SUPPRESS)
    parser.add_argument(
        "--evidence-dir", type=Path,
        help="NEW output directory for unreviewed nested PNG captures",
    )
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
            run_preview(
                args.package_dir, args.bin_dir, args.debug,
                acceptance_notifications=args.acceptance_notifications,
                evidence_dir=args.evidence_dir,
                accept_display_crash=args.accept_display_crash,
                acceptance_profile=args.acceptance_profile,
            )
        else:
            parser.error("Unknown preview mode")
    except PreviewError as error:
        print(f"[preview] {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
