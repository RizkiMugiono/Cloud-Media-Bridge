//! Caller-supplied offers for the same pinned model and workload.
//! A recommendation is evidence for a user decision, never an authorization.
use serde_json::{json, Value};

fn label(v: &Value) -> bool {
    v.as_str()
        .is_some_and(|s| !s.trim().is_empty() && s.len() <= 512 && !s.chars().any(char::is_control))
}

fn positive(v: &Value) -> bool {
    v.as_u64().is_some_and(|n| n > 0)
}

fn offer(v: &Value, gaps: &mut Vec<String>) -> Result<Value, String> {
    for key in ["provider", "gpu", "region"] {
        if !label(&v[key]) {
            gaps.push(format!("Specify {key}"));
        }
    }
    if !v["provider"].as_str().is_some_and(|id| crate::provider_info(id).is_ok()) {
        gaps.push("Use a configured provider".into());
    }
    if !positive(&v["quantity"]) {
        gaps.push("Specify GPU quantity".into());
    }
    let q = &v["billing_quote"];
    if q.is_null() {
        gaps.push("Obtain a provider quote".into());
        return Ok(Value::Null);
    }
    let forecast = crate::billing::estimate(q)?;
    if q["provider"] != v["provider"] {
        return Err("Quoted provider differs from offer".into());
    }
    for key in [
        "pricing_source",
        "verified_at",
        "commitment_terms",
        "billable_start",
        "billable_end",
        "stop_effect",
        "terminate_effect",
        "ancillary_terms",
    ] {
        if !label(&q[key]) {
            gaps.push(format!("Confirm {key}"));
        }
    }
    if q["rate_basis"] != "whole_allocation" {
        gaps.push("Quote the entire GPU allocation".into());
    }
    let compute = if q["billing_model"] == "unknown" {
        gaps.push("Resolve unknown billing terms".into());
        None
    } else if forecast["requires_contract_review"] == true {
        let fixed = q["full_commitment_micro_usd"].as_u64();
        if fixed.is_none()
            || !label(&q["commitment_period_and_renewal"])
            || !positive(&q["billable_elapsed_ms"])
        {
            gaps.push(
                "Obtain full contract liability, period, renewal and expected elapsed time".into(),
            );
        }
        fixed
    } else {
        forecast["estimated_incremental_compute_micro_usd"].as_u64()
    };
    let ancillary = q["ancillary_micro_usd"].as_u64();
    if ancillary.is_none() {
        gaps.push("Estimate ancillary charges".into());
    }
    let total = match (compute, ancillary) {
        (Some(a), Some(b)) => Some(a.checked_add(b).ok_or("Total cost overflow")?),
        _ => None,
    };
    Ok(json!({"compute":forecast,"estimated_total_micro_usd":total,
        "quote_freshness":crate::market::check(q).unwrap_or_else(|_|json!({"status":"source_or_timestamp_requires_review"}))}))
}

pub(crate) fn evaluate(plan: &Value) -> Result<Value, String> {
    let mut gaps = Vec::new();
    let selected = offer(plan, &mut gaps)?;
    if !label(&plan["compatibility_reason"]) {
        gaps.push("Explain selected GPU compatibility with this exact model and workload".into());
    }
    let selected_cost_valid = gaps.is_empty();
    if !positive(&plan["max_elapsed_ms"]) || !positive(&plan["max_total_micro_usd"]) {
        gaps.push("Specify time limit and total budget".into());
    }
    if let (Some(elapsed), Some(limit)) = (
        plan["billing_quote"]["billable_elapsed_ms"].as_u64(),
        plan["max_elapsed_ms"].as_u64(),
    ) {
        if elapsed > limit {
            gaps.push("Expected duration exceeds time limit".into());
        }
    }
    if selected["compute"]["over_approved_compute_budget"] == true
        || selected["estimated_total_micro_usd"]
            .as_u64()
            .zip(plan["max_total_micro_usd"].as_u64())
            .is_some_and(|(a, b)| a > b)
    {
        gaps.push("Selected offer exceeds budget".into());
    }
    let policy = &plan["stop_policy"];
    let valid = match policy["mode"].as_str() {
        Some("after_job" | "manual" | "persistent" | "budget_limit" | "session_end" | "error_trigger") => true,
        Some("idle_timeout" | "reuse") => positive(&policy["idle_timeout_ms"]),
        Some("scheduled" | "billing_boundary") => positive(&policy["stop_at_unix_ms"]),
        Some("disconnect_timeout") => positive(&policy["disconnect_timeout_ms"]),
        _ => false,
    };
    if !valid {
        gaps.push("Choose an explicit stop trigger and its timing".into());
    }
    if !matches!(
        policy["action"].as_str(),
        Some("stop" | "terminate" | "scale_to_zero" | "suspend")
    ) {
        gaps.push("Choose the exact provider action".into());
    }
    if !label(&policy["provider_capability_evidence"]) {
        gaps.push("Verify provider supports the requested action".into());
    }
    if plan["provider"] == "lambda-cloud" && policy["action"] == "stop" {
        gaps.push("Catalog flags possible termination-only billing; reconcile with provider before proposing stop".into());
    }
    let mut alternatives = Vec::new();
    for alt in plan["alternatives"].as_array().into_iter().flatten() {
        let mut issues = Vec::new();
        let estimate = offer(alt, &mut issues)?;
        if alt["model_id"] != plan["model_id"] || alt["model_sha"] != plan["model_sha"] || alt["task"] != plan["task"] {
            issues.push("Alternative must cover the same exact model revision and task".into());
        }
        if !label(&alt["compatibility_reason"]) {
            issues.push("Explain workload compatibility".into());
        }
        if estimate["compute"]["over_approved_compute_budget"] == true || estimate["estimated_total_micro_usd"]
            .as_u64()
            .zip(plan["max_total_micro_usd"].as_u64())
            .is_some_and(|(a, b)| a > b)
            || alt["billing_quote"]["billable_elapsed_ms"]
                .as_u64()
                .zip(plan["max_elapsed_ms"].as_u64())
                .is_some_and(|(a, b)| a > b)
        {
            issues.push("Outside proposed cost or time limits".into());
        }
        let saving = estimate["estimated_total_micro_usd"]
            .as_u64()
            .zip(selected["estimated_total_micro_usd"].as_u64())
            .and_then(|(cost, selected)| selected.checked_sub(cost));
        alternatives.push(
            json!({"offer":alt,"estimate":estimate,"missing_evidence":issues,
            "potential_saving_micro_usd":saving}),
        );
    }
    if alternatives.is_empty() && !label(&plan["alternatives_unavailable_reason"]) {
        gaps.push(
            "Compare a compatible alternative or document its unavailability with the user".into(),
        );
    }
    let cheaper = alternatives
        .iter()
        .filter(|a| {
            selected_cost_valid && a["missing_evidence"].as_array().is_some_and(Vec::is_empty)
                && a["potential_saving_micro_usd"]
                    .as_u64()
                    .is_some_and(|n| n > 0)
        })
        .max_by_key(|a| a["potential_saving_micro_usd"].as_u64());
    Ok(json!({"lower_cost_alternative_to_discuss":cheaper,
        "comparison_basis":"Whole-allocation duration rounded to provider increments plus ancillary estimate, using caller-supplied compatibility evidence.",
        "selected_estimate":selected,"alternatives":alternatives,"review_items":gaps,
        "user_selects_offer_and_policy":true,"executor_requires_fresh_single_use_consent":true,
        "reconcile_before_retry":true,"require_owned_idle_resource_and_provider_confirmation":true}))
}
