# Windows agent compatibility

## Latest independent retest — 2026-11-24

Tested code revision: `5b99967de6166ccdcde0a225dfb76aa8e4788b73`. Five installed Windows agent clients successfully invoked the four selected MCP stdio tools, and all 20 successful structured results matched the CLI reference. There were **21 attempts**, including one rejected Claude Code doctor request with invalid parameters, followed by a successful retry.

| Client | Version | Matching target calls | Attempts |
| --- | --- | ---: | ---: |
| Codex CLI | 0.167.0-alpha.3.2 | 4/4 | 4 |
| Claude Code | 2.1.318 | 4/4 | 5 |
| Hermes Agent | 0.23.1 (3b4215785882) | 4/4 | 4 |
| DeepSeek Harness | 0.2.2-rc.3 | 4/4 | 4 |
| OpenCode | 2.1.4 | 4/4 | 4 |

[Sanitized evidence](../tests/evidence/agent-compatibility.json) records the individual results and source file hashes. The independent operator supplied the report, CLI reference, relay audit and agent logs; the repository editor compared these files rather than repeating the local execution. The raw session logs and local paths are deliberately not published. The debug executable hash is operator-reported, not independently rehashed by the editor.

## Method and scope

Use the CLI reference `cloud-media-bridge tools call TOOL arguments.json` and the same fixture arguments via the agent's MCP client.

| Tool | Arguments |
| --- | --- |
| doctor | Empty object |
| provider_profile_review | profile from examples/external-provider.example.json |
| billing_estimate | quote from examples/billing-per-second-5s.example.json |
| provider_handoff | manifest from examples/external-deployment-manifest.NOT-EXECUTABLE.json |

The supplied stdio relay forwarded JSON-RPC to the tested bridge binary, correlated tool responses by request ID, and recorded whether structuredContent was present, the call was successful, and the result equaled the corresponding CLI result. The editor matched all 21 audit rows against the summary and reviewed client logs; exact fixture inputs and results are visible for Codex, Claude Code, DeepSeek Harness and OpenCode. Hermes' client log records the four named calls and its relay audit records their successful equality, but not the exact argument objects. The evidence contains SHA-256 hashes of the CLI result objects, serialized as UTF-8 JSON with sorted keys, compact separators and unescaped Unicode.

CLI listed 27 tools per the operator. Claude Code's initialization log contains 27 bridge tools. The submitted structured report reports complete list equality for Codex, Claude Code and OpenCode, but leaves Hermes and DeepSeek count/equality null. The relay itself did not capture lists; therefore this retest does not independently establish full list equality for all five. Explicit process exit codes were not supplied.

The first Claude Code doctor attempt was rejected, not counted as successful. Test-only permissions were relaxed for that client; never bypass approval for live cloud operations. The OpenCode log also shows a resource-list query; it is not included in the 20 target calls. DeepSeek succeeds on the revision containing the MCP cache metadata fix introduced in 2f40c7c. The handoff fixture returned approval_granted=false and provider_operation_sent=false; no deployment was made. No incremental paid usage was reported by the operator; no billing account records were independently accessed.

## Historical and CI evidence

The [prior five-client evidence at 2f40c7c](../tests/evidence/agent-compatibility-2f40c7c.json) is retained separately. Its recorded results apply only to that revision. The latest tested code passed [offline CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45327961794) on Windows, Linux and macOS and [native candidate CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45327961850) across six native targets. Independent WSL report: 110 Rust tests, Clippy and an offline CPU fixture demo passed. These are not real GPU inference, deployment, lifecycle or invoice results.
