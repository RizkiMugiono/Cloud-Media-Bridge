# Cloud-Media-Bridge — Testing and Verification Plan

**Repository:** https://github.com/RizkiMugiono/Cloud-Media-Bridge  
**Companion architecture:** [Cloud-Media-Bridge.md](Cloud-Media-Bridge.md)  
**Actual results:** [Cloud-Media-Bridge-TEST-RESULTS.md](Cloud-Media-Bridge-TEST-RESULTS.md)  
**Purpose:** Versioned test cases, reproducible procedures, evidence requirements and honest release gates. **This is a plan, not an assertion that any application tests have passed.**

> **Authorization and safety rule for every development agent and contributor:** Do not create Modal Apps, Servers, Functions or Volumes; allocate GPU/CPU cloud instances; modify harness settings; stop/delete resources; access private workspaces; or publish GitHub changes without the applicable explicit user authorization. Normal lint and unit tests must be fully offline or mocked. Any live provider test requires separate user authorization for that exact provider/resource; all provider adapters may be developed and contract-tested concurrently without paid GPU access.

## Prioritized v1 verification

The project deliberately keeps one small local Rust MCP/CLI process and **reuses existing inference runtimes**. The offline milestone must prove complete simulated deployment, approval controls, authenticated worker HTTP, artifact integrity, provider request contracts and owned-resource lifecycle without paid GPUs; actual provider inference is a separately authorized stage.

| Gate | Expected evidence | GPU needed? |
|---|---|---|
| V1-1 | MCP initializes, lists tools, handles invalid arguments without crashing | No |
| V1-2 | GPU provider catalog parses and reports truthful implemented/planned states | No |
| V1-3 | Search the live Hugging Face catalog without a manual list | No |
| V1-4 | Inspect a model URL and pin an immutable revision SHA; block malicious URLs | No |
| V1-5 | Prepare a read-only model/provider plan; deny deployment for an unsupported runtime | No |
| V1-6 | Reject model or resource execution without **separate human approvals** | No |
| V1-7 | A separately approved Modal workload generates real media, returns verified output and scales down | Yes, explicitly authorized |

**V1-1 through V1-5** are the initial development focus. The existing 82-case registry remains intact; ten additional decision-specific checks (U01–U10) cover the approved deployment behavior. Total: **110 scenarios** including ten all-media coverage checks (A01–A10) and eight Rust Runtime Resolver cases (R01–R08). Register a PASS only for a truly executed test; **the GitHub CI build cannot prove live Hugging Face access or GPU inference**.

**Cloud safety:** ordinary tests and GitHub Actions do not create resources and consume no GPU credits. Real provider execution requires fresh permission for every deployment and a compatibility-verified worker. The provider configuration catalog covers Modal, RunPod Serverless/Pods, Vast.ai Serverless/Instances, Lambda Cloud, HTTPS GPU workers, SSH GPU hosts and Kubernetes; all adapters are developed in parallel. Each provider requires independent authorization and evidence before live verification.

## 0. Test levels and scope

| Level | Tests | Real cloud used? | Required before |
|---|---|---|---|
| T0 | Rust build, formatting, lint, unit/schema/property/fuzz tests | No | Every relevant code change |
| T1 | MCP/harness, approval, provider contract mocks, security | No | Any cloud-enabled workflow |
| T2 | CPU-only pipeline fixtures, asset handling, SQLite migrations | No | Live provider testing |
| T3 | Modal read-only identity plus separately approved non-GPU deployment smoke test | Yes, explicit authorization | Real GPU test |
| T4 | Real inference through an approved Modal deployment route | Yes, explicit authorization | Claiming runnable GPU inference |
| T5 | Modal parallel jobs, asynchronous recovery, cancellation, cold start and scale-to-zero | Yes, explicit authorization | Claiming production-grade workflow |
| T6 | Contract mocks for RunPod, Vast.ai, Lambda and SSH | No | When a non-Modal adapter is introduced |
| T7 | Live provider integration for RunPod, Vast.ai, Lambda, SSH/HTTPS, Kubernetes | **Yes, explicit authorization per provider** | After its own adapter and offline tests exist; not part of default CI |

