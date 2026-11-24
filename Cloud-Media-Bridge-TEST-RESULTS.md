# Cloud-Media-Bridge — Test Results and Verification Ledger

**Date:** 2026-11-24  
**Repository:** https://github.com/RizkiMugiono/Cloud-Media-Bridge  
**Specification:** [Cloud-Media-Bridge.md](Cloud-Media-Bridge.md)  
**Test plan:** [Cloud-Media-Bridge-TESTING.md](Cloud-Media-Bridge-TESTING.md)  
**Overall application status:** **62 Rust unit tests + 16 Rust MCP integration tests + 14 Python worker tests passed in offline CI on Linux, Windows and macOS; CPU Docker fixture passed on Linux. Real GPU inference, provider stop and actual billing remain untested**

This file records **actual observations only**. All-media catalog checks demonstrate classification and MCP contract behavior, **not** model inference or true universal runtime compatibility. Passing documentation checks do not prove an implemented application is secure, functional or deployable. The implemented Rust code covers MCP, a read-only provider catalog, media classification, evidence-based Rust runtime matching, Hub discovery functions and a non-mutating planner. Real Hub connectivity, GPU deployment and inference still lack verified live tests.

## 1. Verified documentation and repository checks

| Check | Result | Evidence |
|---|---|---|
| Repository is accessible and writable to the authorized account | PASS | GitHub connector read and document commits succeeded |
| Architecture document is present and readable | PASS | `Cloud-Media-Bridge.md` present on `main` |
| Test plan is present and readable | PASS | `Cloud-Media-Bridge-TESTING.md` present on `main` |
| Markdown code fences in both documents are balanced | PASS | Architecture: 4 fence delimiters; test plan: 6; balanced = true |
| Test case IDs are unique and complete | PASS | 140 unique IDs: H01–H18, P01–P18, M01–M16, D01–D15, J01–J15, U01–U10, A01–A10, R01–R08, O01–O14, C01–C08, B01–B08 |
| Architecture and test plan reference each other | PASS | Cross-references verified in both source documents |
| Application Rust source compilation and static analysis | PASS | [Latest cloud-control/billing PR CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45306337305) passed cargo build/test/Clippy on Linux, Windows and macOS with no GPU |
| CLI integration tests | PASS | [Latest offline CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45306337305): 62 Rust unit, 16 MCP integration and 14 Python worker/security tests passed on Linux, Windows and macOS. GPU inference remains untested |
| Modal CLI authentication (current working environment) | NOT VERIFIED | No login check was performed in this GitHub CI run; earlier claims are not fresh evidence |
| Modal resource provisioning and real GPU inference | NOT RUN | No resources created and no GPU jobs submitted |

[Offline CI evidence on the merged main branch](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45196722011): **PASS on Linux, Windows and macOS** — 4 unit tests, 4 MCP integration tests, and Rust Clippy on each OS; validation of 9 provider catalog entries on Linux. No provider credentials or GPU were used. **Live Hugging Face networking is NOT RUN.**

[All-media offline CI evidence](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45207348636) — **PASS on Linux and Windows: 12 unit tests, 7 MCP process tests, Rust Clippy, 61 media-task catalog checks and both generic media request/result schema checks.** This evidence does not include live Hub access or GPU inference.

[Runtime resolver CI evidence](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45217160090) — 21 unit and 9 MCP process tests passed on Linux, Windows and macOS, Clippy passed, and 20 non-executable upstream runtime routes passed registry validation. No real model inference, provider deployment or paid GPU was used.

[Latest provider control-plane and billing offline CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45306337305) — Linux, Windows and macOS: 62 Rust unit tests, 16 MCP integration tests, 14 Python HTTP worker tests and strict Rust Clippy checks passed. Additional Linux-only validation checked billing quotes, nine GPU provider billing annotations and CPU/Docker fixture. **No cloud keys, paid GPU, real provider invoice, or real GPU stop were used.**

Local application evidence covers only the scenarios below.

