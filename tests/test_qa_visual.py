"""Unit tests for safe visual QA capture without a GPU or a real Niri."""
import importlib.util
import json
from pathlib import Path
import socket
import struct
import tempfile
import threading
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "qa_visual.py"
spec = importlib.util.spec_from_file_location("qa_visual", SCRIPT)
assert spec and spec.loader
qa = importlib.util.module_from_spec(spec)
spec.loader.exec_module(qa)


def png_header(width=1280, height=720):
    return qa.PNG_SIGNATURE + struct.pack(">I", 13) + b"IHDR" + struct.pack(">II", width, height) + b"\x08\x06\x00\x00\x00\x00\x00\x00\x00"


class VisualQATests(unittest.TestCase):
    def test_rejects_missing_niri_socket_before_any_command(self):
        with self.assertRaisesRegex(qa.VisualQAError, "missing"):
            qa.nested_socket({}, None)

    def test_rejects_parent_niri_socket(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "niri.sock"
            server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            server.bind(str(path))
            try:
                with self.assertRaisesRegex(qa.VisualQAError, "equals parent"):
                    qa.nested_socket({"NIRI_SOCKET": str(path)}, str(path))
                self.assertEqual(qa.nested_socket({"NIRI_SOCKET": str(path)}, None), path)
            finally:
                server.close()

    def test_only_one_nested_output_is_accepted(self):
        with patch.object(qa, "niri_command", return_value=json.dumps({
            "Virtual-1": {"logical": {"scale": 1.0, "width": 1200, "height": 800}}
        })):
            name, details = qa.sole_output("niri", {})
        self.assertEqual(name, "Virtual-1")
        self.assertEqual(details["logical"]["width"], 1200)
        with patch.object(qa, "niri_command", return_value=json.dumps({
            "one": {"logical": {"scale": 1.0}},
            "two": {"logical": {"scale": 1.0}},
        })):
            with self.assertRaisesRegex(qa.VisualQAError, "exactly one"):
                qa.sole_output("niri", {})

    def test_mount_requires_verified_dom_not_just_started_shell(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "shell.log"
            log.write_text("Svelte entrypoint mounted\n")
            with self.assertRaisesRegex(qa.VisualQAError, "Timed out"):
                qa.wait_for_shell_ui(log, timeout=0)
            log.write_text("Svelte entrypoint mounted\nSvelte panel DOM verified\n")
            qa.wait_for_shell_ui(log, timeout=1)

    def test_invalid_png_does_not_pass_capture(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "screen.png"
            path.write_bytes(b"not a screenshot at all")
            with self.assertRaisesRegex(qa.VisualQAError, "Invalid PNG"):
                qa.png_dimensions(path)
            path.write_bytes(png_header(width=640, height=480))
            self.assertEqual(qa.png_dimensions(path), (640, 480))

    def test_core_smoke_checks_real_get_state_response(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "core.sock"
            listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            listener.bind(str(path))
            listener.listen(1)
            errors = []

            def serve():
                try:
                    conn, _ = listener.accept()
                    with conn:
                        raw = conn.recv(4096).decode("utf-8")
                        request = json.loads(raw)
                        self.assertEqual(request["method"], "get_state")
                        conn.sendall(json.dumps({
                            "jsonrpc": "2.0", "id": 1,
                            "result": {"version": "0.1.0", "status": "bootstrap"}
                        }).encode("utf-8") + b"\n")
                except Exception as error:
                    errors.append(error)

            thread = threading.Thread(target=serve)
            thread.start()
            try:
                self.assertEqual(qa.check_core(path)["version"], "0.1.0")
            finally:
                thread.join(timeout=3)
                listener.close()
            self.assertFalse(errors)

    def test_captures_four_scales_and_writes_pending_manifest(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            niri_socket = root / "niri.sock"
            server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            server.bind(str(niri_socket))
            state = {"scale": 1.0}
            calls = []

            def command(_niri, environment, *args):
                self.assertEqual(environment["NIRI_SOCKET"], str(niri_socket))
                calls.append(args)
                if args == ("--json", "outputs"):
                    return json.dumps({"Virtual-1": {"logical": {
                        "scale": state["scale"], "width": 1280, "height": 720
                    }}})
                if args[:2] == ("output", "Virtual-1"):
                    self.assertEqual(args[2], "scale")
                    state["scale"] = float(args[3])
                if args[:2] == ("action", "screenshot-screen"):
                    path = Path(args[-1])
                    self.assertEqual(args[-2], "--path")
                    path.write_bytes(png_header() + b"fake-screenshot")
                return ""

            try:
                with patch.object(qa, "niri_command", side_effect=command), \
                     patch.object(qa, "wait_for_shell_ui"), \
                     patch.object(qa, "check_core", return_value={"status": "bootstrap"}), \
                     patch.object(qa.time, "sleep"):
                    path = qa.capture("niri", {"NIRI_SOCKET": str(niri_socket)},
                                      None, root / "shell.log", root / "core.sock",
                                      root / "captures", packaged=True)
                manifest = json.loads((path / "manifest.json").read_text())
                self.assertEqual(manifest["status"], "captured_review_pending")
                self.assertEqual(manifest["visual_review"], "pending")
                self.assertEqual(manifest["build"], "packaged-nix")
                self.assertEqual([item["scale"] for item in manifest["screenshots"]],
                                 [1.0, 1.25, 1.5, 2.0])
                self.assertTrue(all((path / item["file"]).is_file()
                                    for item in manifest["screenshots"]))
                self.assertEqual(len([a for a in calls if a[:2] == (
                    "action", "screenshot-screen")]), 4)
            finally:
                server.close()

    def test_failure_records_failed_not_a_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            sock = root / "niri.sock"
            server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            server.bind(str(sock))
            try:
                with patch.object(qa, "wait_for_shell_ui", side_effect=qa.VisualQAError("DOM failed")):
                    with self.assertRaisesRegex(qa.VisualQAError, "DOM failed"):
                        qa.capture("niri", {"NIRI_SOCKET": str(sock)}, None,
                                   root / "shell.log", root / "core.sock",
                                   root / "evidence", packaged=True)
                manifests = list((root / "evidence").glob("*/manifest.json"))
                self.assertEqual(len(manifests), 1)
                self.assertEqual(json.loads(manifests[0].read_text())["status"], "failed")
            finally:
                server.close()


if __name__ == "__main__":
    unittest.main()
