# Offline end-to-end execution, worker interoperability, and provider contracts

**Status:** The repo now includes an executable Rust **offline simulation** and a genuine local HTTP **fixture worker**. These exercise the lifecycle without any cloud credential, GPU, paid resource, pretrained weights, or model download. The simulation is intentionally distinct from real AI inference.

## What runs today

```text
AI agent / Rust CLI/MCP
  -> prepare exact mock deployment plan (random nonce + SHA-256 binding)
  -> human terminal challenge OR explicit test-only approval shortcut
  -> consume one-shot grant
  -> mock provider creates resource with ownership and request ID
  -> task-scoped fixture worker creates locally verifiable output
  -> SHA-256 and size verified, result returned to agent
  -> provider confirms the bridge-owned mock resource has stopped
  -> journal and mock provider observations persist in a private session directory
```

The runtime resolution layer and media taxonomy remain independent of the fixture. A fixture `image` output is a basic SVG; `audio`, `music`, `speech` are real WAV containers with test samples; `three_d` produces a tiny OBJ; `video`, `document`, `spatial` and `multimodal` use **explicitly labeled JSON simulation artifacts**. None of these are outputs from an AI model.

### Rust CLI: offline deployment

Run from the repository root after installing stable Rust and Cargo:

```sh
cargo test --locked --workspace --all-targets
cargo run --locked -- offline demo ./test-session-image image
cargo run --locked -- offline verify ./test-session-image
cargo run --locked -- offline status ./test-session-image
```

The `demo` shortcut automatically supplies a **test-only grant**, never a human approval, and refuses to run against a real GPU provider. A new session needs a new directory.

To exercise a separate human terminal confirmation in the fixture-only workflow:

```sh
cargo run --locked -- offline prepare ./test-session-music music
cargo run --locked -- offline approve ./test-session-music
cargo run --locked -- offline run ./test-session-music
cargo run --locked -- offline verify ./test-session-music
cargo run --locked -- offline reconcile ./test-session-music
```

`offline approve` requires an interactive TTY and asks for the exact random challenge. Redirected stdin or an MCP tool argument cannot approve. This is **not a complete protection against a malicious agent with same-user shell/PTY privileges**; real money-bearing deployment must use an independently trusted approval channel and transaction-bound one-shot authorization.

The default fixture path must be a writable user-owned directory. Journals are atomically updated and resource ownership is checked before mock stopping. A stale session lock after a process crash requires human inspection/reconciliation; deleting a lock blindly is unsafe.

## Rust-to-Python HTTP worker interop (CPU only)

[`workers/media_worker.py`](../workers/media_worker.py) is a task-scoped Python HTTP worker for existing inference runtimes; this is **not** an inference engine written from scratch. It implements:

- `GET /health`: status and task family, no secret.
- `POST /v1/media/assets`: authenticated media asset staging with bounded in-memory storage and opaque handles.
- `POST /v1/media/generate`: authenticated, pinned model ID/SHA and task-specific inputs.
- `GET /v1/media/artifacts/{id}`: authenticated binary download with a SHA-256 header.

Worker HTTP contract schemas are [worker-request](../schemas/worker-request.schema.json) and [worker-result](../schemas/worker-result.schema.json). The previous [generic request](../schemas/media-request.schema.json) and [generic result](../schemas/media-result.schema.json) envelopes describe future provider-facing contracts, not the current worker wire format. In fixture mode the worker explicitly requires `schema_version=1`; production mode rejects `fixture_kind` and never returns test media.

It binds `127.0.0.1` by default. Never expose a worker directly on the public internet; remote GPU use requires provider-managed TLS/authenticated ingress or trusted private networking plus a worker token provided through the user's own secret store. The worker **does not need the GPU cloud provisioning API key**.

Start a local fixture worker from a shell using a random private token (illustrative shell commands):

```sh
export CMB_WORKER_TOKEN="replace-with-a-random-secret-at-least-16-chars"
export CMB_MODEL_ID="fixture/offline-model"
export CMB_MODEL_SHA="0000000000000000000000000000000000000000"
export CMB_TASK="text-to-image"
export CMB_RUNTIME_FAMILY="diffusers"
export CMB_WORKER_MODE="fixture"
export CMB_ENABLE_TEST_FIXTURE="1"
python workers/media_worker.py
```

In another terminal, with the same token set:

```sh
cargo run --locked -- offline worker-smoke ./worker-fixture-output 8000 music
python -m unittest discover -s workers/tests -v
```

The Rust bridge checks the worker's `simulation_only=true` health, sends a fixture request to **loopback only**, receives an artifact handle, downloads its bytes and verifies size/SHA-256. It refuses any external HTTP URL in this testing mode. MCP tools `offline_fixture_demo`, `offline_fixture_status`, `offline_fixture_verify`, and `offline_worker_smoke` are available; state-changing fixture actions require an explicit `confirm_fixture_only=true` argument.

