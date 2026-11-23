# Cloud-Media-Bridge — Architecture and Implementation Specification

**Repository:** https://github.com/RizkiMugiono/Cloud-Media-Bridge  
**Specification date:** 2026-11-23  
**Status:** Architecture specification; not proof of implemented or tested software  
**License:** Apache-2.0  
**Companion documents:** [Test Plan](Cloud-Media-Bridge-TESTING.md) · [Test Results](Cloud-Media-Bridge-TEST-RESULTS.md)

> **Implementation contract for any development agent or contributor:** Read the architecture, test plan, and current test results before changing code. Implement real features rather than simulated success paths. Keep the requirements traceable to tests. Do not commit changes, provision cloud resources, start GPU jobs, modify a user's harness configuration, or delete resources without the applicable authorization. Default tests must not use paid cloud resources.

## 0. Purpose and agreed scope

Cloud-Media-Bridge is a **local, user-installed open-source tool** that connects an AI harness used by an individual to **GPU compute accounts owned by that individual**. It is not a centrally hosted SaaS, a shared gateway operated by the repository maintainer, an LLM host, a marketplace, or a billing intermediary.

Approved design principles:

1. **One repository, multiple compute providers through versioned adapters.** Modal is the only real-cloud implementation and live test target in the first phase. RunPod Serverless, RunPod Pods, Vast.ai Serverless/Instances, Lambda GPU VMs, generic SSH hosts, and optional Kubernetes are future adapters. A proposed adapter must never be advertised as operational.
2. **Rust for the local control plane:** CLI, MCP stdio, harness integration, model source discovery, independent human approval, routing, job orchestration, local persistence, error handling, privacy and security controls.
3. **Reuse inference software.** Modal runs a compatible serving image or a Modal App wrapping an existing library pipeline. Rust runs the local CLI/MCP control plane.

4. **Human model selection:** an AI agent may discover, inspect, compare and propose models; the human chooses and approves the canonical model ID, task and immutable revision SHA. The router may automatically choose a compatible deployment route, but **must never silently substitute another model or revision**.
5. **User-owned provider choice:** the selected provider, workspace and environment must belong to the user. Model approval does not authorize resource provisioning, deletion or switching accounts/providers.
6. **Media scope, not LLM hosting:** image, video, speech/ASR/TTS, voice, music, sound effects, audio, vision, OCR, enhancement, spatial/3D and future non-LLM media tasks. Discovery is broader than runtime compatibility.
7. **Safe, model-aware parallel execution:** replicas, batching and per-worker concurrency must follow tested model/runtime/GPU properties and provider capacity. No repository-wide one-GPU or one-job restriction.
8. **Scale-to-zero where the provider actually supports it.** Pods, VMs and externally managed hosts have distinct stop/terminate and storage lifecycles; they cannot all be promised automatic zero-cost idleness.
9. **No repository-imposed financial budget, price gating or usage-cost cap.** Technical limits for safety, memory, payload size, concurrency, timeouts, retries and user authorization are mandatory.
10. **No Kiri dependency.** Use standard OCI/container-image workflows where supported; production runtime is determined by the selected compute provider.
11. **No maintainer-owned processing backend or shared credential database.** No maintainer telemetry by default.

## 1. System architecture and trust boundaries

~~~text
User workstation / compatible harness execution environment
+-------------------------------------------------------------------+
| AI harness: Codex / Claude Code / OpenCode / Hermes Agent /       |
| DeepSeek-based harness / Grok-based agent / any compatible client  |
|                 MCP stdio (preferred) or CLI                       |
|                              |                                     |
|                 Cloud-Media-Bridge (Rust)                           |
| Discovery -> Inspect -> Human approval -> Execution planning        |
| Source registry -> Runtime matching -> Provider adapters           |
| Local SQLite -> Job reconciliation -> Artifact handling            |
+-----------------------------+-------------------------------------+
                              | TLS and user's own credentials
                   +----------+---------------+
                   |                          |
               Modal adapter           Future adapters
              (first live)        RunPod / Vast / Lambda /
                   |                SSH / Kubernetes
                   v                          v
            User's Modal account       User's provider account
                   |                          |
                   +-------------+------------+
                                 |
                  Modal container or library pipeline
                                 |
                          Media artifacts
                                 |
                         User / AI harness