## 2. Application test-case status ledger

**44 mapped scenarios have offline test evidence** (H01–H02, U01–U03, A01–A07, R01–R06, O01–O12, C01–C07, B01–B07). **96 scenarios remain NOT RUN**, including live Hugging Face access, real AI inference, provider provisioning and actual GPU stopping. A local mock success is not evidence of real provider compatibility. See the scenario rows and linked CI evidence below.

| Test ID | Scenario requiring verification | Status | Evidence |
|---|---|---|---|
| H01 | Request `tools/list` over MCP stdio | **PASS** | GitHub Actions CI `45196722011`; `crates/bridge-cli/tests/stdio.rs::h01_list_and_call_local_doctor` |
| H02 | Send invalid or unknown MCP arguments | **PASS** | GitHub Actions CI `45195797719`; `crates/bridge-cli/tests/stdio.rs::h02_invalid_requests_leave_session_usable` |
| H03 | Agent submits media generation without human model approval | **NOT RUN** | No execution evidence |
| H04 | Agent adds `approved=true` to a tool request | **NOT RUN** | No execution evidence |
| H05 | Agent outputs text claiming "the user approved" | **NOT RUN** | No execution evidence |
| H06 | Human explicitly rejects the proposed model | **NOT RUN** | No execution evidence |
| H07 | Approve revision SHA A; source default branch moves to SHA B | **NOT RUN** | No execution evidence |
| H08 | Agent/router attempts substitution from model A to model B | **NOT RUN** | No execution evidence |
| H09 | Approval for task A is reused for task B | **NOT RUN** | No execution evidence |
| H10 | Approval grant expires or is revoked | **NOT RUN** | No execution evidence |
| H11 | Agent attempts privileged approval via MCP stdin/tool | **NOT RUN** | No execution evidence |
| H12 | Approval is attempted in headless/no-TTY mode | **NOT RUN** | No execution evidence |
| H13 | Agent silently switches compute provider profile from A to B | **NOT RUN** | No execution evidence |
| H14 | Model approval is presented as authorization to provision GPU resources | **NOT RUN** | No execution evidence |
| H15 | Wrong profile credentials are supplied | **NOT RUN** | No execution evidence |
| H16 | Cloud-only agent attempts to launch unavailable local executable | **NOT RUN** | No execution evidence |
| H17 | Harness configuration file already exists | **NOT RUN** | No execution evidence |
| H18 | Test multiple harness/protocol versions | **NOT RUN** | No execution evidence |
| P01 | Capabilities include multiple resource/deployment modes | **NOT RUN** | No execution evidence |
| P02 | Call `plan()` | **NOT RUN** | No execution evidence |
| P03 | Call `provision()` without a resource grant | **NOT RUN** | No execution evidence |
| P04 | Call `submit()` without matching approved job/model | **NOT RUN** | No execution evidence |
| P05 | Mismatch provider, account, workspace or environment | **NOT RUN** | No execution evidence |
| P06 | Model card supplies an arbitrary compute endpoint URL | **NOT RUN** | No execution evidence |
| P07 | Network timeout occurs before a safe submission | **NOT RUN** | No execution evidence |
| P08 | Provider accepts submission but response is lost | **NOT RUN** | No execution evidence |
| P09 | Provider does not support cancellation | **NOT RUN** | No execution evidence |
| P10 | Provider API responds with 429, 503 or 5xx | **NOT RUN** | No execution evidence |
| P11 | Execute `cleanup --dry-run` | **NOT RUN** | No execution evidence |
| P12 | Cleanup targets resource owned by another application | **NOT RUN** | No execution evidence |
| P13 | GPU pod does not support automatic scale-to-zero | **NOT RUN** | No execution evidence |
| P14 | Adapter exists as an unimplemented skeleton | **NOT RUN** | No execution evidence |
| P15 | Provider authentication fails | **NOT RUN** | No execution evidence |
| P16 | Requested GPU is unavailable | **NOT RUN** | No execution evidence |
| P17 | Provider API schema changes incompatibly | **NOT RUN** | No execution evidence |
| P18 | Two independent installations/users share a provider class | **NOT RUN** | No execution evidence |
| M01 | Valid Hugging Face model URL | **NOT RUN** | No execution evidence |
| M02 | HTTP, localhost, private or link-local target | **NOT RUN** | No execution evidence |
| M03 | Redirect from allowed source to unsafe target | **NOT RUN** | No execution evidence |
| M04 | Encoded traversal, symlink or dangerous file path | **NOT RUN** | No execution evidence |
| M05 | Model card contains malicious prompt-injection commands | **NOT RUN** | No execution evidence |
| M06 | Gated or private model | **NOT RUN** | No execution evidence |
| M07 | Checkpoint contains pickle-only weights | **NOT RUN** | No execution evidence |
| M08 | Runtime would require `trust_remote_code=True` | **NOT RUN** | No execution evidence |
| M09 | Model tag claims image-generation support | **NOT RUN** | No execution evidence |
| M10 | Architecture matches but image digest/checkpoint is incompatible | **NOT RUN** | No execution evidence |
| M11 | Discovered model has no compatible container or pipeline | **NOT RUN** | No execution evidence |
| M12 | New Hub entry matches an existing pipeline or serving image | **NOT RUN** | No execution evidence |
| M13 | Router attempts silent compute-provider switch | **NOT RUN** | No execution evidence |
| M14 | Model license/gating requires acceptance | **NOT RUN** | No execution evidence |
| M15 | Runtime manifest is invalid or tampered | **NOT RUN** | No execution evidence |
| M16 | Model memory requirement exceeds GPU capability | **NOT RUN** | No execution evidence |
| D01 | Small valid media input | **NOT RUN** | No execution evidence |
| D02 | Large video submitted inline as MCP JSON | **NOT RUN** | No execution evidence |
| D03 | Spoofed MIME, corrupt codec or decompression bomb | **NOT RUN** | No execution evidence |
| D04 | Input path traversal or symlink escapes authorized scope | **NOT RUN** | No execution evidence |
| D05 | Upload interrupted midway | **NOT RUN** | No execution evidence |
| D06 | Two workers download the same checkpoint concurrently | **NOT RUN** | No execution evidence |
| D07 | Incomplete cache file survives worker crash | **NOT RUN** | No execution evidence |
| D08 | Volume commit/reload race | **NOT RUN** | No execution evidence |
| D09 | Two jobs write outputs concurrently | **NOT RUN** | No execution evidence |
| D10 | Output is truncated or checksum mismatched | **NOT RUN** | No execution evidence |
| D11 | Remote artifact is expired | **NOT RUN** | No execution evidence |
| D12 | SQLite process crashes and restarts | **NOT RUN** | No execution evidence |
| D13 | User revokes approval while job is running | **NOT RUN** | No execution evidence |
| D14 | Source token might appear in logs/trace | **NOT RUN** | No execution evidence |
| D15 | User requests cleanup of data/resources | **NOT RUN** | No execution evidence |
| J01 | Short synchronous generation | **NOT RUN** | No execution evidence |
| J02 | Scale-to-zero worker receives new request | **NOT RUN** | No execution evidence |
| J03 | Remote accepts job, then HTTP response times out | **NOT RUN** | No execution evidence |
| J04 | Bridge/harness stops while video job runs | **NOT RUN** | No execution evidence |
| J05 | User cancels running async job | **NOT RUN** | No execution evidence |
| J06 | User cancels a completed job | **NOT RUN** | No execution evidence |
| J07 | Ten jobs use different models/runtimes | **NOT RUN** | No execution evidence |
| J08 | Ten jobs use the same model | **NOT RUN** | No execution evidence |
| J09 | Model allows only one request per worker | **NOT RUN** | No execution evidence |
| J10 | GPU unavailable or workspace capacity exhausted | **NOT RUN** | No execution evidence |
| J11 | Model initialization exceeds typical startup time | **NOT RUN** | No execution evidence |
| J12 | Remote result expires before download | **NOT RUN** | No execution evidence |
| J13 | VM/pod requires manual stop/termination | **NOT RUN** | No execution evidence |
| J14 | Inference fails in the middle of output streaming | **NOT RUN** | No execution evidence |
| J15 | Remote completes but local state write crashes | **NOT RUN** | No execution evidence |

