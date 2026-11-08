"""A tiny, private D-Bus session config for reproducible Niri/CI acceptance.

Unlike distro /etc/dbus-1/session.conf, this never activates user services.
It listens only on a random Unix socket in a mode-0700 temporary directory.
All operations remain scoped to the short-lived runner subprocess.
"""
from __future__ import annotations

from contextlib import contextmanager
import os
from pathlib import Path
import re
import stat
import tempfile
from typing import Iterator
from urllib.parse import unquote_to_bytes
from xml.sax.saxutils import escape

PRIVATE_BUS_DIR = "KIRA_ACCEPTANCE_PRIVATE_BUS_DIR"

CONFIG = """<!DOCTYPE busconfig PUBLIC
 "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>{address}</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*"/>
    <allow receive_sender="*"/>
    <allow own="*"/>
  </policy>
</busconfig>
"""


@contextmanager
def private_bus_config() -> Iterator[Path]:
    with tempfile.TemporaryDirectory(prefix="kira-private-dbus-") as directory:
        root = Path(directory)
        root.chmod(0o700)
        config = root / "bus.conf"
        config.write_text(
            CONFIG.format(address=escape("unix:tmpdir=" + str(root))),
            encoding="utf-8",
        )
        config.chmod(0o600)
        yield config


def run_session_arguments(runner: str, config: Path) -> list[str]:
    if not config.is_file() or config.is_symlink():
        raise ValueError("A real, private D-Bus config file is required")
    return [runner, "--config-file=" + str(config), "--"]


def verify_private_bus_socket(env: dict[str, str]) -> None:
    """Check the actual socket, not merely a different-looking D-Bus address.

    The acceptance-only bus must have a direct, owned socket inside the fresh
    mode-0700 directory passed by its coordinator. Never trust a path alias,
    abstract socket, multiple fallback addresses or a symlinked bus.
    """
    directory = env.get(PRIVATE_BUS_DIR, "")
    if not directory:
        raise ValueError("Missing private D-Bus directory")
    root = Path(directory)
    if not root.is_absolute():
        raise ValueError("Private D-Bus directory must be absolute")
    try:
        root_stat = root.lstat()
    except OSError as error:
        raise ValueError("Private D-Bus directory does not exist") from error
    if (not stat.S_ISDIR(root_stat.st_mode) or root_stat.st_uid != os.getuid()
            or stat.S_IMODE(root_stat.st_mode) != 0o700):
        raise ValueError("Private D-Bus directory must be an owned 0700 directory")

    address = env.get("DBUS_SESSION_BUS_ADDRESS", "")
    match = re.fullmatch(r"unix:path=([^,;]+)(?:,guid=[0-9A-Fa-f]{32})?", address)
    if match is None:
        raise ValueError("Private D-Bus requires one filesystem Unix socket")
    raw_path = unquote_to_bytes(match.group(1))
    if b"\x00" in raw_path:
        raise ValueError("Invalid D-Bus socket path")
    socket_path = Path(os.fsdecode(raw_path))
    if (not socket_path.is_absolute() or socket_path.parent != root
            or not socket_path.name.startswith("dbus-")):
        raise ValueError("D-Bus socket is not directly inside the private directory")
    try:
        socket_stat = socket_path.lstat()
    except OSError as error:
        raise ValueError("Private D-Bus socket does not exist") from error
    if not stat.S_ISSOCK(socket_stat.st_mode) or socket_stat.st_uid != os.getuid():
        raise ValueError("Private D-Bus socket is not an owned Unix socket")