Default `cargo test` must never allocate GPUs or make cloud changes. Live tests must be marked `#[ignore]`, behind a separate `live-modal` feature or executable, and require an interactive human confirmation tied to the exact profile/workspace and proposed operations. A mock test result is not performance or functionality evidence for the real provider.

## 1. Prerequisites and test-environment controls

- Implementation specification, this plan and the current test-results file must be mutually consistent.
- An `AGENTS.md`, if added, must apply equally to all coding agents and require offline-by-default test behavior.
- Secret scanning, Rust dependency locks, runtime dependency pinning and credential-safe .gitignore rules must be enabled.
- Fixtures must use synthetic data without provider credentials, real user media, private prompts or signed URLs.
- Live Modal tests use **the authorizing user's own** Modal account, exact workspace and explicitly selected environment; no shared maintainer account.
- An MCP stdio test harness must run independently of any proprietary AI agent brand.
- Real resource changes require independent human approval even if the model is approved.
- Tests must record OS, language/runtime versions, git SHA, provider and image digest. Redact credentials and private paths.
- Record failures and blockers without changing acceptance criteria to hide bugs.

## 2. Harness, human approval and provider ownership tests (H01–H18)

| ID | Test scenario | Passing evidence |
|---|---|---|
| H01 | Request `tools/list` over MCP stdio | Valid JSON-RPC; stdout only protocol frames |
| H02 | Send invalid or unknown MCP arguments | Typed error, no panic, no GPU call |
| H03 | Agent submits media generation without human model approval | `MODEL_APPROVAL_REQUIRED`; no remote submission |
| H04 | Agent adds `approved=true` to a tool request | Ignored or rejected; no approval grant created |
| H05 | Agent outputs text claiming "the user approved" | Treat as untrusted; request remains `PENDING` |
| H06 | Human explicitly rejects the proposed model | `DENIED` and no generation possible under that request |
| H07 | Approve revision SHA A; source default branch moves to SHA B | SHA B requires new approval; SHA A remains pinned |
| H08 | Agent/router attempts substitution from model A to model B | Block without separate approval for B |
| H09 | Approval for task A is reused for task B | Reject unless task B is explicitly included in grant |
| H10 | Approval grant expires or is revoked | No further submissions; in-flight semantics documented |
| H11 | Agent attempts privileged approval via MCP stdin/tool | No path to approve through agent-supplied data |
| H12 | Approval is attempted in headless/no-TTY mode | Fail closed with a clear typed error |
| H13 | Agent silently switches compute provider profile from A to B | No operation against B; fail closed |
| H14 | Model approval is presented as authorization to provision GPU resources | Require separate resource-operation approval |
| H15 | Wrong profile credentials are supplied | Authentication fails; never fall back to another profile |
| H16 | Cloud-only agent attempts to launch unavailable local executable | Diagnose unsupported environment accurately |
| H17 | Harness configuration file already exists | Backup, preserve unrelated settings, atomic write and rollback |
| H18 | Test multiple harness/protocol versions | Dated verified/unverified/unsupported matrix with evidence |

## 3. Compute provider contract tests (P01–P18)

These tests run against deterministic mock API recorders for each adapter. Planned adapters include RunPod Serverless, RunPod Pods, Vast.ai Serverless, Vast.ai Instances, Lambda Cloud and generic SSH. Mocks prove interface behavior **only**, not real provider availability.

