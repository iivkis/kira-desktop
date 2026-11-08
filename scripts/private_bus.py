"""A tiny, private D-Bus session config for reproducible Niri/CI acceptance.

Unlike distro /etc/dbus-1/session.conf, this never activates user services.
It listens only on a random Unix socket in a mode-0700 temporary directory.
All operations remain scoped to the short-lived runner subprocess.
"""
from __future__ import annotations

from contextlib import contextmanager
from pathlib import Path
import tempfile
from typing import Iterator
from xml.sax.saxutils import escape

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
