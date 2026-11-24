//! Read-only GPU rental billing estimator. No provider API actions.
//! An hourly displayed price does not imply a one-hour billing minimum.
use serde_json::{json, Value};

const POLICY: &str = include_str!("../../../config/billing-policy.json");
const MS_PER_HOUR: u128 = 3_600_000;

pub(crate) fn policy() -> Result<Value, String> {
    let data: Value = serde_json::from_str(POLICY)
        .map_err(|_| "Bundled GPU billing policy is invalid".to_string())?;
    if data["schema_version"] != 1
        || data["default_lifecycle"]["after_final_job"]
            != "ask_user_to_choose_stop_policy"
    {
        return Err("Bundled billing policy violates immediate-stop contract".into());
    }
    Ok(data)
}

fn number(v: &Value, key: &str) -> Result<u64, String> {
    v.get(key).and_then(Value::as_u64)
        .ok_or_else(|| format!("Billing quote requires nonnegative integer field {key}"))
}

fn ceil_div(n: u128, d: u128) -> Result<u128, String> {
    if d == 0 {
        return Err("Billing increment must be nonzero".into());
    }
    Ok(n / d + u128::from(!n.is_multiple_of(d)))
}

/// A caller-provided quote is not independently verified. Billable elapsed
/// duration includes provisioning, loading, job time and stop confirmation.
pub(crate) fn estimate(quote: &Value) -> Result<Value, String> {
    if !quote.is_object() || quote["schema_version"] != 1 {
        return Err("Billing quote must be a schema_version=1 object".into());
    }
    let mode = quote["billing_model"].as_str().ok_or("Missing billing_model")?;
    let mut result = json!({
        "source":"user_supplied_terms_not_provider_invoice",
        "billing_model":mode,
        "stop_policy_requires_user_choice":true,
        "wait_until_paid_period_ends":false,
        "storage_and_network_charges_not_included":true,
        "invoice_reconciliation_required":true,
        "note":"Billable time includes cold start, model load, media transfer, idle and stop confirmation; not only the inference duration."
    });

    if matches!(
        mode,
        "per_month" | "per_year" | "prepaid"
            | "reserved_commitment" | "fixed_subscription" | "unknown"
    ) {
        result["estimate_status"] = json!("not_estimable_from_job_duration");
        result["estimated_incremental_compute_micro_usd"] = Value::Null;
        result["effective_billable_ms"] = Value::Null;
        result["requires_contract_review"] = json!(true);
        result["billing_continues_after_stop"] =
            json!("depends_on_committed_contract_and_storage");
        return Ok(result);
    }

    let base_step = match mode {
        "per_millisecond" => 1_u64,
        "per_second" => 1000_u64,
        "per_minute" => 60_000_u64,
        "per_hour" => 3_600_000_u64,
        "per_day" => 86_400_000_u64,
        "custom_metered" => 0_u64,
        _ => return Err("Unknown GPU billing model".into()),
    };

    let increment_ms = number(quote, "billing_increment_ms")?;
    if increment_ms == 0 || increment_ms > 366 * 86_400_000 {
        return Err("Invalid provider billing increment".into());
    }
    if base_step != 0 && increment_ms != base_step {
        return Err("Declared billing model and increment disagree; use custom_metered for provider-specific increments".into());
    }
    let min_ms = number(quote, "minimum_billable_ms")?;
    let elapsed_ms = number(quote, "billable_elapsed_ms")?;
    let rate = number(quote, "quoted_micro_usd_per_hour")?;
    if elapsed_ms == 0 || elapsed_ms > 366 * 86_400_000
        || min_ms > 366 * 86_400_000 || rate == 0
    {
        return Err("Quote requires positive bounded elapsed time and price".into());
    }
    let step = u128::from(increment_ms);
    let rounded = ceil_div(u128::from(elapsed_ms), step)?
        .checked_mul(step).ok_or("Billing duration overflow")?;
    let billable = ceil_div(rounded.max(u128::from(min_ms)), step)?
        .checked_mul(step).ok_or("Billing duration overflow")?;
    let amount = ceil_div(u128::from(rate).checked_mul(billable)
        .ok_or("Billing amount overflow")?, MS_PER_HOUR)?;
    let max_budget = quote.get("max_approved_compute_micro_usd")
        .map(|v| v.as_u64().ok_or("Budget must be a nonnegative integer"))
        .transpose()?;
    let over_budget = max_budget.is_some_and(|limit| amount > u128::from(limit));

    result["estimate_status"] = json!(if over_budget {
        "estimate_exceeds_approved_compute_budget"
    } else {
        "estimated_compute_only_not_a_billing_guarantee"
    });
    result["quoted_micro_usd_per_hour"] = json!(rate);
    result["minimum_billable_ms"] = json!(min_ms);
    result["billing_increment_ms"] = json!(increment_ms);
    result["input_billable_elapsed_ms"] = json!(elapsed_ms);
    result["effective_billable_ms"] = json!(billable);
    result["estimated_incremental_compute_micro_usd"] = json!(u64::try_from(amount).map_err(|_| "Compute estimate exceeds supported currency range")?);
    result["max_approved_compute_micro_usd"] = json!(max_budget);
    result["over_approved_compute_budget"] = json!(over_budget);
    result["requires_contract_review"] = json!(false);
    result["billing_continues_after_stop"] =
        json!("other_charges_or_contract_may_continue");
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn quote(model:&str, quantum:u64, minimum:u64, elapsed:u64)->Value{
        json!({
            "schema_version":1,
            "billing_model":model,
            "billing_increment_ms":quantum,
            "minimum_billable_ms":minimum,
            "billable_elapsed_ms":elapsed,
            "quoted_micro_usd_per_hour":1_000_000_u64
        })
    }
    #[test]
    fn hourly_display_with_per_second_billing_is_prorated_and_stops_immediately() {
        let out=estimate(&quote("per_second",1000,0,5000)).unwrap();
        assert_eq!(out["effective_billable_ms"],5000);
        assert_eq!(out["estimated_incremental_compute_micro_usd"],1389);
        assert_eq!(out["stop_policy_requires_user_choice"],true);
        assert_eq!(out["wait_until_paid_period_ends"],false);
    }
    #[test]
    fn true_hourly_minimum_charges_full_hour_but_does_not_hold_gpu_running() {
        let out=estimate(&quote("per_hour",3_600_000,3_600_000,5000)).unwrap();
        assert_eq!(out["effective_billable_ms"],3_600_000);
        assert_eq!(out["estimated_incremental_compute_micro_usd"],1_000_000);
        assert_eq!(out["wait_until_paid_period_ends"],false);
    }
    #[test]
    fn minute_day_and_custom_quanta_round_resource_lifetime() {
        let minute=estimate(&quote("per_minute",60_000,60_000,5000)).unwrap();
        assert_eq!(minute["effective_billable_ms"],60_000);
        let day=estimate(&quote("per_day",86_400_000,86_400_000,5000)).unwrap();
        assert_eq!(day["effective_billable_ms"],86_400_000);
        let custom=estimate(&quote("custom_metered",15_000,30_000,5000)).unwrap();
        assert_eq!(custom["effective_billable_ms"],30_000);
        let millisecond=estimate(&quote("per_millisecond",1,0,5001)).unwrap();
        assert_eq!(millisecond["effective_billable_ms"],5001);
    }
    #[test]
    fn prepaid_monthly_and_yearly_cannot_be_costed_from_five_seconds() {
        for mode in ["per_month","per_year","prepaid","reserved_commitment","fixed_subscription","unknown"] {
            let out=estimate(&json!({"schema_version":1,"billing_model":mode})).unwrap();
            assert!(out["estimated_incremental_compute_micro_usd"].is_null());
            assert_eq!(out["requires_contract_review"],true);
            assert_eq!(out["stop_policy_requires_user_choice"],true);
        }
    }
    #[test]
    fn budget_is_an_estimate_not_an_enforced_provider_limit() {
        let mut q=quote("per_hour",3_600_000,3_600_000,5000);
        q["max_approved_compute_micro_usd"]=json!(999_999);
        let out=estimate(&q).unwrap();
        assert_eq!(out["over_approved_compute_budget"],true);
        assert_eq!(out["estimate_status"],"estimate_exceeds_approved_compute_budget");
    }
    #[test]
    fn unknown_or_inconsistent_terms_are_not_guessed() {
        assert!(estimate(&json!({"schema_version":1,"billing_model":"per_hour"})).is_err());
        assert!(estimate(&quote("per_hour",1000,0,5000)).is_err());
        assert!(estimate(&quote("per_second",0,0,5000)).is_err());
        let mut q=quote("per_second",1000,0,5000);
        q["quoted_micro_usd_per_hour"]=json!(0);
        assert!(estimate(&q).is_err());
    }
    #[test]
    fn user_auto_stop_and_no_unapproved_deletion_remain_unchanged() {
        let p=policy().unwrap();
        assert_eq!(p["default_lifecycle"]["wait_until_end_of_paid_period"],false);
        assert_eq!(p["default_lifecycle"]["idle_reuse_requires_new_explicit_permission"],true);
        assert_eq!(p["default_lifecycle"]["no_destructive_fallback_without_separate_approval"],true);
    }
}
