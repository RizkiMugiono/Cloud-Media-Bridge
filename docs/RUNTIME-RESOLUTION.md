# Runtime Resolution and Existing Inference Engines

**Status:** Runtime Registry and Rust Runtime Resolver are implemented as **read-only candidate selection**. Neither a model-specific GPU worker, provider deployment driver, human approval enforcement nor automatic stopping is operational. CLI/MCP cannot allocate resources or execute arbitrary installers.

## Responsibilities

```text
AI harness (client, controls workflow)
   -> local Rust MCP/CLI (read-only introspection and planning)
      -> live Hugging Face Hub API (model ID, SHA, metadata, access)
      -> Rust Runtime Resolver (evidence-scored candidates, fail closed)
         -> registry of official/community inference runtime families
      -> user-specific GPU provider profile and resource policy
      -> fresh human approval for each exact deployment (future)
      -> provider API deployment and verified worker (future)
      -> results to harness and authorized stop/reconciliation (future)
```

There is **no centralized MCP server**. MCP `stdio` means the AI harness starts the local Rust binary and sends JSON-RPC frames over pipes. The repository is not only information; it supplies an executable local interface. GPU workloads, if implemented, run in the user's own provider account and contain the existing Python/native inference frameworks.

## Framework provenance vs. model compatibility

Runtime family discovery is based on trusted upstream *provenance*, not a claim that every model using that framework has been tested. The [registry](../config/runtime-registry.json) records the source for each framework:

| Runtime family | Upstream | Intended scope | Current registry status |
|---|---|---|---|
| Hugging Face Diffusers | https://github.com/huggingface/diffusers | Compatible image/video diffusion | **Candidate only** |
| Hugging Face Transformers | https://github.com/huggingface/transformers | Supported vision, audio, speech, document, multimodal | **Candidate only** |
| ComfyUI | https://github.com/comfyanonymous/ComfyUI | Reviewed graph and nodes for image/video | **Candidate only** |
| AudioCraft | https://github.com/facebookresearch/audiocraft | Compatible music/sound generation | **Candidate only** |
| pyannote.audio | https://github.com/pyannote/pyannote-audio | Compatible diarization and VAD | **Candidate only** |
| SpeechBrain | https://github.com/speechbrain/speechbrain | Compatible voice and speech tasks | **Candidate only** |
| Model-specific upstream | Model author's documented implementation | New architectures, 3D, motion, custom media models | **Manual review only** |

A source being official means **the source project's provenance is recognized**, not that its current version, example notebook or Space will run the selected model SHA on the user's specific GPU. A Hugging Face Space, example or model card is a *candidate evidence source*; it is not a trusted installer or a transferable, reproducible OCI image until reviewed, pinned and tested.

### Registered deployment alternatives

Each suitable family may define both:

1. **Verified OCI image**: a reviewed image reference pinned by a `@sha256:` digest, including a verified worker implementation, CUDA/user requirements and the tested model revision.
2. **Vetted installer**: a pinned base OCI image, checksum-pinned reviewed installation manifest, verified worker implementation, and tested model revision.

The resolver **prefers OCI** when equally verified and eligible, else may choose the **verified installer**. No image digest or installation commands are fabricated in the repository. All bundled registry entries currently have status `candidate_unverified` and lack executable digests. That is intentional: **this project has not yet built or validated the deployment images**.

### Evidence required before labeling a runtime compatible

A recorded success for a particular model/revision and provider must include:

- Exact immutable Hugging Face revision SHA and the model's pipeline tag/architecture/checkpoint format.
- Verified runtime package and CUDA/driver versions, worker implementation checksum and a pinned OCI image digest or reviewed installer manifest digest.
- Confirmed task schema, input media type, output artifact integrity, GPU memory/disk limits, any access restrictions/license, and explicit nonexecution of untrusted model scripts.
- Provider-specific live test evidence (logs/CI reference and source commit), not merely a Hub README or workflow screenshot.
- Separate, non-reusable human approval of **each deployment request** and reconciliation/stop protection for the provider before any real execution can be exposed.

All these conditions are necessary; evidence of one successful Diffusers image model does not verify a different architecture, video model, user provider or another runtime route.

## What the Rust resolver actually does now

1. Queries the live Hugging Face Hub for the chosen model, resolves an immutable SHA, reads available library/task, architectures and file signals. Publisher metadata is untrusted.
2. Classifies the model into a media family without excluding unfamiliar model types.
3. Compares task + declared library, architecture and representative checkpoint file signals with runtime registry entries. A generic task name **alone** does not provide a positive match.
4. Scores candidates and attaches evidence flags; unknown/specialized architectures are **not automatically matched to an arbitrary installer**.
5. Requires actual model/provider-specific verification and pinned runtime/worker evidence to mark a candidate verified. Synthetic fixtures exercise this logic offline but are not operational proof.
6. Returns `runtime_unknown_requires_review`, `unverified_runtime_candidates_found`, `verified_runtime_candidate_but_deployment_unavailable`, or `non_media_task_out_of_scope`, together with immutable model SHA, candidate routes and explicit safety notes.
7. Always returns `execution_enabled=false` and `safe_to_deploy=false`. No MCP tool provisions cloud resources or launches code.

### Local CLI

```bash
cargo run --locked -- runtimes
cargo run --locked -- runtimes resolve "owner/model" modal
cargo run --locked -- models inspect "owner/model"
cargo run --locked -- plan "owner/model" vast-instances
```

MCP adds `runtimes_list` and `runtime_resolve` beside the existing `models_search`, `models_inspect`, `deployment_policy` and `plan_generation` tools.

`runtimes` is fully offline. `runtimes resolve` and `plan` contact the Hub for model metadata; **they do not consume GPU credit**.

## Provider-specific orchestration (next implementation)

The local Rust bridge should carry out provider API provisioning and job/status operations via **provider-specific adapters**, only after binding the exact model SHA, runtime and hardware plan to a new **human-issued, one-shot deployment grant**.

- Modal can execute a compatible application/function/container under the user's account.
- Vast/RunPod GPU instances and Lambda GPU VMs can run a compatible OCI image or a pinned base-image installer; serverless providers may require their own handler interfaces.
- Existing HTTPS/SSH/Kubernetes worker endpoints need a verified owner/workspace and compatible worker before an agent can use them.
- Resource creation and user approval are not interchangeable. Provider credentials remain local. AI-generated prompts and model cards cannot substitute for an independent user approval grant.
- After the final job and artifact transfers, Rust must reconcile active jobs and **auto-stop only bridge-owned resources using a supported stop API**; never translate stop permission into destructive termination. Durable recovery is needed when the local agent or laptop stops unexpectedly.

None of the above provider actions has been implemented or live-tested in the present branch. Platform configuration is not proof of operational compatibility.

## References

- [Registry](../config/runtime-registry.json)
- [Current media tasks](../config/media-tasks.json)
- [Provider catalog](../config/providers.json)
- [Deployment policy](../config/deployment-policy.json)
- [Provider lifecycle requirements](PROVIDERS.md)
