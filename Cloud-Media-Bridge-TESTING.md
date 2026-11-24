# Testing

Default tests must not rent GPUs or consume provider credits.

```sh
cargo test --locked --workspace --all-targets
cargo build --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
python -m unittest discover -s workers/tests -v
```

GitHub Actions runs Windows, Linux and macOS checks, including Linux CPU-only fixture and pinned-wheel container builds. Preserve tests for model discovery/validation, dynamic catalog parsing and billing, exact plans, provider request contracts, ownership and state, lifecycle decisions, authenticated worker protocol, artifact integrity, uncertain operation recovery, and the closed production gate. Fixture outputs are simulations, not AI inference.

Before enabling a native provider/runtime path, test current official API schemas and read-only account/offer checks, trusted host approval, image and runtime compatibility, and provider-native lifecycle behavior. Only the real GPU smoke tests, performance and actual invoice validation require separately approved GPU credits. No custom authorization or monitoring subsystem is required.

Record actual results and commit SHA in [the results ledger](Cloud-Media-Bridge-TEST-RESULTS.md); historical PASS applies only to the recorded revision. Missing or skipped evidence is NOT RUN.
