# Cloud-Media-Bridge — Implementation Specification

**Repository:** https://github.com/RizkiMugiono/Cloud-Media-Bridge  
**Version:** 2026-11-23  
**Current status:** Rust MCP/CLI includes a tested **offline fixture deployment pipeline**, resource ownership and stop reconciliation, Python task-scoped HTTP media worker, evidence-based runtime resolution and provider request-shape previews. **No real GPU resource deployment, paid-operation approval enforcement, or actual AI model inference has been tested.**  
**Related:** [README](README.md) · [Media models](docs/MEDIA-MODELS.md) · [Runtime selection](docs/RUNTIME-RESOLUTION.md) · [GPU providers](docs/PROVIDERS.md) · [Test plan](Cloud-Media-Bridge-TESTING.md) · [Results](Cloud-Media-Bridge-TEST-RESULTS.md)

## GPU billing and five-second jobs

**The default stop trigger is completion of the final job and its artifact transfers, not a pricing-period boundary.** An hourly rate may be merely a display rate with per-second metering, or it may have a true minimum hour. For five seconds of model execution, include provisioning, image/model loading, idle and stop confirmation when estimating chargeable duration.

The billing policy at [`config/billing-policy.json`](config/billing-policy.json), read-only Rust estimator and [`docs/GPU-BILLING.md`](docs/GPU-BILLING.md) distinguish per-millisecond/second/minute/hour/day usage from prepaid monthly/yearly/reserved commitments. An estimate uses **actual quote increments**, minimums and explicit user-provided prices, not invented live provider rates. A paid deployment requires a fresh quoted compute cap and consent. Storage/network/commitment charges may continue after stop; termination is not silently substituted for stopping. If stop is unsupported or state is uncertain, report action required and potential remaining charges.

No automatic GPU billing cap is yet enforced by providers, and real invoices/scale-down are untested. No always-on central server exists, so provider-side TTL/watchdogs or independent recovery are necessary for a strong crash-proof auto-stop guarantee.

## User-approved deployment requirements (2026-11-23)

The authoritative machine-readable requirements are in [`config/deployment-policy.json`](config/deployment-policy.json). The cloud deployment policy remains **read-only and not enforced for billable GPU operations**. Rust now enforces one-shot approvals and automatic mock-resource stopping for **local fixture simulations only**; these cannot authorize real deployment.

| Requirement | Binding architecture decision |
|---|---|
| Runtime installation | Both a verified prebuilt OCI image and a vetted runtime installation recipe on a compatible base image are supported **as alternatives**, selected according to the model's actual compatibility, task and hardware. No arbitrary model-repository scripts are trusted. |
| GPU development | **All nine configured provider classes are parallel implementation workstreams**, not a Modal-first prerequisite chain. Every provider's operational status and evidence remains independent. |
| Consent | **New human consent for each exact deployment request**, bound to model/revision, runtime/image digest, account/workspace, provider, GPU resource specification and unique request nonce. No reuse of a previously consumed deployment grant. |
| Post-job resource handling | Automatically **stop bridge-created and bridge-owned pod/VM/instance resources after their final active job**, if a validated provider stop action exists. Verify stop confirmation; keep job/resource recovery records. Do not destroy/terminate automatically, or stop unrelated/pre-existing/shared resources. Stop failures or unsupported lifecycle actions must be reported, not silently treated as success. |