## 2a. Approved deployment decision tests (U01–U10)

The following new cases trace the four user decisions. **U01–U03 validate the read-only contract only**; U04–U10 require implementation and remain `NOT RUN`. The current policy tool does **not** enforce a deployment grant or issue a GPU stop call.

| Test ID | Scenario requiring verification | Status | Evidence |
|---|---|---|---|
| U01 | Bundled deployment policy declares both a verified prebuilt OCI image and a vetted base-image runtime recipe | **PASS (contract-only)** | Offline Rust unit `deployment_policy_has_exact_user_decisions` + policy schema validation; PR CI [45202599266](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45202599266) |
| U02 | Each configured provider is in the same parallel development matrix | **PASS (contract-only)** | Offline Rust unit `provider_catalog_matches_parallel_workstreams_without_fake_execution` + 9-provider catalog validation; PR CI [45202599266](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45202599266) |
| U03 | CLI/MCP exposes the policy without provisioning or allowing approval injection | **PASS (contract-only)** | MCP process test `deployment_policy_mcp_is_read_only_and_user_decisions_are_visible`; PR CI [45202599266](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45202599266) |
| U04 | Identical deployment requested twice, or reconfigure/redeploy after approval | **NOT RUN** | No independent execution evidence yet |
| U05 | Model family matches prebuilt image versus vetted installation recipe | **NOT RUN** | No independent execution evidence yet |
| U06 | Two in-flight jobs share one bridge-owned GPU pod | **NOT RUN** | No independent execution evidence yet |
| U07 | Owned pod/VM job succeeds, fails or is cancelled | **NOT RUN** | No independent execution evidence yet |
| U08 | Resource is preexisting, external, shared, wrong workspace or ownership unknown | **NOT RUN** | No independent execution evidence yet |
| U09 | Provider exposes terminate only, stop fails or stop response is ambiguous | **NOT RUN** | No independent execution evidence yet |
| U10 | Bridge crashes after job completion but before stop confirmation | **NOT RUN** | No independent execution evidence yet |

