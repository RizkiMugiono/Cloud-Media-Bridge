# GPU provider configuration

**Cloud-Media-Bridge is provider-extensible, not universal by default.** One local MCP server can discover models and prepare a plan for GPU platforms without requiring a separate MCP server for each provider.

## Provider catalog

Public definitions are stored in [`config/providers.json`](../config/providers.json). They contain **no secrets**.

| ID | Platform | Compute model | Current integration | GPU execution |
|---|---|---|---|---|
| `modal` | Modal | GPU serverless | Read-only identity/capability catalog | Not implemented |
| `runpod-serverless` | RunPod Serverless | Queue / HTTP worker | Planned | Not implemented |
| `runpod-pods` | RunPod Pods | User-managed GPU pod | Planned | Not implemented |
| `vast-serverless` | Vast.ai Serverless | Worker group / endpoint | Planned | Not implemented |
| `vast-instances` | Vast.ai Instances | GPU marketplace / instance | Planned | Not implemented |
| `lambda-cloud` | Lambda Cloud | GPU virtual machine | Planned | Not implemented |
| `generic-https` | Existing HTTPS GPU worker | Pre-deployed authenticated endpoint | Planned | Not implemented |
| `generic-ssh` | Existing GPU host | SSH/OCI | Planned | Not implemented |
| `kubernetes-gpu` | User's GPU cluster | K8s Job/Deployment | Planned | Not implemented |

**Read-only discovery of a configured provider is not authentication to that provider.** The current project does not allocate, stop or terminate any GPU resource.

## Local configuration

`config/providers.json` is the public, versioned starting catalog. Its `credential_env` entries identify environment-variable **names**, never tokens.

A user may create a private copy and point the bridge to it:

```sh
# Linux / macOS (example only)
cp config/providers.json "$HOME/my-gpu-providers.json"
export CMB_PROVIDER_CONFIG="$HOME/my-gpu-providers.json"
cargo run --locked -- providers
```

**Windows:** `CMB_PROVIDER_CONFIG` can also be set through Windows Environment Variables or the relevant terminal. Local shell access is not a prerequisite for browsing this repository.

Do not commit provider keys, profile-specific secrets, instance identifiers, signed URLs, or user-specific SSH credentials. Never paste them into an AI chat.

## How one project can support arbitrary GPU rental services

A new adapter only needs to implement five concepts:

1. **Identify** the selected user account/workspace and capabilities.
2. **Plan** the exact runtime, model SHA, GPU requirements and resource lifecycle with no side effects.
3. **Deploy or connect** only with a separate human-approved resource operation.
4. **Invoke or poll** an existing compatible inference worker, returning media artifacts securely.
5. **Reconcile, stop and clean up** only resources proven to belong to that user/bridge installation.

Serverless, pod and VM providers implement these concepts differently. For an existing HTTPS worker, the bridge can eventually use one **common media-worker API** regardless of which rental company hosts the worker; however the worker itself and any authentication still have to be configured.

### Proposed common media-worker contract (not implemented)

```text
GET  /health                  -> status / runtime / task capabilities
POST /v1/media/generate      -> request ID + model SHA + task + typed inputs
GET  /v1/media/jobs/{id}     -> pending / running / complete / failed
GET  /v1/media/artifacts/{id}-> authenticated artifact bytes
POST /v1/media/jobs/{id}/cancel -> cancellation outcome
```

The caller must validate the worker's HTTPS origin, response schema, output size/hash, account binding and approval. Never send provider provisioning credentials to the inference worker. Merely using a shared HTTP contract does **not** automatically provision a host, pull a model, satisfy gated license requirements, or guarantee CUDA/runtime compatibility.

## How media models are deployed on a GPU rental

A GPU rental provider does not need to maintain a special image-generation, video-generation, speech or music container for every model. What the selected GPU resource needs is a **compatible inference runtime**. The bridge must resolve that runtime from a **trusted, versioned runtime-family recipe**.

### Hugging Face inputs are not complete deployment instructions

| Hub material | What it can tell the bridge | What it does not supply |
|---|---|---|
| Model URL and immutable commit SHA | Exactly which model revision to use | Running service or GPU |
| `config.json` | Model architecture and loader configuration for supported libraries | Installed library versions, CUDA compatibility, inference endpoint |
| `model_index.json` | Diffusers pipeline components when applicable | A running Diffusers worker, validated dependencies |
| Checkpoint files | Model weights / formats | Model loader, preprocessing, decoding, postprocessing |
| Model card / usage examples | Suggested task and runtime clues | Trusted shell commands or security guarantees |
| Docker/Space source (if any) | Possible runtime implementation | Automatic permission to execute untrusted code |

For compatible model families, a shared vetted runtime can accept different `model_id` and `revision_sha` values; **one runtime recipe can serve many models**. New architecture, custom CUDA code, unsupported nodes or an absent loader require a separately verified recipe. Model discovery must not imply runtime compatibility.

### Runtime recipe design (proposed, not yet executable)

