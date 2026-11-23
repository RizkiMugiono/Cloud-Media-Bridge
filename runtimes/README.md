# Reused inference runtimes (no custom inference engine)

The versioned [runtime registry](../config/runtime-registry.json) enumerates candidate runtimes. Each family below is **an upstream project**. No executable OCI image, installer script or model-specific GPU worker has been shipped in this repository.

Runtime resolver logic is in [`crates/bridge-cli/src/runtime.rs`](../crates/bridge-cli/src/runtime.rs). Read [verification and deployment policy](../docs/RUNTIME-RESOLUTION.md) before contributing.

- [Diffusers](diffusers/README.md)
- [Transformers](transformers/README.md)
- [ComfyUI](comfyui/README.md)
- [Specialized / AudioCraft / pyannote / SpeechBrain](specialized/README.md)

Contributor requirement: a reviewed, pinned worker implementation and an independently approved human deployment action are prerequisites for every real GPU deployment. An upstream README, Docker Space, demo or `pipeline_tag` is not sufficient evidence. No centralized MCP server is necessary; the Rust stdio MCP runs with the user's agent.
