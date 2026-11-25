//! Local resource journal and one-shot user-approved GPU provisioning.
//! Live operations are not called by MCP; only a human-controlled TTY CLI may
//! provision, and the built-in registry currently has no verified runtime.
//! The same state machine is tested completely offline using Replay transports.
use crate::crypto::{secure_nonce, sha256};
use crate::provider_api::{self, Deployment, Transport};
use crate::provider_http::CurlTransport;
use crate::runtime;
use serde_json::{json, Value};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

const JOURNAL: &str = "cloud-resource.json";
const VERSION: u64 = 1;

fn root_dir(directory: &str) -> Result<PathBuf,String> {
    let path=PathBuf::from(directory);
    if path.as_os_str().is_empty(){return Err("Cloud session directory is required".into());}
    fs::create_dir_all(&path).map_err(|_|"Unable to create cloud session directory".to_string())?;
    let path=fs::canonicalize(path).map_err(|_|"Invalid cloud session directory".to_string())?;
    if !path.is_dir(){return Err("Cloud session must be a directory".into());}
    Ok(path)
}

struct Lock(PathBuf);
impl Lock {
    fn acquire(root:&Path)->Result<Self,String>{
        let path=root.join(".cloud.lock");
        let mut opts=OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]{
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        opts.open(&path).map_err(|_|"Cloud session locked; reconcile the provider before recovering a crash".to_string())?;
        Ok(Self(path))
    }
}
impl Drop for Lock {fn drop(&mut self){let _=fs::remove_file(&self.0);}}

fn save(root:&Path,value:&Value)->Result<(),String>{
    let bytes=serde_json::to_vec_pretty(value).map_err(|_|"Could not serialize journal".to_string())?;
    if bytes.len()>128*1024 {return Err("Cloud journal too large".into());}
    let tmp=root.join(format!(".journal-{}.tmp",secure_nonce()?));
    let mut opts=OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]{
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file=opts.open(&tmp).map_err(|_|"Cannot create private journal temp file".to_string())?;
    file.write_all(&bytes).map_err(|_|"Cannot save cloud journal".to_string())?;
    file.sync_all().map_err(|_|"Cannot sync cloud journal".to_string())?;
    drop(file);
    fs::rename(&tmp,root.join(JOURNAL)).map_err(|_|"Cannot commit cloud journal".to_string())?;
    #[cfg(unix)]
    fs::File::open(root).and_then(|dir|dir.sync_all()).map_err(|_|"Cannot sync cloud journal directory; reconcile before retry".to_string())?;
    Ok(())
}