The term **deployment** here includes initial creation, redeployment, runtime/model image changes and resource reconfiguration. Reusing an already-approved running worker for a generation call is not necessarily a new deployment; execution still requires the applicable user/model permissions. The human approves the auto-stop behavior as part of the exact resource plan. No fixed repository financial limits are imposed.

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
| User deployment policy accessible via CLI/MCP | Implemented as a **read-only contract**; not yet enforced |
| Hugging Face live search, with pipeline-task filter | Implemented; needs real network verification |
| All-media task classification / discovery | Implemented using `config/media-tasks.json`; inference remains unverified |
| Inspect model ID/URL and immutable revision | Implemented; needs real Hub verification |
| Rust upstream Runtime Registry and evidence-scored Runtime Resolver | Implemented, **read-only candidate matching**, no verified executable runtime bundle |
| Provider-aware, non-mutating generation plan | Implemented; includes runtime resolution and always returns `safe_to_deploy=false` until deployment and authorization are real |
| Offline mock deployment engine, one-shot consent and owned-resource auto-stop | **Implemented and offline-tested** with a test-only provider; no cloud credentials or GPU |
| Authenticated task-scoped HTTP media worker | Python worker and Rust loopback client implemented; fixture interop tested, real HF inference **not GPU-verified** |
| Provider HTTP/CLI request contract previews | Nine provider classes covered by offline Rust builders; **no live provisioning or stop calls** |
| Human authorization grants for actual cloud billing | Planned; offline terminal-only proof is not an independent production approval boundary |
| Modal account integration / deployment / GPU inference | Planned |
| RunPod, Vast.ai, Lambda and generic host deployment | Planned, not claimed operational |
| Media artifact download and validation | Implemented for **offline fixtures**, including HTTP loopback and SHA-256 checks; real remote artifact staging still planned |
| Durable asynchronous jobs, advanced parallel tuning | Future, after the first verified end-to-end deployment |

The status above refers to code availability. A feature is **verified** only when its corresponding tests have succeeded with recorded evidence.

## 3. Model catalog and all media modalities

The model catalog is **dynamic** and must never rely on a hardcoded list of model names. An AI harness or user can:

1. Search Hugging Face live.
2. Paste `https://huggingface.co/owner/model` or `owner/model`.
3. Inspect metadata: task, framework/library, access restrictions, gated status and license.
4. Resolve an immutable commit SHA.
5. See whether there is a compatible serving runtime.
6. Select the exact model/revision and confirm it as the human.

**Media scope:** model families are image, video, audio/sound, speech/voice, music, 3D, spatial/motion, documents and multimodal/any-to-any. Task names are read from versioned [`config/media-tasks.json`](config/media-tasks.json), currently covering known and extension tags across all nine families. The bridge uses actual Hub pipeline tags and optional model metadata tags; if a new tag is not yet in the catalog, it remains discoverable as `inferred_media_task_requires_review` or `unclassified_requires_review` instead of being silently rejected. A pure text-only task has a separate `non_media_task_out_of_scope` status. Audio-only generation, music, ASR, diarization, video understanding, OCR, point clouds and image segmentation are all within the media scope.

Task classification and model detection are **not** GPU execution. Runtime matching must be based on actual verified loader/checkpoint compatibility, including CPU/GPU, media codecs, inputs and outputs. The task catalog must never be interpreted as a list of ready-to-deploy images.

A newly published model becomes discoverable without a repository update. **It becomes runnable only if an existing compatible serving image or library pipeline is proven to load it.** Unknown architectures must not be deployed optimistically. Discovery status and execution status remain separate.

External metadata and model cards are **untrusted data**, never instructions to the agent or shell. By default, do not execute arbitrary remote Python, unsafe pickle or unverified `trust_remote_code`.

## 4. GPU platforms: one configuration format, provider-specific drivers

The checked-in [`config/providers.json`](config/providers.json) catalog includes **parallel adapter workstreams**:

- **Modal** serverless.
- **RunPod** Serverless and Pods.
- **Vast.ai** Serverless and rented Instances.
- **Lambda** GPU virtual machines.
- A user-controlled **HTTPS media worker** on any compatible GPU rental.
- A user-controlled **SSH/OCI GPU host**.
- A user-owned **Kubernetes GPU cluster**.