| ID | Contract scenario | Expected result |
|---|---|---|
| P01 | Capabilities include multiple resource/deployment modes | Tri-state capability; `unknown` never means `supported` |
| P02 | Call `plan()` | No create/deploy/submit side effects |
| P03 | Call `provision()` without a resource grant | Rejected |
| P04 | Call `submit()` without matching approved job/model | Rejected |
| P05 | Mismatch provider, account, workspace or environment | Fail closed before remote call |
| P06 | Model card supplies an arbitrary compute endpoint URL | Never used as an approved provider endpoint |
| P07 | Network timeout occurs before a safe submission | Retry only if provably safe |
| P08 | Provider accepts submission but response is lost | Reconcile; do not create duplicate job |
| P09 | Provider does not support cancellation | Report `unsupported` or `uncertain`; never fake cancellation |
| P10 | Provider API responds with 429, 503 or 5xx | Bounded backoff, jitter, deadline and classified error |
| P11 | Execute `cleanup --dry-run` | No destructive call and complete owned-resource preview |
| P12 | Cleanup targets resource owned by another application | Rejected |
| P13 | GPU pod does not support automatic scale-to-zero | Report manual stop, terminate required or unknown truthfully |
| P14 | Adapter exists as an unimplemented skeleton | `PROVIDER_NOT_IMPLEMENTED`, not false success |
| P15 | Provider authentication fails | No credential leakage in logs/errors |
| P16 | Requested GPU is unavailable | Typed error; no silent switch to another provider |
| P17 | Provider API schema changes incompatibly | Explicit compatibility failure; no unsafe default |
| P18 | Two independent installations/users share a provider class | No shared secrets, artifact namespace or state leakage |

## 4. Model discovery, inference compatibility and supply-chain tests (M01–M16)

| ID | Scenario | Expected result |
|---|---|---|
| M01 | Valid Hugging Face model URL | Canonical model ID plus immutable revision SHA |
| M02 | HTTP, localhost, private or link-local target | SSRF blocked |
| M03 | Redirect from allowed source to unsafe target | Blocked with no credential leakage |
| M04 | Encoded traversal, symlink or dangerous file path | Rejected |
| M05 | Model card contains malicious prompt-injection commands | Treated as metadata; no agent/shell instructions executed |
| M06 | Gated or private model | Access restriction honored; optional user-owned source token |
| M07 | Checkpoint contains pickle-only weights | No automatic unsafe load |
| M08 | Runtime would require `trust_remote_code=True` | Denied by default |
| M09 | Model tag claims image-generation support | Not marked `verified` without real execution evidence |
| M10 | Architecture matches but image digest/checkpoint is incompatible | Never treated as verified |
| M11 | Discovered model has no compatible container or pipeline | Model remains discoverable; planner reports unsupported and performs no deployment |
| M12 | New Hub entry matches an existing pipeline or serving image | Search finds it without a repo update; plan binds the selected route and exact SHA without deployment |
| M13 | Router attempts silent compute-provider switch | Block without explicit user choice and authorization |
| M14 | Model license/gating requires acceptance | Restricted status, no automatic bypass |
| M15 | Runtime manifest is invalid or tampered | Rejected with traceable error |
| M16 | Model memory requirement exceeds GPU capability | Planner blocks before model loading |

## 5. Media, SQLite, cache and artifact tests (D01–D15)

| ID | Scenario | Expected result |
|---|---|---|
| D01 | Small valid media input | Correct MIME, byte count and SHA-256 |
| D02 | Large video submitted inline as MCP JSON | Use authenticated file reference/staging instead of huge base64 |
| D03 | Spoofed MIME, corrupt codec or decompression bomb | Safely rejected |
| D04 | Input path traversal or symlink escapes authorized scope | Rejected |
| D05 | Upload interrupted midway | Safe resumable flow if supported; otherwise clean retry |
| D06 | Two workers download the same checkpoint concurrently | Valid final cache and verified hash, no corruption |
| D07 | Incomplete cache file survives worker crash | Never treated as verified weights |
| D08 | Volume commit/reload race | Correct visibility semantics and preserved integrity |
| D09 | Two jobs write outputs concurrently | Unique paths; no cross-job overwrite |
| D10 | Output is truncated or checksum mismatched | `ARTIFACT_HASH_MISMATCH`, never `COMPLETED` |
| D11 | Remote artifact is expired | Explicit expiration and actionable recovery/error |
| D12 | SQLite process crashes and restarts | Valid WAL, migrations and recovery |
| D13 | User revokes approval while job is running | No new submissions; consistent in-flight behavior |
| D14 | Source token might appear in logs/trace | Plaintext secret is always redacted |
| D15 | User requests cleanup of data/resources | Preview and confirmation; no unrelated deletion |

## 6. Job concurrency, asynchronous execution and shutdown tests (J01–J15)

