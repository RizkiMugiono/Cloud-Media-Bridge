# Cloud-Media-Bridge

A local Rust MCP/CLI bridge for AI agents to run open-source media models on the user's own cloud GPU account. The agent is the operator; the provider supplies compute and billing. No central service, user database, payment system, custom account authentication, or independent scheduler is required.

**User → AI agent → Cloud-Media-Bridge → user-owned cloud GPU → media result**

## Agent workflow

1. Discover a compatible Hugging Face model and pin its revision. Resolve an existing upstream runtime or compatible image/recipe.
2. Read the provider's current GPU catalog, prices, availability and billing rules. Compare compatible options using the billing tools; do not store static prices.
3. Review a plan containing the model, GPU, account, region, duration, total cost exposure and user-selected lifecycle. Obtain the user's explicit approval through the agent host or provider UI.
4. Use the approved provider-native API/CLI and reviewed runtime to deploy and run inference. The bridge supplies provider request drafts and runtime/worker mechanisms, not a new cloud platform. Check the current provider contract before dispatch.
5. Retrieve the result and verify artifact integrity. Apply only the chosen lifecycle; verify actual provider status and residual charges. Prefer provider-native idle scaling/TTL when available.

If the user declines, terms change, the host cannot obtain real approval, or a request is uncertain, do not deploy or blindly retry. Never terminate shared resources or substitute termination for stop. No agent-provided boolean is proof of consent.

## Install and connect

```sh
cargo build --locked --release
cargo run --locked -- doctor
cargo run --locked -- mcp
```

Configure an MCP client using [the stdio example](examples/mcp-stdio-client.json). The executable is in `target/release/`. Use the user's existing local provider credentials, never put keys in manifests, messages or the repository.

## Main tools

| Task | CLI | MCP |
|---|---|---|
| Model discovery | `models search`, `models inspect` | Discover/inspect model tools |
| Runtime matching | `runtimes resolve MODEL PROVIDER` | Runtime resolver |
| Current sources | `market sources`, `market refresh PROVIDER` | `provider_sources`, `market_refresh` |
| Cost | `billing estimate QUOTE.json` | `billing_estimate` |
| Plan and compare | `cloud review MANIFEST.json` | `cloud_review` |
| Exact deployment draft | `cloud handoff MANIFEST.json` | `provider_handoff` |
| Save plan / inspect | `cloud prepare SESSION MANIFEST.json`, `cloud status SESSION` | Session prepare/status |
| Lifecycle | `cloud advice OBSERVATION.json` | `lifecycle_advice`, `lifecycle_action` |

Use `--help` and MCP `tools/list` for exact interfaces. See [example manifest](examples/cloud-deployment-manifest.NOT-EXECUTABLE.json); it is illustrative only.

## Providers and actual status

| Provider | Integration |
|---|---|
| Modal | Official pricing and CLI handoff |
| RunPod Pods | Current catalog, native API code and handoff |
| RunPod Serverless | Current catalog and request draft |
| Vast Instances | Current marketplace, native API code and handoff |
| Vast Serverless | Official source and agent handoff |
| Lambda Cloud | Instance catalog and request draft |
| HTTPS, SSH, Kubernetes | Existing user infrastructure and native handoff |

Provider-native request drafts are **not** certified executors. The built-in `cloud deploy/generate/stop` remain disabled for unverified native execution; the old environment opt-in cannot override the release gate. RunPod/Vast account checks and resource/job inspection are read-only. Agent-host authorization is the primary boundary; the manual native CLI fallback is not independent identity authentication. There is no custom signing service or monitoring daemon.

Existing media runtimes should be reused first. A pinned OCI worker candidate and local CPU/fixture tests are retained as optional fallbacks. No GPU inference, actual charge, or provider lifecycle has been verified by the offline tests.

## Documentation

- [Dynamic provider sources](docs/LIVE-SOURCES.md)
- [Provider reference](docs/PROVIDERS.md)
- [GPU billing](docs/GPU-BILLING.md)
- [Media model discovery](docs/MEDIA-MODELS.md)
- [Runtime resolution](docs/RUNTIME-RESOLUTION.md)
- [Optional worker build](docs/GPU-SMOKE-BUNDLE.md)
- [Offline testing](Cloud-Media-Bridge-TESTING.md) and [historical results](Cloud-Media-Bridge-TEST-RESULTS.md)

Apache-2.0.

### Any additional provider, no repository edit

Create a local [external provider profile](examples/external-provider.example.json) with the real provider ID, official pricing/documentation URLs, native interface and supported lifecycle actions. Use `providers profile PROFILE.json` or MCP `provider_profile_review`, then pass that profile as `provider_profile` in the deployment manifest and each external alternative. For runtime matching and quote checking use the optional profile argument. The Rust bridge validates and compares the same plan, produces a provider-native agent handoff, and guides lifecycle reconciliation. It does not fetch arbitrary URLs or execute arbitrary profile commands. The agent verifies sources and constructs the native call using the current official SDK/API/CLI, obtains approval, and dispatches through its own tools. The illustrative profile is not a real provider. See [generic provider workflow](docs/GENERIC-PROVIDERS.md).

### Agent interoperability

Five Windows agent clients completed four MCP tool calls each, with all 20 target results matching CLI on [tested code revision 5b99967](docs/AGENT-COMPATIBILITY.md). One invalid-parameter attempt was rejected and retried. Full tool-list equality for all five is not independently established by the latest audit; live GPU operation remains unverified.

The local executable supports MCP stdio for handshake revisions 2024-11-05, 2025-03-26, 2025-06-18 and 2025-11-25, and the basic 2026-07-28 stateless discovery/tools interface. No HTTP server or full optional MCP feature set is required. Any agent with a shell can use the same validated tool interface without MCP:

```sh
cloud-media-bridge tools list
cloud-media-bridge tools call provider_profile_review arguments.json
```

`arguments.json` contains the exact MCP arguments object, for example `{ "profile": { ... } }`. CLI tool results are JSON, not MCP envelopes. Agent host authorization still applies. See [agent integration](docs/AGENT-INTEGRATION.md). Protocol fixtures are not evidence that every third-party client has been tested.

An agent may supply a pinned upstream runtime recipe instead of a finished OCI image, including for a newly discovered runtime architecture. See the [recipe-only external manifest](examples/external-deployment-manifest.NOT-EXECUTABLE.json). Recipe references are validated structurally, not downloaded or executed; review their actual contents, dependencies and compatibility. If the selected provider needs an OCI image, build and pin it before the final native request and refresh user approval when the plan changes.

## Release candidates

The [native distribution workflow](.github/workflows/distribution.yml) builds and checks Windows, Linux and macOS x86-64/ARM64 binaries. See [distribution](docs/RELEASE.md) and [provider verification / real GPU smoke checklist](docs/PROVIDER-VERIFICATION.md). Candidate CI artifacts are not a published release or a claim of live GPU readiness.