All configured adapters are developed in parallel rather than waiting for one vendor to be completed. Each provider still requires its own implementation and independent verification. Additional providers can be represented by an updated catalog and a small driver when their API semantics require it. **Listing a platform in configuration does not mean deployment works.** The `integration`, `driver`, `capabilities` and `scale_to_zero` fields make the status explicit.

Do not hardcode a central provider account or share credentials between users. Profiles and API tokens are always user-owned, private and local.

A single compatible HTTPS media-worker contract can allow the same inference API to work on several rental platforms **after the user has provisioned that worker**. Provisioning, identity, token handling and cleanup remain provider-specific.

### Model-to-runtime resolution (Rust candidate resolver now implemented)

The [Runtime Registry](config/runtime-registry.json) and `crates/bridge-cli/src/runtime.rs` provide a real, tested Rust **read-only matching engine**. It compares the Hugging Face model's library, task, file signals and architecture to official upstream framework candidates (Diffusers, Transformers, ComfyUI, AudioCraft, SpeechBrain, pyannote and specialized models). It prefers a verified OCI image to an equally verified installer, requires immutable model revision and exact worker/provider evidence, and rejects unverified claims.

All bundled records are explicitly `candidate_unverified`, with no pinned executable images or installer manifests. The current resolver returns candidates, blocking requirements and `safe_to_deploy=false`; it **does not install frameworks, run Python, deploy GPU resources, authenticate cloud providers or enforce per-deployment approval**. Unknown model architectures remain discoverable and require separate runtime verification.

See [runtime verification requirements](docs/RUNTIME-RESOLUTION.md).

### Full model-to-runtime execution path (required for every provider)

**Hugging Face model configuration is not a deployment specification.** The Hub may expose `config.json`, `model_index.json`, checkpoint weights, model-card metadata and example code. This information can identify a model family, but it cannot guarantee that CUDA, Python libraries, custom operators, assets, input/output schemas or an inference HTTP service exist.

A provider-neutral **RuntimeResolver** must translate verified metadata into a **RuntimeRecipe** maintained and reviewed by this project:

1. Resolve the canonical model ID and immutable commit SHA; check access, license, files, task and architecture.
2. Match model and task to a supported runtime family and pinned environment: Diffusers for compatible image/video pipelines; Transformers for compatible media/vision/speech tasks; established audio/music runtime families; ComfyUI only with pinned vetted nodes and workflows; or a signed/prebuilt serving image.
3. Check exact dependencies, GPU architecture, VRAM, disk/model download size, input schema, memory assumptions, trust policy, startup and artifact output.
4. If no safe compatible recipe exists, report `RUNTIME_UNSUPPORTED` or `RUNTIME_UNVERIFIED`, including missing requirements. Do not execute arbitrary Python/install commands copied from a model card.
5. Build a **read-only deployment plan** binding the selected model SHA, recipe digest, provider profile, GPU offer/region, source token requirements and cleanup behavior.
6. Obtain **separate human approval** for model selection and any GPU resource creation/change.
7. Provision the provider resource using its documented interface; use a vetted OCI runtime image or bootstrap a known runtime from a vetted base image, then download/verify the approved weights and start an authenticated task-serving worker.
8. Submit inputs, verify output artifact integrity, retrieve results, and apply the approved lifecycle policy. For an instance/VM, report and perform a verified stop/terminate action; **do not claim serverless scale-to-zero**.

Recipes should be **per runtime family**, not one file per Hugging Face model. A new model can be automatically discovered and, if compatible with an existing recipe, run by passing its ID and SHA without a repo release. A genuinely new architecture/custom operator requires a separately verified recipe or image.

### Vast.ai instance deployment in particular

Vast.ai instances support **standard Linux Docker images**; they are not restricted to dedicated Vast-provided inference containers. The instance/template can select a general base or PyTorch image, a pinned custom OCI image, and a startup/provisioning script. The Vast instance-creation API accepts an image and startup configuration after the user selects a GPU offer.

