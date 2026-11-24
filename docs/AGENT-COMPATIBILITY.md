# Windows agent compatibility

On 2026-11-24, five agent CLIs completed the same MCP stdio test against Cloud-Media-Bridge revision `2f40c7c08ff59e5d824042b2263cf326ee679522`. Each read all 27 tool definitions, issued all four calls, and exited with status 0. The [machine-readable evidence](../tests/evidence/agent-compatibility.json) records each client's call order, result comparisons, model route and the shared binary digest.

| Client | Tested version | CLI-matched MCP calls |
| --- | --- | --- |
| Codex CLI | 0.171.0-alpha.13 | 4/4 |
| Claude Code | 2.1.318 | 4/4 |
| Hermes Agent | 0.23.1 (3b4215785882) | 4/4 |
| DeepSeek Harness | 0.2.2-rc.3 | 4/4 |
| OpenCode | 2.1.4 | 4/4 |

## How the comparison was made

Build the tested revision with `cargo build --locked --offline`. Run `cloud-media-bridge tools list` and `cloud-media-bridge tools call TOOL arguments.json` to obtain the CLI reference values. Construct each arguments object from the exact JSON value in the indicated repository fixture.

| Tool | Arguments |
| --- | --- |
| doctor | Empty object |
| provider_profile_review | `profile` from `examples/external-provider.example.json` |
| billing_estimate | `quote` from `examples/billing-per-second-5s.example.json` |
| provider_handoff | `manifest` from `examples/external-deployment-manifest.NOT-EXECUTABLE.json` |

Each agent received the four argument objects and used its configured MCP client to call the bridge. A local stdio relay forwarded JSON-RPC messages and correlated responses by request ID. It checked the complete tool list, input objects, tool success and structured results against the CLI reference. The JSON evidence contains SHA-256 hashes of CLI result objects serialized as UTF-8 JSON with sorted keys, compact separators and unescaped Unicode.

DeepSeek Harness completed discovery and all four calls through its stdio MCP plugin on the revision containing the modern tool-list cache metadata. Hermes used its installed Python runtime with lazy installation disabled. Only the Hermes session traced to the tested binary is included. The handoff fixture returned `approval_granted=false` and `provider_operation_sent=false`. This test covers local discovery and these four operations.

The tested revision passed 109 local Rust tests and Clippy. The Windows Python worker suite passed 17 tests and skipped one Unix PTY test. The [CI run for the tested revision](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45321444694) passed on Windows, Ubuntu and macOS, including the CPU container check on Ubuntu.
