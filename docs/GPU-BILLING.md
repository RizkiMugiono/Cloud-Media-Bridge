# GPU billing increments, commitments and stop policy

**Decision:** the user chooses the lifecycle policy independently of the billing model. Immediate stop is a recommendation for short jobs; when explicitly selected, the bridge attempts to stop its own resource after the last job and artifact transfer. Manual stop instead retains the resource and may incur ongoing charges. The stop must be observed in the provider API; otherwise the journal must report uncertainty and continuing potential charges.

A provider's **displayed USD/hour price is not necessarily the actual billing increment**. The user/agent must distinguish quoted unit price, metering granularity, minimum charge, prepaid period, stopping effects, and unrelated fees. No rates are hardcoded as universal prices.

## Example: a five-second job

| Contract | What the estimator does | Post-job operation |
|---|---|---|
| USD/hour displayed, actually **per second** | Uses real billable duration rounded to whole seconds | Immediate stop recommended; user chooses |
| Per millisecond | Rounds by one millisecond | Immediate stop recommended; user chooses |
| Per minute | Uses an applicable minute minimum/increment | Immediate stop recommended; user chooses |
| **Full hour minimum** | A five-second resource lifetime may incur an entire hour | Still stop immediately, unless the user explicitly opted into a warm instance |
| Per day | Applies provider-specific day increment/minimum | Stop after the job; no automatic day-long idle |
| Per month/year/prepaid/reserved | Cannot derive the contractual obligation from five seconds of inference | Stop **compute** where supported; no promise of a refund or contract cancellation |
| Termination-only provider | Stop may not exist or a guest OS shutdown may continue charging | Report action required; never silently destroy/terminate |

**The five seconds of inference are not necessarily five seconds of billed GPU.** Cold start, loading model weights, downloading model assets, executing, transferring media and waiting for a verified stop may all extend billable wall-clock time. Storage, network, reserved GPU agreements and taxes are separate.

## Current provider-specific information (recheck at deployment)

| Provider | Known published behavior | Additional charges |
|---|---|---|
| Modal | Serverless CPU/GPU compute usage billed per second; idle workers should scale to zero when configured | Check volumes and other services |
| RunPod Pods | GPU compute charged per second, including pod lifetime rather than just model execution | Stopped pods still charge storage |
| RunPod Serverless | Per-second worker compute; cold-start and idle effects depend on worker configuration | Endpoint/volume charges may exist |
| Vast.ai Instances | GPU compute charged per second while in billable running state, even though quotes display USD/hour | Storage continues while stopped; host/network fees vary |
| Lambda Cloud on-demand | Advertised hourly usage billed in one-minute increments; compute billing continues until termination | Filesystems and other resources may have separate charges |
| Others | Vary by vendor, contract, instance and region | Do not infer from a generic USD/hour headline |

Official provider references: [Modal pricing](https://modal.com/pricing), [RunPod billing](https://github.com/runpod/docs/blob/main/accounts-billing/billing.mdx), [Vast.ai pricing](https://github.com/vast-ai/docs/blob/main/guides/instances/pricing.mdx), [Vast.ai billing](https://github.com/vast-ai/docs/blob/main/guides/reference/billing.mdx), [Lambda Cloud billing](https://docs.lambda.com/public-cloud/billing/). Provider policies change. Before actual deployment, read the user's selected offer and billing terms.

## Operator and agent workflow

1. **Read source terms.** Fetch the actual selected offer/plan, not just the nominal price. Identify `billing_model`, `billing_increment_ms`, `minimum_billable_ms`, refundable vs. sunk commitments, storage/network fees and stop/terminate actions.
2. **Estimate total resource lifetime.** Use expected provisioning + model load + job + transfer + shutdown-confirmation, not inference duration alone. Treat the result as a forecast; do not present it as a provider invoice.
3. **Present a quote and maximum approved compute exposure.** Use a new human approval per deployment. If the quote is missing, monthly/yearly terms are unclear or estimated exposure exceeds the budget, the automatic paid-deploy path must fail closed.
4. **Apply the approved lifecycle after the last job and artifact transfer.** If immediate stop was selected, stop where supported. Verify the provider state. If it is uncertain or unsupported, report remaining charges and require operator reconciliation. Never automatically delete persistent data or terminate committed resources.
5. **Reconcile actual provider billing.** Compare provider charges against the forecast and report discrepancies. A local MCP process cannot guarantee cleanup if the user machine or network disappears; provider-side TTL/watchdog or another independent controller is needed for stronger guarantees.

### Warm reuse is an explicit opt-in

Keeping a GPU alive is never inferred from an already-paid period. A user may select manual retention with explicit acknowledgment that charges can continue without a hard cap. A bounded reuse window, scheduled stop and budget enforcement require an independently reliable expiry mechanism and cost guard and are not executable yet.

## Read-only Rust billing tools

```bash
# Read billing policy (no provider API calls)
cargo run --locked -- billing

# Entirely illustrative offline examples; these are not provider quotes
cargo run --locked -- billing estimate examples/billing-per-second-5s.example.json
cargo run --locked -- billing estimate examples/billing-hour-minimum-5s.example.json
cargo run --locked -- billing estimate examples/billing-prepaid-monthly.example.json
```

MCP exposes `billing_policy` and `billing_estimate`. The quote schema for **usage metering** accepts:

```json
{
  "schema_version": 1,
  "billing_model": "per_second",
  "billing_increment_ms": 1000,
  "minimum_billable_ms": 0,
  "billable_elapsed_ms": 5000,
  "quoted_micro_usd_per_hour": 1000000,
  "max_approved_compute_micro_usd": 50000,
  "pricing_source": "ILLUSTRATION_NOT_A_REAL_PROVIDER_QUOTE",
  "provider": "example-provider"
}
```

USD is represented using integer **micro-USD** (1,000,000 micro-USD = USD 1), and durations in milliseconds. For metered quotes, the estimator computes `ceil(quoted_micro_usd_per_hour × max(minimum_billable_ms, round_up(elapsed_ms, increment_ms)) / 3,600,000)`, rounding the final amount upward to micro-USD.

For **calendar, annual, prepaid, reserved and unknown contracts**, the estimator intentionally returns `not_estimable_from_job_duration` instead of pretending the full contract can be inferred from a five-second task.

**Important limitations:** The billing estimator only calculates **illustrative compute cost** supplied by the caller; it does not fetch live market prices, impose an actual provider-side hard cap, predict unexpected failures, calculate persistent storage/network charges, or guarantee an invoice. The user owns the provider billing account; the bridge must still safely handle known faults and must not transfer unverified cost risk into an assertion of success.

See the [machine-readable billing policy](../config/billing-policy.json), [deployment policy](../config/deployment-policy.json), [provider capabilities](../config/providers.json) and [control-plane implementation](../crates/bridge-cli/src/cloud_operator.rs).
