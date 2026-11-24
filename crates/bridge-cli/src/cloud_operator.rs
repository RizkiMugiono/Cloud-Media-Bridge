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
    fs::rename(&tmp,root.join(JOURNAL)).map_err(|_|"Cannot commit cloud journal".to_string())
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
        return Err("Actual provider billing granularity, minimum, runtime estimate and compute budget must be confirmed before paid deployment".into());
    }
    let forecast=crate::billing::estimate(quote)?;
    if forecast["requires_contract_review"]==true {
        return Err("Calendar/prepaid/unknown GPU contracts require separate human contract review; automated paid deployment remains disabled".into());
    }
    if forecast["over_approved_compute_budget"]==true
        || forecast["max_approved_compute_micro_usd"].is_null()
    {
        return Err("GPU compute estimate exceeds an explicitly approved budget or no compute budget is supplied".into());
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
    if !review["advice"]["review_items"].as_array().is_some_and(Vec::is_empty) {
        return Err("Resolve missing quote, comparison, lifecycle and budget evidence before deployment".into());
    }
    should_stop_after_job(plan)?;
    if plan["quantity"] != 1 {
        return Err("Current provisioning contract supports one GPU only".into());
    }
    Ok(())
}

// This build intentionally preserves adapters without enabling paid cloud operations.
// A registry edit or the former environment opt-in must not bypass the release gate.
fn production_boundary() -> Result<(), String> {
    let policy = crate::deployment_policy()?;
    if policy["execution"]["production_authorization_verified"] != true {
        return Err("Live GPU execution disabled: independent user authorization and exact provider allocation are not release-verified".into());
    }
    Ok(())
}

fn approved_with_tty(plan:&Value)->Result<(),String>{
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        return Err("Each paid deployment requires a local human terminal. MCP/redirected input cannot approve".into());
    }
    if env::var("CMB_ENABLE_LIVE_GPU").as_deref()!=Ok("I_ACCEPT_GPU_CHARGES"){
        return Err("CMB_ENABLE_LIVE_GPU must be explicitly set to I_ACCEPT_GPU_CHARGES".into());
    }
    eprintln!("REAL GPU DEPLOYMENT. This operation can create billable resources.");
    eprintln!("Provider: {} | Account: {} | Workspace: {}",plan["provider"],plan["account"],plan["workspace"]);
    eprintln!("Model: {} at {}",plan["model_id"],plan["model_sha"]);
    eprintln!("Image: {} | GPU: {} | Disk GB: {}",plan["image"],plan["gpu"],plan["disk_gb"]);
    eprintln!("Plan SHA256: {}",sha256(plan.to_string().as_bytes()));
    eprintln!("Billing model: {} | estimate (micro-USD, compute only): {} | max approved compute (micro-USD): {}",
        plan["billing_quote"]["billing_model"],
        crate::billing::estimate(&plan["billing_quote"])?["estimated_incremental_compute_micro_usd"],
        plan["billing_quote"]["max_approved_compute_micro_usd"]);
    eprintln!("Source: {}. Stopping compute does not cancel prepaid commitments, storage, network or taxes.",
        plan["billing_quote"]["pricing_source"]);
    eprintln!("Exact plan (including lifecycle choice, allocation scope and cost disclosures): {}", plan);
    eprintln!("Only an explicitly selected lifecycle policy can trigger a stop; manual retention may incur continuing, uncapped charges.");
    eprintln!("Stopping compute does not imply deletion, termination or cancellation of commitments.");
    let nonce=plan["nonce"].as_str().ok_or("Missing nonce")?;
    eprint!("Type DEPLOY {nonce} exactly: ");
    io::stderr().flush().map_err(|_|"Cannot flush terminal".to_string())?;
    let mut answer=String::new();
    io::stdin().read_line(&mut answer).map_err(|_|"Cannot read terminal approval".to_string())?;
    if answer.trim_end_matches(['\r','\n'])!=format!("DEPLOY {nonce}"){
        return Err("Deployment approval challenge did not match".into());
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
    approved_with_tty(&state["plan"])?;
    let token=env::var("CMB_WORKER_TOKEN").map_err(|_|"CMB_WORKER_TOKEN must be supplied securely in environment".to_string())?;
    let plan=state["plan"].clone();
    let deployment=request(&plan,&token)?;
    state["status"]=json!("CREATE_IN_FLIGHT");
    state["approval"]=json!({"status":"approved","consumed":true,"origin":"human_terminal",
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
    if state["status"]!="CREATED" && state["status"]!="STOP_UNCERTAIN" {
        return Err("Only recorded, owned resources are eligible for stop reconciliation".into());
    }
    let resource=state["resource"].clone();
    let provider=state["plan"]["provider"].as_str().ok_or("Unknown provider")?;
    let id=resource["id"].as_str().ok_or("Missing resource id")?;
    let owner=state["plan"]["request_id"].as_str().ok_or("Missing request id")?;
    let answer=provider_api::stop_owned(api,provider,id,owner,&resource)?;
    state["status"]=answer["state"].clone();
    state["resource"]["stop_state"]=answer["state"].clone();
    save(root,&state)?;
    Ok(answer)
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
        || state["approval"]["origin"]!="human_terminal"
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

    // Save the active job state before any remote media request. If the agent
    // crashes, the operator must inspect/reconcile rather than guessing idle.
    state["status"]=json!("RUNNING");
    state["job"]["active_jobs"]=json!(1);
    state["job"]["state"]=json!("running");
    state["resource"]["active_jobs"]=json!(1);
    save(&root,&state)?;

    let plan=&state["plan"];
    let result=crate::remote_worker::generate(&root.join("artifacts"),
        plan["model_id"].as_str().ok_or("Missing model ID")?,
        plan["model_sha"].as_str().ok_or("Missing model SHA")?,
        plan["task"].as_str().ok_or("Missing task")?,
        plan["runtime_family"].as_str().ok_or("Missing family")?,
        plan["request_id"].as_str().ok_or("Missing request ID")?,
        &input);

    let mut state=read(&root)?;
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
    // Explicit manual CLI stop is a separate user-authorized operation.
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        return Err("Manual cloud stop requires a human terminal".into());
    }
    let id = state["resource"]["id"].as_str().ok_or("Missing resource id")?;
    let challenge = format!("STOP {id}");
    eprintln!("Non-destructive stop of recorded resource {} on {}. Storage charges may continue.", state["resource"]["id"], state["plan"]["provider"]);
    eprint!("Type {challenge} exactly: ");
    io::stderr().flush().map_err(|_| "Cannot flush terminal".to_string())?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer).map_err(|_| "Cannot read terminal approval".to_string())?;
    if answer.trim_end_matches(['\r','\n']) != challenge {
        return Err("Stop approval challenge did not match".into());
    }
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
            json!({"desiredStatus":"RUNNING"}),json!({"success":true}),
            json!({"status":"EXITED","runtimeStatus":"stopped"})
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
            json!({"actual_status":"running"}),json!({"success":true}),
            json!({"actual_status":"running"})
        ],requests:vec![]};
        let result=reconcile_with(&dir,&mut replay).unwrap();
        assert_eq!(result["state"],"STOP_UNCERTAIN");
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