## 2b. Media-family coverage test ledger (A01–A10)

These scenarios document whether media discovery, task classification and actual inference are verified. **A01–A07 passed offline unit/MCP contract tests on Linux and Windows; A08–A09 need real Hub access, A10 real authorized inference.** A catalog entry is not evidence that a model runs.

| Test ID | Scenario requiring verification | Status | Evidence |
|---|---|---|---|
| A01 | Read media catalog and classify representative image, video, audio, speech, music, 3D, spatial, document and multimodal tasks | **PASS (offline contract only)** | [Linux/Windows CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45206248356) · `taxonomy_covers_all_media_families` |
| A02 | Broad `text-to-audio` model contains Hub `music` tag | **PASS (offline contract only)** | [Linux/Windows CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45206248356) · `broad_audio_task_can_also_be_music` |
| A03 | New future media task or missing/unknown `pipeline_tag` | **PASS (offline contract only)** | [Linux/Windows CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45206248356) · `future_and_untagged_models_are_not_dropped` |
| A04 | Known pure `text-generation` model with no media inputs/outputs | **PASS (offline contract only)** | [Linux/Windows CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45206248356) · `pure_text_model_is_out_of_scope_without_unsafe_fallback` |
| A05 | Model has no pipeline tag but an explicit recognized media task in metadata tags | **PASS (offline contract only)** | [Linux/Windows CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45206248356) · `known_task_can_be_recovered_from_hub_tags` |
| A06 | MCP `media_tasks_list` returns read-only taxonomy | **PASS (offline contract only)** | [Linux/Windows CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45206248356) · `media_task_catalog_covers_all_modality_families_without_enabling_gpu` |
| A07 | MCP media search filters reject empty request, `approve=true` injection or invalid result count | **PASS (offline contract only)** | [Linux/Windows CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45206248356) · `hub_search_requires_query_or_task_and_never_accepts_approval_flag` |
| A08 | Live Hub search with `task` filter across image, video, audio, music, speech and multimodal; inspect real catalog results | **NOT RUN** | Pending matching test evidence |
| A09 | Live inspection and planning with model ID and immutable SHA, including gated models and unclassified media tags | **NOT RUN** | Pending matching test evidence |
| A10 | Verified media inference for each **claimed runtime family** and output type, including music, voice, video and 3D where supported | **NOT RUN** | Pending matching test evidence |

