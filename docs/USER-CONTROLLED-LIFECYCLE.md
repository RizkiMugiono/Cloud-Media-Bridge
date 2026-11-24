# User-controlled, agent-operated GPU workflow

The user decides, the agent recommends and operates, and the Rust bridge supplies planning and guarded execution mechanisms. The current release is read-only for real cloud operations; the provider REST/HTTPS clients and inference implementation are retained, not certified or enabled. The production release gate is deliberately closed even with the former environment opt-in.

## Plan and approval

Use `cloud review` / MCP `cloud_review` for a proposed manifest, then `cloud prepare` / `cloud_session_prepare` for a fresh exact plan snapshot. `cloud status` returns that same plan and digest. Inspect the immutable model revision, compatible runtime, provider/account/workspace, GPU, region, quantity and exact offer. Compare alternatives for the same model SHA/task, whole-allocation expected duration, real billing increments/minimums, full commitment liability when applicable, and ancillary charges. Prices, compatibility reasons and provider evidence are caller-supplied; no live market search or benchmark is implied. A lower estimated total is advice, not automatic selection.

Use a trusted user approval boundary independent of agent text or local shell privileges. Authenticate the user, bind the exact plan digest and unique request ID, recheck freshness and provider allocation, and consume the grant once before any actual provisioning. Changes require a fresh plan and approval. The current digest is an integrity identifier, not authentication; terminal-only consent is not independently trustworthy. Production approval and provider allocation are not yet release-verified.

## Lifecycle

Use `cloud advice` / MCP `lifecycle_advice` with the saved plan, a caller-supplied receipt (request_id, plan_sha256, resource_id, end_user_approved) and observed provider/account/workspace, resource_id, owner_request_id, verified_owned, shared, provider_state, active_jobs, active_transfers, elapsed_ms, exposure_micro_usd and supported_action. These are untrusted assertions and the output never authorizes execution. Verify with the provider and authenticated user before action.

| Trigger | Required observation |
|---|---|
| after_job | final_job_terminal |
| manual / persistent | user_requested_stop |
| idle_timeout / reuse | idle_ms reaches approved idle_timeout_ms |
| scheduled / billing_boundary | now_unix_ms reaches approved stop_at_unix_ms (UTC epoch ms; billing boundary must be externally calculated and verified) |
| budget_limit | approved time or exposure reached |
| session_end | session_ended |
| disconnect_timeout | disconnected_ms reaches approved disconnect_timeout_ms |
| error_trigger | terminal_error_reconciled |

Actions stop, suspend, scale_to_zero and terminate are distinct; destructive termination requires separate confirmation. Never substitute an action, stop shared/unowned resources, or act while job or transfer status is uncertain. At a time/cost limit before the approved trigger, advise escalation. No independently verified scheduler, watchdog, hard budget cap or real provider billing is implemented. Stop may leave storage/commitment fees. An already-paid hour does not automatically authorize reuse.

The preserved operator has non-destructive stop and manual retention contract code for RunPod/Vast only; the release gate disables live deploy/generate/stop. Other triggers/actions are **guidance only**, not actual enforcement. The sample manifest is illustrative and NOT EXECUTABLE. Do not send secrets in manifests or MCP calls.