Model sources are separate integrations: Hugging Face first,
then ModelScope, Civitai and other supported sources.
~~~

**Four independent identities:** (1) the AI harness, (2) the user's local bridge installation, (3) the model-source account, and (4) the compute provider account/workspace. Authenticating to one must not be treated as authentication to another. Every remote job is bound to exactly one approved provider profile and workspace.

**Local-first does not prohibit cloud worker endpoints.** A GPU inference worker may expose an authenticated private or access-controlled HTTP/Function endpoint within the user's own provider account. The project must not create a centralized maintainer-operated gateway.

## 2. Independent adapter dimensions

- **ModelSource:** what model is discovered and downloaded, including source URL, metadata, file manifest, immutable SHA, license and access restrictions.
- **ComputeProvider:** where the workload runs and how resources are authenticated, planned, provisioned, submitted, monitored, cancelled and cleaned up.
- **Deployment route / RuntimeFamily:** how a compatible checkpoint is loaded, which operators exist, how inputs and outputs are serialized, and what GPU resources are required.
- **HarnessAdapter:** how a client launches the local MCP process or invokes the CLI; no dependence on a specific underlying LLM brand.
- **ArtifactTransport:** how input/output media and model weights are staged, checked, downloaded and retained.

Use distinct fields and types for `model_source`, `compute_provider` and `deployment_route`. They are not interchangeable.

## 3. Compute provider landscape and lifecycle differences

| Compute class | Example | Initial support status | Lifecycle expectation |
|---|---|---|---|
| Serverless GPU | Modal | First implementation and real-GPU tests | Automatic scale-to-zero when correctly configured and verified |
| Serverless GPU | RunPod Serverless | Adapter contract/mock first | Provider-specific idle and endpoint behavior |
| GPU pod | RunPod Pods | Adapter contract/mock first | User-approved stop/terminate; residual storage may remain |
| Serverless/marketplace | Vast.ai | Adapter contract/mock first | Endpoint or rented-instance lifecycle varies |
| GPU VM | Lambda Cloud | Adapter contract/mock first | VM stop/terminate and attached storage lifecycle |
| General GPU VM | Provider with supported API | Future provider-specific adapter | Explicit resource management |
| Existing GPU host | SSH/OCI | Opt-in future adapter | Externally managed unless control is verified |
| GPU Kubernetes | User-owned Kubernetes | Optional later adapter | Cluster-specific scheduling and cleanup |

The project must describe **extensibility**, not guaranteed compatibility with every GPU rental platform.

Lifecycle capability values: `automatic`, `manual_stop`, `terminate_required`, `externally_managed`, `unavailable` and `unknown`. A stopped GPU VM may still have billable storage or network resources. Display this provider-reported behavior; never equate VM stop with serverless scale-to-zero.

All resource mutation needs separately recorded human authorization. This is **resource-operation consent**, not an application-level price limit.

## 4. Compute provider adapter contract

The following Rust trait is illustrative. Implementations must use typed asynchronous interfaces and validated schemas:

~~~rust
#[async_trait::async_trait]
pub trait ComputeProvider: Send + Sync {
    fn id(&self) -> ComputeProviderId;
    async fn authenticate_status(&self, profile: &ProviderProfile) -> Result<Identity>;
    async fn capabilities(&self, profile: &ProviderProfile) -> Result<ProviderCapabilities>;
    async fn list_resources(&self, scope: &ProviderScope) -> Result<Vec<Resource>>;
    async fn plan(&self, request: &ExecutionPlanRequest) -> Result<ExecutionPlan>;
    async fn provision(&self, approved: &ApprovedProvision) -> Result<Deployment>;
    async fn submit(&self, job: &ApprovedJob) -> Result<RemoteJobHandle>;
    async fn reconcile(&self, handle: &RemoteJobHandle) -> Result<RemoteJobSnapshot>;
    async fn cancel(&self, handle: &RemoteJobHandle) -> Result<CancelOutcome>;
    async fn list_artifacts(&self, handle: &RemoteJobHandle) -> Result<Vec<RemoteArtifact>>;
    async fn cleanup(&self, approved: &ApprovedCleanup) -> Result<CleanupReport>;
}
~~~

`ProviderCapabilities` is versioned and evidence-based. Features have `supported`, `unsupported` or `unknown` values and a query timestamp. Include:

