# User-controlled, agent-operated GPU workflow

The end user decides; the agent recommends and operates; the Rust bridge validates and supplies mechanisms. Discovery, cost calculation and plan preparation never authorize spending. This is not a provider-independent autonomous cloud manager.

## Before approval

1. Inspect model ID, immutable revision, task, compatible runtime and required VRAM. Reject unverified runtime compatibility for real deployment.
2. Compare compatible offers using actual provider pricing, observed/estimated performance, startup and output transfer, region, GPU count and available memory. Prefer total workload cost within the user's quality and latency constraints, not cheapest displayed hourly price. Explain unknowns and alternatives; the current tools estimate caller-supplied quotes, not a live GPU marketplace.
3. Show the actual billing unit (ms, seconds, minutes, hours, days, months, years or custom/commitment), rounding/minimum, start/end events, additional storage/network/tax/commitment costs, estimated total exposure and approved compute budget. A quote is not an enforced spending cap.
4. Present supported lifecycle choices and explicitly distinguish unavailable mechanisms. Disclose what happens after success, failure, cancellation or lost agent connection; explain residual fees.
5. Prepare an exact plan, show it to the user, and obtain a fresh, human-terminal approval. Never treat an MCP message or a manifest boolean as approval. Any changed model, runtime, provider, offer, GPU, billing or lifecycle requires a new plan and approval. The current TTY guard is not independent from an agent with the same local OS privileges.

## Lifecycle options

| Choice | Mechanism status |
|---|---|
| immediate_after_job | Operator contract attempts owned-resource non-destructive stop after job/artifact handling; live unverified |
| manual_stop | Operator contract retains resource after job; requires ongoing-charge acknowledgment and explicit terminal stop; live unverified |
| idle_timeout, scheduled_stop, billing_boundary | Planned; require reliable timers and provider-specific billing/expiry |
| budget_limit | Planned; requires reliable usage measurement and a verified enforcement mechanism |
| session_end, disconnect_timeout, error_trigger | Planned; require durable state and independent recovery if the agent disappears |
| persistent, suspend, terminate, scale_to_zero | Planned as distinct policies; provider capabilities and destructive permissions must be independently verified |

Unsupported options fail closed rather than silently falling back to immediate or manual. A user may choose the existing manual mode for indefinite retention, acknowledging there is no hard expiry or enforced spending cap. Never treat stop permission as terminate/delete permission. Never stop shared, unrelated or unproven resources.

A manual mode manifest fragment is:

```json
{"lifecycle_policy":{"mode":"manual_stop","acknowledge_unbounded_idle_charges":true}}
```

An immediate mode fragment is:

```json
{"lifecycle_policy":{"mode":"immediate_after_job"}}
```

A paid deployment also requires nonempty decision_context fields: pricing_verified_at, gpu_selection_reason, alternatives_reviewed, allocation_scope, storage_network_and_commitment_terms, stop_billing_effect, expected_duration_and_maximum_exposure. They are caller-supplied disclosure text, not independent evidence. Do not put secrets in the journal. The existing operator only supports specific RunPod/Vast request shapes, not arbitrary GPU counts/regions; disclose actual allocation constraints and do not promise unsupported controls.

The bundled runtime registry currently blocks actual paid deployment. No verified production scheduler, hard budget enforcement, or independent watchdog is provided. Offline tests must not rent GPU resources.