For Cloud-Media-Bridge, the Vast adapter must: query live offers/capabilities → validate GPU/VRAM/disk/runtime → present the exact Docker image/recipe and resource lifecycle → obtain user consent → create instance in the user's account → verify the worker is healthy and authenticated → load pinned Hugging Face model → run media inference → fetch and checksum outputs → stop/destroy only bridge-owned resources with user authorization. Do not attempt nested Docker inside a Vast Docker instance.

Avoid arbitrary user-supplied public startup scripts or treating a model repository's README as a trusted shell script. In SSH/Jupyter launch modes, the platform may override the image ENTRYPOINT; the adapter must choose the proper run mode or configure service startup explicitly. For cloud workers, send least-privilege, model-specific source access only when strictly required, never GPU-provider provisioning credentials.

Vast.ai serverless endpoints are a **different** deployment mode, requiring their own endpoint/worker lifecycle. Provider configurations must not conflate Vast instances with Vast serverless.

### Shared worker protocol vs. provider-specific infrastructure

The same task-serving API may be reused on Modal, RunPod Pods, Vast instances, Lambda VMs, SSH GPU hosts and Kubernetes, provided the chosen runtime image is compatible. That **does not eliminate** provider-specific provisioning, networking, authentication, storage, cleanup and billing behavior. Modal Functions, RunPod Serverless and Vast Serverless may require native worker/handler adapters rather than an identical long-running HTTP server. A common logical request/response contract is still useful without forcing every provider into the same transport.

### Provider adapter responsibilities

1. Detect correct user account/workspace and authenticated capabilities.
2. Build a **read-only** execution/resource plan with an exact model SHA, task, runtime, GPU request and lifecycle warning.
3. Require new human consent for **every deployment operation** (new, repeat, reconfigure or redeploy), separately from the model-selection approval, with a fresh nonce and one-shot grant.
4. Provision or connect only through a verified adapter.
5. Submit/poll/cancel only with safe idempotency and authenticated transport.
6. Return hash-checked artifacts, reconcile the resource state, automatically stop bridge-owned pods/VMs after the last job when provider stop is supported, and report any resource that remains active or billable.

Unknown capabilities fail closed. No unverified fallback to another model or GPU provider.

### Resource lifecycle

| Mode | Expected behavior |
|---|---|
| Serverless | Scale-to-zero only when supported and tested; storage/network charges may persist |
| Bridge-created GPU pod / VM / marketplace instance | **Automatic stop after the final active job** when a provider stop API is available, with confirmed status and crash recovery. If no stop API exists, report action required; do **not** automatically destroy/terminate. Storage or other fees may persist. |
| Existing HTTPS/SSH host | Remains user-managed. Never stop a pre-existing machine without explicit, verifiable bridge ownership and authorization. |
| Kubernetes | Depends on cluster/node autoscaling and configuration |

The repository does **not** implement an automatic per-user cost ceiling. It must still prevent unintended cloud resource creation and include ordinary technical safety limits.

## 5. Reuse inference runtimes, do not build new engines

No separate native Rust inference engine or universal Python inference engine is required for **v1**. Rust only runs the local MCP/CLI and provider orchestration.

There are two **equally supported alternatives** selected by verified compatibility rather than provider branding; both require a pinned, vetted runtime recipe:

1. **Compatible OCI serving image**: user selects a pinned existing media-serving image that can load the model/revision and exposes an authenticated media request API.
2. **Library-backed application**: a small provider-native deployment that invokes an existing tested library pipeline (such as Diffusers or a suitable speech/video runtime) and exposes the same media API.

For both routes, runtime requirements, license/gating, task schema, input media, GPU VRAM, output format and security must be verified. A Hugging Face link or `config.json` alone cannot deploy itself. The recipe supplies the loader, dependencies, execution entry point, authenticated worker transport and artifact contract.

**Do not build a custom scheduler, orchestration database, giant universal image, or global server until a proven use case requires it.** Use the provider's supported job/scale lifecycle for the first verified deployment.

