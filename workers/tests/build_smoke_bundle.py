"""Construct a deliberately tiny CI-only wheel and candidate context.

No model weights, CUDA, credentials or GPU dependencies are involved. The
test proves the actual image build and hashed offline installation path.
"""
import base64
import csv
import hashlib
import io
import json
from pathlib import Path
import re
import shutil
import sys
from zipfile import ZipFile

def sha(data):
    return hashlib.sha256(data).hexdigest()

def wheel_record(data):
    return "sha256=" + base64.urlsafe_b64encode(hashlib.sha256(data).digest()).decode().rstrip("=")

def main():
    root = Path(sys.argv[1])
    base = sys.argv[2]
    if not re.fullmatch(r"[A-Za-z0-9._/:-]+@sha256:[a-f0-9]{64}", base):
        raise ValueError("Expected an actual digest-pinned local CPU base image")
    root.mkdir(parents=True, exist_ok=True)
    wheels = root / "wheelhouse"
    wheels.mkdir()
    dist = "cmb_smoke_probe-0.0.1.dist-info"
    entries = {
        "cmb_smoke_probe/__init__.py": b"CI_ONLY = True\n",
        dist + "/METADATA": b"Metadata-Version: 2.1\nName: cmb-smoke-probe\nVersion: 0.0.1\n\n",
        dist + "/WHEEL": b"Wheel-Version: 1.0\nGenerator: cmb-ci\nRoot-Is-Purelib: true\nTag: py3-none-any\n\n",
    }
    record = io.StringIO(newline="")
    writer = csv.writer(record)
    for name, data in sorted(entries.items()):
        writer.writerow([name, wheel_record(data), str(len(data))])
    writer.writerow([dist + "/RECORD", "", ""])
    entries[dist + "/RECORD"] = record.getvalue().encode()
    wheel = wheels / "cmb_smoke_probe-0.0.1-py3-none-any.whl"
    with ZipFile(wheel, "w") as archive:
        for name, data in sorted(entries.items()):
            archive.writestr(name, data)
    lock = root / "requirements.lock"
    lock.write_text("cmb-smoke-probe==0.0.1 --hash=sha256:" + sha(wheel.read_bytes()) + "\n", encoding="utf-8")
    workers = Path(__file__).resolve().parents[1]
    worker = root / "media_worker.py"
    shutil.copyfile(workers / "media_worker.py", worker)
    shutil.copyfile(workers / "Dockerfile.gpu-smoke", root / "Dockerfile")
    release = {
        "schema_version": 1, "purpose": "gpu_smoke", "model_id": "fixture/ci-only",
        "model_sha": "a" * 40, "task": "text-to-image", "runtime_family": "diffusers",
        "base_image": base, "worker_sha256": sha(worker.read_bytes()),
        "dependency_lock_sha256": sha(lock.read_bytes()),
    }
    (root / "runtime-release.json").write_text(json.dumps(release), encoding="utf-8")
    print("Prepared CI-only build context; no actual model or GPU dependency")

if __name__ == "__main__":
    main()
