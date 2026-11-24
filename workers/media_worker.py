#!/usr/bin/env python3
"""Loopback worker for offline media protocol fixtures."""
from __future__ import annotations

import hashlib
import hmac
import io
import json
import os
import re
import threading
import uuid
import wave
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any

MAX_JSON = 1024 * 1024
MAX_INPUT = 32 * 1024 * 1024
MAX_OUTPUT = 32 * 1024 * 1024
MAX_ARTIFACTS = 32
MAX_TOTAL_ASSET_BYTES = 64 * 1024 * 1024
ID_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}/[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
SHA_RE = re.compile(r"^(?:[a-fA-F0-9]{40}|[a-fA-F0-9]{64})$")
# Tasks outside this set require an independently reviewed runtime-family adapter.
SUPPORTED = {
    "diffusers": {"text-to-image", "image-to-image"},
    "transformers": {"text-to-audio", "text-to-speech", "automatic-speech-recognition"},
}


class WorkerError(Exception):
    def __init__(self, code: str, message: str, status: int = 400):
        super().__init__(message)
        self.code = code
        self.status = status


def _fixture(kind: str) -> tuple[str, str, bytes]:
    if kind == "image":
        return "image", "image/svg+xml", (
            b'<svg xmlns="http://www.w3.org/2000/svg" width="320" height="80">'
            b'<text x="8" y="40">OFFLINE FIXTURE - NOT AI</text></svg>'
        )
    if kind in {"audio", "music", "speech"}:
        stream = io.BytesIO()
        with wave.open(stream, "wb") as w:
            w.setnchannels(1)
            w.setsampwidth(2)
            w.setframerate(8000)
            w.writeframes(b"\x00\x00" * 8000)
        return kind, "audio/wav", stream.getvalue()
    if kind == "three_d":
        return kind, "model/obj", b"# OFFLINE FIXTURE - NOT AI\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n"
    return kind, "application/json", json.dumps(
        {"simulation_only": True, "media_kind": kind, "not_real_ai_inference": True}
    ).encode()


class Worker:
    def __init__(self, model_id: str, revision: str, task: str, family: str,
                 token: str, fixture: bool = False,
                 deployment_request_id: str | None = None):
        if not ID_RE.fullmatch(model_id) or not SHA_RE.fullmatch(revision):
            raise ValueError("Model identifier and immutable SHA must be valid")
        if not token or len(token) < 16:
            raise ValueError("CMB_WORKER_TOKEN must be at least 16 characters")
        if not fixture and (family not in SUPPORTED or task not in SUPPORTED[family]):
            raise ValueError("This worker does not implement the chosen model task/runtime")
        self.model_id = model_id
        self.revision = revision
        self.task = task
        self.family = family
        self.token = token
        self.fixture = fixture
        if deployment_request_id is not None and not re.fullmatch(r"[A-Za-z0-9_-]{1,128}", deployment_request_id):
            raise ValueError("Invalid deployment request ID")
        self.deployment_request_id = deployment_request_id
        self.assets: dict[str, tuple[bytes, str, str]] = {}
        self.asset_bytes = 0
        self.lock = threading.RLock()

    def authorize(self, header: str) -> bool:
        return hmac.compare_digest(header, f"Bearer {self.token}")

    def store(self, kind: str, mime: str, data: bytes) -> dict[str, Any]:
        if not data or len(data) > MAX_OUTPUT:
            raise WorkerError("INVALID_ARTIFACT", "Artifact empty or too large", 422)
        name = uuid.uuid4().hex
        digest = hashlib.sha256(data).hexdigest()
        with self.lock:
            if len(self.assets) >= MAX_ARTIFACTS or self.asset_bytes + len(data) > MAX_TOTAL_ASSET_BYTES:
                raise WorkerError("ASSET_LIMIT", "Worker asset capacity exceeded", 503)
            self.assets[name] = (data, mime, digest)
            self.asset_bytes += len(data)
        return {"kind": kind, "media_type": mime, "size_bytes": len(data),
                "sha256": digest, "asset_ref": name,
                "metadata": {"simulation_only": self.fixture}}

    def load(self, asset_id: str) -> tuple[bytes, str, str]:
        if not re.fullmatch(r"[0-9a-f]{32}", asset_id):
            raise WorkerError("INVALID_ASSET_REF", "Invalid asset identifier", 400)
        with self.lock:
            if asset_id not in self.assets:
                raise WorkerError("ASSET_NOT_FOUND", "Asset not available", 404)
            return self.assets[asset_id]

    def _input(self, items: list[dict[str, Any]], kind: str) -> dict[str, Any]:
        for item in items:
            if isinstance(item, dict) and item.get("kind") == kind:
                return item
        raise WorkerError("MISSING_INPUT", f"Required {kind} input is missing")

    def _text(self, items: list[dict[str, Any]]) -> str:
        value = self._input(items, "text").get("text")
        if not isinstance(value, str) or not value or len(value) > 5000:
            raise WorkerError("INVALID_PROMPT", "Expected a nonempty bounded prompt")
        return value

    def generate(self, request: dict[str, Any]) -> dict[str, Any]:
        if not isinstance(request, dict):
            raise WorkerError("INVALID_REQUEST", "Request must be an object")
        if request.get("schema_version") != 1 or isinstance(request.get("schema_version"), bool):
            raise WorkerError("INVALID_SCHEMA_VERSION", "Worker protocol schema_version=1 required")
        model = request.get("model")
        if not isinstance(model, dict) or model.get("id") != self.model_id \
                or model.get("revision_sha") != self.revision:
            raise WorkerError("MODEL_MISMATCH", "Request must match pinned worker model SHA")
        if request.get("task") != self.task:
            raise WorkerError("TASK_MISMATCH", "Task differs from preapproved worker")
        request_id = request.get("request_id")
        if not isinstance(request_id, str) or not 1 <= len(request_id) <= 128:
            raise WorkerError("INVALID_REQUEST_ID", "Valid request ID required")
        items = request.get("inputs")
        params = request.get("parameters", {})
        if not isinstance(items, list) or len(items) > 16 or not isinstance(params, dict):
            raise WorkerError("INVALID_INPUT", "Invalid media inputs or parameters")
        if not self.fixture and "fixture_kind" in request:
            raise WorkerError("FIXTURE_DISABLED", "Fixture requests are not permitted in real inference mode")
        if self.fixture:
            kind = request.get("fixture_kind")
            if kind not in {"image", "video", "audio", "speech", "music", "three_d", "spatial", "document", "multimodal"}:
                raise WorkerError("INVALID_FIXTURE", "Unknown fixture kind")
            kind, mime, data = _fixture(kind)
        else:
            raise WorkerError("ADVISORY_ONLY", "This repository supports offline fixtures only", 403)
        artifact = self.store(kind, mime, data)
        return {"schema_version": 1, "request_id": request_id, "status": "completed",
                "simulation_only": self.fixture, "artifacts": [artifact]}


def handler_for(worker: Worker) -> type[BaseHTTPRequestHandler]:
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, _fmt: str, *_args: object) -> None:
            # Avoid logging authorization tokens or private model input.
            pass

        def _response(self, status: int, payload: dict[str, Any]) -> None:
            body = json.dumps(payload, separators=(",", ":")).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(body)

        def _read(self, limit: int) -> bytes:
            length = self.headers.get("Content-Length")
            if length is None or not length.isascii() or not length.isdigit():
                raise WorkerError("LENGTH_REQUIRED", "Content-Length required", 411)
            n = int(length)
            if n < 1 or n > limit:
                raise WorkerError("PAYLOAD_TOO_LARGE", "Request size out of bounds", 413)
            data = self.rfile.read(n)
            if len(data) != n:
                raise WorkerError("INCOMPLETE_BODY", "Truncated request body", 400)
            return data

        def _auth(self) -> bool:
            if worker.authorize(self.headers.get("Authorization", "")):
                return True
            self._response(401, {"error": {"code": "UNAUTHORIZED", "message": "Authentication required"}})
            return False

        def do_GET(self) -> None:
            if self.path == "/health":
                if not worker.fixture and not self._auth():
                    return
                self._response(200, {
                    "status": "ok", "family": worker.family, "task": worker.task,
                    "model_id": worker.model_id, "revision_sha": worker.revision,
                    "deployment_request_id": worker.deployment_request_id,
                    "simulation_only": worker.fixture
                })
                return
            if not self._auth():
                return
            if not self.path.startswith("/v1/media/artifacts/"):
                self._response(404, {"error": {"code": "NOT_FOUND", "message": "Unknown endpoint"}})
                return
            name = self.path.split("/")[-1]
            try:
                data, mime, digest = worker.load(name)
                if hashlib.sha256(data).hexdigest() != digest:
                    raise WorkerError("INTEGRITY_FAILURE", "Stored artifact checksum mismatch", 500)
                self.send_response(200)
                self.send_header("Content-Type", mime)
                self.send_header("Content-Length", str(len(data)))
                self.send_header("X-Content-SHA256", digest)
                self.send_header("Cache-Control", "no-store")
                self.end_headers()
                self.wfile.write(data)
            except WorkerError as e:
                self._response(e.status, {"error": {"code": e.code, "message": str(e)}})

        def do_POST(self) -> None:
            if not self._auth():
                return
            try:
                if self.path == "/v1/media/assets":
                    data = self._read(MAX_INPUT)
                    kind = self.headers.get("X-CMB-Media-Kind", "")
                    mime = self.headers.get("Content-Type", "")
                    if kind not in {"image", "audio", "video", "document"} or mime not in {
                        "image/png", "image/jpeg", "image/webp", "audio/wav", "audio/x-wav",
                        "video/mp4", "application/pdf"
                    }:
                        raise WorkerError("BAD_MEDIA_TYPE", "Unsupported input media type", 415)
                    self._response(201, worker.store(kind, mime, data))
                    return
                if self.path != "/v1/media/generate":
                    raise WorkerError("NOT_FOUND", "Unknown endpoint", 404)
                data = self._read(MAX_JSON)
                request = json.loads(data)
                self._response(200, worker.generate(request))
            except WorkerError as e:
                self._response(e.status, {"error": {"code": e.code, "message": str(e)}})
            except (ValueError, TypeError, UnicodeError, json.JSONDecodeError):
                self._response(400, {"error": {"code": "INVALID_JSON", "message": "Invalid JSON request"}})
            except Exception:
                # Never leak library stack traces, tokens, private paths or model inputs.
                self._response(500, {"error": {"code": "INFERENCE_FAILED", "message": "Worker inference failed"}})

    return Handler


def main() -> None:
    fixture = os.getenv("CMB_WORKER_MODE") == "fixture"
    host = os.getenv("CMB_BIND", "127.0.0.1")
    if fixture and (host not in {"127.0.0.1", "::1"} or os.getenv("CMB_ENABLE_TEST_FIXTURE") != "1"):
        raise SystemExit("Fixture mode is restricted to local loopback tests")
    if host not in {"127.0.0.1", "::1"} and os.getenv("CMB_PRIVATE_NETWORK_ACK") != "1":
        raise SystemExit("Non-loopback worker binding requires explicit private-network/TLS ingress acknowledgment")
    if not fixture:
        raise SystemExit("This worker supports offline fixtures only")
    token = os.environ["CMB_WORKER_TOKEN"]
    worker = Worker(
        os.environ["CMB_MODEL_ID"], os.environ["CMB_MODEL_SHA"],
        os.environ["CMB_TASK"], os.environ["CMB_RUNTIME_FAMILY"], token, fixture,
        os.getenv("CMB_DEPLOYMENT_REQUEST_ID")
    )
    server = ThreadingHTTPServer((host, int(os.getenv("CMB_PORT", "8000"))), handler_for(worker))
    server.serve_forever()


if __name__ == "__main__":
    main()
