"""Mocked preview lifecycle tests. No graphics hardware or Niri required."""
import importlib.util
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import tempfile
import time
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "dev_preview.py"
spec = importlib.util.spec_from_file_location("dev_preview", SCRIPT)
assert spec and spec.loader
preview = importlib.util.module_from_spec(spec)
spec.loader.exec_module(preview)


class PreviewTests(unittest.TestCase):
    def test_config_does_not_start_host_services(self):
        config = preview.preview_config(Path("/usr/bin/python3"), SCRIPT, Path("/tmp/ready"), "abc")
        self.assertIn("spawn-at-startup", config)
        self.assertIn('"report"', config)
        self.assertIn("Alt+Shift+Q", config)
        self.assertNotIn("waybar", config)
        self.assertNotIn("--session", config)

    def test_refuses_parent_wayland_socket(self):
        with tempfile.TemporaryDirectory() as temp:
            with self.assertRaisesRegex(preview.PreviewError, "PARENT Wayland"):
                preview.nested_display_path("wayland-1", "wayland-1", Path(temp))
            self.assertEqual(
                preview.nested_display_path("wayland-2", "wayland-1", Path(temp)),
                Path(temp) / "wayland-2",
            )

    def test_report_records_child_display(self):
        with tempfile.TemporaryDirectory() as temp:
            filename = Path(temp) / "ready.json"
            original = os.environ.get("WAYLAND_DISPLAY")
            os.environ["WAYLAND_DISPLAY"] = "wayland-nested"
            try:
                preview.report_ready(filename, "nonce")
            finally:
                if original is None:
                    os.environ.pop("WAYLAND_DISPLAY", None)
                else:
                    os.environ["WAYLAND_DISPLAY"] = original
            self.assertEqual(json.loads(filename.read_text())["display"], "wayland-nested")

    def test_refuses_non_wayland_session(self):
        env = os.environ.copy()
        env.pop("WAYLAND_DISPLAY", None)
        completed = subprocess.run(
            [sys.executable, str(SCRIPT)],
            env=env, capture_output=True, text=True, timeout=6,
        )
        self.assertNotEqual(completed.returncode, 0)
        self.assertIn("Wayland session", completed.stderr)

    def test_lifecycle_with_mock_niri_core_and_shell(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            runtime = root / "runtime"
            runtime.mkdir()
            parent = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            parent.bind(str(runtime / "wayland-parent"))
            parent.listen(1)

            bins = root / "result" / "bin"
            bins.mkdir(parents=True)
            output = root / "shell-env.json"
            (bins / "kira-core").write_text("""#!/usr/bin/env python3
import os, socket, time
sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
sock.bind(os.environ['KIRA_CORE_SOCKET'])
sock.listen(2)
while True: time.sleep(.1)
""")
            (bins / "kira-shell").write_text("""#!/usr/bin/env python3
import json, os, time
from pathlib import Path
Path(os.environ['PREVIEW_AUDIT']).write_text(json.dumps({
  'wayland': os.environ.get('WAYLAND_DISPLAY'),
  'core': os.environ.get('KIRA_CORE_SOCKET'),
  'preview': os.environ.get('KIRA_PREVIEW'),
}))
while True: time.sleep(.1)
""")
            for binary in bins.iterdir():
                binary.chmod(0o755)

            niri = root / "niri"
            niri.write_text("""#!/usr/bin/env python3
import os, shlex, socket, subprocess, sys, time
from pathlib import Path
args = sys.argv[1:]
if args[0] == 'validate': sys.exit(0)
config = Path(args[args.index('--config') + 1])
line = next(line for line in config.read_text().splitlines() if line.startswith('spawn-at-startup '))
command = shlex.split(line[len('spawn-at-startup '):])
path = Path(os.environ['XDG_RUNTIME_DIR']) / 'wayland-mock-nested'
sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
sock.bind(str(path)); sock.listen(2)
env = os.environ.copy()
env['WAYLAND_DISPLAY'] = path.name
subprocess.run(command, env=env, check=True)
while True: time.sleep(.1)
""")
            niri.chmod(0o755)

            env = os.environ.copy()
            env.update({
                "XDG_RUNTIME_DIR": str(runtime),
                "WAYLAND_DISPLAY": "wayland-parent",
                "PATH": f"{root}:{env.get('PATH', '')}",
                "PREVIEW_AUDIT": str(output),
            })
            process = subprocess.Popen(
                [sys.executable, str(SCRIPT), "--package-dir", str(root / "result")],
                env=env, start_new_session=True,
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
            )
            try:
                until = time.monotonic() + 8
                while process.poll() is None and not output.exists() and time.monotonic() < until:
                    time.sleep(0.1)
                self.assertTrue(output.exists(), "Shell did not launch in mock nested Niri")
                actual = json.loads(output.read_text())
                self.assertEqual(actual["wayland"], "wayland-mock-nested")
                self.assertEqual(actual["preview"], "1")
                self.assertIn("kira-preview-", actual["core"])
                self.assertFalse((runtime / "kira-desktop.sock").exists())
            finally:
                if process.poll() is None:
                    process.send_signal(signal.SIGINT)
                stdout, stderr = process.communicate(timeout=8)
                parent.close()
            self.assertEqual(process.returncode, 0, stdout + stderr)
            self.assertIn("Nested display: wayland-mock-nested", stdout)
            self.assertIn("cleaned up", stdout)


if __name__ == "__main__":
    unittest.main()
