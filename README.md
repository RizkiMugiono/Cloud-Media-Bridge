# Cloud-Media-Bridge

**Status:** specification and test-plan stage — no verified production implementation.

An open-source, local-first bridge connecting a user's AI harness to **their own** GPU compute provider account. It supports a Rust-based local control plane (MCP stdio/CLI), explicit human-approved model selection, and a dual inference-engine design (native Rust + universal Python/PyTorch runtimes).

The architecture targets many AI media workloads (image, video, speech, voice, music, audio and more) **excluding LLM hosting**. Model sources and compute providers are extensible; **Modal.com is the only provider planned for initial real-GPU testing**. Additional providers such as RunPod, Vast.ai and Lambda are future integrations, not currently verified.

This repository **does not provide a central hosted service**, use the maintainer's cloud account to process user media, or silently select/replace models. It does not guarantee every discovered model is executable.

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
