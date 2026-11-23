# Cloud-Media-Bridge — Minimal Implementation Specification

**Repository:** https://github.com/RizkiMugiono/Cloud-Media-Bridge  
**Version:** 2026-11-23 (simplified local bridge)  
**Current status:** A working Rust MCP/CLI prototype is under development. Real GPU inference is not implemented or verified.  
**Related:** [README](README.md) · [GPU providers](docs/PROVIDERS.md) · [Test plan](Cloud-Media-Bridge-TESTING.md) · [Results](Cloud-Media-Bridge-TEST-RESULTS.md)

## 1. Purpose

Cloud-Media-Bridge is **one local, open-source MCP stdio/CLI process** that connects the user's chosen AI agent to a live catalog of open-source media models and the user's own GPU rental account.

It is **not** a hosted SaaS, a shared payment system, a central proxy, a model marketplace, an LLM hosting service, or a newly invented deep-learning inference engine.

```text
MCP-compatible AI harness / CLI client
  -> Local Cloud-Media-Bridge (Rust)
     -> Live Hugging Face search / inspect / revision SHA
     -> User chooses model and GPU provider
     -> Compatible existing container or library pipeline
     -> Human approves the resource operation
     -> GPU in user's provider account -> media result
```

One MCP bridge is sufficient. Codex, Claude Code, OpenCode, Hermes Agent, DeepSeek-based agents and Grok-based tool wrappers are **possible clients**, subject to their actual MCP/CLI capabilities and individually tested versions. The project is not tied to any one AI brand.

## 2. Actual code vs. planned features

| Capability | Current implementation |
|---|---|
| Local Rust binary, MCP stdio, `doctor` | Implemented; process-level tests included |
| Catalog of nine GPU provider classes | Implemented as **read-only configurations** |
| Hugging Face live search | Implemented; needs real network verification |
| Inspect model ID/URL and immutable revision | Implemented; needs real Hub verification |
| Provider-aware, non-mutating generation plan | Implemented; always returns `safe_to_deploy=false` until a runtime is verified |
| Human authorization grants | Planned; execution must not be offered as approved until built |
| Modal account integration / deployment / GPU inference | Planned |
| RunPod, Vast.ai, Lambda and generic host deployment | Planned, not claimed operational |
| Media artifact download and validation | Planned |
| Durable asynchronous jobs, advanced parallel tuning | Future, after minimal end-to-end path |

The status above refers to code availability. A feature is **verified** only when its corresponding tests have succeeded with recorded evidence.

## 3. Model catalog

The model catalog is **dynamic** and must never rely on a hardcoded list of model names. An AI harness or user can:

1. Search Hugging Face live.
2. Paste `https://huggingface.co/owner/model` or `owner/model`.
3. Inspect metadata: task, framework/library, access restrictions, gated status and license.
4. Resolve an immutable commit SHA.
5. See whether there is a compatible serving runtime.
6. Select the exact model/revision and confirm it as the human.

A newly published model becomes discoverable without a repository update. **It becomes runnable only if an existing compatible serving image or library pipeline is proven to load it.** Unknown architectures must not be deployed optimistically. Discovery status and execution status remain separate.

External metadata and model cards are **untrusted data**, never instructions to the agent or shell. By default, do not execute arbitrary remote Python, unsafe pickle or unverified `trust_remote_code`.

## 4. GPU platforms: one configuration format, provider-specific drivers

The checked-in [`config/providers.json`](config/providers.json) catalog includes:

- **Modal** serverless (first real-GPU test target).
- **RunPod** Serverless and Pods.
- **Vast.ai** Serverless and rented Instances.
- **Lambda** GPU virtual machines.
- A user-controlled **HTTPS media worker** on any compatible GPU rental.
- A user-controlled **SSH/OCI GPU host**.
- A user-owned **Kubernetes GPU cluster**.

Additional providers can be represented by an updated catalog and a small driver when their API semantics require it. **Listing a platform in configuration does not mean deployment works.** The `integration`, `driver`, `capabilities` and `scale_to_zero` fields make the status explicit.

Do not hardcode a central provider account or share credentials between users. Profiles and API tokens are always user-owned, private and local.

A single compatible HTTPS media-worker contract can allow the same inference API to work on several rental platforms **after the user has provisioned that worker**. Provisioning, identity, token handling and cleanup remain provider-specific.

### Provider adapter responsibilities

1. Detect correct user account/workspace and authenticated capabilities.
2. Build a **read-only** execution/resource plan with an exact model SHA, task, runtime, GPU request and lifecycle warning.
3. Request human resource-operation approval separately from model approval.
4. Provision or connect only through a verified adapter.
5. Submit/poll/cancel only with safe idempotency and authenticated transport.
6. Return hash-checked artifacts and report any resources that remain billable.

Unknown capabilities fail closed. No unverified fallback to another model or GPU provider.

### Resource lifecycle