- Deployment method: serverless endpoint/function, pod, VM or existing host; supported CPU/GPU types, regions and image types.
- Lifecycle: idle policy, automatic scale-to-zero availability, provisioning and shutdown semantics.
- Job modes: synchronous, asynchronous, streaming and batch; cancellation, idempotency, reconciliation and result retention.
- Networking: private/HTTPS endpoints, authentication method, ingress policy, payload limits and file transport.
- Artifacts: staging method, cache/volume semantics, retention, concurrency, integrity verification and cleanup.
- Runtime: container/OCI support, driver/CUDA compatibility, GPU memory and multi-GPU behavior.
- Scaling: replica and per-worker safe concurrency, queues and workspace quota/capacity reporting.

**Required adapter invariants:**

1. Bind all operations to an exact `(provider_id, account_identity, workspace_id, environment_id)` tuple.
2. Before provisioning, changing or destroying cloud resources, validate a separate user-approved resource-operation grant.
3. `plan()` is read-only and must never implicitly allocate a GPU or submit a job.
4. `submit()` uses a correlation/idempotency ID and stores a durable remote handle when available.
5. Unknown capabilities must never be promoted silently to supported.
6. Cancellation reports `requested`, `confirmed`, `uncertain` or `unsupported` accurately.
7. Retry only when the operation is idempotent or after remote reconciliation proves it is safe.
8. Scope bridge-managed resources with installation ID, provider profile and runtime version; never delete unrelated user resources.
9. Preserve safe provider error codes/request IDs while redacting secrets and private media URLs.
10. Report resources that remain active when shutdown or cleanup cannot be confirmed.

### Modal: first operational adapter

- Use the currently documented Modal CLI/API for account setup and deployment. **Do not invent a native Rust SDK.** A small Modal CLI/Python SDK helper is permitted inside this provider adapter when the supported operations require it; the local control plane remains Rust.
- Check user identity, workspace and environment during setup and before every privileged resource operation or job submission.
- Prefer authenticated serverless workers deployed from reproducible OCI images where the supported Modal API permits this.
- Handle Modal Server HTTP 503 during cold starts with bounded retry, backoff, jitter and request idempotency.
- For durable video/music jobs, evaluate provider-backed asynchronous Modal Functions, spawn/FunctionCall handles and real restart/reconciliation behavior. A detached local thread is not durable.
- Configure `min_containers=0` and `buffer_containers=0` only on resource types that support them. Startup timeout, exit grace, per-container concurrency and autoscaling targets are runtime-specific, not universal constants.
- Use Modal Volumes for weight caches where appropriate, while respecting cross-container cache visibility, concurrent write hazards and remote result expiration.
- **Only Modal receives live provider tests in the first release.** Other adapters are documented as planned or contract-only until separately verified.

## 5. AI harness compatibility

Supported integration mechanisms are **MCP stdio** and an independent **local CLI**. Examples of target clients are Codex, Claude Code, OpenCode, Hermes Agent, DeepSeek-based harnesses and Grok-based tools. A model name alone does not guarantee that a corresponding harness exposes MCP.

- The local binary must run in the execution environment accessible to the harness. A cloud-only agent cannot automatically start an executable on a user's laptop.
- Reserve stdout strictly for the MCP JSON-RPC protocol. Logs, progress, diagnostics and interactive text must go to sanitized stderr or local logs.
- Provide a standalone CLI when a harness lacks MCP, where the client has permitted shell/CLI access.
- Harness configuration is never rewritten without explicit consent. Back up existing files, preserve unrelated settings, write atomically and support rollback.
- Test initialization, protocol/version negotiation, tool discovery, invalid arguments, reconnect, cancellation and client-specific configuration.
- Label each integration with the exact client version and OS as `verified`, `documented-unverified` or `unsupported`. Do not imply testing from protocol compatibility alone.

**Read-only MCP tools:** `list_providers`, `provider_capabilities`, `models_search`, `models_inspect`, `models_compare`, `runtimes_list`, `jobs_status`, `jobs_list`, `artifacts_list`, `doctor`.

**State-changing MCP tools:** `request_model_approval` (creates only `PENDING`), `prepare_deployment` (read-only plan), `submit_generation` (requires valid grant), `jobs_cancel` and `artifacts_fetch`. No MCP tool may grant its own approval through `approve=true` or a hidden provider switch.

