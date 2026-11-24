//! Construct an action draft from an explicit lifecycle choice and caller
//! observations. This is never approval, a provider response or dispatch.
use serde_json::{json,Value};
fn id(v:&Value)->Option<&str>{
    v.as_str().filter(|s|!s.is_empty()&&s.len()<=128&&s.bytes().all(|b|b.is_ascii_alphanumeric()||b"-_.".contains(&b)))
}
pub(crate) fn draft(plan:&Value,receipt:&Value,observed:&Value)->Result<Value,String>{
    let advice=crate::lifecycle::advise(plan,receipt,observed)?;
    let recommendation=advice["recommendation"].as_str().ok_or("No lifecycle advice")?;
    let due=matches!(recommendation,"request_approved_stop_and_verify"|"request_approved_scale_to_zero_and_verify"|"request_approved_suspend_and_verify"|"obtain_separate_destructive_confirmation");
    let mut out=json!({"advice":advice,"action_draft":null,"provider_operation_sent":false,"approval_granted":false,
        "requires_trusted_executor":true,"requires_provider_reconciliation":true});
    if !due{return Ok(out);}
    let rid=id(&observed["resource_id"]).ok_or("Invalid provider resource ID")?;
    let provider=plan["provider"].as_str().ok_or("Missing provider")?;
    let action=plan["stop_policy"]["action"].as_str().ok_or("Missing action")?;
    let candidate=match (provider,action){
        ("runpod-pods","stop")=>json!({"transport":"rest_draft","method":"POST","url":format!("https://api.runpod.io/v2/pods/{rid}/action"),"body":{"action":"stop"},"verify_get":format!("https://api.runpod.io/v2/pods/{rid}"),"expected_state":"stopped"}),
        ("runpod-pods","terminate")=>json!({"transport":"rest_draft","method":"DELETE","url":format!("https://api.runpod.io/v2/pods/{rid}"),"verify_get":format!("https://api.runpod.io/v2/pods/{rid}"),"expected_state":"terminated"}),
        ("vast-instances","stop") if rid.bytes().all(|b|b.is_ascii_digit())=>json!({"transport":"rest_draft","method":"PUT","url":format!("https://cloud.vast.ai/api/v0/instances/{rid}/"),"body":{"state":"stopped"},"verify_get":format!("https://cloud.vast.ai/api/v0/instances/{rid}/"),"expected_state":"stopped"}),
        ("vast-instances","terminate") if rid.bytes().all(|b|b.is_ascii_digit())=>json!({"transport":"official_cli_draft","executable":"vastai","arguments":["destroy","instance",rid],"expected_state":"terminated"}),
        ("lambda-cloud","terminate")=>json!({"transport":"rest_draft","method":"POST","url":"https://cloud.lambda.com/api/v1/instance-operations/terminate","body":{"instance_ids":[rid]},"verify_get":format!("https://cloud.lambda.com/api/v1/instances/{rid}"),"expected_state":"terminated"}),
        ("modal","scale_to_zero")=>json!({"transport":"official_cli_draft","executable":"modal","arguments":["server","autoscaler","update",rid,"--min-containers","0","--buffer-containers","0"],"verification":"Inspect current container metrics and active requests; zero minimum does not prove zero active containers"}),
        ("runpod-serverless"|"vast-serverless","scale_to_zero")=>json!({"transport":"official_agent_handoff","resource_id":rid,"requirements":["Verify exact endpoint/workergroup identity","Set only the approved minimum and idle policy if supported","Wait for zero actual workers and no in-flight requests"],"expected_state":"scaled_to_zero"}),
        _=>json!({"transport":"not_implemented_or_external","resource_id":rid,"reason":"Use independently authorized provider-native mechanism only; never substitute a destructive action"}),
    };
    out["action_draft"]=candidate;
    out["selected_action"]=json!(action);
    out["separate_destructive_confirmation_required"]=json!(action=="terminate");
    out["signed_operation_scope"]=json!({"operation":action,"resource_id":rid});
    out["official_sources"]=crate::market::sources(Some(provider))?;
    Ok(out)
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn native_stop_draft_never_dispatches_and_uncertainty_has_no_draft(){
        let mut p:Value=serde_json::from_str(include_str!("../../../examples/cloud-deployment-manifest.NOT-EXECUTABLE.json")).unwrap();
        p["request_id"]=json!("req");
        let receipt=json!({"request_id":"req","plan_sha256":crate::crypto::sha256(p.to_string().as_bytes()),"end_user_approved":true,"resource_id":"r1"});
        let mut observed=json!({"provider":p["provider"],"account":p["account"],"workspace":p["workspace"],"owner_request_id":"req","resource_id":"r1","verified_owned":true,"shared":false,"provider_state":"running","active_jobs":0,"active_transfers":0,"elapsed_ms":100,"exposure_micro_usd":20,"supported_action":"stop","final_job_terminal":true});
        let result=draft(&p,&receipt,&observed).unwrap();
        assert_eq!(result["action_draft"]["method"],"POST");
        assert_eq!(result["action_draft"]["body"]["action"],"stop");
        assert_eq!(result["provider_operation_sent"],false);
        observed["shared"]=json!(true);
        assert!(draft(&p,&receipt,&observed).unwrap()["action_draft"].is_null());
    }
}