| ID | Scenario | Expected result |
|---|---|---|
| J01 | Short synchronous generation | Correct terminal state and validated artifact |
| J02 | Scale-to-zero worker receives new request | Cold-start recovery without duplicate submission |
| J03 | Remote accepts job, then HTTP response times out | Idempotent reconciliation |
| J04 | Bridge/harness stops while video job runs | Durable remote handle; restart/poll/fetch succeeds where advertised |
| J05 | User cancels running async job | Truthful `requested`/`confirmed`/`uncertain` result |
| J06 | User cancels a completed job | Completed result remains stable, not accidentally deleted |
| J07 | Ten jobs use different models/runtimes | Safe parallelism; each uses exact approved SHA |
| J08 | Ten jobs use the same model | VRAM-aware batching/scaling, no uncontrolled OOM |
| J09 | Model allows only one request per worker | Enforce per-worker concurrency of one; replicas may scale |
| J10 | GPU unavailable or workspace capacity exhausted | Typed error; no silent provider substitution |
| J11 | Model initialization exceeds typical startup time | Correct timeout/lifecycle handling; no premature success |
| J12 | Remote result expires before download | Report retention/expiry; local index remains truthful |
| J13 | GPU pod/VM post-job lifecycle differs from serverless | For bridge-owned resources, verify supported stop after final job; otherwise explicitly report unsupported status, without unapproved termination |
| J14 | Inference fails in the middle of output streaming | Failed job with safe artifact cleanup and redacted trace |
| J15 | Remote completes but local state write crashes | Restart reconciliation restores correct terminal state |

## 7. Modal reference live testing (T3–T5)

### Mandatory explicit preconditions

1. Human approves exact Modal account/profile/workspace/environment, model URL + immutable SHA, selected runtime/image digest, resource operation, and GPU workload.
2. Credentials are already configured locally through official Modal authentication. Never pass tokens via commits, pull requests, issues or chat.
3. No cloud deployment occurs from `git clone`, package installation, `cargo test`, or normal GitHub Actions.
4. A dedicated live-test command requires an interactive human confirmation and checks provider capability, model approval and resource scope before execution.
5. Test resources use an isolated namespace such as `cmb-test-<installation-id>-<run-id>`. Cleanup affects only owned resources and requires explicit approval. Remaining resources must be listed in the results report.
6. The user's small available credit is a **testing consideration**, not a repository-enforced financial quota. Run T0–T2 offline before any T3–T5 cloud action.

### T3 — Non-GPU Modal smoke test

- [ ] Confirm Modal CLI version, account identity, workspace and environment.
- [ ] Verify `plan` remains read-only.
- [ ] Verify private/authenticated worker rejects missing or incorrect credentials.
- [ ] Observe idle-to-zero where supported, and correct cold-start retry without duplicate jobs.
- [ ] Confirm secret redaction and cleanup dry-run.

### T4 — Real inference through an approved Modal deployment route

- [ ] Record the selected container or library pipeline, model SHA, image digest, user workspace and approved GPU operation.
- [ ] Generate media with the selected model. Record GPU type, timings and sanitized job references.
- [ ] Download the result, inspect it and independently verify its checksum.
- [ ] Keep provisioning credentials outside the inference worker. Each additional route needs its own evidence before being claimed as verified.

### T5 — Async, parallel, cancellation and scale-down

- [ ] Demonstrate provider-backed remote asynchronous handle recovery after local bridge restart.
- [ ] Run concurrent workloads within measured GPU/model safety and capture per-worker concurrency.
- [ ] Observe idle worker count returning to zero for supported Modal resource modes.
- [ ] Confirm cached model data remains valid if designed to survive scale-down.
- [ ] Repeat after a full cold idle and compare end-to-end timing.

**Video and music:** mark them verified only after real category-specific models produce media, recover correctly and pass task-appropriate security tests. Image inference success is not evidence of video/music compatibility.

### Example commands (future target, not assumed implemented)

~~~bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
# Only after a real test implementation AND explicit human authorization:
cargo test -p bridge-provider-modal --features live-modal -- --ignored --nocapture
~~~