## 6. Dynamic model discovery and source trust

The `ModelSource` interface supports `resolve_url`, `get_metadata`, `list_files`, `resolve_revision` and `download_verified`.

Query the Hugging Face Hub at search time with pagination and support direct model URLs. Include newly published and updated entries visible to the user without a repository-maintained list or an inference-provider filter. Model compatibility is evaluated after discovery. Hugging Face is the first source. ModelScope, Civitai and other sources are optional future adapters independent of the GPU provider.

**Discovery pipeline:**

1. Canonicalize and validate URL; allow HTTPS only and a controlled provider host list. Block URL credentials, private/link-local/loopback targets, unsafe redirects, SSRF and path traversal.
2. Retrieve metadata, architecture/config, model card, task, publisher, license, gated/private flags, dependencies, file manifest, weight formats and available revisions.
3. Resolve mutable branches/tags to an **immutable commit SHA before user approval**.
4. Inspect weights; prefer compatible `safetensors`. Never automatically execute pickle or `trust_remote_code=True`.
5. Match a checkpoint against exact runtime-family manifests, task schemas, operator support, GPU needs and security policy.
6. Assign `discovered`, `candidate-compatible`, `verified`, `unsupported`, `restricted` or `unknown-risk`.
7. Use `verified` only after real inference evidence for the exact combination of model SHA, runtime image digest, GPU profile, task and tested parameters.
8. Present choices to the human. The agent may suggest a model but may not silently choose, substitute or update the approved model.

Model licenses are independent from this repository's Apache-2.0 license. Gated model restrictions and acceptance conditions remain binding.

Metadata may refresh automatically, but approvals and jobs are bound to immutable revision SHA. A new revision requires new approval.

## 7. Human approval and explicit resource authorization

**Model approval grant:** provider profile, model source, canonical model ID, immutable revision SHA, task, trust profile, permitted parameter constraints, scope (one-shot/reusable) and expiry.

**Resource-operation grant:** compute provider, verified account/workspace/environment, exact operation (provision/update/stop/delete), resource specification and fresh nonce. It is separate from model selection.

Approval state machine:

~~~text
DRAFT -> PENDING -> APPROVED
                 -> DENIED
                 -> EXPIRED
                 -> REVOKED
~~~

Only a trusted local **human-interactive** channel may move a request to `APPROVED`. A dedicated terminal TTY or protected OS-native UI is acceptable when the trust assumptions are documented. MCP requests and model card text cannot grant approval. In headless contexts, fail closed.

Example local command: `cloud-media-bridge approvals approve --request-id <ID>`. It must require genuine user interaction and must not corrupt MCP stdio.

**Trust boundary caveat:** a fully privileged agent with arbitrary shell access under the same operating-system user may be able to modify local files or invoke commands. A local approval UI is not a guarantee against such a compromised agent. Document OS permissions, keychain use and stronger privilege separation when available.

Reject before GPU submission if the SHA, model, task, trust requirements, provider/workspace, scope or expiration differs from the grant. Engine fallback may use the **same exact model SHA** only with verified compatibility and no weaker trust posture. A different model or provider requires fresh human choice.

## 8. Deploying a compatible model on Modal

