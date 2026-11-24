# Agent integration

Use the same Rust executable on Windows, Linux or macOS. Install Rust and a local curl executable for built-in HTTP discovery; build with cargo build --locked --release, then use the absolute executable path or put the binary on PATH. The MCP stdio example is examples/mcp-stdio-client.json. Configure the installed agent host to start cloud-media-bridge mcp using its documented MCP configuration format. No agent-specific backend is needed. Do not paste cloud credentials into agent prompts or config committed to Git.

The common host interfaces are:

- Local MCP stdio: use the bridge command and one argument mcp. Supports legacy initialize/initialized tools handshake for 2024-11-05 through 2025-11-25 and basic 2026-07-28 server/discover and stateless tools/list, tools/call. Optional protocol features (such as sampling, elicitation, resources, HTTP transport) are not advertised.
- Local shell: tools list returns the same tool definitions; tools call TOOL ARGUMENTS.json uses the same argument validation and implementation and returns plain JSON. No arbitrary shell runner or paid provider operation is exposed through this interface.
- The agent must be able to reach official provider documentation, a trusted local provider CLI or API, and a user approval interface. A chat-only or MCP-only agent without native action access cannot provision a provider merely by reading a draft.

Use doctor and tools list as a local smoke test. Actual third-party agent compatibility must be tested separately. See MCP upstream documentation: https://modelcontextprotocol.io/ and https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2026-07-28/server/discover.mdx .

For MCP 2026-07-28, `server/discover` and `tools/list` include the required cache metadata (`ttlMs: 0`, `cacheScope: private`) to prevent stale list reuse. Legacy protocol responses and ordinary tool calls are unchanged. See https://modelcontextprotocol.io/specification/2026-07-28/server/utilities/caching and https://modelcontextprotocol.io/specification/2026-07-28/server/discover .