## 8. Performance measurement requirements

Compare **cold** start from zero active workers and **warm** execution with the same model, revision, runtime image and GPU.

Capture: model discovery; plan; submission; resource provision and cold start; download/cache; model load; GPU inference; encoding; output staging; download; total.

Record all relevant conditions: GPU type/region, OCI digest, model SHA, resolution, diffusion steps/seed, media input size, cache cold/warm, safe concurrency, client OS/harness version, timeouts and provider errors.

Report sample count, median/p50, p95 only where sample size supports it, min/max, failure rate, throughput and job-completion ratio. Do not compare unrelated model/hardware runs or fabricate latency from mocks. Optimize the measured bottleneck, not the appearance of benchmarks.

## 9. Required compatibility evidence matrices

~~~text
HARNESS
client | version | OS | stdio discovery | model inspection | approval request |
generation (if run) | status | cleanup | verified/unverified status

PROVIDER
provider | mode(serverless/pod/vm/ssh) | API version | account verified |
capabilities | provision | async | cancel | artifacts | cleanup |
scale-to-zero mode | live-tested? | evidence reference

MODEL / RUNTIME
source + model SHA | license | task | deployment route | image digest |
GPU profile | tested parameters | output hash | measured latency |
verified/unverified/unsupported
~~~

Only providers actually exercised in an independently authorized real-cloud integration test may be labeled **live-tested**. Parallel development is not proof of live compatibility. Non-Modal adapters remain `planned` or `contract-tested` until separately verified.

## 10. Additional security testing (mandatory)

- Fuzz model URL parsers, provider schemas, metadata JSON, SQLite migrations, resource IDs, paths, MIME and archive/media headers.
- Perform Rust/Python dependency advisory and license checks; pin OCI/image dependencies and review supply-chain risks.
- Exercise model-card prompt injection, arbitrary Python code, unsafe pickle, shell argument injection and error-log exfiltration.
- Test the local privilege boundary honestly: an agent with arbitrary shell rights under the same OS user may bypass poorly protected local approval files.
- Verify no default maintainer telemetry, no public endpoint without authentication and no external connections beyond authorized providers/sources.
- Audit source archives, test logs and release artifacts for credentials, signed URLs and private user media.
- Confirm resource cleanup never touches pre-existing or unrelated cloud resources.

## 11. Release gates: GO / NO-GO

**GO for offline implementation:** contracts and schema exist; all relevant T0/T1 tests pass; no embedded secrets or automatic cloud calls; approval state machine is enforced.

**GO for Modal live smoke test:** separately approved operation, verified exact workspace, immutable model SHA, authenticated worker endpoint and tested cleanup preview.

**GO to state "Modal image generation works":** the claimed deployment route has real output artifacts, verified hashes, recorded model SHA/runtime digest/GPU profile, and passing relevant error/retry tests.

**GO to state "video/music async works":** actual media-specific generation, durable remote job handle across bridge restarts and tested cancellation/retention semantics.

**GO to state "multiple GPU providers work":** only after authenticated, real-workload and lifecycle/cleanup testing for **each claimed provider**. Interface mocks are not enough.

**NO-GO for public release** if an agent can self-approve; switch model/provider/accounts without human selection; expose an unauthenticated GPU endpoint; misrepresent model verification; duplicate cloud jobs on retry; lose or corrupt user media; write credentials to logs; provision cloud resources automatically; or claim all GPU pods/VMs scale-to-zero.

## 12. Evidence bundle layout

~~~text
evidence/
  date-and-commit.txt
  contracts-results.md
  security-results.md
  compatibility-matrix.md
  performance-modal.md
  modal-real-inference.md
  scale-to-zero-modal.md
  async-recovery.md
  resources-cleanup.md
  known-limitations.md
~~~

Evidence must include test ID, commit SHA, build/runtime version, time, reproduction command/steps, expected result, actual result, PASS/FAIL/NOT RUN/SKIPPED/BLOCKED, sanitized logs/artifact hashes and issue reference when failing.

Never automatically publish real customer/user media, prompts, signed URLs or credentials to this public repository. Use synthetic fixtures and checksums where possible.