| Mode | Expected behavior |
|---|---|
| Serverless | Scale-to-zero only when supported and tested; storage/network charges may persist |
| GPU pod / VM / marketplace instance | Manual stop/termination may be needed; stopping GPU does not guarantee all fees stop |
| Existing HTTPS/SSH host | Lifecycle remains user-managed unless ownership and cleanup semantics are verified |
| Kubernetes | Depends on cluster/node autoscaling and configuration |

The repository does **not** implement an automatic per-user cost ceiling. It must still prevent unintended cloud resource creation and include ordinary technical safety limits.

## 5. Reuse inference runtimes, do not build new engines

No separate native Rust inference engine or universal Python inference engine is required for **v1**. Rust only runs the local MCP/CLI and provider orchestration.

There are two acceptable execution routes:

1. **Compatible OCI serving image**: user selects a pinned existing media-serving image that can load the model/revision and exposes an authenticated media request API.
2. **Library-backed application**: a small provider-native deployment that invokes an existing tested library pipeline (such as Diffusers or a suitable speech/video runtime) and exposes the same media API.

For both routes, runtime requirements, license/gating, task schema, input media, GPU VRAM, output format and security must be verified. A Hugging Face link alone cannot deploy itself.

**Do not build a custom scheduler, orchestration database, giant universal image, or global server until a proven use case requires it.** Use the provider's supported job/scale lifecycle for the minimal first test.

## 6. Human approval and privacy

The AI agent can **search, compare and suggest**. It cannot decide to spend money or select a model on behalf of the user without approval.

**Model approval** must bind canonical model ID, immutable SHA, task and the selected source. **Resource approval** must separately bind compute provider, account/workspace, GPU/resource parameters, runtime image and action. Changing any relevant dimension invalidates the corresponding grant.

The current read-only implementation **does not provide approval or execution tools**. Do not implement GPU execution until the approval path exists and has negative security tests.

Privileged approval must be via an independent user interface/TTY and not a boolean inside an MCP tool request. A local shell-capable compromised agent operating under the same OS user may bypass weak approval storage; document this trust limitation and use stronger OS identity separation when possible.

Credentials must never be checked into Git, echoed in MCP stdout, or sent to a model container as provisioning secrets. No maintainer-owned telemetry is collected by default.

## 7. CLI and MCP contracts

**Currently implemented read-only CLI:**

```text
cloud-media-bridge doctor
cloud-media-bridge mcp
cloud-media-bridge providers [provider-id]
cloud-media-bridge models search <query> [limit: 1..50]
cloud-media-bridge models inspect <owner/model | https://huggingface.co/owner/model>
cloud-media-bridge plan <model> <provider-id>
```

**Currently implemented read-only MCP tools:** `doctor`, `providers_list`, `providers_show`, `models_search`, `models_inspect`, `plan_generation`.

MCP stdout must contain only JSON-RPC messages; errors must not leak tokens or private files. Search queries are bounded; model URLs must be validated against the Hugging Face HTTPS origin.

**Future tools after validation:** request human model approval, request resource-operation approval, generate media, poll job, cancel and fetch artifact. No success stubs or mock outputs in production paths.

## 8. Minimal implementation order

1. **Local MCP and provider catalog (offline).** MCP tests, invalid input handling, provider listing and schema validation.
2. **Dynamic catalog (no GPU).** Live model search, inspection, SHA checks, access/license states and model URL safety.
3. **Read-only planning (no GPU).** User-selectable provider; runtime compatibility and identity checks; clear unsupported results.
4. **Human approvals (no GPU).** Negative tests proving an agent cannot approve its own model or cloud resource.
5. **Modal adapter (initial live provider).** Verify the user's workspace, use one compatible image/pipeline, deploy with explicit consent and retrieve a real media artifact.
6. **Result safety and cleanup.** Verify hashes, no credential leakage, confirmed GPU scale-to-zero or explicit lingering-resource report.
7. **Other provider drivers.** Implement and live-test separately. A generic HTTPS worker can reduce duplicated inference logic, not eliminate provider API differences.

## 9. Testing and release claims

The [test plan](Cloud-Media-Bridge-TESTING.md) retains 82 detailed cases, but **the first release is gated by the seven small capabilities above**. Advanced load testing, durable video/music jobs and multiple live providers are later phases.

- Default CI uses Rust unit/integration tests and synthetic fixtures; **no GPU or paid cloud services**.
- **First real GPU tests: Modal only**, after separately approved use of the user's account.
- A configuration/mock test does not prove real inference or provider compatibility.
- Every report must distinguish PASS, FAIL, NOT RUN, BLOCKED and explicitly planned features.
- Do not claim production readiness or broad media support until actual output and cleanup have evidence.

## 10. References

- [Hugging Face Hub APIs](https://huggingface.co/docs/huggingface_hub/package_reference/hf_api)
- [Modal Server CLI](https://modal.com/docs/cli/latest/server)
- [RunPod Serverless](https://docs.runpod.io/serverless/endpoints/overview)
- [Vast.ai API](https://vast.ai/developers/api)
- [Lambda Cloud API](https://docs.lambda.com/api/cloud)
- [MCP specification](https://modelcontextprotocol.io/specification/)

Provider features and current API versions require re-verification when a driver is implemented. Links are references, not evidence of working deployment.