1. **Existing serving container.** Select a trusted image pinned by digest that accepts the approved model ID and SHA or already contains that exact revision. Check its entrypoint, port, request schema, resource requirements and response format. Use [Modal Server deployment](https://modal.com/docs/cli/latest/server) for compatible HTTP servers.
2. **Library pipeline.** When a compatible pipeline exists for the task and architecture, package it as a Modal App with pinned dependencies. Pass the model ID and approved SHA to the loader and return the generated artifact. The [Modal Diffusers example](https://modal.com/docs/examples/text_to_image) illustrates this pattern.
3. **Unmatched or restricted model.** Keep it discoverable. Report the missing runtime, access or evidence during planning. Planning performs no deployment or GPU allocation.

Publish compatibility by task, architecture and format. Keep worker libraries isolated from the local CLI. Block arbitrary downloaded code, unsafe pickle and unapproved custom loaders. Bind execution to the approved workspace, revision, resource operation and runtime digest.

### Versioned runtime manifest

~~~yaml
runtime_id: diffusers.sdxl
deployment_route: library_pipeline
manifest_version: 1
image_digest: "sha256:<image-digest>"
tasks: [text-to-image, image-to-image]
architectures: [sdxl]
weight_formats: [safetensors]
input_schema_ref: schemas/tasks/text-to-image.json
output_schema_ref: schemas/media-output.json
gpu_requirements: {min_vram_gb: 16, cuda_capability: null}
concurrency: {max_safe_inflight_per_worker: 1, batching: false}
execution: {sync: true, async: true, cancellation: best-effort}
trust_policy: {remote_code: denied, pickle: denied}
verification: []
~~~

**This is a schema example, not a verified SDXL hardware requirement, runtime guarantee or asynchronous support claim.** Actual values must come from implementation tests.

Each manifest includes runtime image digest, engine/framework versions, task schemas, driver compatibility, checkpoint family, memory headroom, safe concurrency, trust policy and real verification references.

**Routing order:** verify approval → inspect source/revision → find compatible runtimes → enforce trust/schema/GPU requirements → compare measured performance when evidence is comparable → otherwise apply a documented deterministic preference → submit to the user's selected provider. Never change model or provider without renewed choice. Fallback must be idempotent.

## 9. Jobs, parallel inference, cancellation and recovery

Job state machine:

~~~text
CREATED -> APPROVAL_REQUIRED -> READY -> SUBMITTING -> QUEUED
                                               -> PROVISIONING
                                               -> RUNNING -> FINALIZING -> COMPLETED
         FAILED | CANCEL_REQUESTED -> CANCELLED / CANCEL_UNCERTAIN
         EXPIRED | ORPHANED
~~~

- Persist state transitions atomically in local SQLite with migrations and a WAL journal.
- Short synchronous jobs require explicit deadlines. Modal cold-start 503 retry must be bounded and idempotent.
- Durable asynchronous video/music must have a provider-backed remote job handle recoverable after harness/bridge restart.
- If network failure occurs after ambiguous submission, reconcile remotely before retrying; do not duplicate GPU jobs.
- Worker replicas may run in parallel. Within one worker, batch/concurrency only when verified for that model, GPU and VRAM budget.
- Technical backpressure and memory admission protect correctness; they are **not financial usage quotas**.
- Cancellation accurately distinguishes requested, confirmed, uncertain and unsupported; cleanup is separate.
- Remote job results and queues may expire. Local job history must never claim indefinite remote retention.
- Only approved, bridge-owned resources may be cleaned up. Externally managed pods/VMs require truthful lifecycle reporting.

### Normalized job request

~~~json
{
  "schema_version": 1,
  "job_id": "generated-uuid",
  "idempotency_key": "generated-uuid",
  "compute_provider": "modal",
  "provider_profile": "personal-modal",
  "account_id": "verified-provider-account",
  "workspace_id": "verified-workspace",
  "model_source": "huggingface",
  "model_id": "owner/model",
  "revision_sha": "immutable-commit-sha",
  "task": "text-to-image",
  "model_approval_ref": "grant-id",
  "deployment_route": "library_pipeline",
  "runtime_manifest_digest": "sha256:<digest>",
  "inputs": {"prompt": "An illuminated forest"},
  "input_asset_refs": []
}
~~~

A remote job handle additionally stores the opaque remote ID, provider resource ID, transport, submit timestamp, expiration/retention, last reconciled state and attempts. Never serialize credentials or private signed URLs into public diagnostics.

## 10. Media input, model cache and result artifacts

**InputAsset:** validated media MIME (including signature inspection), local reference/path, size, SHA-256, duration/dimensions if available, upload state and temporary staging location. Use authenticated chunked/resumable transfer where supported. Do not inline large video files in MCP JSON.

Validate misleading MIME types, codec corruption, archive traversal, path/symlink escapes, oversized images/video frames and decompression bombs.

**Artifact:** job ID, media type, size, SHA-256, storage reference, creation/expiry, verified local path and download state. Mark a job `COMPLETED` only after output integrity is verified.

**Weight cache:** key by source, model ID, immutable SHA, weight digest and runtime family. Incomplete downloads go to temporary paths and must never appear as valid cached weights. Pin model revisions and image digests.

On Modal Volumes do not assume cross-container local file locks or transactionally atomic multi-writer operations. Coordinate content-addressed downloads safely; publish completed files only after checksum verification; test simultaneous first-use downloads. Assign unique output paths per job/attempt.

Cloud artifacts and cache may persist independently of zero active GPU containers. Show retention and authenticated cleanup controls when supported.

## 11. Security and privacy baseline

Threats include malicious model repositories, prompt injection in model cards, unsafe custom model code, poisoned checkpoints, compromised agents, SSRF, hostile media files, leaked provider credentials, wrong-account operations, untrusted container images, interrupted jobs and concurrent filesystem writers.

Mandatory controls:

1. BYO credentials; use the OS credential store where supported. Do not commit provider or gated-model tokens, bake them into images, or expose provisioning credentials to inference code unnecessarily.
2. TLS and authenticated private or access-controlled worker endpoints. Never silently enable unauthenticated public inference.
3. Canonical URL validation, provider-domain allowlists, redirect checks, SSRF and path traversal defenses.
4. Lockfiles, pinned OCI digests, dependency and license scanning; safe weight formats; no arbitrary remote code by default.
5. Human approval independent of MCP tool input; truthfully document the local same-OS-user shell limitation.
6. Provider profile/workspace binding before every remote privileged operation, and no silent fallback across accounts.
7. Least-privilege worker secrets, isolated worker runtimes, limited outbound connectivity where possible, and sanitized logs.
8. Technical limits for payload, pixel count, media duration, VRAM, request concurrency, timeouts and retries; **no repository-enforced budget or price threshold**.
9. Dry-run and human approval for destructive cleanup; preserve resources not proven to be bridge-owned.
10. Treat model card contents and metadata as untrusted data, not shell commands, agent instructions or installer directives.
11. No maintainer telemetry by default. Network connections only to the chosen provider, model source and required artifact/image infrastructure.
12. Never publish user media, bearer tokens, private prompts, signed artifact URLs or credential diagnostics in a public test report.

## 12. Performance and scale-to-zero

Objective: **minimize end-to-end latency without compromising explicit user model choice, security or actual provider capabilities**.

Measure independent spans: `catalog_search`, `inspect`, `approval_wait` (reported separately), `submission`, `provision`, `cold_start`, `weights_download`, `weights_load`, `inference`, `encoding`, `artifact_upload`, `artifact_download` and `total`.

Optimization priorities:

1. No unnecessary centralized remote gateway on the local fast path.
2. Small dedicated container images per runtime family.
3. Immutable, reusable image layers and runtime digests.
4. Weight caches surviving compute scale-down if storage supports it.
5. Concurrent safe metadata operations before GPU allocation.
6. Verified batching and model-aware concurrency/replica scaling.
7. Provider-native GPU selection based on model requirements.
8. Benchmark actual bottlenecks with cold and warm cases separately.

True scale-to-zero trades some first-request latency for no continuously active compute. Do not promise warm-start latency when compute is cold. Provider workspace capacity and GPU memory limits continue to apply.

## 13. Proposed public CLI

~~~text
cloud-media-bridge doctor
cloud-media-bridge harness list
cloud-media-bridge harness configure codex|claude-code|opencode|hermes|custom
cloud-media-bridge providers list
cloud-media-bridge providers capabilities --provider modal
cloud-media-bridge providers profiles list
cloud-media-bridge providers setup modal
cloud-media-bridge providers use modal --profile PERSONAL
cloud-media-bridge resources plan --runtime diffusers.sdxl
cloud-media-bridge resources deploy --plan-id <ID>       # human resource approval
cloud-media-bridge resources list
cloud-media-bridge resources cleanup --dry-run
cloud-media-bridge models search --source huggingface --task text-to-image
cloud-media-bridge models inspect <URL>
cloud-media-bridge approvals request --model <URL> --task text-to-image
cloud-media-bridge approvals approve --request-id <ID>  # human TTY only
cloud-media-bridge approvals list|revoke
cloud-media-bridge generate --model <URL> --task <TASK> --inputs <JSON>
cloud-media-bridge jobs list|status|cancel|fetch <JOB_ID>
cloud-media-bridge artifacts verify|fetch|delete <ARTIFACT_ID>
~~~

These are **proposed commands**, not proof of implemented CLI features. An unimplemented adapter must return `PROVIDER_NOT_IMPLEMENTED` or `FEATURE_UNSUPPORTED` without substituting another provider.

## 14. Target repository structure

~~~text
Cloud-Media-Bridge/
├── Cargo.toml; Cargo.lock; rust-toolchain.toml
├── README.md; LICENSE; .gitignore; .env.example
├── AGENTS.md                                # instructions for any coding agent
├── Cloud-Media-Bridge.md                   # product specification
├── Cloud-Media-Bridge-TESTING.md           # test plan
├── Cloud-Media-Bridge-TEST-RESULTS.md      # real results and NOT RUN statuses
├── crates/
│   ├── bridge-cli/
│   ├── bridge-mcp-stdio/
│   ├── bridge-harness/
│   ├── bridge-sources/
│   ├── bridge-model-inspector/
│   ├── bridge-approval/
│   ├── bridge-router/
│   ├── bridge-provider-api/
│   ├── bridge-provider-modal/
│   ├── bridge-provider-runpod/
│   ├── bridge-provider-vast/
│   ├── bridge-provider-lambda/
│   ├── bridge-provider-ssh/
│   ├── bridge-runtime-registry/
│   ├── bridge-jobs/
│   ├── bridge-assets/
│   ├── bridge-local-store/
│   ├── bridge-security/
│   ├── bridge-types/
│   └── bridge-telemetry/
├── workers/                                 # Modal Apps wrapping compatible library pipelines

├── providers/
│   ├── modal/{deployment,profiles,capabilities}/
│   ├── runpod/{serverless,pods}/
│   ├── vast/{serverless,instances}/
│   ├── lambda/{instances}/
│   └── generic-ssh/
├── runtimes/manifests/
├── deployment/{modal,images,profiles}/
├── schemas/{provider,model,approval,job,artifact,runtime}/
├── migrations/
├── docs/{ARCHITECTURE,QUICKSTART,PROVIDERS,HARNESSES,SECURITY,PRIVACY,SCALE_TO_ZERO,LIMITATIONS}.md
├── examples/{harness-configs,model-requests}/
├── tests/{unit,contracts,security,integration,fixtures,benchmarks}/
└── .github/workflows/                   # offline by default; live tests opt-in
~~~

`README.md` must distinguish implemented, verified and planned functions with dated evidence, not badges suggesting untested providers/models work. `AGENTS.md` must be vendor-neutral and refer all contributing agents to the architecture, test plan and actual test results.

### Required .gitignore coverage

~~~gitignore
.env
.env.*
!.env.example
*.pem
*.key
*.p12
*.sqlite
*.sqlite-*
*.db
secrets/
credentials/
artifacts/
outputs/
cache/
.model-cache/
__pycache__/
.venv/
venv/
.pytest_cache/
*.log
!docs/examples/*.log.example
~~~

Do not ignore legitimate Rust sources or lockfiles. No real provider tokens, gated-model secrets, user media or model weights in repository history.

## 15. Implementation phases and release gates

The v1 delivery scope is a local MCP connection, live Hub discovery and inspection, read-only planning, and an approved Modal execution route. The full contracts and production gates below also cover later milestones. SQLite recovery, multi-provider contracts, concurrency tuning and additional harness integrations can follow v1.

**Phase 0 — Contracts and threat model (offline):** shared types, provider traits, capability schema, error taxonomy, approval invariants, account binding, SQLite migrations and versioned worker protocol.

**Phase 1 — Local Rust bridge (offline):** MCP stdio and CLI, Hugging Face discovery/SHA resolution, inspection, model approval UI, harness configuration, runtime registry and mocked provider conformance. Approval-bypass security tests must pass before cloud actions exist.

**Phase 2 — Modal account integration:** supported Modal CLI/API, workspace identity, read-only planning, separately approved provisioning/cleanup, authenticated workers, image digests and scale-to-zero. Live tests require a fresh user authorization.

**Phase 3 — Verify a Modal execution route:** after separate authorization, run an approved model revision through a compatible serving image or library pipeline. Verify output hashes and record the route, image digest, GPU and timings. Limit verification claims to the tested combination.

**Phase 4 — Async, concurrency, assets:** durable video/audio job handles; restart recovery; concurrent models/workers; secure media transfer; content-addressed cache; reliable cancellation and cleanup.

**Phase 5 — Multi-harness verification:** test clients and OS versions individually, including Codex, Claude Code, OpenCode, Hermes and specifically identified DeepSeek/Grok-based harnesses. MCP compliance alone is not proof of integration.

**Phase 6 — Additional compute providers:** one adapter and separately authorized live integration suite at a time, after Modal validation. Do not claim other platforms were tested under Modal-only tests.

**Production claim requires evidence:** real Rust control plane, human model approval, workspace isolation, protected endpoints, verified media inference, Modal scale-to-zero, reliable retry/reconciliation, safe concurrency/cache/artifacts, truthful unsupported features and full test-plan results.

## 16. Machine-readable errors

`HARNESS_ENV_UNSUPPORTED`, `MCP_PROTOCOL_UNSUPPORTED`, `PROVIDER_NOT_IMPLEMENTED`, `PROVIDER_CAPABILITY_UNKNOWN`, `PROVIDER_AUTH_REQUIRED`, `PROVIDER_PROFILE_MISMATCH`, `PROVIDER_RESOURCE_APPROVAL_REQUIRED`, `PROVIDER_LIFECYCLE_UNSUPPORTED`, `MODEL_APPROVAL_REQUIRED`, `MODEL_REVISION_CHANGED`, `MODEL_UNSUPPORTED`, `MODEL_UNSUPPORTED_UNSAFE`, `MODEL_ACCESS_RESTRICTED`, `RUNTIME_UNVERIFIED`, `INPUT_INVALID`, `ASSET_TRANSFER_FAILED`, `WORKER_UNAVAILABLE`, `REMOTE_SUBMISSION_UNCERTAIN`, `JOB_STATE_UNAVAILABLE`, `JOB_CANCEL_UNCERTAIN`, `ARTIFACT_EXPIRED`, `ARTIFACT_HASH_MISMATCH`.

Every error includes a typed code, retryability, safe recommended action, correlation ID and provider error code where available. Never leak tokens, private media or full signed URLs.

## 17. Explicit exclusions and anti-patterns

- Do not hardcode every model ID or claim every Hugging Face model is runnable.
- Do not deploy a maintainer-owned gateway, common user database, payment system or LLM host.
- Do not let an AI agent approve a model, change model/provider, or create GPU resources without the required human grant.
- Do not impose repository-level financial budgets; keep technical correctness and safety limits.
- Do not combine all worker dependencies into one huge image.
- Do not equate GPU VM stop with zero ongoing cost or local detached threads with durable async execution.
- Do not assume every provider offers a native Rust API.
- Do not require Kiri.
- Do not execute untrusted Hugging Face Python or pickle without a separately reviewed threat and isolation policy.
- Do not report mock-only provider implementations as verified.

## 18. Primary reference documentation

- Modal Server CLI: https://modal.com/docs/cli/latest/server
- Modal Server scale-to-zero: https://modal.com/docs/examples/server
- Modal asynchronous Functions: https://modal.com/docs/sdk/py/latest/Function
- Modal Volume concurrency: https://modal.com/docs/sdk/py/latest/Volume
- RunPod API overview: https://docs.runpod.io/
- Vast.ai API: https://vast.ai/developers/api
- Lambda GPU Cloud: https://docs.lambda.com/api/cloud
- MCP specifications: https://modelcontextprotocol.io/specification/
- Hugging Face checkpoint security: https://huggingface.co/docs/hub/security-pickle

Provider APIs and defaults change. Validate the exact CLI flags, account APIs, retention, authentication and lifecycle against the current provider documentation at implementation/test time. Documentation links are design references, **not evidence that a feature works**.

## 19. Vendor-neutral implementation work order

1. Work in `RizkiMugiono/Cloud-Media-Bridge`; preserve Apache-2.0 and existing project conventions.
2. Read the product spec, test plan and actual test results; create requirement-to-code-to-test traceability.
3. Implement the core Rust components in phases. Use honest contract mocks for non-Modal providers and a tested Modal adapter for real inference.
4. Prioritize approval, account/profile isolation, immutable model revision, input validation, typed schemas and durable job reconciliation before adding many model runtimes.
5. Run offline tests by default. Require explicit human permission for cloud provisioning, real GPU usage, destructive cleanup or GitHub writes.
6. Report source code changes, reproducible test commands, PASS/FAIL/NOT RUN evidence, performance measurements when real, known limitations and deployment resource cleanup.