### Actual model loading (not GPU-verified)

The Python worker has **real inference code paths** for the following compatible tasks, but **no execution evidence** with real Hugging Face weights or GPU has yet been collected:

| Upstream framework | Implemented worker path | Unverified prerequisites |
|---|---|---|
| Diffusers | `text-to-image`, `image-to-image` | Model SHA, safetensors, supported pipeline, PyTorch+CUDA, Pillow, VRAM, codecs |
| Transformers | `text-to-audio`, `text-to-speech`, `automatic-speech-recognition` | Supported pipeline, model SHA, PyTorch+CUDA, NumPy, mono PCM WAV and correct preprocessing |
| ComfyUI, AudioCraft, pyannote, SpeechBrain, 3D and other specialized models | Registry candidates only | Reviewed model-specific worker code, workflows/installers, pinned images and independent tests |
| Video, motion, document and arbitrary any-to-any | Classification and media contract only | Verified runtime-family implementation; fixture JSON is **not video inference** |

An official upstream library does not imply every arbitrary model works. The code does not install arbitrary model-card scripts, accept `trust_remote_code` or silently substitute another model. Real workers must be built into pinned and reviewed OCI images or audited installers before being enabled on billable hardware.

## Provider operation previews (offline)

Use `provider-preview` or MCP `provider_operation_preview` to inspect proposed request/stop shapes, without credentials or network traffic:

```sh
cargo run --locked -- provider-preview runpod-pods "ghcr.io/example/worker@sha256:<64-hex-digest>" "NVIDIA RTX 4090" 50
cargo run --locked -- provider-preview vast-instances "ghcr.io/example/worker@sha256:<64-hex-digest>" "NVIDIA RTX 4090" 50 12345
```

Replace the image example with an actual reviewed OCI digest before any future deployment. Preview generation **never invokes** a cloud API.

| Provider | Preview contract status | Idle/stop contract |
|---|---|---|
| Modal | CLI `modal server deploy` image/HTTP intent; GPU flags require live CLI verification | Serverless scale-to-zero, needs observation |
| RunPod Pods | REST v2 pod creation shape | REST v2 `action:stop`; require `EXITED` confirmation |
| RunPod Serverless | REST v2 endpoint creation with `workers.min=0` | Idle worker scale-to-zero |
| Vast.ai Instances | Selected offer creation shape | `state:stopped` operation; verify instance state |
| Vast.ai Serverless | Separate serverless endpoint handler not yet implemented | Requires separate autoscaling verification |
| Lambda Cloud | Launch intent requires region/type/SSH/image/cloud-init | No verified non-destructive stop API: **do not auto-terminate** |
| Existing authenticated HTTPS worker | Connect to a preexisting endpoint | No ownership = no stop |
| Existing SSH host | Vetted installer required | No ownership = no stop |
| Kubernetes GPU | Owned GPU Job template intent | Verify workload cleanup and actual node scale-down independently |

Provider previews are **not operational adapters**: they cannot provision, execute, stop or charge anything. Each request must be wired to the official provider API/CLI and tested with explicit account authorization later. Vendor APIs may change; these layouts are grounded in their public 2026 documentation but lack live credentials or actual release-grade verification.

## Failure and security coverage

Offline Rust + Python tests cover: one-shot approval and replay, forged approval flags, tampered exact deployment plan, resource ownership, concurrent active-job stop deferral, stop failure and retry, crash-window resource discovery by request ID, artifact mutation, SHA-256 standard vectors, nine fixture categories, private worker authentication, invalid model SHA/task, output bounds and path traversal, and RunPod/Vast stop intent.

**Important limitations:** Fixture reconciliation only occurs when the CLI/MCP process runs again; there is **no always-on watchdog** and a powered-off laptop cannot issue a delayed stop. For real pods/VMs, use provider-side TTL/watchdog where available or disclose the residual billing risk, and never claim crash-proof guaranteed stop. No fixture evidence proves a real GPU model runs.

## Remaining live validation

Before calling real deployment production-ready, all required real-cloud steps must pass separately for **each provider and each claimed runtime family**: pinned image/installer build, model SHA download and license checks, full model load on a compatible GPU, media output and checksum, one-shot independently human-approved resource provisioning, authentic API status and stop/scale-to-zero reconciliation, crash/error/cancellation behavior, account/workspace isolation and billing exposure. No default GitHub Action performs these operations.

### Provider documentation

- [RunPod API v2 pods](https://docs.runpod.io/api-reference-v2/pods/create-a-pod), [stop action](https://docs.runpod.io/api-reference-v2/pods/trigger-a-pod-state-transition), [serverless endpoint](https://docs.runpod.io/api-reference-v2/serverless/create-a-serverless-endpoint)
- [Vast.ai REST API](https://vast.ai/developers/api)
- [Modal Server CLI](https://modal.com/docs/cli/latest/server)
- [Lambda Cloud API](https://docs.lambda.com/api/cloud)