## 6. Human approval and privacy

The AI agent can **search, compare and suggest**. It cannot decide to spend money or select a model on behalf of the user without approval.

**Model approval** must bind canonical model ID, immutable SHA, task and the selected source. **Deployment/resource approval** must be a **fresh, one-shot human approval for every exact deployment request**; bind provider, account/workspace, GPU/resource parameters, runtime image/recipe digest, action and a unique nonce. Approval for one deployment can never authorize a second deployment even with the same parameters. Changing any relevant dimension invalidates the grant.

The current read-only implementation **does not provide approval or execution tools**. The CLI/MCP can only display the intended policy, never enforce approval, deploy or stop a cloud resource. Do not implement GPU execution until the approval path exists and has negative security tests.

Privileged approval must be via an independent user interface/TTY and not a boolean inside an MCP tool request. A local shell-capable compromised agent operating under the same OS user may bypass weak approval storage; document this trust limitation and use stronger OS identity separation when possible.

Credentials must never be checked into Git, echoed in MCP stdout, or sent to a model container as provisioning secrets. No maintainer-owned telemetry is collected by default.

### Automatic post-job stop contract

- Apply only to resources that the bridge created and whose ownership is recorded with a provider identity, workspace, resource ID, deployment request ID and ownership marker. **Never touch pre-existing, shared or unidentified resources.**
- After successful, failed or cancelled jobs: check provider-backed status; wait for **all active jobs and artifact transfers** on that resource to finish. If another request is running or queued against the same resource, defer stopping. Reconcile ambiguous submissions before stopping.
- Execute a provider-native **stop** action where it exists. Confirm stopped state through the provider; mark `STOP_PENDING`, `STOP_CONFIRMED`, `STOP_UNCERTAIN`, `STOP_FAILED` or `STOP_UNSUPPORTED` accurately. Do not claim zero cost unless billing behavior is separately verified.
- When a provider offers **termination only** (for example some GPU VM APIs), do **not** reinterpret auto-stop consent as permission to destroy storage. Surface `STOP_UNSUPPORTED_ACTION_REQUIRED` and the remaining-charge warning. Termination requires its own explicit destructive approval.
- Persist the bridge-owned resource/job ledger durably so restart/crash can recover and retry authorized stop reconciliation. For providers with serverless scale-to-zero, verify that idle workers actually reach zero, not an invented VM stop.
- Stop functionality must be **implemented and safety-tested before a provider adapter exposes a production deploy/execute operation**. User confirmation occurs at the deployment stage; the subsequent stop is part of the disclosed lifecycle policy and does not require reapproval for that exact owned resource.



## 7. CLI and MCP contracts

**Current CLI (read-only operations unless explicitly labeled offline fixture):**

```text
cloud-media-bridge doctor
cloud-media-bridge mcp
cloud-media-bridge providers [provider-id]
cloud-media-bridge policy
cloud-media-bridge runtimes
cloud-media-bridge runtimes resolve <model> <provider-id>
cloud-media-bridge models tasks
cloud-media-bridge models search <query> [limit: 1..50]
cloud-media-bridge models search-task <pipeline_tag> [limit: 1..50]
cloud-media-bridge models inspect <owner/model | https://huggingface.co/owner/model>
cloud-media-bridge plan <model> <provider-id>
cloud-media-bridge provider-preview <provider> <image@sha256> <gpu> <disk_gb> [offer_id]
cloud-media-bridge offline demo <new-directory> <media-kind>
cloud-media-bridge offline prepare|approve|run|status|verify|reconcile <directory> [media-kind]
cloud-media-bridge offline worker-smoke <directory> <loopback-port> <media-kind>
```

**Read-only MCP tools:** `doctor`, `providers_list`, `providers_show`, `deployment_policy`, `media_tasks_list`, `models_search`, `models_inspect`, `runtimes_list`, `runtime_resolve`, `plan_generation`, `provider_operation_preview`.