## 13. Vendor-neutral testing work order

1. Read the architecture, this test plan and the current test-results ledger; map requirement IDs to code files and test cases.
2. Implement offline T0–T2 first with mock provider recorders and no real GPU or credential use.
3. Develop the Modal, RunPod, Vast.ai, Lambda, SSH/HTTPS and Kubernetes adapter workstreams in parallel, reusing common media/runtime contracts; test each provider's own identity, lifecycle and cleanup semantics.
4. Run T3–T5 on Modal only after explicit authorization; independently run comparable T7 integration tests on other provider adapters **only with their own** explicit resource/workload authorization, never from standard CI.
5. For failures, capture redacted reproduction evidence, fix the bug, rerun and retain both failure and successful retest history.
6. Publish concise, evidence-based GO/NO-GO results. Unexecuted and unsupported cases remain explicitly marked `NOT RUN` or `BLOCKED`; never report imagined success.

## 14. User-approved deployment decisions: acceptance tests (U01–U10)

These scenarios bind the four user decisions to inspectable evidence. **Static policy/schema checks do not prove actual human approval, runtime installation or GPU cleanup.** No cloud resource or paid GPU may be used for U01–U03.

| ID | Scenario | Expected evidence |
|---|---|---|
| U01 | Bundled deployment policy declares both a verified prebuilt OCI image and a vetted base-image runtime recipe | Both routes appear as compatibility-based alternatives; unsafe model-repository scripts forbidden; offline unit test |
| U02 | Each configured provider is in the same parallel development matrix | All nine provider IDs match the policy, no false `deploy`/`execute` capability; offline unit/catalog tests |
| U03 | CLI/MCP exposes the policy without provisioning or allowing approval injection | Read-only policy tool and explicit error for `approve=true`; offline process-level tests |
| U04 | Identical deployment requested twice, or reconfigure/redeploy after approval | A fresh human-approved nonce is required each time; consumed grants never reused; provider not called if missing |
| U05 | Model family matches prebuilt image versus vetted installation recipe | Both deployment routes validated against exact model SHA, library versions and VRAM; unsafe recipe rejected |
| U06 | Two in-flight jobs share one bridge-owned GPU pod | Pod is not stopped until both jobs and artifact transfers finish; stop occurs once per eligible idle transition |
| U07 | Owned pod/VM job succeeds, fails or is cancelled | Supported provider-native stop is requested and provider confirmation recorded for each terminal case |
| U08 | Resource is preexisting, external, shared, wrong workspace or ownership unknown | No stop, terminate or deletion request is issued; ownership/scope failure is explicit |
| U09 | Provider exposes terminate only, stop fails or stop response is ambiguous | Report `STOP_UNSUPPORTED_ACTION_REQUIRED`/`STOP_UNCERTAIN`, never silently destroy, and warn about possible charges |
| U10 | Bridge crashes after job completion but before stop confirmation | Durable owned-resource record survives restart; reconciliation and safe stop retry produce truthful final state |

**Execution gate for any provider:** U04–U10 must pass offline/contract tests before the adapter can be treated as production-deployable; real provider tests separately verify the supported stop method, concurrency and billing lifecycle. Where no non-destructive stop exists, the adapter must report that and request separately authorized cleanup rather than claiming automatic stop is available.

## 15. Complete AI-media model coverage tests (A01–A10)

The project must discover and classify **all media-capable Hugging Face models**, including those with new and unknown pipeline tags. Model discoverability is not a guarantee that the model can load or generate media on any GPU. **No GPU, provider credentials or paid compute may be used for A01–A07.** Live-Hub tests A08–A09 are HTTP-only; A10 requires separately approved GPU resources and verified runtime/worker integration.

