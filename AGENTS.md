# Instructions for contributing agents and maintainers

These rules apply equally to **any** AI coding harness, human contributor, or automated test runner.

1. Read `README.md`, `Cloud-Media-Bridge.md`, `Cloud-Media-Bridge-TESTING.md`, and the most recent result ledger first.
2. Keep one local Rust MCP/CLI bridge, a live Hugging Face catalog, user-owned GPU provider profiles, and reusable compatible inference runtimes. Do not introduce a central SaaS or custom inference engine as a prerequisite.
3. Maintain neutrality toward AI-agent vendors and GPU cloud providers. Document actual compatibility, not brand claims.
4. **Never infer that model discovery proves executable compatibility.** Pin an immutable model revision for every execution plan.
5. Human model selection and cloud resource-operation approval are separate requirements. MCP tool text cannot approve resource use.
6. No cloud provisioning, GPU inference, destructive cleanup, credential rotation, or provider switching without explicit human authorization for that operation.
7. Run **offline** build, schema, MCP and security tests by default. Do not run or enable GPU tests from ordinary CI.
8. Do not add credentials, private prompts, user media, signed URLs or internal profile/account details to commits, fixtures or logs.
9. Keep test evidence honest: PASS requires a reproduced result at the specified commit. Mocks are not real provider tests; missing evidence means NOT RUN.
10. Update the compatibility matrix, test plan and results when an implementation changes. Mark unsupported and unimplemented paths explicitly. Agents must consult the README agent workflow and actual `tools/list` output: current planning is read-only, and no deploy, approval, GPU generation, or artifact retrieval tools are implemented. Never simulate successful execution.
11. Preserve the Apache-2.0 license and do not alter unrelated repository settings without authorization.