## 2c. Rust Runtime Resolver verification ledger (R01–R08)

These tests track the read-only runtime candidate matcher. No OCI/installer is executable yet and no GPU provider deployment has been implemented. Offline fixture verification does not establish a real model test.

| Test ID | Scenario requiring verification | Status | Evidence |
|---|---|---|---|
| R01 | Versioned Runtime Registry and OCI/installer candidates | **PASS (offline only)** | [CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45215723211); `bundled_registry_is_valid_and_contains_both_routes` |
| R02 | Library/task/checkpoint signals avoid false runtime matches | **PASS (offline only)** | [CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45215723211); `diffusers_uses_checkpoint_and_library_evidence_not_task_alone` |
| R03 | Audio, music, voice, video runtime families receive candidates | **PASS (offline only)** | [CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45215723211); `transformers_and_all_media_tasks_are_not_hardwired_to_images` |
| R04 | Unknown, gated, remote-code and invalid-revision models fail closed | **PASS (offline only)** | [CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45215723211); `unknown_and_text_only_models_fail_closed; gated_and_remote_code_are_explicitly_blocked; missing_model_revision_and_modified_registry_are_rejected` |
| R05 | Real verification evidence required; synthetic OCI preference test | **PASS (offline only)** | [CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45215723211); `fake_verified_label_never_passes_without_model_and_worker_evidence; synthetic_exact_evidence_prefers_pinned_image_over_installer; exact_model_upstream_recipe_can_match_a_new_media_architecture` |
| R06 | Read-only runtime CLI/MCP tools and injected approval rejection | **PASS (offline only)** | [CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45215723211); `runtime_registry_is_read_only_and_lists_both_delivery_routes; runtime_registry_cli_never_calls_cloud_providers` |
| R07 | Live Hub model metadata and runtime matching | **NOT RUN** | No matching execution evidence recorded |
| R08 | Real GPU runtime deployment, media result and resource cleanup | **NOT RUN** | No matching execution evidence recorded |

[Offline E2E integration CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45301466162) — 38 Rust unit and 14 Rust MCP process tests, Python HTTP worker tests (13 on Unix, 12 plus one Unix-only skip on Windows), and a non-root CPU-only Docker fixture roundtrip on Linux. Runtime/model/GPU claims are not derived from this evidence.

## 2d. Offline end-to-end execution test ledger (O01–O14)

**Fixture-only acceptance:** O01–O12 completed as offline tests on Linux and tested on other OSes where applicable; the pseudoterminal test is Unix-only. These exercises never use paid GPUs or real model weights. O04 verifies both rejection of redirected/non-interactive approval and a **positive pseudoterminal challenge** exercised on Linux; it does not prevent a privileged same-OS agent from emulating a terminal and is not production-level independent consent. O13–O14 are explicitly deferred for model/GPU/provider approval and cannot be marked PASS.

