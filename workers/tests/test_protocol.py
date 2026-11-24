"""CPU-only unit/integration checks: real HTTP transport, no AI weights or GPU."""
import hashlib
import json
import os
import subprocess
import tempfile
import pathlib
import sys
import threading
import unittest
import urllib.error
import urllib.request

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
from media_worker import Worker, WorkerError, handler_for, ThreadingHTTPServer, SHA_RE

TOKEN = "offline-test-secret-not-for-production"
SHA = "0" * 40
MODEL = "fixture/offline-model"


class WorkerHTTPTests(unittest.TestCase):
    def setUp(self):
        self.worker = Worker(MODEL, SHA, "text-to-image", "diffusers", TOKEN, fixture=True)
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), handler_for(self.worker))
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.url = f"http://127.0.0.1:{self.server.server_port}"

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)

    def request(self, path, body=None, token=TOKEN, headers=None):
        custom_headers = {"Authorization": f"Bearer {token}"} if token is not None else {}
        if headers:
            custom_headers.update(headers)
        request = urllib.request.Request(self.url + path, data=body, headers=custom_headers)
        return urllib.request.urlopen(request, timeout=3)

    def generate(self, kind):
        request = {
            "schema_version": 1,
            "request_id": f"mock-{kind}",
            "task": "text-to-image",
            "model": {"id": MODEL, "revision_sha": SHA},
            "inputs": [{"kind": "text", "text": "fixture only"}],
            "fixture_kind": kind
        }
        with self.request("/v1/media/generate", json.dumps(request).encode(),
                          headers={"Content-Type": "application/json"}) as response:
            self.assertEqual(response.status, 200)
            return json.load(response)

    def test_all_nine_families_generate_and_retrieve_hash_checked_fixture_bytes(self):
        kinds = ["image", "video", "audio", "speech", "music", "three_d",
                 "spatial", "document", "multimodal"]
        for kind in kinds:
            output = self.generate(kind)
            self.assertTrue(output["simulation_only"])
            self.assertEqual(output["status"], "completed")
            artifact = output["artifacts"][0]
            self.assertEqual(artifact["kind"], kind)
            with self.request("/v1/media/artifacts/" + artifact["asset_ref"]) as response:
                binary = response.read()
                self.assertEqual(response.headers["X-Content-SHA256"], artifact["sha256"])
                self.assertEqual(response.headers["Content-Type"], artifact["media_type"])
            self.assertEqual(hashlib.sha256(binary).hexdigest(), artifact["sha256"])
            self.assertEqual(len(binary), artifact["size_bytes"])
            if kind in {"music", "audio", "speech"}:
                self.assertTrue(binary.startswith(b"RIFF"))
            if kind == "video":
                self.assertEqual(artifact["media_type"], "application/json")
                self.assertIn(b"not_real_ai_inference", binary)

    def test_health_and_authentication(self):
        with self.request("/health", token=None) as response:
            health = json.load(response)
            self.assertEqual(health["status"], "ok")
            self.assertTrue(health["simulation_only"])
        with self.assertRaises(urllib.error.HTTPError) as e:
            self.request("/v1/media/generate", b"{}", token="invalid")
        self.assertEqual(e.exception.code, 401)

    def test_staged_image_is_authenticated_and_not_an_arbitrary_file_url(self):
        with self.request("/v1/media/assets", b"\x89PNG\r\n\x1a\nmock-test",
                          headers={"Content-Type": "image/png",
                                   "X-CMB-Media-Kind": "image"}) as response:
            self.assertEqual(response.status, 201)
            artifact = json.load(response)
        self.assertEqual(artifact["media_type"], "image/png")
        self.assertEqual(len(artifact["asset_ref"]), 32)
        with self.request("/v1/media/artifacts/" + artifact["asset_ref"]) as response:
            self.assertEqual(response.read(), b"\x89PNG\r\n\x1a\nmock-test")
        with self.assertRaises(urllib.error.HTTPError) as error:
            self.request("/v1/media/artifacts/../../etc/passwd")
        self.assertIn(error.exception.code, (400, 404))

    def test_mismatched_revision_and_task_rejected(self):
        payload = {"schema_version": 1, "request_id": "sample", "model": {"id": MODEL, "revision_sha": "f" * 40},
                   "task": "text-to-image", "inputs": [], "fixture_kind": "image"}
        with self.assertRaises(urllib.error.HTTPError) as e:
            self.request("/v1/media/generate", json.dumps(payload).encode())
        self.assertEqual(e.exception.code, 400)
        payload["model"]["revision_sha"] = SHA
        payload["task"] = "some-other-task"
        with self.assertRaises(urllib.error.HTTPError) as e:
            self.request("/v1/media/generate", json.dumps(payload).encode())
        self.assertEqual(e.exception.code, 400)

    def test_rust_mcp_bridge_calls_fixture_worker_over_real_loopback_http(self):
        binary = (pathlib.Path(__file__).resolve().parents[2] / "target" / "debug" /
                  ("cloud-media-bridge.exe" if os.name == "nt" else "cloud-media-bridge"))
        self.assertTrue(binary.is_file(), "CI must build the Rust bridge before cross-language tests")
        with tempfile.TemporaryDirectory(prefix="cmb-worker-cross-language-") as output_dir:
            env = os.environ.copy()
            env["CMB_WORKER_TOKEN"] = TOKEN
            result = subprocess.run(
                [str(binary), "offline", "worker-smoke", output_dir,
                 str(self.server.server_port), "music"],
                capture_output=True, text=True, timeout=25, env=env
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            receipt = json.loads(result.stdout)
            self.assertTrue(receipt["simulation_only"])
            self.assertTrue(receipt["artifact_verified"])
            self.assertFalse(receipt["real_ai_inference"])
            self.assertFalse(receipt["provider_gpu_used"])
            data = pathlib.Path(receipt["saved_file"]).read_bytes()
            self.assertEqual(hashlib.sha256(data).hexdigest(), receipt["sha256"])
            self.assertTrue(data.startswith(b"RIFF"))

    def test_missing_or_incorrect_protocol_version_is_rejected(self):
        payload = {
            "request_id": "wrong-version",
            "task": "text-to-image",
            "model": {"id": MODEL, "revision_sha": SHA},
            "inputs": [],
            "fixture_kind": "image",
        }
        for value in (None, 2, True):
            with self.subTest(value=value):
                request = dict(payload)
                if value is not None:
                    request["schema_version"] = value
                with self.assertRaises(urllib.error.HTTPError) as error:
                    self.request("/v1/media/generate", json.dumps(request).encode())
                self.assertEqual(error.exception.code, 400)

    def test_invalid_json_and_media_type_rejected(self):
        with self.assertRaises(urllib.error.HTTPError) as e:
            self.request("/v1/media/generate", b"not json")
        self.assertEqual(e.exception.code, 400)
        with self.assertRaises(urllib.error.HTTPError) as e:
            self.request("/v1/media/assets", b"bad",
                         headers={"Content-Type": "text/html", "X-CMB-Media-Kind": "image"})
        self.assertEqual(e.exception.code, 415)


class WorkerSecurityTests(unittest.TestCase):
    def test_model_revision_and_task_are_pinned(self):
        with self.assertRaises(ValueError):
            Worker("../internal", SHA, "text-to-image", "diffusers", TOKEN, fixture=False)
        with self.assertRaises(ValueError):
            Worker(MODEL, "main", "text-to-image", "diffusers", TOKEN, fixture=False)
        with self.assertRaises(ValueError):
            Worker(MODEL, SHA, "not-a-real-task", "diffusers", TOKEN, fixture=False)
        with self.assertRaises(ValueError):
            Worker(MODEL, SHA, "text-to-image", "diffusers", "short", fixture=False)
        self.assertTrue(SHA_RE.fullmatch(SHA))

    def test_nonfixture_runtime_does_not_silently_use_test_artifacts(self):
        worker = Worker(MODEL, SHA, "text-to-image", "diffusers", TOKEN, fixture=False)
        self.assertFalse(worker.fixture)
        with self.assertRaises(WorkerError) as error:
            worker.generate({"schema_version": 1, "request_id": "x", "task": "text-to-video",
                             "model": {"id": MODEL, "revision_sha": SHA}, "inputs": []})
        self.assertEqual(error.exception.code, "TASK_MISMATCH")

    def test_fixture_flag_cannot_trigger_real_inference_worker(self):
        worker = Worker(MODEL, SHA, "text-to-image", "diffusers", TOKEN, fixture=False)
        with self.assertRaises(WorkerError) as error:
            worker.generate({"schema_version": 1, "request_id": "test-only",
                             "task": "text-to-image",
                             "model": {"id": MODEL, "revision_sha": SHA},
                             "inputs": [], "fixture_kind": "image"})
        self.assertEqual(error.exception.code, "FIXTURE_DISABLED")

    def test_asset_handles_cannot_read_filesystem(self):
        worker = Worker(MODEL, SHA, "text-to-image", "diffusers", TOKEN, fixture=True)
        for text in ["../etc/passwd", "file:///etc/passwd", "https://internal.host", "a" * 33]:
            with self.assertRaises(WorkerError):
                worker.load(text)


if __name__ == "__main__":
    unittest.main()