**Explicitly simulated MCP tools:** `offline_fixture_demo`, `offline_fixture_status`, `offline_fixture_verify`, `offline_worker_smoke`. Only fixture-demo and worker-smoke write local outputs; they require an explicit mock-only confirmation and cannot contact real GPU providers.

MCP stdout must contain only JSON-RPC messages; errors must not leak tokens or private files. Search queries are bounded; model URLs must be validated against the Hugging Face HTTPS origin.

**Future tools after validation:** request human model approval, request resource-operation approval, generate media, poll job, cancel and fetch artifact. No success stubs or mock outputs in production paths.

## Offline implementation and remaining production boundary

See [the runnable offline end-to-end guide](docs/OFFLINE-E2E.md). The Rust mock-only workflow exercises exact deployment plan hashing, a nonce-bound one-shot approval, fixture provider resource allocation, nine typed output categories, artifact integrity and provider-observed mock stop, including error and crash-window recovery. The optional Python worker implements an authenticated media HTTP interface and real Diffusers/Transformers loader code paths for a **limited explicit task allowlist**, but GPU/model integration is NOT VERIFIED. The worker fixture path is only accessible in loopback test mode; provider previews are network-free request shapes.

**A fixture is not a real AI media generation result.** Actual provider deployment must wait for vetted OCI images/installers, secure worker ingress, user credentials and a stronger approval trust boundary. Automatic resource stop must be independently verified on each provider and is not possible when a provider exposes only destructive termination or when the user's client is offline without a provider-side watchdog.

## 8. Implementation order

1. **Local MCP, provider catalog and policy contract (offline).** MCP tests, invalid input handling, provider listing and policy schema validation.
2. **Dynamic all-media catalog (no GPU).** Image/video/audio/music/speech/3D/spatial/document/multimodal search, task filters, inspection, SHA checks, access/license states, unknown task fallback and model URL safety.
3. **Read-only planning (no GPU).** Rust Runtime Resolver matches official upstream candidate families using model task/library/files; recognizes unsupported/unverified paths, and returns provider-specific, non-executable plans.
4. **Human deployment approvals (no GPU).** Implement one-shot, non-reusable user approval for each deployment operation. Negative tests must prove an agent cannot approve itself or reuse an earlier grant.
5. **Parallel GPU provider adapters.** Develop Modal, RunPod Serverless/Pods, Vast.ai Serverless/Instances, Lambda Cloud, generic HTTPS, SSH and Kubernetes tracks concurrently. Share runtime resolution and request/artifact contracts; implement authentication, provisioning and lifecycle per provider. Each adapter requires its own independent live evidence before an operational claim.
6. **Result safety and automatic owned-resource stopping.** Verify hashes and secrets, track every active job, stop only bridge-owned pod/VM resources after the final job, confirm stop state and recover cleanup after restart/crash. Report unsupported stop semantics and residual fees rather than performing unapproved termination.
7. **Independent provider verification.** Unit and contract tests run together without GPUs; each provider's live deployment, cancellation, cleanup and artifact tests require separate authorization and evidence. A generic HTTPS worker can reduce duplicated inference logic without removing provider API differences.

## 9. Testing and release claims

The [test plan](Cloud-Media-Bridge-TESTING.md) retains the original 82 cases, 10 deployment-decision cases, 10 all-media cases, 8 Runtime Resolver cases, and 14 offline end-to-end cases (124 total), with the initial deployment milestone gated by the seven capabilities above. Advanced load testing, durable video/music jobs and multiple live providers are later phases.

- Default CI uses Rust unit/integration tests and synthetic fixtures; **no GPU or paid cloud services**.
- All provider adapters are developed concurrently; each provider's real GPU tests require separately approved access and must not run by default. Modal may be used first for economical validation, but is not an implementation dependency.
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
