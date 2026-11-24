//! Pure evaluation of externally supplied consent and resource observations.
//! Output is advice. The executor independently authenticates the end user,
//! consumes deployment consent atomically and rechecks provider state.
use crate::crypto::sha256;
use serde_json::{json, Value};

pub(crate) fn advise(plan: &Value, receipt: &Value, observed: &Value) -> Result<Value, String> {
    let digest = sha256(plan.to_string().as_bytes());
    let request = plan["request_id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("Use the exact plan from cloud prepare")?;
    let advice = crate::planning::review(plan)?;
    let result = |next: &str| {
        Ok(json!({"recommendation":next,"plan_sha256":digest,
        "bridge_can_execute":false,"approval_granted":false,"caller_evidence_only":true}))
    };
    if receipt["plan_sha256"] != digest
        || receipt["request_id"] != request
        || receipt["end_user_approved"] != true
    {
        return result("ask_user_for_fresh_plan_bound_consent");
    }
    if !advice["advice"]["review_items"]
        .as_array()
        .is_some_and(Vec::is_empty)
    {
        return result("resolve_plan_review_items_with_user");
    }
    if observed["provider"] != plan["provider"]
        || observed["account"] != plan["account"]
        || observed["workspace"] != plan["workspace"]
        || observed["owner_request_id"] != request
        || observed["verified_owned"] != true
        || observed["shared"] != false
        || observed["resource_id"].as_str().is_none_or(str::is_empty)
        || observed["resource_id"] != receipt["resource_id"]
    {
        return result("reconcile_identity_without_mutation");
    }
    match observed["provider_state"].as_str() {
        Some("stopped" | "terminated" | "scaled_to_zero") => {
            return result("verify_remaining_charges")
        }
        Some("running") => (),
        _ => return result("reconcile_provider_state_without_retrying_creation"),
    }
    if observed["active_jobs"] != 0 || observed["active_transfers"] != 0 {
        return result("reconcile_active_work_and_time_or_budget_exposure");
    }
    if observed["supported_action"] != plan["stop_policy"]["action"] {
        return result("verify_provider_action_without_substitution");
    }
    let policy = &plan["stop_policy"];
    let elapsed = observed["elapsed_ms"].as_u64();
    let exposure = observed["exposure_micro_usd"].as_u64();
    let (Some(elapsed), Some(exposure)) = (elapsed, exposure) else {
        return result("reconcile_time_and_cost");
    };
    let reached_limit = Some(elapsed) >= plan["max_elapsed_ms"].as_u64()
        || Some(exposure) >= plan["max_total_micro_usd"].as_u64();
    let due = match policy["mode"].as_str() {
        Some("after_job") => observed["final_job_terminal"].as_bool(),
        Some("manual" | "persistent") => observed["user_requested_stop"].as_bool(),
        Some("budget_limit") => Some(reached_limit),
        Some("session_end") => observed["session_ended"].as_bool(),
        Some("error_trigger") => observed["terminal_error_reconciled"].as_bool(),
        Some("disconnect_timeout") => observed["disconnected_ms"].as_u64().zip(policy["disconnect_timeout_ms"].as_u64()).map(|(n,limit)| n >= limit),
        Some("idle_timeout" | "reuse") => observed["idle_ms"]
            .as_u64()
            .zip(policy["idle_timeout_ms"].as_u64())
            .map(|(idle, limit)| idle >= limit),
        Some("scheduled" | "billing_boundary") => observed["now_unix_ms"]
            .as_u64()
            .zip(policy["stop_at_unix_ms"].as_u64())
            .map(|(now, end)| now >= end),
        _ => None,
    };
    if reached_limit && due != Some(true) {
        return result("limit_reached_escalate_to_user");
    }
    match due {
        None => result("reconcile_trigger_evidence"),
        Some(false) => result("monitor_within_approved_limits"),
        Some(true) => match policy["action"].as_str() {
            Some("stop") => result("request_approved_stop_and_verify"),
            Some("scale_to_zero") => result("request_approved_scale_to_zero_and_verify"),
            Some("suspend") => result("request_approved_suspend_and_verify"),
            Some("terminate") => result("obtain_separate_destructive_confirmation"),
            _ => result("review_provider_capability"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Value {
        serde_json::from_str(include_str!(
            "../../../examples/cloud-deployment-manifest.NOT-EXECUTABLE.json"
        ))
        .unwrap()
    }

    fn receipt(plan: &Value) -> Value {
        json!({"request_id":"req","plan_sha256":sha256(plan.to_string().as_bytes()),
            "end_user_approved":true,"resource_id":"r1"})
    }

    fn observed(plan: &Value) -> Value {
        json!({"provider":plan["provider"],"account":plan["account"],"workspace":plan["workspace"],
            "resource_id":"r1","owner_request_id":"req","verified_owned":true,"shared":false,
            "provider_state":"running","active_jobs":0,"active_transfers":0,"elapsed_ms":100,
            "exposure_micro_usd":20,"supported_action":"stop","final_job_terminal":true,
            "user_requested_stop":true,"idle_ms":25,"now_unix_ms":100})
    }

    #[test]
    fn each_user_trigger_has_a_due_and_waiting_case() {
        for (mode, signal, threshold, waiting) in [
            ("after_job", "final_job_terminal", "", json!(false)),
            ("manual", "user_requested_stop", "", json!(false)),
            ("reuse", "idle_ms", "idle_timeout_ms", json!(24)),
            ("idle_timeout", "idle_ms", "idle_timeout_ms", json!(24)),
            ("scheduled", "now_unix_ms", "stop_at_unix_ms", json!(24)),
        ] {
            let mut p = manifest();
            p["request_id"] = json!("req");
            p["stop_policy"]["mode"] = json!(mode);
            if !threshold.is_empty() {
                p["stop_policy"][threshold] = json!(25);
            }
            let r = receipt(&p);
            let mut obs = observed(&p);
            assert_eq!(
                advise(&p, &r, &obs).unwrap()["recommendation"],
                "request_approved_stop_and_verify"
            );
            obs[signal] = waiting;
            assert_eq!(
                advise(&p, &r, &obs).unwrap()["recommendation"],
                "monitor_within_approved_limits"
            );
        }
    }

    #[test]
    fn alternative_revision_and_new_trigger_evidence_are_checked() {
        let mut p = manifest();
        p["request_id"] = json!("req");
        p["alternatives"][0]["model_sha"] = json!("different");
        let review = crate::planning::review(&p).unwrap();
        assert!(review["advice"]["lower_cost_alternative_to_discuss"].is_null());
        for (mode, signal) in [("session_end", "session_ended"), ("error_trigger", "terminal_error_reconciled")] {
            p["stop_policy"]["mode"] = json!(mode);
            let r = receipt(&p);
            let mut obs = observed(&p);
            assert_eq!(advise(&p, &r, &obs).unwrap()["recommendation"], "reconcile_trigger_evidence");
            obs[signal] = json!(true);
            assert_eq!(advise(&p, &r, &obs).unwrap()["recommendation"], "request_approved_stop_and_verify");
        }
    }

    #[test]
    fn changed_terms_shared_resources_and_uncertainty_cannot_yield_stop_advice() {
        let mut p = manifest();
        p["request_id"] = json!("req");
        let r = receipt(&p);
        for (field, value, expected) in [
            ("shared", json!(true), "reconcile_identity_without_mutation"),
            (
                "active_transfers",
                json!(1),
                "reconcile_active_work_and_time_or_budget_exposure",
            ),
            (
                "provider_state",
                json!("unknown"),
                "reconcile_provider_state_without_retrying_creation",
            ),
            (
                "supported_action",
                json!("terminate"),
                "verify_provider_action_without_substitution",
            ),
        ] {
            let mut obs = observed(&p);
            obs[field] = value;
            assert_eq!(advise(&p, &r, &obs).unwrap()["recommendation"], expected);
        }
        p["region"] = json!("different");
        assert_eq!(
            advise(&p, &r, &observed(&p)).unwrap()["recommendation"],
            "ask_user_for_fresh_plan_bound_consent"
        );
    }

    #[test]
    fn cost_review_compares_full_rounded_allocations_without_selecting_one() {
        let p = manifest();
        let review = crate::planning::review(&p).unwrap();
        assert_eq!(
            review["advice"]["selected_estimate"]["estimated_total_micro_usd"],
            1889
        );
        assert_eq!(
            review["advice"]["lower_cost_alternative_to_discuss"]["potential_saving_micro_usd"],
            416
        );
        assert_eq!(review["plan"]["gpu"], p["gpu"]);
        assert_eq!(review["approval_granted"], false);
        let mut missing = p;
        missing["stop_policy"] = Value::Null;
        assert!(
            !crate::planning::review(&missing).unwrap()["advice"]["review_items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn fixed_commitment_and_provider_actions_require_explicit_terms() {
        let mut p = manifest();
        p["request_id"] = json!("req");
        p["billing_quote"]["billing_model"] = json!("per_year");
        let incomplete = crate::planning::review(&p).unwrap();
        assert!(incomplete["advice"]["selected_estimate"]["estimated_total_micro_usd"].is_null());
        p["billing_quote"]["full_commitment_micro_usd"] = json!(20000);
        p["billing_quote"]["commitment_period_and_renewal"] = json!("one year, no renewal");
        let reviewed = crate::planning::review(&p).unwrap();
        assert_eq!(
            reviewed["advice"]["selected_estimate"]["estimated_total_micro_usd"],
            20500
        );
        for (action, expected) in [
            ("scale_to_zero", "request_approved_scale_to_zero_and_verify"),
            ("terminate", "obtain_separate_destructive_confirmation"),
        ] {
            p["stop_policy"]["action"] = json!(action);
            let mut obs = observed(&p);
            obs["supported_action"] = json!(action);
            assert_eq!(
                advise(&p, &receipt(&p), &obs).unwrap()["recommendation"],
                expected
            );
        }
        p["stop_policy"]["mode"] = json!("manual");
        let mut obs = observed(&p);
        obs["supported_action"] = p["stop_policy"]["action"].clone();
        obs["user_requested_stop"] = json!(false);
        obs["elapsed_ms"] = p["max_elapsed_ms"].clone();
        assert_eq!(
            advise(&p, &receipt(&p), &obs).unwrap()["recommendation"],
            "limit_reached_escalate_to_user"
        );
    }
}