fn read(root:&Path)->Result<Value,String>{
    let path=root.join(JOURNAL);
    let metadata=fs::symlink_metadata(&path).map_err(|_|"Cloud journal missing".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len()>128*1024 {
        return Err("Invalid or linked cloud journal refused".into());
    }
    let data=fs::read(&path).map_err(|_|"Cannot read cloud journal".to_string())?;
    let state:Value=serde_json::from_slice(&data).map_err(|_|"Invalid cloud journal JSON".to_string())?;
    if state["schema_version"]!=VERSION || state["simulation_only"]!=false {
        return Err("Not a real cloud resource journal".into());
    }
    if state["plan_sha256"]!=sha256(state["plan"].to_string().as_bytes()){
        return Err("Cloud plan digest does not match; refuse mutation".into());
    }
    Ok(state)
}

fn request<'a>(plan:&'a Value,token:&'a str)->Result<Deployment<'a>,String>{
    let get = |k:&str| plan[k].as_str().ok_or_else(||format!("Missing plan field: {k}"));
    let d=Deployment{
        provider:get("provider")?,
        request_id:get("request_id")?,
        image:get("image")?,
        gpu:get("gpu")?,
        disk_gb:plan["disk_gb"].as_u64().ok_or("Missing bounded disk_gb")?,
        region:get("region")?,
        quantity:plan["quantity"].as_u64().ok_or("Missing GPU count")?,
        cloud:plan["provider_parameters"]["cloud"].as_str().unwrap_or(""),
        offer_id:plan["offer_id"].as_u64(),
        model_id:get("model_id")?,
        model_sha:get("model_sha")?,
        task:get("task")?,
        family:get("runtime_family")?,
        worker_token:token
    };
    d.validate()?;
    Ok(d)
}

// Read-only review supports more triggers than the current operator can enforce.
fn should_stop_after_job(plan: &Value) -> Result<bool, String> {
    if plan["stop_policy"]["action"] != "stop" {
        return Err("This operator does not implement the selected provider action".into());
    }
    match plan["stop_policy"]["mode"].as_str() {
        Some("after_job") => Ok(true),
        Some("manual" | "persistent") => Ok(false),
        _ => Err("Selected lifecycle requires an independently verified enforcement mechanism".into()),
    }
}

pub(crate) fn prepare(directory:&str,manifest:&str)->Result<Value,String>{
    let raw = crate::planning::load_manifest(manifest, 16384)?;
    if raw.get("request_id").is_some() || raw.get("nonce").is_some() {
        return Err("Prepare requires a new manifest, not a previously issued request".into());
    }
    let review = crate::planning::review(&raw)?;
    let mut plan = review["plan"].clone();
    let nonce = secure_nonce()?;
    let req_id = secure_nonce()?;
    plan["nonce"] = json!(nonce);
    plan["request_id"] = json!(req_id);
    let root=root_dir(directory)?;
    let _lock=Lock::acquire(&root)?;
    if root.join(JOURNAL).exists(){return Err("Cloud session already exists; never overwrite it".into());}
    let plan_sha=sha256(plan.to_string().as_bytes());
    let state=json!({
        "schema_version":VERSION,
        "simulation_only":false,
        "status":"awaiting_human_approval",
        "plan":plan,
        "plan_sha256":plan_sha,
        "advice":review["advice"],
        "approval":{"status":"pending","consumed":false,"origin":null},
        "resource":null,
        "job":{"active_jobs":0,"active_artifact_transfers":0,"state":"not_submitted"},
        "last_error":null
    });
    save(&root,&state)?;
    Ok(json!({
        "status":"awaiting_human_approval","session":root.to_string_lossy(),
        "plan_sha256":plan_sha,"request_id":req_id,
        "billing_estimate":review["advice"]["selected_estimate"],
        "stop_policy":plan["stop_policy"],"plan":plan,"advice":review["advice"],
        "approval_granted":false,"bridge_can_execute":false,
        "message":"Plan only. No GPU is provisioned; independent authorization and live runtime/provider verification remain required."
    }))
}

fn verified_runtime(plan:&Value)->Result<(),String>{
    let registry=runtime::registry()?;
    let Some(rows)=registry["entries"].as_array() else {return Err("Registry is empty".into())};
    let evidence=rows.iter().find(|row|
        row["id"]==plan["runtime_entry_id"] && row["execution_enabled"]==true
        && row["verification"]["status"]=="verified"
        && row["verification"]["model_tested"]==true
        && row["verification"]["provider_tested"]==true
        && row["verification"]["reviewer_approved"]==true
        && row["verification"]["tested_model_revision"]==plan["model_sha"]
        && row["verification"]["tested_provider_ids"].as_array()
            .is_some_and(|ids|ids.contains(&plan["provider"]))
        && row["delivery"]["oci_image_digest"]==plan["image"]
        && row["delivery"]["worker_implementation_digest"].as_str()
            .is_some_and(|v|v.starts_with("sha256:")&&v.len()==71)
    );
    if evidence.is_none() {
        return Err("No release-verified OCI worker for this exact model SHA and provider. Cloud deployment is blocked, not simulated as success".into());
    }
    Ok(())
}

fn verified_billing_terms(plan:&Value)->Result<(),String>{
    let quote=&plan["billing_quote"];
    if quote.is_null() {
        return Err("Actual provider billing granularity, minimum, and runtime estimate must be confirmed before paid deployment".into());
    }
    let forecast=crate::billing::estimate(quote)?;
    if forecast["requires_contract_review"]==true {
        return Err("Calendar/prepaid/unknown GPU contracts require separate human contract review; automated paid deployment remains disabled".into());
    }
    if forecast["over_approved_compute_budget"]==true {
        return Err("GPU compute estimate exceeds an explicitly approved budget".into());
    }
    if quote["pricing_source"].as_str().is_none_or(|v|v.is_empty()||v.len()>512)
        || quote["provider"]!=plan["provider"]
    {
        return Err("GPU pricing source and quoted provider must match the deployment".into());
    }
    Ok(())
}

fn verified_decision_context(plan: &Value) -> Result<(), String> {
    let review = crate::planning::review(plan)?;
    if crate::market::check(&plan["billing_quote"])?["status"] != "freshness_window_met_caller_assertion_only" {
        return Err("Refresh the exact provider quote before deployment".into());
    }
    if !review["advice"]["review_items"].as_array().is_some_and(Vec::is_empty) {
        return Err("Resolve missing quote, comparison, lifecycle and budget evidence before deployment".into());
    }
    should_stop_after_job(plan)?;
    let handoff=crate::handoff::review(plan)?;
    if !handoff["provider_review_items"].as_array().is_some_and(Vec::is_empty){return Err("Resolve exact provider allocation and native request requirements".into());}
    Ok(())
}

// Manual operator fallback, not an identity system. A trusted agent host
// must independently approve any paid native provider invocation. This CLI
// still has its separate unverified-production gate.
fn confirm_operator(plan:&Value,operation:&str,resource:Value,input:Value)->Result<(),String>{
    if !io::stdin().is_terminal()||!io::stderr().is_terminal(){
        return Err("Manual native operator fallback requires an interactive terminal; use the agent host approval and official provider tools for the agent-driven path".into());
    }
    let scope=json!({"operation":operation,"plan_sha256":sha256(plan.to_string().as_bytes()),
        "resource_id":resource,"input_sha256":input});
    let challenge=format!("CONFIRM {}",sha256(scope.to_string().as_bytes()));
    eprintln!("Exact operation: {}\nPlan: {}\nType {} to confirm.",scope,plan,challenge);
    let mut answer=String::new();
    io::stdin().read_line(&mut answer).map_err(|_|"Cannot read operator confirmation")?;
    if answer.trim_end_matches(['\r','\n'])!=challenge{return Err("Operator did not confirm exact operation".into());}
    Ok(())
}

// This build intentionally preserves adapters without enabling paid cloud operations.
// A registry edit or the former environment opt-in must not bypass the release gate.
fn production_boundary() -> Result<(), String> {
    let policy = crate::deployment_policy()?;
    if policy["execution"]["production_authorization_verified"] != true {
        return Err("Live GPU execution disabled: native provider/runtime and host approval boundary are not release-verified".into());
    }
    Ok(())
}

/// No MCP caller can reach this entrypoint; users explicitly invoke a CLI in
/// their own terminal. Bundled registry has zero enabled workers today.
pub(crate) fn deploy(directory:&str)->Result<Value,String>{
    production_boundary()?;
    let root=root_dir(directory)?;
    let _lock=Lock::acquire(&root)?;
    let mut state=read(&root)?;
    if state["status"]!="awaiting_human_approval" || state["approval"]["consumed"]!=false {
        return Err("Deployment request was already used, changed or cancelled".into());
    }
    verified_runtime(&state["plan"])?;
    verified_billing_terms(&state["plan"])?;
    verified_decision_context(&state["plan"])?;
    if env::var("CMB_ENABLE_LIVE_GPU").as_deref()!=Ok("I_ACCEPT_GPU_CHARGES"){return Err("Explicit live opt-in required in addition to operator confirmation".into());}
    let token=env::var("CMB_WORKER_TOKEN").map_err(|_|"CMB_WORKER_TOKEN must be supplied securely in environment".to_string())?;
    let plan=state["plan"].clone();
    let deployment=request(&plan,&token)?;
    verify_plan_account(&plan,&mut CurlTransport)?;
    confirm_operator(&plan,"deploy",Value::Null,Value::Null)?;
    state["status"]=json!("CREATE_IN_FLIGHT");
    state["approval"]=json!({"status":"approved","consumed":true,"origin":"operator_terminal_confirmation",
        "bound_plan_sha256":state["plan_sha256"]});
    save(&root,&state)?;
    let mut api=CurlTransport;
    match provider_api::create(&mut api,&deployment) {
        Ok(id)=>{
            state["status"]=json!("CREATED");
            state["resource"]=json!({
                "id":id,
                "owner_request_id":plan["request_id"],
                "provider":plan["provider"],
                "created_by_bridge":true,
                "active_jobs":0,
                "active_artifact_transfers":0,
                "stop_state":"ACTIVE_UNVERIFIED"
            });
            save(&root,&state)?;
            Ok(json!({"status":"CREATED","provider":plan["provider"],"resource_id":id,
                      "message":"Resource creation was accepted; worker health, output and approved lifecycle are still to be verified. Billing may be active."}))
        }
        Err(_)=>{
            state["status"]=json!("CREATE_UNCERTAIN");
            state["last_error"]=json!("Provider request failed or returned uncertain response. Identify matching resource in provider account manually; do not repeat create.");
            save(&root,&state)?;
            Err("GPU resource creation uncertain. Do not retry or assume no charges; reconcile manually".into())
        }
    }
}

pub(crate) fn status(directory:&str)->Result<Value,String>{
    let root=root_dir(directory)?;
    let state=read(&root)?;
    Ok(json!({
        "status":state["status"],
        "provider":state["plan"]["provider"],
        "request_id":state["plan"]["request_id"],
        "resource":state["resource"],
        "job":state["job"],
        "approval":state["approval"],
        "stop_policy":state["plan"]["stop_policy"],
        "plan":state["plan"],"plan_sha256":state["plan_sha256"],"advice":state["advice"],
        "last_error":state["last_error"],
        "possible_storage_or_network_charges":true,
        "live_cloud_operations_unverified":true
    }))
}

fn reconcile_with<T:Transport>(root:&Path,api:&mut T)->Result<Value,String>{
    let mut state=read(root)?;
    if state["status"]!="CREATED" {
        return Err("Only recorded, owned resources are eligible for stop reconciliation".into());
    }
    let resource=state["resource"].clone();
    let provider=state["plan"]["provider"].as_str().ok_or("Unknown provider")?;
    let id=resource["id"].as_str().ok_or("Missing resource id")?;
    let owner=state["plan"]["request_id"].as_str().ok_or("Missing request id")?;
    // Persist an uncertain mutation boundary BEFORE sending any stop request.
    // After a crash, recovery is GET-only and never blindly resends stop.
    let (provider,id,owner)=(provider.to_string(),id.to_string(),owner.to_string());
    state["status"]=json!("STOP_IN_FLIGHT");
    save(root,&state)?;
    let answer=match provider_api::stop_owned(api,&provider,&id,&owner,&resource){
        Ok(answer)=>answer,
        Err(e)=>{
            state["status"]=json!("STOP_UNCERTAIN");
            state["resource"]["stop_state"]=json!("STOP_UNCERTAIN");
            state["last_error"]=json!("Stop may not have completed; read-only provider reconciliation required");
            save(root,&state)?;
            return Err(e);
        }
    };
    state["status"]=answer["state"].clone();
    state["resource"]["stop_state"]=answer["state"].clone();
    save(root,&state)?;
    Ok(answer)
}

// No mutation or provider billing event is inferred from a GET result.
fn inspect_with<T:Transport>(state:&Value,api:&mut T)->Result<Value,String>{
    let resource=&state["resource"];
    let plan=&state["plan"];
    let provider=plan["provider"].as_str().ok_or("Missing provider")?;
    let id=resource["id"].as_str().ok_or("No recorded resource ID; use provider inventory to reconcile uncertain creation")?;
    let owner=plan["request_id"].as_str().ok_or("Missing request ID")?;
    let observed=provider_api::observe_owned(api,provider,id,owner,resource)?;
    Ok(json!({"journal_status":state["status"],"observation":observed,
        "next_step":match (state["status"].as_str(),observed["provider_state"].as_str()){
            (Some("STOP_IN_FLIGHT"|"STOP_UNCERTAIN"),Some("stopped"))=>"verify_remaining_charges_and_record_operator_reconciliation",
            (Some("RUNNING"|"JOB_UNCERTAIN"),_)=>"reconcile_worker_jobs_and_transfers_before_any_mutation",
            (Some("STOP_IN_FLIGHT"|"STOP_UNCERTAIN"),_)=>"reconcile_previous_stop_without_replay",
            (_,Some("unknown"))=>"reconcile_provider_state_without_mutation",
            _=>"monitor_and_apply_only_user_approved_policy",
        },"journal_modified":false,"provider_mutation_sent":false,"approval_granted":false}))
}
fn verify_plan_account<T:Transport>(plan:&Value,api:&mut T)->Result<Value,String>{
    provider_api::verify_account(api,plan["provider"].as_str().ok_or("Missing provider")?,plan["account"].as_str().ok_or("Missing account ID")?)
}
pub(crate) fn account_check(directory:&str)->Result<Value,String>{
    let root=fs::canonicalize(directory).map_err(|_|"Session directory missing")?;
    let state=read(&root)?;
    verify_plan_account(&state["plan"],&mut CurlTransport)
}
pub(crate) fn inspect(directory:&str)->Result<Value,String>{
    let root=fs::canonicalize(directory).map_err(|_|"Session directory missing")?;
    let state=read(&root)?;
    verify_plan_account(&state["plan"],&mut CurlTransport)?;
    inspect_with(&state,&mut CurlTransport)
}

pub(crate) fn job_check(directory:&str)->Result<Value,String>{
    let root=fs::canonicalize(directory).map_err(|_|"Session directory missing")?;
    let state=read(&root)?;
    verify_plan_account(&state["plan"],&mut CurlTransport)?;
    let job=&state["job"];
    let id=job["request_id"].as_str().ok_or("No recorded job request ID; do not retry uncertain work")?;
    let instance=job["worker_instance_id"].as_str().ok_or("No recorded worker instance; reconcile manually")?;
    let observation=crate::remote_worker::inspect_job(&state["plan"],id,instance)?;
    Ok(json!({"journal_status":state["status"],"observation":observation,
        "journal_modified":false,"provider_mutation_sent":false,"approval_granted":false}))
}

/// Operator-only real worker execution. The credentialed provider lifecycle
/// remains bound to the original one-shot human-approved deployment plan.
/// This operation never approves a new deployment.
pub(crate) fn generate(directory:&str, inputs_manifest:&str)->Result<Value,String>{
    production_boundary()?;
    let root=root_dir(directory)?;
    let _lock=Lock::acquire(&root)?;
    let mut state=read(&root)?;
    if state["status"]!="CREATED"
        || state["approval"]["origin"]!="operator_terminal_confirmation"
        || state["approval"]["status"]!="approved"
        || state["approval"]["consumed"]!=true
        || state["approval"]["bound_plan_sha256"]!=state["plan_sha256"]
        || state["resource"]["created_by_bridge"]!=true
        || state["resource"]["owner_request_id"]!=state["plan"]["request_id"]
        || state["resource"]["provider"]!=state["plan"]["provider"]
        || state["job"]["active_jobs"].as_u64().unwrap_or(u64::MAX)!=0
        || state["resource"]["active_jobs"].as_u64().unwrap_or(u64::MAX)!=0
    {
        return Err("Session is not an approved, idle, bridge-owned cloud deployment".into());
    }
    let auto_stop = should_stop_after_job(&state["plan"])?;
    verified_runtime(&state["plan"])?;
    let path=Path::new(inputs_manifest);
    let bytes=fs::read(path).map_err(|_|"Cannot read media input manifest".to_string())?;
    if bytes.len()>1024*1024 {return Err("Media input manifest too large".into());}
    let input:Value=serde_json::from_slice(&bytes)
        .map_err(|_|"Invalid media input JSON".to_string())?;
    if !input["inputs"].is_array() {
        return Err("Input JSON needs an inputs array".into());
    }

    verify_plan_account(&state["plan"],&mut CurlTransport)?;
    let worker_instance=crate::remote_worker::instance(&state["plan"])?;
    let job_id=secure_nonce()?;
    confirm_operator(&state["plan"],"generate",state["resource"]["id"].clone(),json!(sha256(&bytes)))?;
    state["job"]["request_id"]=json!(job_id);
    state["job"]["worker_instance_id"]=json!(worker_instance);
    // Save the active job state before any remote media request. If the agent
    // crashes, the operator must inspect/reconcile rather than guessing idle.
    state["status"]=json!("RUNNING");
    state["job"]["active_jobs"]=json!(1);
    state["job"]["state"]=json!("running");
    state["resource"]["active_jobs"]=json!(1);
    save(&root,&state)?;

    let plan=&state["plan"];
    let result=crate::remote_worker::generate(&root.join("artifacts"),plan,&job_id,&worker_instance,&input);

    let mut state=read(&root)?;
    if result.is_err(){
        state["status"]=json!("JOB_UNCERTAIN");
        state["job"]["state"]=json!("failed_or_uncertain");
        state["last_error"]=json!("Reconcile active work and transfers before retry or stop.");
        save(&root,&state)?;
        return Ok(json!({"generation":{"status":"FAILED_OR_UNCERTAIN","artifact_verified":false},
            "cleanup":{"state":"RECONCILIATION_REQUIRED","possible_continuing_gpu_charges":true},
            "message":"No automatic stop or repeat while job state is uncertain."}));
    }
    state["job"]["active_jobs"]=json!(0);
    state["resource"]["active_jobs"]=json!(0);
    state["job"]["state"]=json!(if result.is_ok(){"completed"}else{"failed_or_uncertain"});
    state["status"]=json!("CREATED");
    if let Ok(ref receipt)=result {
        state["job"]["receipt"]=receipt.clone();
    } else {
        state["last_error"]=json!("Worker inference or artifact retrieval failed; verify resource state before further operation.");
    }
    save(&root,&state)?;

    let cleanup = if auto_stop {
        let mut api = CurlTransport;
        reconcile_with(&root, &mut api)
    } else {
        Ok(json!({"state":"RETAINED_BY_APPROVED_POLICY","possible_continuing_gpu_charges":true,
            "manual_stop_required":true}))
    };
    if let Err(ref error)=cleanup {
        let mut state=read(&root)?;
        state["status"]=json!("STOP_UNCERTAIN");
        state["last_error"]=json!(format!("Automatic stop not confirmed: {error}; charges may continue"));
        save(&root,&state)?;
    }
    // The caller sees both generation and cleanup independently. An artifact
    // does not imply that compute stopped or provider billing has ceased.
    Ok(json!({
        "generation": match result {
            Ok(receipt)=>receipt,
            Err(_)=>json!({"status":"FAILED_OR_UNCERTAIN","artifact_verified":false})
        },
        "cleanup":match cleanup {
            Ok(result)=>result,
            Err(_)=>json!({"state":"STOP_UNCERTAIN","manual_reconciliation_required":true,
                "possible_continuing_gpu_charges":true})
        },
        "message":if auto_stop {"Approved provider stop attempted; a powered-off client cannot guarantee recovery."} else {"Manual-stop policy retained compute; charges may continue until the user stops it."}
    }))
}

/// Executed only as an explicit operator CLI command. No model or worker
/// generation is claimed completed merely by requesting stop.
pub(crate) fn stop(directory:&str)->Result<Value,String>{
    production_boundary()?;
    let root=root_dir(directory)?;
    let _lock=Lock::acquire(&root)?;
    let state = read(&root)?;
    if state["status"]!="CREATED"{return Err("Reconcile uncertain resource before stop; never blindly replay".into());}
    verify_plan_account(&state["plan"],&mut CurlTransport)?;
    confirm_operator(&state["plan"],"stop",state["resource"]["id"].clone(),Value::Null)?;
    let mut api=CurlTransport;
    reconcile_with(&root,&mut api)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_api::Operation;
    struct Replay{replies:Vec<Value>,requests:Vec<Operation>}
    impl Transport for Replay {
        fn send(&mut self,op:&Operation)->Result<Value,String>{
            self.requests.push(op.clone());
            if self.replies.is_empty(){return Err("No mocked provider HTTP response".into());}
            Ok(self.replies.remove(0))
        }
    }
    fn session()->PathBuf{
        let path=env::temp_dir().join(format!("cmb-cloud-contract-{}",secure_nonce().unwrap()));
        fs::create_dir(&path).unwrap();
        path
    }
    fn state(provider:&str,id:&str)->Value{
        let plan=json!({"provider":provider,"request_id":"req001","model_id":"example/media",
            "model_sha":"0123456789abcdef0123456789abcdef01234567","image":format!("ghcr.io/test/worker@sha256:{}", "a".repeat(64)),
            "nonce":"n01"});
        json!({"schema_version":VERSION,"simulation_only":false,"status":"CREATED",
            "plan_sha256":sha256(plan.to_string().as_bytes()),
            "plan":plan,
            "resource":{"provider":provider,"id":id,"owner_request_id":"req001",
                "created_by_bridge":true,"active_jobs":0,"active_artifact_transfers":0},
            "job":{"active_jobs":0,"active_artifact_transfers":0}})
    }
    #[test]
    fn production_is_disabled_and_operator_never_substitutes_lifecycle() {
        assert!(production_boundary().is_err());
        assert!(should_stop_after_job(&json!({})).is_err());
        assert!(should_stop_after_job(&json!({"stop_policy":{"mode":"idle_timeout","action":"stop"}})).is_err());
        assert!(should_stop_after_job(&json!({"stop_policy":{"mode":"after_job","action":"terminate"}})).is_err());
        for billing in ["per_second","per_hour","per_year"] {
            let mut plan = json!({"billing_quote":{"billing_model":billing},"stop_policy":{"mode":"after_job","action":"stop"}});
            assert!(should_stop_after_job(&plan).unwrap());
            plan["stop_policy"]["mode"] = json!("manual");
            assert!(!should_stop_after_job(&plan).unwrap());
        }
    }

    #[test]
    fn a_provider_confirmed_stop_is_persisted_before_reporting_success() {
        let dir=session();
        save(&dir,&state("runpod-pods","p01")).unwrap();
        let mut replay=Replay{replies:vec![
            json!({"id":"p01","name":"cmb-req001","status":"RUNNING"}),json!({"success":true}),
            json!({"id":"p01","name":"cmb-req001","status":"EXITED","runtimeStatus":"stopped"})
        ],requests:vec![]};
        let result=reconcile_with(&dir,&mut replay).unwrap();
        assert_eq!(result["state"],"STOP_CONFIRMED");
        assert_eq!(read(&dir).unwrap()["status"],"STOP_CONFIRMED");
        assert_eq!(replay.requests.len(),3);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn uncertain_stop_is_never_turned_into_a_false_success() {
        let dir=session();
        save(&dir,&state("vast-instances","123")).unwrap();
        let mut replay=Replay{replies:vec![
            json!({"id":123,"label":"cmb-req001","actual_status":"running"}),json!({"success":true}),
            json!({"id":123,"label":"cmb-req001","actual_status":"running"})
        ],requests:vec![]};
        let result=reconcile_with(&dir,&mut replay).unwrap();
        assert_eq!(result["state"],"STOP_UNCERTAIN");
        assert_eq!(read(&dir).unwrap()["status"],"STOP_UNCERTAIN");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn stop_transport_error_is_persisted_and_recovery_never_replays(){
        let dir=session();
        save(&dir,&state("runpod-pods","p01")).unwrap();
        let mut fake=Replay{replies:vec![json!({"id":"p01","name":"cmb-req001","status":"RUNNING"})],requests:vec![]};
        assert!(reconcile_with(&dir,&mut fake).is_err());
        assert_eq!(read(&dir).unwrap()["status"],"STOP_UNCERTAIN");
        assert_eq!(fake.requests.len(),2);
        let mut recovery=Replay{replies:vec![json!({"id":"p01","name":"cmb-req001","status":"EXITED"})],requests:vec![]};
        assert!(reconcile_with(&dir,&mut recovery).is_err());
        assert!(recovery.requests.is_empty());
        let obs=inspect_with(&read(&dir).unwrap(),&mut recovery).unwrap();
        assert_eq!(obs["observation"]["provider_state"],"stopped");
        assert_eq!(obs["journal_modified"],false);
        assert_eq!(recovery.requests.len(),1);
        assert_eq!(recovery.requests[0].method,"GET");
        assert_eq!(read(&dir).unwrap()["status"],"STOP_UNCERTAIN");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn journal_tampering_or_unknown_resources_refuse_any_mutation() {
        let dir=session();
        let mut state=state("runpod-pods","p01");
        state["plan"]["provider"]=json!("evil-provider");
        save(&dir,&state).unwrap();
        assert!(read(&dir).is_err());
        let mut replay=Replay{replies:vec![],requests:vec![]};
        assert!(reconcile_with(&dir,&mut replay).is_err());
        assert!(replay.requests.is_empty());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn paid_deployment_requires_verified_billing_terms_within_budget() {
        let mut plan=json!({"provider":"runpod-pods","billing_quote":null});
        assert!(verified_billing_terms(&plan).is_err());

        plan["billing_quote"]=json!({
            "schema_version":1,"billing_model":"per_month","provider":"runpod-pods",
            "pricing_source":"example_monthly_commitment"
        });
        assert!(verified_billing_terms(&plan).is_err());

        plan["billing_quote"]=json!({
            "schema_version":1,"billing_model":"per_hour",
            "billing_increment_ms":3_600_000,"minimum_billable_ms":3_600_000,
            "billable_elapsed_ms":5_000,"quoted_micro_usd_per_hour":1_000_000,
            "max_approved_compute_micro_usd":50_000,
            "provider":"runpod-pods","pricing_source":"offline_quote_not_live"
        });
        assert!(verified_billing_terms(&plan).is_err());
        plan["billing_quote"]["max_approved_compute_micro_usd"]=json!(1_000_000);
        assert!(verified_billing_terms(&plan).is_ok());
        plan["billing_quote"].as_object_mut().unwrap().remove("max_approved_compute_micro_usd");
        assert!(verified_billing_terms(&plan).is_ok());
        plan["billing_quote"]["provider"]=json!("vast-instances");
        assert!(verified_billing_terms(&plan).is_err());
    }

    #[test]
    fn deployment_is_blocked_without_pinned_and_live_verified_runtime_evidence() {
        let dir=session();
        let plan=json!({"provider":"runpod-pods","runtime_entry_id":"hf-diffusers-image-oci",
            "model_sha":"0123456789abcdef0123456789abcdef01234567","image":format!("ghcr.io/test/worker@sha256:{}", "a".repeat(64))});
        assert!(verified_runtime(&plan).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
