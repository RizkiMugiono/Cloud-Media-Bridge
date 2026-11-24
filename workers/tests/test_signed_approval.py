"""Real ephemeral OpenSSH signature integration; no cloud or GPU."""
import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
BIN = ROOT / "target" / "debug" / ("cloud-media-bridge.exe" if os.name == "nt" else "cloud-media-bridge")
KEYGEN = shutil.which("ssh-keygen")

@unittest.skipUnless(KEYGEN and BIN.is_file(), "OpenSSH and built bridge required")
class SignedApprovalTests(unittest.TestCase):
    def test_signature_tampering_and_replay(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            key = root / "key"
            subprocess.run([KEYGEN, "-q", "-t", "ed25519", "-N", "", "-f", str(key)], check=True)
            allowed = root / "allowed"
            allowed.write_text("user@example " + (root / "key.pub").read_text(), encoding="utf-8")
            ledger = root / "ledger"
            ledger.mkdir()
            env = dict(os.environ, CMB_AUTH_SSH_KEYGEN=str(pathlib.Path(KEYGEN).resolve()),
                       CMB_AUTH_ALLOWED_SIGNERS=str(allowed), CMB_AUTH_PRINCIPAL="user@example", CMB_AUTH_LEDGER=str(ledger))
            session = root / "session"
            request = root / "operation.json"
            request.write_text('{"operation":"deploy"}', encoding="utf-8")
            def run(*args, ok=True):
                p = subprocess.run([str(BIN), *map(str,args)], env=env, capture_output=True, text=True, timeout=20)
                self.assertEqual(p.returncode == 0, ok, p.stderr)
                return json.loads(p.stdout) if ok else None
            run("cloud","prepare",session,ROOT / "examples" / "cloud-deployment-manifest.NOT-EXECUTABLE.json")
            challenge = run("authorization","challenge",session,request)
            grant = root / "grant.json"
            grant.write_bytes(challenge["sign_exact_utf8"].encode())
            subprocess.run([KEYGEN,"-Y","sign","-f",str(key),"-n","cloud-media-bridge-v1",str(grant)], check=True, capture_output=True)
            sig = root / "grant.json.sig"
            self.assertTrue(run("authorization","verify",session,grant,sig,request)["signature_verified"])
            wrong = root / "wrong.json"
            wrong.write_text(json.dumps(dict(challenge["payload"], grant_id="c"*48)), encoding="utf-8")
            run("authorization","verify",session,wrong,sig,request,ok=False)
            self.assertTrue(run("authorization","consume",session,grant,sig,request)["grant_consumed"])
            run("authorization","consume",session,grant,sig,request,ok=False)

if __name__ == "__main__":
    unittest.main()
