"""CPU-only verification of smoke release integrity and guarded inference dispatch."""
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from media_worker import Worker, WorkerError, validate_smoke_release

MODEL = "example/model"
SHA = "a" * 40
TOKEN = "offline-test-token-not-for-production"

def sha(data):
    return hashlib.sha256(data).hexdigest()

class SmokeBundleTests(unittest.TestCase):
    def test_release_integrity_and_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            worker = root / "media_worker.py"
            worker.write_bytes(b"reviewed worker")
            lock = root / "requirements.lock"
            lock.write_text("example==1 --hash=sha256:" + "a" * 64)
            release = root / "runtime-release.json"
            value = {"schema_version": 1, "purpose": "gpu_smoke",
                     "model_id": MODEL, "model_sha": SHA, "task": "text-to-image",
                     "runtime_family": "diffusers", "worker_sha256": sha(worker.read_bytes()),
                     "dependency_lock_sha256": sha(lock.read_bytes()),
                     "base_image": "example/base@sha256:" + "a" * 64}
            release.write_text(json.dumps(value))
            digest = sha(release.read_bytes())
            args = (release, digest, worker, lock, MODEL, SHA, "text-to-image", "diffusers")
            self.assertEqual(validate_smoke_release(*args), value)
            lock.write_bytes(b"changed")
            with self.assertRaises(ValueError):
                validate_smoke_release(*args)

    def test_mocked_inference_dispatch_is_serialized_and_not_a_gpu_test(self):
        release = {"model_id": MODEL, "model_sha": SHA, "task": "text-to-image",
                   "runtime_family": "diffusers"}
        worker = Worker(MODEL, SHA, "text-to-image", "diffusers", TOKEN,
                        deployment_request_id="test", smoke_release=release)
        request = {"schema_version": 1, "model": {"id": MODEL, "revision_sha": SHA},
                   "task": "text-to-image", "request_id": "test", "inputs": []}
        with patch.object(worker, "_real_generate", side_effect=WorkerError("TEST_ONLY", "mock, no GPU")) as inference:
            worker.inference_lock.acquire()
            try:
                with self.assertRaises(WorkerError) as error:
                    worker.generate(request)
                self.assertEqual(error.exception.code, "WORKER_BUSY")
                inference.assert_not_called()
            finally:
                worker.inference_lock.release()
            with self.assertRaises(WorkerError) as error:
                worker.generate(request)
            self.assertEqual(error.exception.code, "TEST_ONLY")
            inference.assert_called_once()
            self.assertFalse(worker.inference_lock.locked())

if __name__ == "__main__":
    unittest.main()
