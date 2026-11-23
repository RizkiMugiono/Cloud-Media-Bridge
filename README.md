# Cloud-Media-Bridge

**Status:** local Rust CLI prototype. Windows and Ubuntu WSL passed the first two MCP integration tests on 2026-11-23.

Cloud-Media-Bridge connects an individual's MCP-compatible agent to their own Modal workspace. The working prototype exposes a local diagnostic tool on Windows and Linux.

## Version 1 scope

| Step | Design | Status |
|---|---|---|
| Connect | Register the Rust CLI as a local MCP stdio server and call `doctor`. | Tested through a process-level client on Windows and Ubuntu WSL. |
| Discover | Search the live Hugging Face Hub or paste any accessible model URL. Resolve the selected revision to a commit SHA. | Planned |
| Plan | Match the model family and task to a compatible serving container or library pipeline. Show access requirements, workspace, GPU and compatibility evidence. | Planned |
| Execute | After human approval of the model and resource operation, use Modal to deploy or invoke the selected workload and return its output. | Planned |

New Hub entries are discoverable without a bridge release. Existing pipelines can serve additional compatible models by ID and SHA. A new architecture may require additional runtime support. The bridge reuses existing inference software. Building an inference engine is outside v1. Additional providers, durable job queues and concurrency tuning are roadmap items.

## Run the local CLI

With Rust installed, run these commands from the repository root:

~~~sh
cargo fetch --locked
cargo test --offline --locked
cargo run --offline --locked -- doctor
~~~

For an MCP client, launch the compiled `cloud-media-bridge` binary with the single argument `mcp`. It implements the 2025-11-25 initialization handshake and exposes one read-only tool, `doctor`, through `tools/list` and `tools/call`. The process uses newline-delimited JSON-RPC over stdio. It requires no credentials or network connection. Media generation and provider adapters are planned.

## Start here

| Document | Purpose |
|---|---|
| [Cloud-Media-Bridge.md](Cloud-Media-Bridge.md) | Approved implementation specification, architectural contracts, security and phased work order for coding agents |
| [Cloud-Media-Bridge-TESTING.md](Cloud-Media-Bridge-TESTING.md) | Detailed test plan, test IDs, expected behavior, release gates and Modal-only live-test procedure |
| [Cloud-Media-Bridge-TEST-RESULTS.md](Cloud-Media-Bridge-TEST-RESULTS.md) | Actual documentation checks and transparent **NOT RUN** statuses for unexecuted application/GPU tests |

## Safety and testing rules

- **User-owned credentials:** authentication to GPU providers stays under the end user's control.
- **Human approval:** model ID, immutable revision and appropriate resource operations require explicit approval.
- **Safe defaults:** building/testing documentation and local mocks must not deploy cloud resources or consume GPU by default.
- **Scale-to-zero:** required where a provider supports it; GPU pods/VMs can have different lifecycle/billing behavior.
- **Provider claims:** implement and verify providers individually. Initial live tests only on Modal with separate authorization.
- **No fabricated results:** a spec or mock test is never proof that real GPU inference works.

**License:** [Apache-2.0](LICENSE).