| Test ID | Scenario | Status | Evidence |
|---|---|---|---|
| O01 | OS CSPRNG random deployment nonce and SHA-256 integrity | **PASS (offline/mock only)** | [PR CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45300810727) — `crypto::tests::known_sha256_vectors; crypto::tests::two_random_nonces_are_distinct` |
| O02 | Nine local media family fixture workflows | **PASS (offline/mock only)** | [PR CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45300810727) — `offline::tests::all_nine_media_paths_have_real_verified_fixture_artifacts_and_auto_stop` |
| O03 | Reject missing, replayed and tampered approval | **PASS (offline/mock only)** | [PR CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45300810727) — `offline::tests::missing_or_replayed_approval_is_rejected; plan_tampering_and_cross_session_replay_are_denied` |
| O04 | Human terminal-only approval and fixture-only shortcut | **PASS (offline/mock only)** | [Unix TTY + CLI CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45301466162) — `test_interactive_tty_approval_consumes_a_single_mock_grant`; `piped_cli_cannot_issue_fake_human_approval` |
| O05 | Worker failure with owned GPU mock resource | **PASS (offline/mock only)** | [PR CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45300810727) — `offline::tests::failure_auto_stops_only_owned_mock_resource` |
| O06 | Owner-scoped stop with active-job deferral and no termination | **PASS (offline/mock only)** | [PR CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45300810727) — `offline::tests::ownership_is_mandatory_and_stop_never_terminates_resources; crash_window_reconciles_provider_owned_resource_by_idempotency` |
| O07 | Provider stop failure and safe retry | **PASS (offline/mock only)** | [PR CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45300810727) — `offline::tests::stop_failure_is_visible_and_reconcilable_without_second_stop` |
| O08 | Crash-window journal recovery | **PASS (offline/mock only)** | [PR CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45300810727) — `offline::tests::crash_window_reconciles_provider_owned_resource_by_idempotency` |
| O09 | Tampered artifact detection and safe paths | **PASS (offline/mock only)** | [PR CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45300810727) — `offline::tests::artifact_mutation_and_symlinks_fail_integrity_checks; worker HTTP artifact tests` |
| O10 | Provider API request/stop previews across all nine providers | **PASS (offline/mock only)** | [PR CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45300810727) — `provider_ops::tests::all_nine_providers_have_safe_offline_contracts; runpod_and_vast_use_official_stop_not_termination; reject_unknown_image_digest_shell_injection_and_invalid_disk` |
| O11 | Authenticated Python worker HTTP across nine fixture kinds | **PASS (offline/mock only)** | [PR CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45300810727) — `workers/tests/test_protocol.py: authenticated HTTP, all 9 fixtures, worker protocol checks` |
| O12 | Cross-language Rust-to-Python media worker transport | **PASS (offline/mock only)** | [PR CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45300810727) — `workers/tests/test_protocol.py::test_rust_mcp_bridge_calls_fixture_worker_over_real_loopback_http` |
| O13 | Actual Diffusers/Transformers inference from pinned Hugging Face revision on GPU | **NOT RUN (needs real GPU/provider authorization)** | Requires separate authorized paid-cloud/model inference test |
| O14 | Real cloud provider deployment, one-shot human approval and auto-stop/scale-to-zero | **NOT RUN (needs real GPU/provider authorization)** | Requires separate authorized paid-cloud/model inference test |

## 2e. Real provider and worker control-plane test ledger (C01–C08)

These are offline adapter/worker-contract tests only. Real GPUs and provider credentials have NOT been used.

