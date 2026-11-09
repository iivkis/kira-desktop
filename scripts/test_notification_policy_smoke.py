"""Pure contract tests for the private-bus notification policy smoke driver."""
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from dev_preview import PreviewError
from smoke_notification_policy_bus import expected_proof, require_binary


class NotificationPolicySmokeTests(unittest.TestCase):
    def test_real_result_requires_every_boolean_proof(self):
        full = {
            "same_sender_replaced": True,
            "foreign_sender_isolated": True,
            "stale_id_rejected": True,
            "private_bus": True,
        }
        import json
        self.assertEqual(expected_proof(json.dumps(full)), full)
        for key in full:
            with self.subTest(missing=key), self.assertRaises(PreviewError):
                expected_proof(json.dumps({k: v for k, v in full.items() if k != key}))
            with self.subTest(false=key), self.assertRaises(PreviewError):
                expected_proof(json.dumps({**full, key: False}))
        for answer in ('', '{}', '[]', 'null', 'garbage', '{"private_bus":"true"}'):
            with self.subTest(answer=answer), self.assertRaises(PreviewError):
                expected_proof(answer)
        with self.assertRaises(PreviewError):
            expected_proof(json.dumps({**full, "unapproved_proof": True}))

    def test_disposable_binaries_must_exist_and_be_executable(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "fixture"
            with self.assertRaises(PreviewError):
                require_binary(path)
            path.write_text("not a real executable but an executable path", encoding="utf-8")
            path.chmod(0o600)
            with self.assertRaises(PreviewError):
                require_binary(path)
            path.chmod(0o700)
            self.assertEqual(require_binary(path), path.resolve())


if __name__ == "__main__":
    unittest.main()