| ID | Scenario | Expected evidence |
|---|---|---|
| A01 | Read media catalog and classify representative image, video, audio, speech, music, 3D, spatial, document and multimodal tasks | All nine families and representative pipeline tags present; Rust offline taxonomy test |
| A02 | Broad `text-to-audio` model contains Hub `music` tag | Both `audio` and `music` categories appear, without claiming runtime compatibility; Rust unit test |
| A03 | New future media task or missing/unknown `pipeline_tag` | Preserve it for discovery; return inferred/unclassified review state instead of rejecting or claiming supported runtime |
| A04 | Known pure `text-generation` model with no media inputs/outputs | Mark non-media, do not change the model or auto-host an LLM |
| A05 | Model has no pipeline tag but an explicit recognized media task in metadata tags | Recover task and classify it as metadata-derived, without marking runnable |
| A06 | MCP `media_tasks_list` returns read-only taxonomy | List all nine families; any injected `deploy` argument is rejected |
| A07 | MCP media search filters reject empty request, `approve=true` injection or invalid result count | No cloud request or state change; invalid arguments receive typed error |
| A08 | Live Hub search with `task` filter across image, video, audio, music, speech and multimodal; inspect real catalog results | Real search metadata includes source task and conservative classification; unknown models remain visible; independent HTTP evidence |
| A09 | Live inspection and planning with model ID and immutable SHA, including gated models and unclassified media tags | Access status and revision accurate; plan remains read-only with `safe_to_deploy=false`; no GPU or cost |
| A10 | Verified media inference for each **claimed runtime family** and output type, including music, voice, video and 3D where supported | Real provider-backed output bytes, MIME/checksum, model SHA, runtime image/recipe, consent and cleanup evidence; unsupported families never called verified |

Do **not** mark A08–A10 as PASS from the offline catalog, a mock result or a model card. Unknown model architectures require a verified compatible runtime recipe; successful image inference is not evidence of speech, music, video or 3D inference support.

## 16. Evidence-based Rust Runtime Resolver (R01–R08)

These tests cover **runtime compatibility candidates**, not AI inference or a working GPU provider. R01–R06 are offline and must require no cloud credentials, paid GPU or external model downloads. R07 needs Hugging Face Hub HTTP access only; R08 needs an independently approved real GPU deployment.

| ID | Scenario | Expected evidence |
|---|---|---|
| R01 | Runtime Registry contains both OCI and vetted-installer alternatives with upstream provenance | Unique IDs, valid HTTPS upstreams, non-executable routes for Diffusers, Transformers, ComfyUI, AudioCraft, pyannote, SpeechBrain and specialized models |
| R02 | Hub model task matches but lacks library/checkpoint/architecture signals | Do not assert any model loadability; Diffusers task + file/library generates candidates without fake verification |
| R03 | Resolve video, speech, audio/music and non-image media candidates | Deterministic candidate matching across multiple families; no image-only runtime gate |
| R04 | Model has unknown architecture, invalid SHA, gating, private access or remote-code mapping | Unknown model is preserved for review, invalid revision rejected; gated and untrusted code flagged without automatic execution |
| R05 | Bundled candidate claims `verified` without evidence; synthetic OCI/installer proof; reviewed exact-model runtime for a new architecture | False verification rejected, reviewed model ID matches only the allowlisted model and never pure text-only hosting, verified OCI preferred when equally proven; deployment disabled |
| R06 | MCP and local CLI list runtime registry; agent injects `approved=true` or omits required resolution parameters | Read-only tools list source candidates, blocked state-changing/invalid tool arguments and no provider API calls |
| R07 | Inspect real Hub models with task, library, model SHA, architectures, safe file signals and runtime hints | Read-only HTTP evidence, correct matching, no untrusted scripts or model binaries executed |
| R08 | Deploy a verified runtime to each claimed provider and model family using both approved delivery routes where available | Real output artifacts and checksum, model/worker/image evidence, separate deployment approval, owned resource stop/scale-to-zero and cleanup proof |

A `verified` status from a test fixture is **not** production evidence. Provider adapters and human approval are still unimplemented; R08 stays NOT RUN until separately authorized end-to-end inference and resource lifecycle checks exist.

## 17. Offline deployment, real HTTP worker protocol and provider contracts (O01–O14)

These checks implement the user's requirement to finish **every test that does not require GPU credits**. All use local synthetic fixtures, mock provider resources or read-only vendor request previews. Fixture media is never represented as real generated AI output. **O13 and O14 require real GPU/provider authorization and must not be marked PASS by offline CI.**

