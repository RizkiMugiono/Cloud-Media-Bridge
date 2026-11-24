# Distribution

The distribution workflow builds a native Rust release executable for six OS/architecture combinations and runs doctor plus the actual Rust stdio integration tests on each runner. Each job uploads a candidate directory containing the binary, license, README, and SHA-256 checksum. It does not publish a GitHub Release or merge a PR, and does not request GPU credentials.

Download the matching candidate from the workflow run only after all six jobs pass. Verify SHA-256 before use. Windows users need cloud-media-bridge.exe; Linux/macOS users need cloud-media-bridge (set execute permission if the download strips it). A local curl executable is still required for the existing built-in HTTP discovery tools. Provider CLIs and user authentication are managed by the user/agent separately.

After the actual provider smoke-test evidence is recorded and the maintainer explicitly approves release, publish the verified candidates and hashes from the reviewed commit. Do not label these builds as a production-ready cloud executor merely because the local MCP/CLI smoke tests pass. No release publishing automation is enabled.
