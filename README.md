# Cloud-Media-Bridge

**A local MCP/CLI bridge for discovering Hugging Face media models and preparing execution in the user's own GPU account.** No central service, shared user accounts, or hosted LLMs.

**Current status:** early Rust prototype. The MCP `doctor` tool and basic error-handling tests were previously reported as passing on Windows and Ubuntu WSL. Live Hugging Face discovery, read-only planning, and the provider catalog have been added to the current development branch; their CI/integration status must be checked before claiming they work. **No media generation or cloud deployment is implemented yet.**

## System architecture

```text
User's AI harness (any client supporting local MCP stdio)
    -> Cloud-Media-Bridge (single local Rust process)
       -> Hugging Face Hub (live search / URL inspection / model SHA)
       -> User-selected GPU provider profile (read-only plan)
       -> Approved, compatible serving image or library pipeline (future)
       -> Media result (future)
```

A model that appears on the Hub is **discoverable**, not automatically **runnable**. Hugging Face model configuration and weights do not, by themselves, provide an executable inference server. The bridge must match each model to a verified runtime recipe (serving image or existing library pipeline), install it on the selected provider, and load the exact model revision. Models sharing an established pipeline can run without hardcoding model names; new architectures may require an additional runtime recipe. The project reuses existing inference software rather than building a separate inference engine.

## What works and what does not

| Capability | Status |
|---|---|
| Rust local CLI and MCP stdio `doctor` | Implemented; previous process-level tests reported passing |
| MCP provider listing and inspection | Implemented; offline test code included |
| Live Hugging Face model search | Implemented; requires network, `curl`, and live verification |
| Model URL inspection and immutable revision SHA | Implemented; requires live Hub verification |
| Read-only generation plan for a selected provider | Implemented; explicitly blocks unverified execution |
| Human model and cloud-resource approval workflow | Planned; **no generated media or provisioning permitted** |
| Modal deployment and image/audio/video generation | Planned; no GPU tests run |
| RunPod / Vast.ai / Lambda / GPU host deployment | Planned; configured in the catalog, **not operational adapters** |

## Quick start (Windows, Linux, macOS)

Requires a current stable Rust toolchain, Cargo, and a `curl` executable on the local `PATH`. Build and run:

```sh
cargo test --locked --workspace
cargo run --locked -- doctor
cargo run --locked -- providers
cargo run --locked -- providers modal
cargo run --locked -- models search "text-to-image" 10
cargo run --locked -- models inspect "stabilityai/stable-diffusion-xl-base-1.0"
cargo run --locked -- plan "stabilityai/stable-diffusion-xl-base-1.0" modal
```

The last three commands access Hugging Face over HTTPS; the other commands run without provider credentials or GPUs. **None of these commands provisions resources or submits inference jobs.**

For any local MCP-compatible AI harness, launch the `cloud-media-bridge` binary with the argument `mcp`. The server exposes `doctor`, `providers_list`, `providers_show`, `models_search`, `models_inspect`, and `plan_generation`. Stdout is reserved for MCP JSON-RPC messages. The standalone CLI works without a specific AI-agent brand.

## Configuring GPU platforms

Public, non-secret provider definitions live at [`config/providers.json`](config/providers.json). The catalog covers **Modal, RunPod Serverless, RunPod Pods, Vast.ai Serverless, Vast.ai Instances, Lambda Cloud, existing HTTPS workers, generic SSH GPU hosts, and Kubernetes GPU clusters**.

To use a modified local catalog, copy it outside the repository and set the `CMB_PROVIDER_CONFIG` environment variable to that file's path. Keep private API credentials in the provider's own local authentication mechanism or protected environment variables, never in a public configuration file.

**Catalog entry != supported deployment.** Each provider has unique authentication, endpoint, provisioning and cleanup semantics. Status fields explicitly say `read_only` or `planned` until a working driver and live tests exist. A future generic HTTPS worker protocol can cover many rented GPU hosts once their compatible worker is deployed separately.

**GPU rentals such as Vast.ai:** they can run standard Docker images, including vendor base/PyTorch images or a verified custom serving image. Where an image does not contain a model-specific runtime, a reviewed startup/provisioning recipe installs a compatible library and loads the approved Hugging Face revision. This is not automatic execution of arbitrary scripts found in a model repository. For GPU pods and VMs, the bridge must also report and perform only user-approved stop/termination operations.

See [Provider configuration and lifecycle](docs/PROVIDERS.md).

## Safety and GPU credits

- Model selection and immutable revision must be explicitly approved by the human.
- Provisioning/terminating cloud resources requires a separate approval; do not treat selecting a model as permission to spend credits.
- No automatic provider switching, public unauthenticated worker, remote arbitrary Python model code, or unsafe deserialization.
- Only Modal is scheduled for **first real-GPU validation**. The remaining providers are configuration/roadmap targets.
- All default tests are offline. The existing small Modal trial credit must not be consumed by build, lint, search, inspection, planning or CI.

## Project documents

- [Implementation specification](Cloud-Media-Bridge.md) — architecture and implementation requirements.
- [Detailed test plan](Cloud-Media-Bridge-TESTING.md) — primary v1 gates and 82 broader test scenarios.
- [Test result ledger](Cloud-Media-Bridge-TEST-RESULTS.md) — evidence and `NOT RUN` status for unexecuted tests.
- [Provider support matrix](docs/PROVIDERS.md).
- [Contributor instructions](AGENTS.md).

**License:** [Apache-2.0](LICENSE).
