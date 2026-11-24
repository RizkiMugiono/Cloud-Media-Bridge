# Provider verification and release boundary

Public source review on 2026-11-24 is not an authenticated account test, current user-specific offer, successful deployment or invoice.

| Mode | Official contract reviewed | Remaining |
|---|---|---|
| RunPod Pods | [Official SDK](https://github.com/runpod/runpod-python/blob/main/runpod/api/ctl_commands.py), [migration reference](https://github.com/runpod/runpod-plugins-official/blob/main/plugins/runpod/skills/runpod-migrate/reference/graphql-to-v2.md): v2 create, get, action stop, delete, catalog | Account, offer, worker ingress, actual stop and invoice |
| RunPod Serverless | Same SDK: v2 queue endpoint uses templateId, gpu pools/count, workers idleTimeout, scaling queueDelay | Queue worker template, account, inference and actual zero workers |
| Modal | [Server CLI](https://modal.com/docs/cli/latest/server), [public pricing](https://modal.com/pricing) | Workspace rates, authenticated ingress, exact model/runtime, deployment, actual zero containers |
| Lambda | [Cloud API](https://docs.lambda.com/api/cloud): instance-types and launch | Account, workspace, exact region/instance/image and lifecycle |
| Vast instances / serverless | Existing official links in config/live-sources.json; exact live schemas not independently checked this pass | Verify current official API contract and offer before any native request |
| External profile | [DigitalOcean GPU Droplets](https://docs.digitalocean.com/products/droplets/how-to/gpu/create/), [billing](https://docs.digitalocean.com/products/droplets/details/pricing/) and [power-off](https://docs.digitalocean.com/reference/doctl/reference/compute/droplet-action/power-off/) | Account, quote, actual deployment and explicitly approved lifecycle |

The RunPod v1 retirement was announced for 2026-11-15. The bridge retains v2 REST, not obsolete v1. Its separate read-only GraphQL account lookup remains documented in the official migration reference. Public docs confirm the stop endpoint and operation body. Do not send an image-only draft to a serverless endpoint create API; select and validate the exact queue template first.

## Minimal paid smoke-test sequence (not run)

1. Select one actual provider account/workspace and compatible small open-license media model with pinned revision. Review exact upstream license, GPU memory, dependency lock and runtime image digest. Prefer a prebuilt compatible upstream image; if unavailable build on CPU first.
2. Obtain live account-specific GPU offer, available region, metering, minimum, ancillary charges and cleanup terms. Specify exact maximum exposure and elapsed time. An estimate is not a hard provider spending cap.
3. Present the exact operation to the human through the agent/provider approval boundary. No create if approval, quote, compatibility, ownership or recovery plan is missing.
4. Dispatch via official provider tools, record exact ID and inspect actual allocation/health. Do not blindly retry unknown creation outcomes.
5. Generate one small artifact with real GPU inference. Retrieve it and check file type, nonempty data, SHA-256, model identity and GPU evidence. A fixture result is not inference evidence.
6. Apply only the chosen lifecycle, inspect actual final resource/worker state, and account for residual charges. If cleanup fails, report resource ID and native manual recovery instructions. Do not claim zero charges just from desired state.
7. Record sanitized evidence (provider and mode, model SHA, runtime digest, GPU, tool/version, actual result hash, final resource state, charge evidence, and separately approved limits) in a dated test report. Mark missing evidence NOT RUN. Test a serverless provider, an instance provider, and an external profile independently before broadening verified support.

No paid smoke test was run in this source-review pass. The native production gate stays closed. The preferred real execution path is the user's approved agent using the provider's current official native interface.
