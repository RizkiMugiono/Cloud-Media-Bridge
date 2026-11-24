#!/usr/bin/env python3
"""Task-scoped existing-library media worker.

The local bridge remains Rust/MCP. This remote worker is Python because
Hugging Face Diffusers/Transformers inference implementations are Python.
Production requires a pinned, independently verified OCI image/installer,
private networking or authenticated TLS ingress, and explicit resource approval.
Unit tests run exclusively in FIXTURE mode, without model downloads or GPUs.
"""
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


def _as_wav(audio: Any, sample_rate: int) -> bytes:
    # Frameworks generally return a float NumPy array; imports occur only on
    # actual inference paths, never during offline protocol tests.
    if not 8000 <= sample_rate <= 192000:
        raise WorkerError("BAD_SAMPLE_RATE", "Unsupported output sample rate")
    import numpy as np  # type: ignore[import-not-found]
    samples = np.asarray(audio, dtype=np.float32).reshape(-1)
    if samples.size > 10_000_000:
        raise WorkerError("OUTPUT_TOO_LARGE", "Audio output exceeds safety limit")
    pcm = (np.clip(samples, -1.0, 1.0) * 32767.0).astype("<i2").tobytes()
    out = io.BytesIO()
    with wave.open(out, "wb") as writer:
        writer.setnchannels(1)
        writer.setsampwidth(2)
        writer.setframerate(sample_rate)
        writer.writeframes(pcm)
    return out.getvalue()


class Worker:
    def __init__(self, model_id: str, revision: str, task: str, family: str,
                 token: str, fixture: bool = False):
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
        self.assets: dict[str, tuple[bytes, str, str]] = {}
        self.asset_bytes = 0
        self.lock = threading.RLock()
        self.pipeline: Any = None

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

    def _real_generate(self, inputs: list[dict[str, Any]],
                       params: dict[str, Any]) -> tuple[str, str, bytes]:
        if self.family == "diffusers":
            try:
                import torch  # type: ignore[import-not-found]
                from diffusers import DiffusionPipeline, AutoPipelineForImage2Image  # type: ignore[import-not-found]
                from PIL import Image  # type: ignore[import-not-found]
            except ImportError as exc:
                raise WorkerError("RUNTIME_DEPENDENCIES_MISSING", str(exc), 503) from exc
            if not torch.cuda.is_available():
                raise WorkerError("GPU_REQUIRED", "CUDA device is required", 503)
            if self.pipeline is None:
                cls = AutoPipelineForImage2Image if self.task == "image-to-image" else DiffusionPipeline
                # force safetensors and never supply custom_pipeline or trust_remote_code
                self.pipeline = cls.from_pretrained(self.model_id, revision=self.revision,
                    torch_dtype=torch.float16, use_safetensors=True).to("cuda")
            prompt = self._text(inputs)
            steps = params.get("num_inference_steps", 20)
            if not isinstance(steps, int) or not 1 <= steps <= 60:
                raise WorkerError("INVALID_PARAMETER", "num_inference_steps must be 1..60")
            kwargs: dict[str, Any] = {"prompt": prompt, "num_inference_steps": steps}
            if self.task == "image-to-image":
                item = self._input(inputs, "image")
                raw, mime, _ = self.load(str(item.get("asset_ref", "")))
                if mime not in {"image/png", "image/jpeg", "image/webp"} or len(raw) > MAX_INPUT:
                    raise WorkerError("INVALID_IMAGE", "Staged image is not an allowed type")
                src = Image.open(io.BytesIO(raw))
                src.verify()
                kwargs["image"] = Image.open(io.BytesIO(raw)).convert("RGB")
            with torch.inference_mode():
                result = self.pipeline(**kwargs)
            picture = result.images[0]
            buffer = io.BytesIO()
            picture.save(buffer, format="PNG")
            return "image", "image/png", buffer.getvalue()

        if self.family == "transformers":
            try:
                from transformers import pipeline  # type: ignore[import-not-found]
                import torch  # type: ignore[import-not-found]
            except ImportError as exc:
                raise WorkerError("RUNTIME_DEPENDENCIES_MISSING", str(exc), 503) from exc
            if not torch.cuda.is_available():
                raise WorkerError("GPU_REQUIRED", "CUDA device is required", 503)
            if self.pipeline is None:
                self.pipeline = pipeline(self.task, model=self.model_id,
                    revision=self.revision, trust_remote_code=False, device=0)
            if self.task == "automatic-speech-recognition":
                item = self._input(inputs, "audio")
                audio, mime, _ = self.load(str(item.get("asset_ref", "")))
                if mime not in {"audio/wav", "audio/x-wav"}:
                    raise WorkerError("INVALID_AUDIO", "Only staged WAV input is supported")
                import numpy as np  # type: ignore[import-not-found]
                with wave.open(io.BytesIO(audio), "rb") as w:
                    if w.getnchannels() != 1 or w.getsampwidth() != 2:
                        raise WorkerError("INVALID_AUDIO", "Use mono 16-bit WAV")
                    sample_rate = w.getframerate()
                    samples = np.frombuffer(w.readframes(w.getnframes()), dtype="<i2").astype(np.float32) / 32768.0
                result = self.pipeline({"array": samples, "sampling_rate": sample_rate})
                return "text", "application/json", json.dumps(
                    {"text": result["text"]}, ensure_ascii=False).encode()
            prompt = self._text(inputs)
            result = self.pipeline(prompt)
            if not isinstance(result, dict) or "audio" not in result or "sampling_rate" not in result:
                raise WorkerError("INCOMPATIBLE_PIPELINE", "Expected an audio-generating Transformers pipeline", 422)
            data = _as_wav(result["audio"], int(result["sampling_rate"]))
            return ("speech" if self.task == "text-to-speech" else "audio"), "audio/wav", data
        raise WorkerError("UNSUPPORTED_RUNTIME", "No verified handler for this model family", 422)

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
            kind, mime, data = self._real_generate(items, params)
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
                self._response(200, {"status": "ok", "family": worker.family, "task": worker.task,
                                     "simulation_only": worker.fixture})
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
    token = os.environ["CMB_WORKER_TOKEN"]
    worker = Worker(
        os.environ["CMB_MODEL_ID"], os.environ["CMB_MODEL_SHA"],
        os.environ["CMB_TASK"], os.environ["CMB_RUNTIME_FAMILY"], token, fixture
    )
    server = ThreadingHTTPServer((host, int(os.getenv("CMB_PORT", "8000"))), handler_for(worker))
    server.serve_forever()


if __name__ == "__main__":
    main()
