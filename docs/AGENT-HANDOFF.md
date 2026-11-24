# GPU plan handoff

The agent proposes an offer and alternatives. The end user selects the pinned model, exact provider/account/workspace, GPU, region, quantity, offer, runtime, cost and time limits, trigger and action. Validate every quote and runtime against the actual account before presenting a deployable choice. In the sample, all prices, compatibility statements and capability evidence are illustrative. A blank review list means supplied fields passed structural checks.

A five-second request at USD 1 per allocation-hour could cost 1,389 micro-USD for five metered seconds, or 1,000,000 micro-USD with a full-hour minimum. Include time from provisioning through confirmed stop. Compare whole-allocation costs and ancillary storage, egress, IP, taxes and ongoing commitments over a disclosed horizon. Calendar contracts have unknown per-job cost until reviewed separately. Supply full_commitment_micro_usd, commitment_period_and_renewal and billable_elapsed_ms for a fixed commitment. The comparison includes the full quoted liability and ancillary charges. A quoted budget is a planning limit. Investigate missing terms and resolve review items with the user.

## External approval and reconciliation

1. Run cloud review or cloud_review on the proposed manifest. Check the comparison, assumptions and review items. The selected offer stays as supplied. The agent can explain a cheaper compatible alternative and its expected duration.
2. Prepare a fresh snapshot and show its full contents. The executor must obtain human consent through a trusted UI, binding the account, complete saved plan digest and unique request ID. Authenticate that consent independently of agent text. At execution, recheck quote freshness, actual offer parameters, runtime evidence, budget and policy support. Atomically consume the single-use deployment grant before creation. A material change, redeployment or expired quote requires a new plan and fresh consent. The bridge's local digest is an integrity identifier, not proof of consent.
3. The executor persists the provider resource ID against the consumed request. Preserve existing and shared resources. For uncertain create, reconcile the provider inventory before considering another operation. For uncertain work or transfers, inspect the worker and provider before cleanup. Recheck ownership and observations before a mutation. If the client can disconnect, disclose the recovery controller or provider-side expiry and its tested limits.
4. For an existing allocation, cloud advice / lifecycle_advice takes an object with plan (verbatim from prepare), receipt (plan_sha256, request_id, resource_id, end_user_approved) and observed. These are caller assertions. The output is never a credential or proof of provider state. The executor must enforce expiry, prevent replay and keep the provider's confirmed final state and invoice evidence.

| Selected trigger | Evidence from executor |
|---|---|
| after_job | final_job_terminal after completion, failure or cancellation is reconciled |
| idle_timeout | idle_ms reaches idle_timeout_ms |
| scheduled | now_unix_ms reaches stop_at_unix_ms (UTC epoch milliseconds) |
| manual | authenticated user_requested_stop |
| reuse | bounded idle_timeout_ms between authorized jobs, within approved time and cost |

For every trigger, observed supplies provider, account, workspace, resource_id, owner_request_id, verified_owned, shared, provider_state, active_jobs, active_transfers, elapsed_ms, exposure_micro_usd and supported_action. Use authoritative data. A limit reached before the authorized trigger requires escalation. The executor must prevent additional work beyond approved limits. A stop decision requires zero active jobs and transfers. Use the exact approved action. Non-destructive stop can leave persistent storage charges. Scale-to-zero needs provider confirmation. Termination can remove data and requires separate destructive confirmation. Unknown state, ownership mismatch or unsupported action means reconcile or contact the user. No destructive fallback.