| Test ID | Scenario | Status | Evidence |
|---|---|---|---|
| C01 | RunPod v2 create and response parsing | **PASS (offline contract only)** | [Cross-platform CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45304466027): runpod_and_vast_create_requests_have_exact_pinned_identity_and_private_worker_token; new_resource_id_is_parsed_from_actual_provider_response_shape |
| C02 | Vast selected offer and safe stop | **PASS (offline contract only)** | [Cross-platform CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45304466027): Vast PUT operation and selected-offer validation; provider stop operation contract |
| C03 | Provider HTTPS credential/endpoint validation | **PASS (offline contract only)** | [Cross-platform CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45304466027): allowlist_rejects_credentials_in_urls_and_alternate_hosts; secure_curl_config_keeps_token_off_argv |
| C04 | Exact plan and one-shot approval journal | **PASS (offline contract only)** | [Cross-platform CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45304466027): deployment_is_blocked_without_pinned_and_live_verified_runtime_evidence; journal_tampering_or_unknown_resources_refuse_any_mutation |
| C05 | Bridge ownership and active job/transfer stop gate | **PASS (offline contract only)** | [Cross-platform CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45304466027): stop_checks_exact_ownership_and_last_job_before_sending_requests |
| C06 | Provider-confirmed stop and uncertain state | **PASS (offline contract only)** | [Cross-platform CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45304466027): successful_stop_must_be_confirmed_by_provider_observation; an_accepted_stop_is_not_falsely_reported_as_confirmed |
| C07 | Authenticated real worker health and remote artifact integrity | **PASS (offline contract only)** | [Cross-platform CI](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45304466027): test_real_worker_health_requires_auth_and_identifies_exact_deployment; refuse_simulated_results_and_mismatched_or_unsafe_artifacts |
| C08 | Actual real provider model inference and auto-stop | **NOT RUN** | Requires separately approved provider credentials/GPU credits |

## 2f. GPU rental billing and lifecycle test ledger (B01–B08)

The billing estimates use user-provided illustrative prices, not live provider rates or paid cloud invoices. B08 must remain NOT RUN until separately authorized real cloud usage and billing reconciliation.

