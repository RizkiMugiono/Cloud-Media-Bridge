//! Read-only supervision from a separate trusted operator process. The
//! recorded intent timestamp is conservative, not provider metering.
use serde_json::{json,Value};
use std::time::{SystemTime,UNIX_EPOCH};

pub(crate) fn now_ms()->Result<u64,String>{
    SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_|"Invalid supervisor clock")?
        .as_millis().try_into().map_err(|_|"Clock overflow".into())
}

pub(crate) fn evaluate(state:&Value,now:u64)->Value{
    let plan=&state["plan"];
    let status=state["status"].as_str().unwrap_or("unknown");
    let mut alerts:Vec<&str>=Vec::new();
    let possibly_billable=!matches!(status,"awaiting_human_approval");
    let elapsed=state["create_intent_unix_ms"].as_u64().and_then(|t|now.checked_sub(t));
    if possibly_billable&&elapsed.is_none(){alerts.push("missing_or_future_create_intent_time");}
    let stop_confirmed=status=="STOP_CONFIRMED";
    let elapsed=if stop_confirmed{
        state["stop_confirmed_unix_ms"].as_u64().filter(|t|*t<=now)
            .zip(state["create_intent_unix_ms"].as_u64()).and_then(|(stop,start)|stop.checked_sub(start))
    }else{elapsed};
    if stop_confirmed&&elapsed.is_none(){alerts.push("missing_or_invalid_stop_confirmation_time");}
    let mut estimated=None;
    if let Some(ms)=elapsed{
        if ms>0{
            let mut quote=plan["billing_quote"].clone();
            if quote.is_object(){
                quote["billable_elapsed_ms"]=json!(ms);
                if let Ok(forecast)=crate::billing::estimate(&quote){
                    let compute=forecast["estimated_incremental_compute_micro_usd"].as_u64()
                        .or_else(||{
                            if forecast["requires_contract_review"]==true&&quote["billing_model"]!="unknown"{
                                quote["full_commitment_micro_usd"].as_u64()
                            }else{None}
                        });
                    estimated=compute.zip(quote["ancillary_micro_usd"].as_u64()).and_then(|(a,b)|a.checked_add(b));
                }
            }
        }
    }
    if possibly_billable&&estimated.is_none(){alerts.push("estimated_exposure_unavailable");}
    if possibly_billable&&!stop_confirmed{
        match (elapsed,plan["max_elapsed_ms"].as_u64()){
            (Some(e),Some(limit)) if limit>0&&e>=limit=>alerts.push("time_limit_reached"),
            (_,Some(limit)) if limit>0=>(),
            _=>alerts.push("time_limit_or_elapsed_unknown"),
        }
        match (estimated,plan["max_total_micro_usd"].as_u64()){
            (Some(cost),Some(limit)) if limit>0&&cost>=limit=>alerts.push("estimated_budget_limit_reached"),
            (Some(_),Some(limit)) if limit>0=>(),
            _=>alerts.push("budget_or_exposure_unknown"),
        }
        if matches!(plan["stop_policy"]["mode"].as_str(),Some("scheduled"|"billing_boundary")){
            match plan["stop_policy"]["stop_at_unix_ms"].as_u64(){
                Some(t) if now>=t=>alerts.push("scheduled_policy_time_reached"),
                Some(_)=>(),
                None=>alerts.push("scheduled_policy_time_unknown"),
            }
        }
    }
    match status{
        "awaiting_human_approval"=>(),
        "CREATE_IN_FLIGHT"|"CREATE_UNCERTAIN"=>alerts.push("creation_uncertain_never_retry"),
        "RUNNING"|"JOB_UNCERTAIN"=>alerts.push("job_or_transfer_uncertain"),
        "STOP_IN_FLIGHT"|"STOP_UNCERTAIN"=>alerts.push("stop_uncertain_never_retry"),
        "STOP_CONFIRMED"=>alerts.push("verify_residual_charges_and_invoice"),
        "CREATED"=>alerts.push("verify_live_state_and_approved_lifecycle"),
        _=>alerts.push("unrecognized_journal_state"),
    }
    json!({"journal_status":status,"observed_at_unix_ms":now,"request_id":plan["request_id"],
        "resource_id":state["resource"]["id"],"provider":plan["provider"],"stop_policy":plan["stop_policy"],
        "elapsed_since_create_intent_ms":elapsed,"illustrative_total_exposure_micro_usd":estimated,
        "alerts":alerts,"needs_operator_attention":!alerts.is_empty(),
        "provider_invoice_verified":false,"hard_budget_enforced":false,
        "provider_mutation_sent":false,"approval_granted":false})
}

#[cfg(test)]
mod tests{
    use super::*;
    fn state()->Value{
        let p:Value=serde_json::from_str(include_str!("../../../examples/cloud-deployment-manifest.NOT-EXECUTABLE.json")).unwrap();
        json!({"status":"CREATED","plan":p,"create_intent_unix_ms":1000})
    }
    fn alert(v:&Value,s:&str)->bool{v["alerts"].as_array().unwrap().iter().any(|x|x==s)}
    #[test]
    fn time_cost_and_crash_are_reported_but_never_authorize_action(){
        let mut s=state();s["plan"]["max_total_micro_usd"]=json!(1000);
        let a=evaluate(&s,61000);
        assert!(alert(&a,"time_limit_reached"));
        assert!(alert(&a,"estimated_budget_limit_reached"));
        assert_eq!(a["hard_budget_enforced"],false);
        assert_eq!(a["provider_mutation_sent"],false);
        s["status"]=json!("STOP_IN_FLIGHT");
        assert!(alert(&evaluate(&s,61000),"stop_uncertain_never_retry"));
        assert!(alert(&evaluate(&s,999),"missing_or_future_create_intent_time"));
    }
    #[test]
    fn confirmed_stop_freezes_compute_estimate_but_not_residual_invoice(){
        let mut s=state();s["status"]=json!("STOP_CONFIRMED");s["stop_confirmed_unix_ms"]=json!(6000);
        let a=evaluate(&s,999999);
        assert_eq!(a["elapsed_since_create_intent_ms"],5000);
        assert!(alert(&a,"verify_residual_charges_and_invoice"));
        assert_eq!(a["provider_invoice_verified"],false);
        assert!(!alert(&a,"time_limit_reached"));
    }
    #[test]
    fn pending_plan_does_not_invent_charges(){
        let mut s=state();s["status"]=json!("awaiting_human_approval");
        s["create_intent_unix_ms"]=Value::Null;
        let a=evaluate(&s,1000);assert!(!a["needs_operator_attention"].as_bool().unwrap());
        assert!(a["illustrative_total_exposure_micro_usd"].is_null());
    }
}
