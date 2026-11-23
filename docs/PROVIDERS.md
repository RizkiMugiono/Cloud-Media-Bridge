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
