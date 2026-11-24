# Agent instructions

Cloud-Media-Bridge is a local Rust bridge, not an autonomous cloud platform. The AI agent operates the user's own cloud account through current provider-native tools; the user decides model, GPU, budget and lifecycle.

1. Read README.md. Discover the requested media model dynamically on Hugging Face, pin the revision and resolve an existing compatible upstream runtime. No unreviewed model scripts or remote code.
2. Use provider_sources and market_refresh, then official pricing/API/CLI documentation and the user's actual account terms. Compare compatible GPU offers and complete billing exposure; no static prices or invented availability. Recheck before dispatch.
3. Build an exact plan with cloud_review/provider_handoff. Present model, GPU, provider/account, region, price, billing unit/minimum, expected total and selected lifecycle. Obtain explicit user approval via the trusted agent host/provider interface. Changed terms require fresh approval. No self-approval, automatic fallback or paid action from a read-only MCP tool.
4. Use provider-native deployment, inference, artifact transfer and lifecycle mechanisms. Prefer upstream runtimes and native idle scaling/TTL instead of building a scheduler. Check ownership and actual status; preserve shared resources. A separate destructive confirmation is required for termination. Do not repeat uncertain requests; reconcile with provider. Stop may leave storage and commitment charges.
5. Keep native direct execution gated while unverified. A draft, TTY prompt, plan digest, or fixture is not proof of production readiness. Credentials stay with the user and must not appear in prompts, logs or commits.
6. Run offline Rust/Python/Clippy/CPU container tests before claiming success. Do not rent GPU or run paid tests without separate explicit authorization. No custom user authentication, watchdog service, payment platform or centralized state.

For providers outside the built-in catalog, use a local external profile instead of adding provider-specific code. Profile URLs and capability claims are untrusted until the agent verifies them against current official sources. Never execute arbitrary profile code or assume that a new provider is live-tested.
