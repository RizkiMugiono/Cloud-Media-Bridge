"""Positive human approval TTY integration for the mock-only deployment.

This exercises the local Rust CLI's interactive challenge with an actual
pseudo-terminal. It DOES NOT represent production trusted-user approval.
"""
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))


class TerminalApprovalTests(unittest.TestCase):
    @unittest.skipIf(os.name == "nt", "Python stdlib pty is Unix-only; Windows tests negative redirected approval")
    def test_interactive_tty_approval_consumes_a_single_mock_grant(self):
        import pty

        binary = (pathlib.Path(__file__).resolve().parents[2] / "target" / "debug" /
                  ("cloud-media-bridge.exe" if os.name == "nt" else "cloud-media-bridge"))
        self.assertTrue(binary.is_file())
        with tempfile.TemporaryDirectory(prefix="cmb-tty-fixture-") as folder:
            session = str(pathlib.Path(folder) / "session")
            prepared = subprocess.run(
                [str(binary), "offline", "prepare", session, "speech"],
                check=True, capture_output=True, text=True, timeout=10
            )
            challenge = json.loads(prepared.stdout)["required_human_challenge"]
            master, slave = pty.openpty()
            try:
                proc = subprocess.Popen(
                    [str(binary), "offline", "approve", session],
                    stdin=slave, stdout=subprocess.PIPE, stderr=slave
                )
                os.close(slave)
                os.write(master, (challenge + "\n").encode())
                out, _ = proc.communicate(timeout=15)
                self.assertEqual(proc.returncode, 0, out)
                result = json.loads(out)
                self.assertEqual(result["approval"], "approved")
                self.assertEqual(result["origin"], "human_terminal")
                self.assertTrue(result["one_shot"])
            finally:
                os.close(master)
            success = subprocess.run(
                [str(binary), "offline", "run", session],
                capture_output=True, text=True, timeout=10
            )
            self.assertEqual(success.returncode, 0, success.stderr)
            output = json.loads(success.stdout)
            self.assertEqual(output["auto_stop_confirmed"], True)
            self.assertEqual(output["result"]["artifacts"][0]["kind"], "speech")
            # One-shot grant must not authorize a second deployment.
            repeat = subprocess.run(
                [str(binary), "offline", "run", session],
                capture_output=True, text=True, timeout=10
            )
            self.assertNotEqual(repeat.returncode, 0)


if __name__ == "__main__":
    unittest.main()