A recipe for a model family should specify: task and model-family matcher; supported file formats; pinned serving image digest **or** vetted base image and pinned library installation; Python/CUDA/driver compatibility; model loader, input/output schemas and media codecs; minimum GPU VRAM and disk requirements; authentication and endpoint transport; input/output staging; health check; cleanup; version and test evidence.

Examples of runtime families—not promises of compatibility—include Diffusers image/video pipelines, established Transformer-based speech and vision runtimes, audio/music pipelines, and pinned ComfyUI graph/node combinations. Avoid one enormous image with every possible model library, and do not require one handwritten configuration file per model.

**Read-only planning and human approval precede provisioning.** A generic description of a family is insufficient to label any arbitrary checkpoint `verified`.

### Vast.ai instance path

Vast.ai uses Linux Docker instances and accepts standard images. A template may use an existing PyTorch or base image, a custom image from a registry, or a reviewed provisioning/startup script. Vast's SSH/Jupyter launch modes can override the image's entrypoint, so worker startup must match the selected launch mode.

```text
AI harness -> local Rust bridge
  -> Hugging Face API: model ID + exact SHA + metadata
  -> RuntimeResolver: compatible reviewed runtime recipe?
  -> Vast adapter: search available GPU offers, validate RAM/VRAM/disk
  -> Human: approve exact model and separate instance creation
  -> Vast API: create rented instance using pinned Docker image/recipe
  -> Instance: start task worker, load verified HF weights at pinned SHA
  -> Client: authenticated inference -> verified media artifact
  -> Vast API: query state and stop/terminate only owned resources
```

When using an existing base/PyTorch image, a vetted bootstrap routine can install the required **pinned library versions**, download verified checkpoint files, and start the corresponding media worker. It must **not** execute arbitrary `requirements.txt`, `setup.py`, `README.md` or remote script from an untrusted model repository without a separately approved, isolated trust policy.

Vast instance creation and Vast serverless endpoint deployment are separate modes. The former rents a running instance that may incur charges until stopped/terminated; neither its storage billing nor its idle behavior is identical to Modal scale-to-zero.

The public Vast API documents image and startup configuration for instance creation; templates describe Docker images, image launch modes and provisioning. **This repository currently has no working Vast adapter, no runtime recipe executor and no real Vast inference results.**

### Same worker protocol, different provisioners

- **Modal:** deploy a compatible Modal Function/App or OCI-based server serving the selected runtime, inside the user's account.
- **RunPod Serverless / Vast Serverless:** use provider-specific worker/endpoint handler conventions with the same logical media-task request and artifact semantics.
- **RunPod Pods / Vast instances / Lambda GPU VMs:** provision a runtime-compatible instance/container or bootstrap a vetted runtime on the allocated machine.
- **User-managed HTTPS/SSH/Kubernetes GPU:** connect to or provision an authenticated worker with a compatible runtime, only within permissions the user has granted.

The same model selection and runtime matching logic is shared. The adapter for each provider must still handle resource offers, lifecycle, authentication, network, storage and service discovery. No adapter should silently fall back to a different provider or switch model revisions.

### Recommended deployment result states

`DISCOVERED` → `COMPATIBILITY_CHECKED` → `PLAN_READY` → `MODEL_APPROVED` → `RESOURCE_APPROVED` → `PROVISIONED` → `WORKER_READY` → `RUNNING` → `ARTIFACT_VERIFIED` → `CLEANUP_RECONCILED`.

Unsupported or untrusted recipes must return an explicit error **before any resource is allocated**. Cancellation and resource termination require truthful confirmation rather than assuming success.

## Compute lifecycle and billing behavior

| Resource class | Idle expectation |
|---|---|
| Serverless with real scale-to-zero | Compute workers can reach zero; storage and other resource charges may persist |
| Pods and instances | Usually require explicit stop/terminate; stopping may not remove all billable resources |
| Preexisting HTTPS/SSH host | Lifecycle is managed outside the bridge unless a verified adapter owns it |
| Kubernetes | Depends on GPU node autoscaler and cluster configuration |

The repository must never silently switch providers or change GPU type to bypass a failed request. Human model approval and cloud resource-operation approval are separate.

## Initial test policy

- **Offline:** validate configuration, MCP tools, model URLs, error handling, input schemas and provider capability mocks.
- **First live provider:** Modal only, and only after the user separately authorizes a specific resource operation.
- **Other live providers:** not yet tested; no claims of operational support.
- **No automatic GPU allocation:** GitHub Actions, `cargo test`, `models search`, `models inspect`, and `plan` must stay read-only.

## Provider references

- [Modal Server CLI](https://modal.com/docs/cli/latest/server)
- [RunPod Serverless](https://docs.runpod.io/serverless/endpoints/overview)
- [RunPod Pods](https://docs.runpod.io/pods/overview)
- [Vast.ai API](https://vast.ai/developers/api)
- [Lambda Cloud API](https://docs.lambda.com/api/cloud)
- [Kubernetes GPU scheduling](https://kubernetes.io/docs/tasks/manage-gpus/scheduling-gpus/)

Vendor APIs change. Adapter-specific commands, authentication and lifecycle must be tested against current provider docs before use.