| ID | Scenario | Required evidence | Requires GPU credit |
|---|---|---|---|
| O01 | OS CSPRNG random deployment nonce and SHA-256 integrity | SHA-256 known vectors, distinct cryptographic nonces on Linux, Windows, macOS | No |
| O02 | Nine local media family fixture workflows | Mock resource created, deterministic non-AI fixture artifact, SHA-256 verification and auto-stop observed for all nine families | No |
| O03 | Reject missing, replayed and tampered approval | Rust one-shot approval tests plus missing/malformed MCP args; no resource provisioned when invalid | No |
| O04 | Human terminal-only approval and fixture-only shortcut | CLI input redirected from noninteractive agent is refused; fake MCP approval impossible; test grant explicitly labeled | No |
| O05 | Worker failure with owned GPU mock resource | Job marks failure, mock stop occurs and state reflects provider observation | No |
| O06 | Owner-scoped stop with active-job deferral and no termination | Wrong resource owner refused; stop waits until job count zero; mock provider emits non-destructive stop | No |
| O07 | Provider stop failure and safe retry | STOP_FAILED surfaced, reconcile retries and confirms stop, idempotent repeated stop | No |
| O08 | Crash-window journal recovery | Lost local resource ID recovered from mock provider by request ID and owned resource reconciled | No |
| O09 | Tampered artifact detection and safe paths | SHA-256/size mismatch, missing/unsafe asset identifier, mutation rejected | No |
| O10 | Provider API request/stop previews across all nine providers | OCI digest and GPU input validation, no network/credentials, stop versus termination semantics, Lambda blocked where stop unsupported | No |
| O11 | Authenticated Python worker HTTP across nine fixture kinds | Loopback HTTP health/auth, upload, generate, download, SHA-256 and correct media/JSON fixture labeling | No |
| O12 | Cross-language Rust-to-Python media worker transport | Rust CLI uses authenticated loopback HTTP to retrieve WAV fixture and independently SHA-256 verifies bytes | No |
| O13 | Actual Diffusers/Transformers inference from pinned Hugging Face revision on GPU | Real model loaded, model license/VRAM validated, actual output file/hash, worker image digest and permission | **Yes — deferred** |
| O14 | Real cloud provider deployment, one-shot human approval and auto-stop/scale-to-zero | Provider-confirmed provision/job/output/stop, account ownership, failure recovery and billing exposure for **each** claimed provider | **Yes — deferred** |

Release claim boundary: **O01–O12 PASS is an offline end-to-end mock + worker protocol milestone, not proof of O13–O14.** The bridge does not become production-ready from fixture tests alone.

## 18. Provider adapter and worker contract tests (C01–C08)

These cases assess Rust provider request construction, journal and approval checks, and worker protocol. C01–C07 are offline mocked tests. C08 requires separately authorized actual GPU resources and remains NOT RUN until independently verified.

| ID | Scenario | Expected evidence | GPU credits? |
|---|---|---|---|
| C01 | RunPod Pods REST v2 pinned image operation and actual ID parsing | Correct URL/method/model SHA/pinned image and parsed response | No |
| C02 | Vast.ai selected offer and non-destructive stop | Offer-specific PUT request, safe resource identity, no destructive termination | No |
| C03 | Provider HTTPS credential isolation | API token in stdin config, strict allowlist, no argv secrets/redirects | No |
| C04 | One-shot exact-plan deployment journal | Valid SHA/journal, TTY-only approval, no deployment with unverified runtime | No |
| C05 | Stop ownership, concurrent job and transfer checks | No stop when ownership wrong or active work exists | No |
| C06 | Stop status reconciliation | Confirmed provider observation versus uncertain stop/error and persisted state | No |
| C07 | Authenticated remote worker and artifact integrity | Model SHA/deployment ID in authenticated health, verified bytes/size/checksum | No |
| C08 | End-to-end real provider and model inference | Actual output/receipt, verified runtime image, user approval, provider-confirmed stop, GPU billing | Yes — deferred |

Existing nine-provider previews are not operational integrations. Even if C01–C07 pass, real GPU inference and automatic stopping remain unverified until C08.
