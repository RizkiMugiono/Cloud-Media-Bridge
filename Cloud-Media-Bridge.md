# Architecture

The single source for the agent workflow is [README.md](README.md).

User → AI agent → local Rust MCP/CLI → official provider API/CLI → compatible media runtime → verified media artifact.

The bridge discovers models and provider sources, compares cost, resolves runtimes, produces exact deployment/lifecycle requests and preserves a small local operation record where required by the native adapter. The user supplies the cloud account, credits and approval. The agent performs operations; the provider controls execution, autoscaling and billing.

No centralized platform, custom user authentication, signing ledger or independent watchdog is part of the target architecture. Use the agent host's trusted approval and provider-native facilities. Native direct execution is not live-verified and remains gated. See README and focused provider, runtime and billing documents.
