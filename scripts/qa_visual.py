#!/usr/bin/env python3
"""Captures reproducible visual-QA evidence from the *nested* Niri preview.

No host compositor commands, no global desktop changes and no screenshot
success claims until the user reviews the captured images.
"""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import socket
import stat
import struct
import subprocess
import time

SCALES = (1.0, 1.25, 1.5, 2.0)
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
PNG_TRAILER = b"\x00\x00\x00\x00IEND\xaeB\x60\x82"


class VisualQAError(Exception):
    pass


def nested_socket(environment: dict[str, str], parent_socket: str | None) -> Path:
    candidate = environment.get("NIRI_SOCKET")
    if not candidate:
        raise VisualQAError("Nested NIRI_SOCKET is missing; refusing to query host Niri")
    path = Path(candidate)
    if not path.is_absolute():
        raise VisualQAError("Nested NIRI_SOCKET is not absolute")
    if parent_socket and path.resolve() == Path(parent_socket).resolve():
        raise VisualQAError("Nested NIRI_SOCKET equals parent; refusing to query host Niri")
    try:
        if not stat.S_ISSOCK(path.stat().st_mode):
            raise VisualQAError("Nested NIRI_SOCKET is not a Unix socket")
    except OSError as error:
        raise VisualQAError(f"Nested NIRI_SOCKET is unavailable: {error}") from error
    return path


def niri_command(niri: str, environment: dict[str, str], *arguments: str) -> str:
    try:
        result = subprocess.run(
            [niri, "msg", *arguments],
            env=environment,
            capture_output=True,
            text=True,
            timeout=10,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise VisualQAError(f"Niri IPC command failed: {error}") from error
    if result.returncode:
        raise VisualQAError(
            f"Niri IPC command exited {result.returncode}: {result.stderr.strip()}"
        )
    return result.stdout


def sole_output(niri: str, environment: dict[str, str]) -> tuple[str, dict]:
    try:
        outputs = json.loads(niri_command(niri, environment, "--json", "outputs"))
    except (TypeError, ValueError) as error:
        raise VisualQAError(f"Malformed Niri output JSON: {error}") from error
    if not isinstance(outputs, dict):
        raise VisualQAError("Niri outputs must be a JSON object")
    connected = [
        (name, details)
        for name, details in outputs.items()
        if isinstance(details, dict) and isinstance(details.get("logical"), dict)
    ]
    if len(connected) != 1:
        raise VisualQAError(f"Expected exactly one nested Niri output, found {len(connected)}")
    return connected[0]


def wait_for_shell_ui(shell_log: Path, timeout: float = 12.0) -> None:
    """A running Shell process is insufficient: verify WebKit actually mounted UI."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        logs = shell_log.read_text(encoding="utf-8", errors="replace") if shell_log.exists() else ""
        if "UI rendering check failed" in logs or "bootstrap exception:" in logs:
            raise VisualQAError("Shell reported a Svelte render failure")
        if "Svelte entrypoint mounted" in logs and "Svelte panel DOM verified" in logs:
            return
        time.sleep(0.15)
    raise VisualQAError("Timed out waiting for Svelte mount and WebKit DOM verification")


def check_core(core_socket: Path) -> dict:
    request = {"jsonrpc": "2.0", "id": 1, "method": "get_state"}
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
            connection.settimeout(3)
            connection.connect(str(core_socket))
            connection.sendall((json.dumps(request) + "\n").encode("utf-8"))
            with connection.makefile("rb") as stream:
                line = stream.readline(65537)
        response = json.loads(line)
    except (OSError, ValueError) as error:
        raise VisualQAError(f"Core get_state smoke test failed: {error}") from error
    if (
        len(line) > 65536
        or response.get("jsonrpc") != "2.0"
        or response.get("id") != 1
        or response.get("error")
        or not isinstance(response.get("result"), dict)
        or response["result"].get("status") != "bootstrap"
    ):
        raise VisualQAError("Core get_state returned an unexpected response")
    return response["result"]


def wait_for_scale(
    niri: str, environment: dict[str, str], output: str, expected: float,
    timeout: float = 8.0,
) -> dict:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        name, details = sole_output(niri, environment)
        logical = details["logical"]
        if name == output and abs(float(logical.get("scale", -1)) - expected) < 0.02:
            return logical
        time.sleep(0.15)
    raise VisualQAError(f"Niri output {output!r} did not reach scale {expected}")


def png_dimensions(path: Path) -> tuple[int, int]:
    with path.open("rb") as stream:
        header = stream.read(24)
    if (
        len(header) != 24
        or not header.startswith(PNG_SIGNATURE)
        or header[12:16] != b"IHDR"
    ):
        raise VisualQAError(f"Invalid PNG screenshot: {path}")
    width, height = struct.unpack(">II", header[16:24])
    if width < 200 or height < 100:
        raise VisualQAError(f"Implausibly small Niri screenshot: {width}x{height}")
    return width, height


def wait_for_png(path: Path, timeout: float = 8.0) -> tuple[int, int]:
    deadline = time.monotonic() + timeout
    previous_size = -1
    while time.monotonic() < deadline:
        if path.is_file():
            current_size = path.stat().st_size
            if current_size >= 40 and current_size == previous_size:
                with path.open("rb") as file:
                    file.seek(-len(PNG_TRAILER), 2)
                    if file.read() == PNG_TRAILER:
                        return png_dimensions(path)
            previous_size = current_size
        time.sleep(0.15)
    raise VisualQAError(f"Niri did not finish saving PNG screenshot: {path}")


def capture(
    niri: str,
    environment: dict[str, str],
    parent_niri_socket: str | None,
    shell_log: Path,
    core_socket: Path,
    output_directory: Path,
    packaged: bool,
) -> Path:
    """Capture 100/125/150/200% and write evidence; visual sign-off is manual."""
    nested_socket(environment, parent_niri_socket)
    output_directory = output_directory.resolve()
    output_directory.mkdir(parents=True, exist_ok=True)
    session = output_directory / (time.strftime("%Y%m%d-%H%M%S") + f"-{os.getpid()}")
    session.mkdir(exist_ok=False)
    manifest: dict = {
        "status": "in_progress",
        "visual_review": "pending",
        "build": "packaged-nix" if packaged else "cargo-debug",
        "screenshots": [],
    }

    def save() -> None:
        (session / "manifest.json").write_text(
            json.dumps(manifest, indent=2, ensure_ascii=False) + "\n",
            encoding="utf-8",
        )

    try:
        wait_for_shell_ui(shell_log)
        manifest["core"] = check_core(core_socket)
        output, _ = sole_output(niri, environment)
        manifest["niri_output"] = output

        for scale in SCALES:
            niri_command(niri, environment, "output", output, "scale", str(scale))
            logical = wait_for_scale(niri, environment, output, scale)
            # The compositor can apply scale before WebKit has rendered a new frame.
            time.sleep(0.7)
            filename = f"niri-scale-{int(scale * 100):03d}.png"
            path = session / filename
            niri_command(
                niri, environment, "action", "screenshot-screen",
                "--path", str(path),
            )
            width, height = wait_for_png(path)
            manifest["screenshots"].append({
                "file": filename,
                "scale": scale,
                "logical": logical,
                "pixel_size": [width, height],
                "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            })
            save()

        manifest["status"] = "captured_review_pending"
        save()
        return session
    except Exception as error:
        manifest["status"] = "failed"
        manifest["error"] = str(error)
        save()
        raise