| Test ID | Scenario requiring verification | Status | Evidence |
|---|---|---|---|
| B01 | Per-second billing despite USD/hour display; immediate stop | **PASS (offline contract only)** | [CI run](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45306337305) — hourly_display_with_per_second_billing_is_prorated_and_stops_immediately |
| B02 | Full-hour minimum without waiting for the hour | **PASS (offline contract only)** | [CI run](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45306189793) — true_hourly_minimum_charges_full_hour_but_does_not_hold_gpu_running |
| B03 | Minute/day/millisecond/custom increments and minimums | **PASS (offline contract only)** | [CI run](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45306189793) — minute_day_and_custom_quanta_round_resource_lifetime |
| B04 | Prepaid, monthly, yearly and reserved compute contracts | **PASS (offline contract only)** | [CI run](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45306189793) — prepaid_monthly_and_yearly_cannot_be_costed_from_five_seconds |
| B05 | Budget estimate overrun blocks operator paid deployment | **PASS (offline contract only)** | [CI run](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45306189793) — budget_is_an_estimate_not_an_enforced_provider_limit; paid_deployment_requires_verified_billing_terms_within_budget |
| B06 | Missing/inconsistent/unverified billing terms are refused | **PASS (offline contract only)** | [CI run](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45306189793) — unknown_or_inconsistent_terms_are_not_guessed; paid_deployment_requires_verified_billing_terms_within_budget |
| B07 | Agent MCP/CLI billing policy and estimate remain read-only | **PASS (offline contract only)** | [CI run](https://github.com/RizkiMugiono/Cloud-Media-Bridge/actions/runs/45306189793) — billing_quote_mcp_estimates_five_seconds_without_provisioning_gpu; offline billing CLI examples in GitHub Actions |
| B08 | Real GPU invoice and provider-confirmed billing stop | **NOT RUN** | Requires separately authorized GPU/provider test |

## 3. Test level status

| Tier | Status | Reason |
|---|---|---|
| T0 — Rust build, lint, unit/schema/fuzz | PARTIAL | Build, format and lint passed. Schema and fuzz coverage pending |
| T1 — MCP, approval, mock provider and security contracts | PARTIAL | MCP H01 and H02 passed |
| T2 — Pipeline CPU fixtures, local storage and media handling | NOT RUN | No tests executed |
| T3 — Modal non-GPU live smoke | NOT RUN | Requires separate user authorization and accessible Modal workspace |
| T4 — Modal GPU inference through an approved deployment route | NOT RUN | No GPU resources used |
| T5 — Modal parallel, asynchronous, recovery, scale-to-zero | NOT RUN | No live executions |
| T6 — Other-provider API contract mocks | PASS (request-shape previews only) | Rust `provider_ops` offline tests cover all nine provider classes; these are NOT live API adapters |
| T7 — Other-provider real-cloud tests | NOT RUN | Every provider requires its own adapter, consent and real-cloud test evidence; development is parallel |

### Local execution on 2026-11-23

Working tree based on `6081519f676a7bc1d86bcadfbae8385bbb883614`. Windows build 26200 and Ubuntu 26.04.1 LTS under WSL, both x86_64, Rust 1.100.0. Windows used the GNU toolchain. Ubuntu used Zig 0.14.1 as the C linker. Each integration test launched the native executable through pipes. H01 checked the handshake, tool schema and tool result. H02 checked malformed JSON, initialization order, unknown method/tool, invalid arguments and recovery. No provider or runtime image participated.

~~~sh
cargo fmt --all -- --check
cargo clippy --locked --offline --workspace --all-targets -- -D warnings
cargo test --locked --offline
~~~

## 4. Required evidence for future results

For each executed scenario, record:

- Test ID and source specification requirement.
- Exact git commit SHA, operating system and runtime/toolchain versions.
- UTC execution time and **redacted** provider profile/workspace identifier when relevant.
- Command, setup and steps sufficient to reproduce the result.
- Expected behavior and observed actual behavior.
- PASS, FAIL, BLOCKED, SKIPPED or NOT RUN with justification.
- Sanitized logs, output/checksum references, failure issue and verified retest data.
- Remote resource IDs and confirmed lifecycle/cleanup status for authorized cloud tests.
- For performance: real cold/warm metrics with GPU type, model SHA and runtime image digest.

Never commit access tokens, token secrets, private account identifiers, signed URLs, personal prompts or user media to this public repository. Use synthetic fixtures and hashes where possible.

## 5. Current GO / NO-GO

- **GO:** offline architecture implementation and mocked local tests.
- **NO-GO:** claiming real inference, production readiness, verified security, scale-to-zero measurements or live provider compatibility.
- **NOT RUN:** any test that lacks a traceable execution record remains explicitly unverified.

This ledger must be updated **by whichever developer, maintainer or automated test runner actually performs the test**, with reproducible evidence. Do not fabricate PASS results.

## User-selected lifecycle follow-up

Historical PASS entries above refer to their linked commits and the old immediate-stop contract. The new user-selected policy and revised estimator have new offline assertions; status **NOT RUN at authoring time**. See the new pull request's actual CI results before promoting any claim. Live GPU, actual invoice, and independent shutdown guarantees remain NOT RUN.

## Consolidated planning follow-up

The advisory source branch reports 76 Rust tests and 15 Python tests across its local environments; those results do not apply to this consolidated commit. New CI evidence is required; no new tests are claimed as PASS at authoring time.

## Dynamic market follow-up

The consolidated preceding commit passed offline CI (run 45309916340). Dynamic market changes require their own CI result. Live credentialed catalog calls, actual quote accuracy, production authorization, inference and GPU lifecycle are NOT RUN at authoring time.

## Authorization and provider handoff follow-up

Dynamic market commit 91ca8c5 passed CI 45310844087; signed authorization commit f95a2d3 passed CI 45312407341. Follow-up provider handoff and real ephemeral signature integration require fresh CI. Live catalog/account, independent operator deployment and GPU operations remain NOT RUN.

## Recovery follow-up

Candidate runtime commit f7c4a6a passed CI 45313388510. Stop-intent/recovery commit b986229 passed CI 45314827449. Runtime packaging and account identity follow-ups require fresh CI. No live account credentials, GPU deployment or real inference were used by these tests.

## Worker job recovery follow-up

Account preflight commit 9cd0fed passed CI 45315074933; CPU-only container packaging commit 80e2ec5 passed CI 45315147002. New job recovery changes require independent CI. No paid GPU or live worker requests were made.
